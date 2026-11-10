// A small GPU runtime for fragment shaders written for WebGL 2 (GLSL ES 3.00), so effects from
// shader libraries run close to verbatim. Each shader is rewritten into the GLSL naga accepts
// (uniforms in one std140 block, separate textures and samplers, explicit locations), compiled
// once, and drawn offscreen as a full-frame quad; the result is read back as a pixmap.
//
// Conventions follow the remocn canvas presentation: `v_uv` runs top-down (0,0 is the top left),
// scene textures are straight (unpremultiplied) RGBA, linear filtering, clamp to edge.

use parking_lot::Mutex;
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::OnceLock;
use tiny_skia::Pixmap;

/// A uniform value: float, vec2/3/4, or an array of vec4.
#[derive(Clone, Debug)]
pub enum U {
    F(f32),
    I(i32),
    V2([f32; 2]),
    V3([f32; 3]),
    V4([f32; 4]),
    V4s(Vec<[f32; 4]>),
}

#[derive(Clone, Debug)]
struct Field {
    name: String,
    ty: String,
    count: usize,
    offset: usize,
}

struct Program {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    fields: Vec<Field>,
    block_size: usize,
    textures: Vec<String>,
}

pub struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    vertex: wgpu::ShaderModule,
    sampler: wgpu::Sampler,
    programs: Mutex<HashMap<String, Result<std::sync::Arc<Program>, String>>>,
}

const VERTEX: &str = r#"
struct Out { @builtin(position) pos: vec4<f32>, @location(0) uv: vec2<f32> };
@vertex fn main(@builtin(vertex_index) i: u32) -> Out {
    var p = array<vec2<f32>, 3>(vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
    var o: Out;
    o.pos = vec4(p[i], 0.0, 1.0);
    o.uv = vec2(p[i].x * 0.5 + 0.5, 0.5 - p[i].y * 0.5);
    return o;
}
"#;

pub fn gpu() -> Option<&'static Gpu> {
    static G: OnceLock<Option<Gpu>> = OnceLock::new();
    G.get_or_init(|| {
        let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
        // Metal on Apple; Vulkan on Linux, DirectX 12 or Vulkan on Windows.
        desc.backends = if cfg!(any(target_os = "macos", target_os = "ios")) { wgpu::Backends::METAL } else { wgpu::Backends::VULKAN | wgpu::Backends::DX12 };
        let instance = wgpu::Instance::new(desc);
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions { power_preference: wgpu::PowerPreference::HighPerformance, ..Default::default() })).ok()?;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("rayna-fx"),
            // What this GPU offers: the iOS simulator falls short of the WebGPU defaults.
            required_limits: adapter.limits(),
            ..Default::default()
        })).ok()?;
        let vertex = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("quad"), source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(VERTEX)) });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Some(Gpu { device, queue, vertex, sampler, programs: Mutex::new(HashMap::new()) })
    })
    .as_ref()
}

/// std140 size and alignment of a type.
fn layout_of(ty: &str) -> (usize, usize) {
    match ty {
        "float" | "int" | "bool" => (4, 4),
        "vec2" => (8, 8),
        "vec3" => (12, 16),
        "vec4" | "mat2" => (16, 16),
        "mat3" => (48, 16),
        "mat4" => (64, 16),
        _ => (16, 16),
    }
}

/// Replace whole-word occurrences of `word`.
fn replace_word(src: &str, word: &str, with: &str) -> String {
    let is_id = |c: char| c.is_alphanumeric() || c == '_';
    let mut out = String::with_capacity(src.len());
    let mut rest = src;
    while let Some(i) = rest.find(word) {
        let before = rest[..i].chars().next_back();
        let after = rest[i + word.len()..].chars().next();
        out.push_str(&rest[..i]);
        if before.is_some_and(is_id) || after.is_some_and(is_id) {
            out.push_str(word);
        } else {
            out.push_str(with);
        }
        rest = &rest[i + word.len()..];
    }
    out.push_str(rest);
    out
}

/// Rewrite GLSL ES 3.00 into what naga's GLSL frontend takes, and describe its uniforms.
fn translate(src: &str, consts: &HashMap<String, String>) -> (String, Vec<Field>, usize, Vec<String>) {
    let mut fields: Vec<Field> = vec![];
    let mut textures: Vec<String> = vec![];
    let mut body = String::new();
    let mut offset = 0usize;
    for line in src.lines() {
        let t = line.trim();
        if t.starts_with("#version") || t.starts_with("precision ") {
            continue;
        }
        if let Some(rest) = t.strip_prefix("uniform ") {
            let decl = rest.split("//").next().unwrap_or("").trim().trim_end_matches(';').trim();
            let mut parts = decl.splitn(2, char::is_whitespace);
            let ty = parts.next().unwrap_or("").to_string();
            let mut name = parts.next().unwrap_or("").trim().to_string();
            if ty == "sampler2D" {
                textures.push(name);
                continue;
            }
            let mut count = 1;
            if let Some(i) = name.find('[') {
                let n = name[i + 1..].trim_end_matches(']').trim().to_string();
                count = consts.get(&n).cloned().unwrap_or(n).parse().unwrap_or(1);
                name = name[..i].to_string();
            }
            let (size, align) = layout_of(&ty);
            let (stride, align) = if count > 1 { (size.div_ceil(16) * 16, 16) } else { (size, align) };
            offset = offset.div_ceil(align) * align;
            fields.push(Field { name, ty, count, offset });
            offset += if count > 1 { stride * count } else { size };
            continue;
        }
        if t.starts_with("in ") && t.contains("v_uv") {
            body.push_str("layout(location = 0) in vec2 v_uv;\n");
            continue;
        }
        if t.starts_with("out vec4") {
            body.push_str(&format!("layout(location = 0) {t}\n"));
            continue;
        }
        body.push_str(line);
        body.push('\n');
    }
    // GLSL ES 1.00 shaders write gl_FragColor and sample with texture2D.
    if body.contains("gl_FragColor") {
        body = format!("layout(location = 0) out vec4 rayna_FragColor;\n{}", replace_word(&body, "gl_FragColor", "rayna_FragColor"));
        body = replace_word(&body, "texture2D", "texture");
    }
    // WebGL's gl_FragCoord counts rows from the bottom; wgpu's from the top.
    let flip = body.contains("gl_FragCoord");
    if flip {
        body = replace_word(&body, "gl_FragCoord", "rayna_FragCoord");
        offset = offset.div_ceil(4) * 4;
        fields.push(Field { name: "rayna_height".into(), ty: "float".into(), count: 1, offset });
        offset += 4;
    }
    // Helper functions that take a sampler2D take the texture instead (naga has no combined
    // samplers as values); calls then pass the texture.
    let mut params: Vec<String> = vec![];
    while let Some(i) = body.find("sampler2D ") {
        let rest = &body[i + 10..];
        let name: String = rest.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
        body.replace_range(i..i + 10, "texture2D ");
        params.push(name);
    }
    for name in &params {
        for f in ["texture", "textureLod", "texelFetch", "textureSize", "textureGrad"] {
            body = body.replace(&format!("{f}({name},"), &format!("{f}(sampler2D({name}, s_linear),"));
        }
    }
    // Sampling a combined sampler becomes sampling a texture with the shared sampler.
    for name in &textures {
        for f in ["texture", "textureLod", "texelFetch", "textureSize", "textureGrad"] {
            body = body.replace(&format!("{f}({name},"), &format!("{f}(sampler2D({name}_t, s_linear),"));
            body = body.replace(&format!("{f}({name} ,"), &format!("{f}(sampler2D({name}_t, s_linear),"));
        }
    }
    // Any other mention of a texture uniform is it passed to one of those helpers.
    for name in &textures {
        body = replace_word(&body, name, &format!("{name}_t"));
    }
    let mut head = String::from("#version 450\n");
    if !fields.is_empty() {
        head.push_str("layout(set = 0, binding = 0, std140) uniform Params {\n");
        for f in &fields {
            if f.ty == "bool" {
                // bool isn't host-shareable: hold it as a uint and read it back as a bool.
                head.push_str(&format!("  uint {}_b;\n", f.name));
            } else if f.count > 1 {
                head.push_str(&format!("  {} {}[{}];\n", f.ty, f.name, f.count));
            } else {
                head.push_str(&format!("  {} {};\n", f.ty, f.name));
            }
        }
        head.push_str("};\n");
        if flip {
            head.push_str("#define rayna_FragCoord vec4(gl_FragCoord.x, rayna_height - gl_FragCoord.y, gl_FragCoord.z, gl_FragCoord.w)\n");
        }
        for f in fields.iter().filter(|f| f.ty == "bool") {
            head.push_str(&format!("#define {0} ({0}_b != 0u)\n", f.name));
        }
    }
    head.push_str("layout(set = 0, binding = 1) uniform sampler s_linear;\n");
    for (i, name) in textures.iter().enumerate() {
        head.push_str(&format!("layout(set = 0, binding = {}) uniform texture2D {name}_t;\n", 2 + i));
    }
    let size = offset.div_ceil(16).max(1) * 16;
    (head + &body, fields, size, textures)
}

impl Gpu {
    fn program(&self, key: &str, src: &str, consts: &HashMap<String, String>) -> Result<std::sync::Arc<Program>, String> {
        if let Some(p) = self.programs.lock().get(key) {
            return p.clone();
        }
        let (glsl, fields, block_size, textures) = translate(src, consts);
        let built = (|| {
            let scope = self.device.push_error_scope(wgpu::ErrorFilter::Validation);
            let module = self.device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(key),
                source: wgpu::ShaderSource::Glsl { shader: Cow::Owned(glsl.clone()), stage: wgpu::naga::ShaderStage::Fragment, defines: &[] },
            });
            let mut entries = vec![
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
            ];
            for i in 0..textures.len() {
                entries.push(wgpu::BindGroupLayoutEntry {
                    binding: 2 + i as u32,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                    count: None,
                });
            }
            let layout = self.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: None, entries: &entries });
            let pl = self.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[Some(&layout)], immediate_size: 0 });
            let pipeline = self.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(key),
                layout: Some(&pl),
                vertex: wgpu::VertexState { module: &self.vertex, entry_point: Some("main"), buffers: &[], compilation_options: Default::default() },
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some("main"),
                    targets: &[Some(wgpu::ColorTargetState { format: wgpu::TextureFormat::Rgba8Unorm, blend: None, write_mask: wgpu::ColorWrites::ALL })],
                    compilation_options: Default::default(),
                }),
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            });
            if let Some(e) = pollster::block_on(scope.pop()) {
                return Err(format!("{key}: {e}"));
            }
            Ok(std::sync::Arc::new(Program { pipeline, layout, fields, block_size, textures }))
        })();
        if let Err(e) = &built {
            eprintln!("Liemo: shader {e}");
        }
        self.programs.lock().insert(key.to_string(), built.clone());
        built
    }

    /// Draw `src` at `w`×`h`. `inputs` are the shader's sampler2D uniforms by name; uniforms
    /// not given are zero. Returns None when the shader can't be built.
    pub fn run(&self, key: &str, src: &str, consts: &HashMap<String, String>, w: u32, h: u32, inputs: &[(&str, &Pixmap)], uniforms: &[(&str, U)]) -> Option<Pixmap> {
        self.run_with(key, src, consts, w, h, inputs, uniforms, false)
    }
    /// Like `run`, for shaders whose output is premultiplied (WebGL's default canvas).
    pub fn run_premul(&self, key: &str, src: &str, consts: &HashMap<String, String>, w: u32, h: u32, inputs: &[(&str, &Pixmap)], uniforms: &[(&str, U)]) -> Option<Pixmap> {
        self.run_with(key, src, consts, w, h, inputs, uniforms, true)
    }
    #[allow(clippy::too_many_arguments)]
    fn run_with(&self, key: &str, src: &str, consts: &HashMap<String, String>, w: u32, h: u32, inputs: &[(&str, &Pixmap)], uniforms: &[(&str, U)], premul: bool) -> Option<Pixmap> {
        let prog = self.program(key, src, consts).ok()?;
        let (w, h) = (w.max(1), h.max(1));
        // Uniform block.
        let mut block = vec![0u8; prog.block_size];
        let height = [("rayna_height", U::F(h as f32))];
        for f in &prog.fields {
            let Some((_, v)) = uniforms.iter().chain(height.iter()).find(|(n, _)| *n == f.name) else { continue };
            let mut put = |off: usize, vals: &[f32]| {
                for (i, x) in vals.iter().enumerate() {
                    let o = off + i * 4;
                    if o + 4 <= block.len() {
                        block[o..o + 4].copy_from_slice(&x.to_le_bytes());
                    }
                }
            };
            match v {
                U::F(x) if f.ty == "int" || f.ty == "bool" => put(f.offset, &[f32::from_bits(*x as i32 as u32)]),
                U::F(x) => put(f.offset, &[*x]),
                U::I(x) => put(f.offset, &[f32::from_bits(*x as u32)]),
                U::V2(x) => put(f.offset, x),
                U::V3(x) => put(f.offset, x),
                U::V4(x) => put(f.offset, x),
                U::V4s(xs) => {
                    for (i, x) in xs.iter().take(f.count).enumerate() {
                        put(f.offset + i * 16, x);
                    }
                }
            }
        }
        let ubuf = self.device.create_buffer(&wgpu::BufferDescriptor { label: None, size: block.len() as u64, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        self.queue.write_buffer(&ubuf, 0, &block);
        // Textures (straight RGBA, as WebGL uploads canvases).
        let blank = crate::render::canvas::pixmap(1, 1);
        let views: Vec<wgpu::TextureView> = prog
            .textures
            .iter()
            .map(|name| {
                let pm = inputs.iter().find(|(n, _)| *n == name).map(|(_, p)| *p).unwrap_or(&blank);
                let size = wgpu::Extent3d { width: pm.width(), height: pm.height(), depth_or_array_layers: 1 };
                let tex = self.device.create_texture(&wgpu::TextureDescriptor {
                    label: None,
                    size,
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[],
                });
                let data = crate::render::canvas::to_straight(pm);
                self.queue.write_texture(
                    wgpu::TexelCopyTextureInfo { texture: &tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
                    &data,
                    wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(4 * pm.width()), rows_per_image: Some(pm.height()) },
                    size,
                );
                tex.create_view(&Default::default())
            })
            .collect();
        let mut entries = vec![
            wgpu::BindGroupEntry { binding: 0, resource: ubuf.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) },
        ];
        for (i, v) in views.iter().enumerate() {
            entries.push(wgpu::BindGroupEntry { binding: 2 + i as u32, resource: wgpu::BindingResource::TextureView(v) });
        }
        let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &prog.layout, entries: &entries });
        // Target and readback.
        let target = self.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = target.create_view(&Default::default());
        let row = (4 * w).div_ceil(256) * 256;
        let read = self.device.create_buffer(&wgpu::BufferDescriptor { label: None, size: (row * h) as u64, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
        let mut enc = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&prog.pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.draw(0..3, 0..1);
        }
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo { texture: &target, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            wgpu::TexelCopyBufferInfo { buffer: &read, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(h) } },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        self.queue.submit([enc.finish()]);
        let slice = read.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
        rx.recv().ok()?.ok()?;
        let data = slice.get_mapped_range().ok()?;
        let mut out = vec![0u8; (w * h * 4) as usize];
        for y in 0..h as usize {
            out[y * w as usize * 4..(y + 1) * w as usize * 4].copy_from_slice(&data[y * row as usize..y * row as usize + w as usize * 4]);
        }
        drop(data);
        read.unmap();
        if premul {
            for px in out.chunks_exact_mut(4) {
                let a = px[3];
                px[0] = px[0].min(a);
                px[1] = px[1].min(a);
                px[2] = px[2].min(a);
            }
            return Pixmap::from_vec(out, tiny_skia::IntSize::from_wh(w, h)?);
        }
        Some(crate::render::canvas::from_straight(&out, w, h))
    }
}
