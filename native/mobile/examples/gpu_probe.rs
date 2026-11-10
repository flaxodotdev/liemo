// Checks the GPU path on iOS: `xcrun simctl spawn booted <binary>`.
fn main() {
    let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
    desc.backends = wgpu::Backends::METAL;
    let instance = wgpu::Instance::new(desc);
    let adapter = match pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())) {
        Ok(a) => a,
        Err(e) => return println!("adapter: {e}"),
    };
    println!("adapter: {:?}", adapter.get_info());
    let l = adapter.limits();
    println!("limits: tex2d {} binds {} ubo {} storage_bufs/stage {} color_attach {} inter-stage {}", l.max_texture_dimension_2d, l.max_bind_groups, l.max_uniform_buffer_binding_size, l.max_storage_buffers_per_shader_stage, l.max_color_attachments, l.max_inter_stage_shader_variables);
    match pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())) {
        Ok(_) => println!("device: ok (default limits)"),
        Err(e) => println!("device: {e}"),
    }
    let r = liemo_mobile::fx::paper::render("shader-caustics", 2.0, 320, 180);
    println!("paper render: {:?}", r.map(|p| (p.width(), p.height())));
}
