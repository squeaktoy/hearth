// Copyright (c) 2023 the Hearth contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later
//
// This file is part of Hearth.
//
// Hearth is free software: you can redistribute it and/or modify it under the
// terms of the GNU Affero General Public License as published by the Free
// Software Foundation, either version 3 of the License, or (at your option)
// any later version.
//
// Hearth is distributed in the hope that it will be useful, but WITHOUT ANY
// WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
// FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more
// details.
//
// You should have received a copy of the GNU Affero General Public License
// along with Hearth. If not, see <https://www.gnu.org/licenses/>.

struct VertexOut {
    [[builtin(position)]] clip_position: vec4<f32>;
    [[location(0)]] uv: vec2<f32>;
};

struct CanvasUniform {
    mvp: mat4x4<f32>;
};

[[group(0), binding(0)]] var<uniform> canvas: CanvasUniform;
[[group(0), binding(1)]] var canvas_t: texture_2d<f32>;
[[group(0), binding(2)]] var canvas_s: sampler;

[[stage(vertex)]]
fn vs_main([[builtin(vertex_index)]] in_vertex_index: u32) -> VertexOut {
    let x = f32(i32(in_vertex_index & 1u));
    let y = f32(i32(in_vertex_index & 2u) / 2);
    let xy = vec2<f32>(x, y);
    let pos = xy * 2.0 - 1.0;

    var out: VertexOut;
    out.clip_position = canvas.mvp * vec4<f32>(pos, 0.0, 1.0);
    out.uv = xy;

    return out;
}

[[stage(fragment)]]
fn fs_main(frag: VertexOut) -> [[location(0)]] vec4<f32> {
    return textureSample(canvas_t, canvas_s, frag.uv);
}
