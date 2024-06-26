use super::{glam::Mat4, *};

use hearth_guest::window::*;

lazy_static::lazy_static! {
    /// The main client window.
    pub static ref MAIN_WINDOW: Window = {
        Window {
            cap: registry::REGISTRY
                .get_service(SERVICE_NAME)
                .unwrap_or_else(|| panic!("requested service {SERVICE_NAME:?} is unavailable"))
        }
    };
}

/// Instance of a desktop window.
pub struct Window {
    cap: Capability,
}

impl Window {
    /// Subscribe to the window events published by this window.
    ///
    /// Returns a Mailbox that recieves all window events.
    pub fn subscribe(&self) -> Mailbox {
        let mailbox = Mailbox::new();
        let reply_cap = mailbox.make_capability(Permissions::SEND | Permissions::MONITOR);
        self.cap.send(&WindowCommand::Subscribe, &[&reply_cap]);
        mailbox
    }

    /// Sets the title of this window.
    pub fn set_title(&self, title: String) {
        self.cap.send(&WindowCommand::SetTitle(title), &[]);
    }

    /// Set the cursor's grab mode.
    pub fn cursor_grab_mode(&self, mode: CursorGrabMode) {
        self.cap.send(&WindowCommand::SetCursorGrab(mode), &[]);
    }

    /// Shows the window's cursor.
    pub fn show_cursor(&self) {
        self.cap.send(&WindowCommand::SetCursorVisible(true), &[]);
    }

    /// Hide the window's cursor.
    pub fn hide_cursor(&self) {
        self.cap.send(&WindowCommand::SetCursorVisible(false), &[]);
    }

    /// Update the window's rending camera
    ///
    /// `vfov` - The vertical field of view, in degrees.
    /// `near` - Near plane distance. All projection uses an infinite far plan.
    /// `view` - The camera's view matrix.
    pub fn set_camera(&self, vfov: f32, near: f32, view: Mat4) {
        self.cap
            .send(&WindowCommand::SetCamera { vfov, near, view }, &[]);
    }
}
