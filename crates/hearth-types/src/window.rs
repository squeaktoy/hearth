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

use glam::Mat4;
use serde::{Deserialize, Serialize};
use winit::{
    dpi::{PhysicalPosition, PhysicalSize},
    event::*,
    window::CursorGrabMode,
};

pub use winit;

/// The name of the service that provides the main client window.
pub const SERVICE_NAME: &str = "hearth.Window";

/// An event on the sender's window.
///
/// Refer to https://docs.rs/winit/latest/winit/event/enum.WindowEvent.html for
/// more info. This enum reimplements it since the original type does not
/// implement De/Serialize.
///
/// If something is missing, wrong, or otherwise broken, please open an issue.
// TODO file dropping/hovering?
// TODO IME support?
// TODO touchpad support?
// TODO touch support?
// TODO port DeviceId?
#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum WindowEvent {
    /// The window has redrawn.
    Redraw {
        /// The time, in seconds, since the last redraw.
        dt: f32,
    },

    Resized(PhysicalSize<u32>),
    CloseRequested,
    ReceivedCharacter(char),
    Focused(bool),
    KeyboardInput {
        input: KeyboardInput,
        is_synthetic: bool,
    },
    ModifiersChanged(ModifiersState),
    CursorMoved {
        position: PhysicalPosition<f64>,
    },
    CursorEntered {},
    CursorLeft {},
    MouseWheel {
        delta: MouseScrollDelta,
        phase: TouchPhase,
    },
    MouseInput {
        state: ElementState,
        button: MouseButton,
        modifiers: ModifiersState,
    },
    ScaleFactorChanged {
        scale_factor: f64,
        new_inner_size: PhysicalSize<u32>,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum WindowCommand {
    /// Subscribes to all [WindowEvents][WindowEvent] on this window using the
    /// first attached capability.
    Subscribe, // and hit that bell

    /// Sets the title of the window.
    SetTitle(String),

    /// Sets the grabbing mode of the cursor.
    SetCursorGrab(CursorGrabMode),

    /// Sets the visibility of the cursor.
    SetCursorVisible(bool),

    /// Updates the window's rendering camera.
    SetCamera {
        /// Vertical field of view in degrees.
        vfov: f32,

        /// Near plane distance. All projection uses an infinite far plane.
        near: f32,

        /// The camera's view matrix.
        view: Mat4,
    },
}
