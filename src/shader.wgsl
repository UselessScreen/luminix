// Vertex shader

struct Uniforms {
    image_aspect: f32,
    window_aspect: f32,
    zoom: f32,
    pan_x: f32,
    pan_y: f32,
}

@group(1) @binding(0)
var<uniform> uniforms: Uniforms;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) tex_coords: vec2<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
}

@vertex
fn vs_main(
    model: VertexInput,
) -> VertexOutput {
    var out: VertexOutput;
    
    // Apply aspect ratio correction
    var pos = model.position;
    
    // Calculate scale to fit image in window while maintaining aspect ratio
    var scale: vec2<f32>;
    if (uniforms.image_aspect > uniforms.window_aspect) {
        // Image is wider than window
        scale = vec2<f32>(1.0, uniforms.window_aspect / uniforms.image_aspect);
    } else {
        // Image is taller than window
        scale = vec2<f32>(uniforms.image_aspect / uniforms.window_aspect, 1.0);
    }
    
    // Apply zoom
    scale = scale / uniforms.zoom;
    
    // Apply pan (in normalized device coordinates)
    pos.x = pos.x * scale.x - uniforms.pan_x * 2.0;
    pos.y = pos.y * scale.y + uniforms.pan_y * 2.0;
    
    out.clip_position = vec4<f32>(pos, 1.0);
    out.tex_coords = model.tex_coords;
    return out;
}

// Fragment shader

@group(0) @binding(0)
var t_diffuse: texture_2d<f32>;
@group(0) @binding(1)
var s_diffuse: sampler;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(t_diffuse, s_diffuse, in.tex_coords);
}

