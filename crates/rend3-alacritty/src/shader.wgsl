struct VertexOutput {
    [[builtin(position)]] clip_position: vec4<f32>;
    [[location(0)]] tex_coords: vec2<f32>;
};

[[stage(vertex)]]
fn vs_main([[builtin(vertex_index)]] in_vertex_index: u32) -> VertexOutput {
    var out: VertexOutput;
    let x = f32(in_vertex_index & 2u) * 0.5;
    let y = 1.0 - f32(in_vertex_index & 1u);
    out.tex_coords = vec2<f32>(x, y);
    out.clip_position = vec4<f32>(out.tex_coords * 1.8 - 0.9, 0.0, 1.0);
    return out;
}

[[group(0), binding(0)]] var t_msdf: texture_2d<f32>;
[[group(0), binding(1)]] var s_msdf: sampler;

fn median(r: f32, g: f32, b: f32) -> f32 {
    return max(min(r, g), min(max(r, g), b));
}

[[stage(fragment)]]
fn fs_main(frag: VertexOutput) -> [[location(0)]] vec4<f32> {
    let sdf = textureSample(t_msdf, s_msdf, frag.tex_coords);
    let dist = median(sdf.r, sdf.g, sdf.b) - 0.5;
    let duv = fwidth(dist);
    let pixel_dist = dist / max(duv, 0.001);
    let alpha = clamp(pixel_dist, 0.0, 1.0);
    return vec4<f32>(1.0, 1.0, 1.0, alpha);
}
