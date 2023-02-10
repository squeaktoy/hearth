struct VertexInput {
    [[location(0)]] position: vec2<f32>;
    [[location(1)]] tex_coords: vec2<f32>;
    [[location(2)]] color: vec4<f32>;
};

struct VertexOutput {
    [[builtin(position)]] clip_position: vec4<f32>;
    [[location(0)]] tex_coords: vec2<f32>;
    [[location(1)]] color: vec4<f32>;
};

struct CameraUniform {
    mvp: mat4x4<f32>;
};

[[group(0), binding(0)]] var<uniform> camera: CameraUniform;

[[group(1), binding(0)]] var t_msdf: texture_2d<f32>;
[[group(1), binding(1)]] var s_msdf: sampler;

[[stage(vertex)]]
fn vs_main(in: VertexInput, [[builtin(vertex_index)]] in_vertex_index: u32) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = camera.mvp * vec4<f32>(in.position, 0.0, 1.0);
    out.tex_coords = in.tex_coords;
    out.color = in.color;
    return out;
}

fn screen_px_range(tex_coords: vec2<f32>) -> f32 {
    let msdf_range = 8.0;
    let unit_range = vec2<f32>(msdf_range) / vec2<f32>(textureDimensions(t_msdf, 0));
    let screen_tex_size = vec2<f32>(1.0) / fwidth(tex_coords);
    return max(0.5 * dot(unit_range, screen_tex_size), 1.0);
}

fn median(r: f32, g: f32, b: f32) -> f32 {
    return max(min(r, g), min(max(r, g), b));
}

[[stage(fragment)]]
fn fs_main(frag: VertexOutput) -> [[location(0)]] vec4<f32> {
    let msd = textureSample(t_msdf, s_msdf, frag.tex_coords);
    let sd = median(msd.r, msd.g, msd.b);
    let dist = screen_px_range(frag.tex_coords) * (sd - 0.5);
    let alpha = clamp(dist + 0.5, 0.0, 1.0);
    return vec4<f32>(frag.color.rgb, alpha);
}
