use glam::Vec3;
use serde::{Deserialize, Serialize};

use crate::Color;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DebugDrawVertex {
    /// The position of this vertex in world space.
    pub position: Vec3,

    /// The color of this vertex. Alpha is ignored and fixed to opaque.
    pub color: Color,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DebugDrawMesh {
    pub vertices: Vec<DebugDrawVertex>,
    pub indices: Vec<u32>,
}

/// An update to a debug draw mesh.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum DebugDrawUpdate {
    /// Updates the contents of this debug draw mesh.
    Contents(DebugDrawMesh),

    /// Sets whether to hide this mesh.
    Hide(bool),

    /// Destroys this debug draw mesh.
    Destroy,
}
