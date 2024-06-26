struct VertexIn {
    [[location(0)]] position: vec3<f32>;
    [[location(1)]] color: vec4<f32>;
};

struct VertexOut {
    [[builtin(position)]] clip_position: vec4<f32>;
    [[location(0)]] color: vec4<f32>;
};

struct CameraUniform {
    mvp: mat4x4<f32>;
};

[[group(0), binding(0)]] var<uniform> camera: CameraUniform;

fn srgb_to_linear(l: vec3<f32>) -> vec3<f32> {
    let cutoff = l > vec3<f32>(0.0405);
    let lower = l / vec3<f32>(12.92);
    let higher = pow((l + vec3<f32>(0.055)) / vec3<f32>(1.055), vec3<f32>(2.4));
    return select(lower, higher, cutoff);
}

[[stage(vertex)]]
fn vs_main(in: VertexIn) -> VertexOut {
    var out: VertexOut;
    out.clip_position = camera.mvp * vec4<f32>(in.position, 1.0);
    out.color = vec4<f32>(srgb_to_linear(in.color.bgr), 1.0);
    return out;
}

[[stage(fragment)]]
fn fs_main(frag: VertexOut) -> [[location(0)]] vec4<f32> {
    return frag.color;
}
