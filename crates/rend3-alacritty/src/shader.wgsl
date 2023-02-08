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

[[stage(vertex)]]
fn vs_main(in: VertexInput, [[builtin(vertex_index)]] in_vertex_index: u32) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = vec4<f32>(in.position, 0.0, 1.0);
    out.tex_coords = in.tex_coords;
    out.color = in.color;
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
    return vec4<f32>(frag.color.rgb, alpha);
}
