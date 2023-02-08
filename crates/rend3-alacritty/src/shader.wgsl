struct VertexOutput {
    [[builtin(position)]] clip_position: vec4<f32>;
};

[[stage(vertex)]]
fn vs_main() -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = vec4<f32>(1.0);
    return out;
}

[[stage(fragment)]]
fn fs_main(frag: VertexOutput) -> [[location(0)]] vec4<f32> {
    return vec4<f32>(1.0);
}
