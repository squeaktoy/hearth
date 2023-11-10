// Copyright (c) 2023 the Hearth contributors.
// SPDX-License-Identifier: Apache-2.0
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

struct SolidVertexIn {
    [[location(0)]] position: vec2<f32>;
    [[location(1)]] color: vec4<f32>;
};

struct SolidVertexOut {
    [[builtin(position)]] clip_position: vec4<f32>;
    [[location(0)]] color: vec4<f32>;
};

struct GlyphVertexIn {
    [[location(0)]] position: vec2<f32>;
    [[location(1)]] tex_coords: vec2<f32>;
    [[location(2)]] color: vec4<f32>;
};

struct GlyphVertexOut {
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

fn srgb_to_linear(l: vec3<f32>) -> vec3<f32> {
    let cutoff = l > vec3<f32>(0.0405);
    let lower = l / vec3<f32>(12.92);
    let higher = pow((l + vec3<f32>(0.055)) / vec3<f32>(1.055), vec3<f32>(2.4));
    return select(lower, higher, cutoff);
}

[[stage(vertex)]]
fn solid_vs(in: SolidVertexIn) -> SolidVertexOut {
    var out: SolidVertexOut;
    out.clip_position = camera.mvp * vec4<f32>(in.position, 0.0, 1.0);
    out.color = vec4<f32>(srgb_to_linear(in.color.rgb), in.color.a);
    return out;
}

[[stage(fragment)]]
fn solid_fs(frag: SolidVertexOut) -> [[location(0)]] vec4<f32> {
    return frag.color;
}

[[stage(vertex)]]
fn glyph_vs(in: GlyphVertexIn, [[builtin(vertex_index)]] in_vertex_index: u32) -> GlyphVertexOut {
    var out: GlyphVertexOut;
    out.clip_position = camera.mvp * vec4<f32>(in.position, 0.0, 1.0);
    out.tex_coords = in.tex_coords;
    out.color = vec4<f32>(srgb_to_linear(in.color.rgb), in.color.a);
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
fn glyph_fs(frag: GlyphVertexOut) -> [[location(0)]] vec4<f32> {
    let msd = textureSample(t_msdf, s_msdf, frag.tex_coords);
    let sd = median(msd.r, msd.g, msd.b);
    let dist = screen_px_range(frag.tex_coords) * (sd - 0.5);
    let alpha = clamp(dist + 0.5, 0.0, 1.0);
    return vec4<f32>(frag.color.rgb, alpha);
}
