use super::*;

use glam::{Mat4, Vec3};
use hearth_guest::{renderer::*, Lump};

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

/// Update the skybox with the given lump containing [TextureData].
pub fn set_skybox(texture: &Lump) {
    let (result, _) = RENDERER.request(
        RendererRequest::SetSkybox {
            texture: texture.get_id(),
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

/// Configuration for the creation of an [Object].
#[derive(Clone, Debug)]
pub struct ObjectConfig<'a> {
    /// A reference to the lump containing this object's [MeshData].
    pub mesh: &'a Lump,

    /// An optional list of skeleton joint matrices for this object.
    pub skeleton: Option<Vec<Mat4>>,

    /// The lump containing this object's [MaterialData].
    pub material: &'a Lump,

    /// The initial transform of this object.
    pub transform: Mat4,
}

/// An object.
pub struct Object(Capability);

impl Drop for Object {
    fn drop(&mut self) {
        self.0.kill();
    }
}

impl Object {
    /// Create a new object in the scene with the given [ObjectConfig].
    pub fn new(config: ObjectConfig) -> Self {
        let (result, caps) = RENDERER.request(
            RendererRequest::AddObject {
                mesh: config.mesh.get_id(),
                skeleton: config.skeleton,
                material: config.material.get_id(),
                transform: config.transform,
            },
            &[],
        );

        let _ = result.expect("failed to create object");

        Self(caps.first().unwrap().clone())
    }

    /// Updates the transform of this object.
    pub fn set_transform(&self, transform: Mat4) {
        self.0.send(&ObjectUpdate::Transform(transform), &[]);
    }

    /// Update the joint matrices of this mesh.
    pub fn set_joint_matrices(&self, joints: Vec<Mat4>) {
        self.0.send(&ObjectUpdate::JointMatrices(joints), &[]);
    }

    /// Update the joint transforms of this mesh.
    pub fn set_joint_transforms(&self, joint_global: Vec<Mat4>, inverse_bind: Vec<Mat4>) {
        self.0.send(
            &ObjectUpdate::JointTransforms {
                joint_global,
                inverse_bind,
            },
            &[],
        );
    }
}
