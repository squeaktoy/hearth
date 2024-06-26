use super::*;

use hearth_guest::debug_draw::*;

lazy_static::lazy_static! {
    static ref DEBUG_DRAW_FACTORY: RequestResponse<(), ()> =
        RequestResponse::expect_service("hearth.DebugDrawFactory");
}

/// An instance of debug draw.
pub struct DebugDraw {
    cap: Capability,
}

impl Drop for DebugDraw {
    fn drop(&mut self) {
        self.cap.send(&DebugDrawUpdate::Destroy, &[]);
    }
}

impl Default for DebugDraw {
    fn default() -> Self {
        Self::new()
    }
}

impl DebugDraw {
    /// Creates a new debug draw mesh
    ///
    /// The contents of this mesh must be initialized with the update method
    pub fn new() -> Self {
        DebugDraw {
            cap: DEBUG_DRAW_FACTORY
                .request((), &[])
                .1
                .get(0)
                .unwrap()
                .clone(),
        }
    }

    /// Hide this debug draw mesh.
    pub fn hide(&self) {
        self.cap.send(&DebugDrawUpdate::Hide(true), &[]);
    }

    /// Show this debug draw mesh.
    pub fn show(&self) {
        self.cap.send(&DebugDrawUpdate::Hide(false), &[]);
    }

    /// Update the contents of this debug draw mesh.
    pub fn update(&self, mesh: DebugDrawMesh) {
        self.cap.send(&DebugDrawUpdate::Contents(mesh), &[]);
    }
}
