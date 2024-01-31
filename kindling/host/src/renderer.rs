// Copyright (c) 2024 the Hearth contributors.
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

use super::*;

use glam::Vec3;
use hearth_guest::renderer::*;

lazy_static::lazy_static! {
    static ref RENDERER: RequestResponse<RendererRequest, RendererResponse> =
        RequestResponse::expect_service("hearth.Renderer");
}

/// Set the global ambient lighting levels.
pub fn set_ambient_lighting(color: Vec3) {
    let (result, _) = RENDERER.request(
        RendererRequest::SetAmbientLighting {
            ambient: color.extend(1.0),
        },
        &[],
    );

    let _ = result.unwrap();
}

/// A directional light.
pub struct DirectionalLight(Capability);

impl Drop for DirectionalLight {
    fn drop(&mut self) {
        self.0.kill();
    }
}

impl DirectionalLight {
    /// Create a new directional light.
    pub fn new(state: DirectionalLightState) -> Self {
        let (result, caps) = RENDERER.request(
            RendererRequest::AddDirectionalLight {
                initial_state: state,
            },
            &[],
        );

        let _ = result.expect("failed to create directional light");

        Self(caps.first().unwrap().clone())
    }

    /// Internal helper function to update this light.
    fn update(&self, update: DirectionalLightUpdate) {
        self.0.send(&update, &[]);
    }

    /// Set this directional light's color.
    pub fn set_color(&self, color: Vec3) {
        self.update(DirectionalLightUpdate::Color(color));
    }

    /// Set this directional light's intensity.
    pub fn set_intensity(&self, intensity: f32) {
        self.update(DirectionalLightUpdate::Intensity(intensity));
    }

    /// Set this directional light's direction.
    pub fn set_direction(&self, direction: Vec3) {
        self.update(DirectionalLightUpdate::Direction(direction));
    }

    /// Set this distanceal light's distance.
    pub fn set_distance(&self, distance: f32) {
        self.update(DirectionalLightUpdate::Distance(distance));
    }
}
