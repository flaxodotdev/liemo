// Animated shader backgrounds from remocn (MIT), built on Paper Shaders
// (https://shaders.paper.design, Apache-2.0; licence and notice in assets/paper).
//
// assets/paper/paper.json holds, per Paper shader, its fragment file, default props and how
// props become uniforms; then remocn's wrappers (their own defaults, always `fit: cover`) and
// remocn's presets on top of those. Paper computes its UVs in a vertex shader; here the same
// maths runs at the top of the fragment shader, so the shared full-frame pass draws them.

use super::gpu::{gpu, U};
use crate::render::canvas::hex_rgba;
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::sync::OnceLock;
use tiny_skia::Pixmap;

const DATA: &str = include_str!("../../assets/paper/paper.json");

macro_rules! frags {
    ($($n:literal),*) => { &[$(($n, include_str!(concat!("../../assets/paper/", $n)))),*] };
}
const FRAGS: &[(&str, &str)] = frags!(
    "color-panels.frag", "dithering.frag", "dot-orbit.frag", "gem-smoke.frag", "god-rays.frag", "grain-gradient.frag",
    "liquid-metal.frag", "mesh-gradient.frag", "metaballs.frag", "neuro-noise.frag", "perlin-noise.frag",
    "pulsing-border.frag", "simplex-noise.frag", "smoke-ring.frag", "spiral.frag", "swirl.frag", "voronoi.frag",
    "warp.frag", "water.frag"
);

/// One background in the library.
pub struct Background {
    /// remocn's component name, e.g. "shader-swirl-candy".
    pub id: String,
    pub name: String,
    pub sub: String,
    paper: String,
    props: Map<String, Value>,
    /// remocn's own shaders (not Paper): drawn by this instead.
    custom: Option<fn(f32, u32, u32) -> Option<Pixmap>>,
}

fn data() -> &'static Value {
    static D: OnceLock<Value> = OnceLock::new();
    D.get_or_init(|| serde_json::from_str(DATA).expect("paper.json"))
}

fn title(id: &str) -> String {
    id.trim_start_matches("shader-")
        .split('-')
        .map(|w| {
            let mut c = w.chars();
            c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn backgrounds() -> &'static Vec<Background> {
    static B: OnceLock<Vec<Background>> = OnceLock::new();
    B.get_or_init(|| {
        let d = data();
        let comps = d["components"].as_object().unwrap();
        let shaders = d["shaders"].as_object().unwrap();
        let resolve = |comp: &str| -> Option<(String, Map<String, Value>)> {
            let c = comps.get(comp)?;
            let paper = c["paper"].as_str()?.to_string();
            let mut props = shaders.get(&paper)?["defaults"].as_object()?.clone();
            for k in ["defaults", "fixed"] {
                for (k, v) in c[k].as_object()? {
                    props.insert(k.clone(), v.clone());
                }
            }
            Some((paper, props))
        };
        let mut out = vec![];
        let mut names: Vec<&String> = comps.keys().collect();
        names.sort();
        for id in names {
            if let Some((paper, props)) = resolve(id) {
                out.push(Background { id: id.clone(), name: title(id), sub: format!("{paper} shader"), paper, props, custom: None });
            }
        }
        let presets = d["presets"].as_object().unwrap();
        let mut ids: Vec<&String> = presets.keys().collect();
        ids.sort();
        for id in ids {
            let p = &presets[id];
            let Some((paper, mut props)) = resolve(p["base"].as_str().unwrap_or("")) else { continue };
            for (k, v) in p["props"].as_object().into_iter().flatten() {
                props.insert(k.clone(), v.clone());
            }
            let base = title(p["base"].as_str().unwrap_or(""));
            let look = title(id).trim_start_matches(&base).trim().to_string();
            out.push(Background { id: id.clone(), name: format!("{base} · {look}"), sub: format!("{paper} shader"), paper, props, custom: None });
        }
        let own = |id: &str, name: &str, f: fn(f32, u32, u32) -> Option<Pixmap>| Background {
            id: id.into(),
            name: name.into(),
            sub: "remocn shader".into(),
            paper: String::new(),
            props: Map::new(),
            custom: Some(f),
        };
        out.push(own("shader-caustics", "Caustics", caustics));
        out.push(own("shader-strata", "Strata", strata));
        out.push(own("shader-weave", "Weave", weave));
        out.push(own("shader-light-tunnel", "Light Tunnel", light_tunnel));
        // remocn animates the accent in from its timeline; these hold it fully in.
        out.push(own("shader-caustics-violet", "Caustics · Violet", caustics_lit));
        out.push(own("shader-strata-violet", "Strata · Violet", strata_lit));
        out.push(own("shader-weave-violet", "Weave · Violet", weave_lit));
        out.sort_by(|a, b| a.id.cmp(&b.id));
        out
    })
}

pub fn background(id: &str) -> Option<&'static Background> {
    backgrounds().iter().find(|b| b.id == id)
}

/// Paper's vertex stage, run per fragment. `uv` is the clip-space position / 2, y up.
const PRELUDE: &str = r#"
in vec2 v_uv;
vec2 v_objectUV; vec2 v_objectBoxSize; vec2 v_responsiveUV; vec2 v_responsiveBoxGivenSize;
vec2 v_patternUV; vec2 v_patternBoxSize; vec2 v_imageUV;
vec3 paperBox(float boxRatio, vec2 givenBoxSize) {
  vec2 box = vec2(0.);
  box.x = boxRatio * min(givenBoxSize.x / boxRatio, givenBoxSize.y);
  float noFitBoxWidth = box.x;
  if (u_fit == 1.) { box.x = boxRatio * min(u_resolution.x / boxRatio, u_resolution.y); }
  else if (u_fit == 2.) { box.x = boxRatio * max(u_resolution.x / boxRatio, u_resolution.y); }
  box.y = box.x / boxRatio;
  return vec3(box, noFitBoxWidth);
}
void paperVaryings() {
  vec2 uv = vec2(v_uv.x - .5, .5 - v_uv.y);
  vec2 boxOrigin = vec2(.5 - u_originX, u_originY - .5);
  vec2 givenBoxSize = vec2(u_worldWidth, u_worldHeight);
  givenBoxSize = max(givenBoxSize, vec2(1.)) * u_pixelRatio;
  float r = u_rotation * 3.14159265358979323846 / 180.;
  mat2 graphicRotation = mat2(cos(r), sin(r), -sin(r), cos(r));
  vec2 graphicOffset = vec2(-u_offsetX, u_offsetY);

  vec2 fixedRatioBoxGivenSize = vec2(
    (u_worldWidth == 0.) ? u_resolution.x : givenBoxSize.x,
    (u_worldHeight == 0.) ? u_resolution.y : givenBoxSize.y);
  v_objectBoxSize = paperBox(1., fixedRatioBoxGivenSize).xy;
  vec2 objectWorldScale = u_resolution.xy / v_objectBoxSize;
  v_objectUV = uv;
  v_objectUV *= objectWorldScale;
  v_objectUV += boxOrigin * (objectWorldScale - 1.);
  v_objectUV += graphicOffset;
  v_objectUV /= u_scale;
  v_objectUV = graphicRotation * v_objectUV;

  v_responsiveBoxGivenSize = vec2(
    (u_worldWidth == 0.) ? u_resolution.x : givenBoxSize.x,
    (u_worldHeight == 0.) ? u_resolution.y : givenBoxSize.y);
  float responsiveRatio = v_responsiveBoxGivenSize.x / v_responsiveBoxGivenSize.y;
  vec2 responsiveBoxSize = paperBox(responsiveRatio, v_responsiveBoxGivenSize).xy;
  vec2 responsiveBoxScale = u_resolution.xy / responsiveBoxSize;
  v_responsiveUV = uv;
  v_responsiveUV *= responsiveBoxScale;
  v_responsiveUV += boxOrigin * (responsiveBoxScale - 1.);
  v_responsiveUV += graphicOffset;
  v_responsiveUV /= u_scale;
  v_responsiveUV.x *= responsiveRatio;
  v_responsiveUV = graphicRotation * v_responsiveUV;
  v_responsiveUV.x /= responsiveRatio;

  vec2 patternBoxGivenSize = vec2(
    (u_worldWidth == 0.) ? u_resolution.x : givenBoxSize.x,
    (u_worldHeight == 0.) ? u_resolution.y : givenBoxSize.y);
  float patternBoxRatio = patternBoxGivenSize.x / patternBoxGivenSize.y;
  vec3 boxSizeData = paperBox(patternBoxRatio, patternBoxGivenSize);
  v_patternBoxSize = boxSizeData.xy;
  float patternBoxNoFitBoxWidth = boxSizeData.z;
  vec2 patternBoxScale = u_resolution.xy / v_patternBoxSize;
  v_patternUV = uv;
  v_patternUV += graphicOffset / patternBoxScale;
  v_patternUV += boxOrigin;
  v_patternUV -= boxOrigin / patternBoxScale;
  v_patternUV *= u_resolution.xy;
  v_patternUV /= u_pixelRatio;
  if (u_fit > 0.) { v_patternUV *= (patternBoxNoFitBoxWidth / v_patternBoxSize.x); }
  v_patternUV /= u_scale;
  v_patternUV = graphicRotation * v_patternUV;
  v_patternUV += boxOrigin / patternBoxScale;
  v_patternUV -= boxOrigin;
  v_patternUV *= .01;

  vec2 imageBoxSize;
  if (u_fit == 1.) { imageBoxSize.x = min(u_resolution.x / u_imageAspectRatio, u_resolution.y) * u_imageAspectRatio; }
  else if (u_fit == 2.) { imageBoxSize.x = max(u_resolution.x / u_imageAspectRatio, u_resolution.y) * u_imageAspectRatio; }
  else { imageBoxSize.x = min(10.0, 10.0 / u_imageAspectRatio * u_imageAspectRatio); }
  imageBoxSize.y = imageBoxSize.x / u_imageAspectRatio;
  vec2 imageBoxScale = u_resolution.xy / imageBoxSize;
  v_imageUV = uv;
  v_imageUV *= imageBoxScale;
  v_imageUV += boxOrigin * (imageBoxScale - 1.);
  v_imageUV += graphicOffset;
  v_imageUV /= u_scale;
  v_imageUV.x *= u_imageAspectRatio;
  v_imageUV = graphicRotation * v_imageUV;
  v_imageUV.x /= u_imageAspectRatio;
  v_imageUV += .5;
  v_imageUV.y = 1. - v_imageUV.y;
}
"#;
const SIZING: [(&str, &str); 12] = [
    ("u_resolution", "vec2"), ("u_pixelRatio", "float"), ("u_imageAspectRatio", "float"), ("u_originX", "float"),
    ("u_originY", "float"), ("u_worldWidth", "float"), ("u_worldHeight", "float"), ("u_fit", "float"),
    ("u_scale", "float"), ("u_rotation", "float"), ("u_offsetX", "float"), ("u_offsetY", "float"),
];

/// A Paper fragment shader with its vertex maths folded in.
fn source(frag: &str) -> String {
    let mut head = String::new();
    let mut body = String::new();
    let mut declared = vec![];
    for line in frag.lines() {
        let t = line.trim().replace("lowp ", "").replace("mediump ", "").replace("highp ", "");
        if t.starts_with("#version") || t.starts_with("precision ") {
            continue;
        }
        if t.starts_with("in vec2 v_") {
            continue;
        }
        if let Some(rest) = t.strip_prefix("uniform ") {
            if let Some(name) = rest.split_whitespace().nth(1) {
                declared.push(name.trim_end_matches(';').split('[').next().unwrap_or("").to_string());
            }
            head.push_str(&t);
            head.push('\n');
            continue;
        }
        body.push_str(&line.replace("lowp ", "").replace("mediump ", "").replace("highp ", ""));
        body.push('\n');
    }
    for (n, ty) in SIZING {
        if !declared.iter().any(|d| d == n) {
            head.push_str(&format!("uniform {ty} {n};\n"));
        }
    }
    let body = body.replacen("void main() {", "void main() {\n  paperVaryings();", 1);
    format!("#version 300 es\nprecision highp float;\n{head}{PRELUDE}{body}")
}

fn sources() -> &'static HashMap<String, String> {
    static S: OnceLock<HashMap<String, String>> = OnceLock::new();
    S.get_or_init(|| FRAGS.iter().map(|(n, f)| (n.to_string(), source(f))).collect())
}

/// Paper's noise texture.
fn noise() -> &'static Pixmap {
    static N: OnceLock<Pixmap> = OnceLock::new();
    N.get_or_init(|| {
        let img = image::load_from_memory(include_bytes!("../../assets/paper/noise.png")).expect("noise.png").to_rgba8();
        let (w, h) = img.dimensions();
        crate::render::canvas::from_straight(img.as_raw(), w, h)
    })
}

fn color(v: &Value) -> [f32; 4] {
    let [r, g, b, a] = hex_rgba(v.as_str().unwrap_or("#000000"));
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, a as f32 / 255.0]
}

/// Draw background `id` at `t` seconds into a `w`×`h` frame, sized as remocn sizes it on a
/// 1280-wide composition so it looks the same at any resolution.
pub fn render(id: &str, t: f32, w: u32, h: u32) -> Option<Pixmap> {
    let bg = background(id)?;
    if let Some(f) = bg.custom {
        return f(t, w, h);
    }
    draw_paper(&bg.paper, &bg.props, t, w, h)
}

/// A remocn shader component (e.g. "shader-grain-gradient") with some props changed, as the
/// remocn transitions drive them.
pub fn render_with(component: &str, overrides: &[(&str, Value)], t: f32, w: u32, h: u32) -> Option<Pixmap> {
    let bg = background(component)?;
    let mut props = bg.props.clone();
    for (k, v) in overrides {
        props.insert(k.to_string(), v.clone());
    }
    draw_paper(&bg.paper, &props, t, w, h)
}

fn draw_paper(paper: &str, p: &Map<String, Value>, t: f32, w: u32, h: u32) -> Option<Pixmap> {
    let g = gpu()?;
    let shader = &data()["shaders"][paper];
    let frag = shader["frag"].as_str()?;
    let src = sources().get(frag)?;
    let enums = &shader["enums"];
    let speed = p.get("speed").and_then(Value::as_f64).unwrap_or(1.0) as f32;
    // remocn renders a 1280-wide composition at pixel ratio 2, and a few shaders have
    // device-pixel constants (border widths, small-canvas fallbacks). Every other use of the
    // resolution is a ratio, so the shader is told it is drawing at that size whatever the
    // real one. Dithering works on real pixels (gl_FragCoord), so it gets the real size.
    let k = w.max(h) as f32 / 1280.0;
    let (res, ratio) = if frag == "dithering.frag" { ([w as f32, h as f32], k) } else { ([w as f32 * 2.0 / k, h as f32 * 2.0 / k], 2.0) };
    let mut uniforms: Vec<(&str, U)> = vec![
        ("u_time", U::F(t * speed)),
        ("u_resolution", U::V2(res)),
        ("u_pixelRatio", U::F(ratio)),
        ("u_imageAspectRatio", U::F(1.0)),
    ];
    let mut names: Vec<String> = vec![];
    let mut values: Vec<U> = vec![];
    let mut noise_in = false;
    for u in shader["uniforms"].as_array()? {
        let (name, kind, prop) = (u[0].as_str()?, u[1].as_str()?, u[2].as_str()?);
        let v = p.get(prop).unwrap_or(&Value::Null);
        let val = match kind {
            "color" => U::V4(color(v)),
            "colors" => U::V4s(v.as_array().map(|a| a.iter().map(color).collect()).unwrap_or_default()),
            "count" => U::F(v.as_array().map(|a| a.len()).unwrap_or(0) as f32),
            "noise" => {
                noise_in = true;
                continue;
            }
            "bool" => U::F(if v.as_bool().unwrap_or(false) { 1.0 } else { 0.0 }),
            "image" => continue,
            "num" => U::F(v.as_f64().unwrap_or(0.0) as f32),
            e if e.starts_with("enum:") => {
                let table = &enums[&e[5..]];
                U::F(v.as_str().and_then(|s| table[s].as_f64()).unwrap_or(0.0) as f32)
            }
            _ => continue,
        };
        names.push(name.to_string());
        values.push(val);
    }
    for (n, v) in names.iter().zip(values) {
        uniforms.push((n.as_str(), v));
    }
    let blank = crate::render::canvas::pixmap(1, 1);
    let inputs: Vec<(&str, &Pixmap)> = if noise_in { vec![("u_noiseTexture", noise()), ("u_image", &blank)] } else { vec![("u_image", &blank)] };
    g.run_premul(&format!("paper:{frag}"), src, &HashMap::new(), w, h, &inputs, &uniforms)
}

// ---------- remocn's own background shaders ----------

fn rgb3(h: &str) -> U {
    let [r, g, b, _] = hex_rgba(h);
    U::V3([r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0])
}
fn own(key: &str, src: &str, w: u32, h: u32, uniforms: &[(&str, U)]) -> Option<Pixmap> {
    gpu()?.run(key, src, &HashMap::new(), w, h, &[], uniforms)
}
fn caustics(t: f32, w: u32, h: u32) -> Option<Pixmap> {
    caustics_with(t, w, h, 0.0)
}
fn caustics_lit(t: f32, w: u32, h: u32) -> Option<Pixmap> {
    caustics_with(t, w, h, 1.0)
}
fn caustics_with(t: f32, w: u32, h: u32, accent: f32) -> Option<Pixmap> {
    let u = [
        ("u_res", U::V2([w as f32, h as f32])),
        ("u_time", U::F(t)),
        ("u_scale", U::F(5.2)),
        ("u_intensity", U::F(1.0)),
        ("u_floor", rgb3("#0a0a0a")),
        ("u_light", rgb3("#2e2e33")),
        ("u_accent", rgb3("#7F57FF")),
        ("u_accentAmt", U::F(accent)),
    ];
    own("remocn:caustics", include_str!("../../assets/remocn/shader-caustics.frag"), w, h, &u)
}
fn strata(t: f32, w: u32, h: u32) -> Option<Pixmap> {
    strata_with(t, w, h, 0.0)
}
fn strata_lit(t: f32, w: u32, h: u32) -> Option<Pixmap> {
    strata_with(t, w, h, 1.0)
}
fn strata_with(t: f32, w: u32, h: u32, accent: f32) -> Option<Pixmap> {
    let u = [
        ("u_res", U::V2([w as f32, h as f32])),
        ("u_time", U::F(t)),
        ("u_layers", U::F(14.0)),
        ("u_amp", U::F(0.16)),
        ("u_c0", rgb3("#050507")),
        ("u_c1", rgb3("#0a0a0d")),
        ("u_c2", rgb3("#111014")),
        ("u_c3", rgb3("#17161b")),
        ("u_accent", rgb3("#6733FF")),
        ("u_accentAmt", U::F(accent)),
    ];
    own("remocn:strata", include_str!("../../assets/remocn/shader-strata.frag"), w, h, &u)
}
fn weave(t: f32, w: u32, h: u32) -> Option<Pixmap> {
    weave_with(t, w, h, 0.0)
}
fn weave_lit(t: f32, w: u32, h: u32) -> Option<Pixmap> {
    weave_with(t, w, h, 1.0)
}
fn weave_with(t: f32, w: u32, h: u32, accent: f32) -> Option<Pixmap> {
    let u = [
        ("u_res", U::V2([w as f32, h as f32])),
        ("u_time", U::F(t)),
        ("u_scale", U::F(22.0)),
        ("u_warp", U::F(0.03)),
        ("u_c0", rgb3("#050506")),
        ("u_c1", rgb3("#0a0a0c")),
        ("u_c2", rgb3("#111015")),
        ("u_c3", rgb3("#1b1a22")),
        ("u_accent", rgb3("#6733FF")),
        ("u_accentAmt", U::F(accent)),
    ];
    own("remocn:weave", include_str!("../../assets/remocn/shader-weave.frag"), w, h, &u)
}

/// remocn's tunnel: its hero field projected onto a cylinder, then a halftone pass over it.
/// The two sources are patched the way remocn patches them.
fn tunnel_sources() -> &'static (String, String) {
    static S: OnceLock<(String, String)> = OnceLock::new();
    S.get_or_init(|| {
        let field = include_str!("../../assets/remocn/shader-light-tunnel-1.frag").replace("iTime", "uMaterialTime").replacen(
            "uniform float uMaterialTime;",
            "uniform float iTime;
  const float uMaterialTime = 0.0;
  uniform float uDepth;
  uniform float uTwist;
  uniform float uSpirals;
  uniform float uCameraZoom;
  vec2 tunnelPosition(vec2 frag) {
    vec2 view = (frag - iResolution * 0.5) / iResolution.y / uCameraZoom;
    float radius = max(length(view), 0.005);
    float z = 0.55 * uDepth / radius + iTime * 1.2;
    float angle = (atan(view.y, view.x)
      + (z * 0.3 + iTime * 1.5) * uTwist) * uSpirals;
    float inward = log(radius / (0.55 * uDepth)) * 5.0 + iTime * 12.0;
    float ring = 0.68 + 0.1 * sin(inward);
    return vec2(cos(angle), sin(angle)) * ring
      + vec2(sin(inward) * 0.08, cos(inward) * 0.06 - 0.25);
  }",
            1,
        )
        .replacen("vec2 pos = (gl_FragCoord.xy - 0.5 * R) / R.y;", "vec2 pos = tunnelPosition(gl_FragCoord.xy);", 1);
        let halftone = include_str!("../../assets/remocn/shader-light-tunnel-2.frag")
            .replacen("uniform vec2 iResolution;", "uniform vec2 iResolution;\nuniform float uGlow;", 1)
            .replacen(
                "fragColor = vec4(clamp(color, 0.0, 1.0), 1.0);",
                "vec2 center = (gl_FragCoord.xy - iResolution * 0.5) / iResolution.y;
  float centerWeight = exp(-dot(center, center) / (2.0 * 0.065 * 0.065));
  if (centerWeight > 0.001) {
    vec3 blurred = vec3(0.0);
    float totalWeight = 0.0;
    vec2 spread = vec2(iResolution.y * 0.025) / iResolution;
    float blurLod = max(0.0, log2(iResolution.y * 0.025));
    vec2 uv = gl_FragCoord.xy / iResolution;
    for (int y = -2; y <= 2; y++) {
      for (int x = -2; x <= 2; x++) {
        vec2 offset = vec2(float(x), float(y));
        float weight = exp(-dot(offset, offset) * 0.5);
        blurred += textureLod(tScene, clamp(uv + offset * spread, 0.0, 1.0), blurLod).rgb * weight;
        totalWeight += weight;
      }
    }
    color = mix(color, blurred / totalWeight, centerWeight);
  }
  fragColor = vec4(clamp(color * uGlow, 0.0, 1.0), 1.0);",
                1,
            );
        (field, halftone)
    })
}

/// Rows reversed: what WebGL's bottom-up texture rows expect.
pub(crate) fn flip_rows(pm: &Pixmap) -> Pixmap {
    let mut out = pm.clone();
    let row = pm.width() as usize * 4;
    let src = pm.data();
    let n = pm.height() as usize;
    for (y, dst) in out.data_mut().chunks_exact_mut(row).enumerate() {
        dst.copy_from_slice(&src[(n - 1 - y) * row..(n - y) * row]);
    }
    out
}

pub fn light_tunnel(t: f32, w: u32, h: u32) -> Option<Pixmap> {
    light_tunnel_with(t, w, h, 0.7, 3.0, 1.0)
}
/// The tunnel with remocn's twist, spiral count and camera zoom.
pub fn light_tunnel_with(t: f32, w: u32, h: u32, twist: f32, spirals: f32, zoom: f32) -> Option<Pixmap> {
    let (field, halftone) = tunnel_sources();
    let k = w.max(h) as f32 / 1280.0;
    let u = [
        ("iResolution", U::V2([w as f32, h as f32])),
        ("iTime", U::F(t)),
        ("uLightMode", U::F(0.0)),
        ("uDarkBackground", U::V3([9.0 / 255.0; 3])),
        ("uLightBackground", U::V3([1.0; 3])),
        ("uPixelRatio", U::F(k)),
        ("uTwist", U::F(twist.clamp(0.0, 2.0))),
        ("uSpirals", U::F(spirals.clamp(1.0, 6.0).round())),
        ("uGlow", U::F(1.0)),
        ("uDepth", U::F(1.0)),
        ("uCameraZoom", U::F(zoom.clamp(1.0, 32.0))),
    ];
    let g = gpu()?;
    let scene = flip_rows(&g.run("remocn:tunnel-field", field, &HashMap::new(), w, h, &[], &u)?);
    g.run("remocn:tunnel-halftone", halftone, &HashMap::new(), w, h, &[("tScene", &scene)], &u)
}
