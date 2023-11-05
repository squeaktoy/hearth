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

use std::sync::Arc;

use hearth_core::{
    anyhow::{self, bail},
    asset::{AssetStore, JsonAssetLoader},
    async_trait, cargo_process_metadata,
    flue::Permissions,
    hearth_types::renderer::*,
    process::ProcessMetadata,
    runtime::{Plugin, RuntimeBuilder},
    tokio::{self, sync::mpsc::UnboundedSender},
    tracing::{error, warn},
    utils::{
        MessageInfo, ProcessRunner, RequestInfo, RequestResponseProcess, ResponseInfo,
        ServiceRunner, SinkProcess,
    },
};
use hearth_rend3::{
    rend3::{types::*, *},
    rend3_routine::pbr::{AlbedoComponent, PbrMaterial},
    Rend3Command, Rend3Plugin,
};

pub struct MeshLoader(Arc<Renderer>);

#[async_trait]
impl JsonAssetLoader for MeshLoader {
    type Asset = MeshHandle;
    type Data = MeshData;

    async fn load_asset(
        &self,
        _store: &AssetStore,
        data: Self::Data,
    ) -> anyhow::Result<Self::Asset> {
        let mesh = Mesh {
            vertex_positions: data.positions,
            vertex_normals: data.normals,
            vertex_tangents: data.tangents,
            vertex_uv0: data.uv0,
            vertex_uv1: data.uv1,
            vertex_colors: data.colors,
            vertex_joint_indices: data.joint_indices,
            vertex_joint_weights: data.joint_weights,
            indices: data.indices,
        };

        let handle = self.0.add_mesh(mesh);

        Ok(handle)
    }
}

pub struct MaterialLoader(Arc<Renderer>);

#[async_trait]
impl JsonAssetLoader for MaterialLoader {
    type Asset = MaterialHandle;
    type Data = MaterialData;

    async fn load_asset(
        &self,
        store: &AssetStore,
        data: Self::Data,
    ) -> anyhow::Result<Self::Asset> {
        let albedo = store.load_asset::<TextureLoader>(&data.albedo).await?;

        let material = PbrMaterial {
            albedo: AlbedoComponent::Texture(albedo.as_ref().to_owned()),
            ..Default::default()
        };

        let handle = self.0.add_material(material);
        Ok(handle)
    }
}

pub struct TextureLoader(Arc<Renderer>);

#[async_trait]
impl JsonAssetLoader for TextureLoader {
    type Asset = TextureHandle;
    type Data = TextureData;

    async fn load_asset(
        &self,
        _store: &AssetStore,
        data: Self::Data,
    ) -> anyhow::Result<Self::Asset> {
        let expected_len = (data.size.x * data.size.y * 4) as usize;

        if data.data.len() != expected_len {
            bail!("invalid texture data length");
        }

        let texture = Texture {
            label: data.label,
            data: data.data,
            format: TextureFormat::Rgba8UnormSrgb,
            size: data.size,
            mip_count: MipmapCount::ONE,
            mip_source: MipmapSource::Uploaded,
        };

        let handle = self.0.add_texture_2d(texture);
        Ok(handle)
    }
}

pub struct CubeTextureLoader(Arc<Renderer>);

#[async_trait]
impl JsonAssetLoader for CubeTextureLoader {
    type Asset = TextureHandle;
    type Data = TextureData;

    async fn load_asset(
        &self,
        _store: &AssetStore,
        data: Self::Data,
    ) -> anyhow::Result<Self::Asset> {
        let expected_len = (data.size.x * data.size.y * 24) as usize;

        if data.data.len() != expected_len {
            bail!("invalid texture data length");
        }

        let texture = Texture {
            label: data.label,
            data: data.data,
            format: TextureFormat::Rgba8UnormSrgb,
            size: data.size,
            mip_count: MipmapCount::ONE,
            mip_source: MipmapSource::Generated,
        };

        let handle = self.0.add_texture_cube(texture);

        Ok(handle)
    }
}

pub struct DirectionalLightInstance {
    renderer: Arc<Renderer>,
    handle: ResourceHandle<DirectionalLight>,
}

#[async_trait]
impl SinkProcess for DirectionalLightInstance {
    type Message = DirectionalLightUpdate;

    async fn on_message<'a>(&'a mut self, message: MessageInfo<'a, Self::Message>) {
        let mut change = DirectionalLightChange::default();

        use DirectionalLightUpdate::*;
        match message.data {
            Color(color) => change.color = Some(color),
            Intensity(intensity) => change.intensity = Some(intensity),
            Direction(direction) => change.direction = Some(direction),
            Distance(distance) => change.distance = Some(distance),
        }

        self.renderer.update_directional_light(&self.handle, change);
    }
}

pub struct ObjectInstance {
    renderer: Arc<Renderer>,
    handle: ObjectHandle,
    skeleton: Option<SkeletonHandle>,
}

#[async_trait]
impl SinkProcess for ObjectInstance {
    type Message = ObjectUpdate;

    async fn on_message<'a>(&'a mut self, message: MessageInfo<'a, Self::Message>) {
        use ObjectUpdate::*;
        match &message.data {
            Transform(transform) => {
                self.renderer.set_object_transform(&self.handle, *transform);
            }
            JointMatrices(matrices) => {
                let Some(skeleton) = self.skeleton.as_ref() else {
                    warn!("tried to update joint matrices on static object");
                    return;
                };

                self.renderer
                    .set_skeleton_joint_matrices(skeleton, matrices.to_owned());
            }
            JointTransforms {
                joint_global,
                inverse_bind,
            } => {
                let Some(skeleton) = self.skeleton.as_ref() else {
                    warn!("tried to update joint transforms on static object");
                    return;
                };

                self.renderer
                    .set_skeleton_joint_transforms(skeleton, &joint_global, &inverse_bind);
            }
        }
    }
}

/// Implements the renderer message protocol.
pub struct RendererService {
    renderer: Arc<Renderer>,
    command_tx: UnboundedSender<Rend3Command>,
}

#[async_trait]
impl RequestResponseProcess for RendererService {
    type Request = RendererRequest;
    type Response = RendererResponse;

    async fn on_request<'a>(
        &'a mut self,
        request: &mut RequestInfo<'a, Self::Request>,
    ) -> ResponseInfo<'a, Self::Response> {
        use RendererRequest::*;
        match &request.data {
            AddDirectionalLight { initial_state } => {
                let light = DirectionalLight {
                    color: initial_state.color,
                    intensity: initial_state.intensity,
                    direction: initial_state.direction,
                    distance: initial_state.distance,
                };

                let handle = self.renderer.add_directional_light(light);

                let instance = DirectionalLightInstance {
                    renderer: self.renderer.clone(),
                    handle,
                };

                let mut meta = cargo_process_metadata!();
                meta.name = Some("DirectionalLight".to_string());
                meta.description = Some(
                    "An instance of a renderer directional light. Accepts DirectionalLightUpdate."
                        .to_string(),
                );

                let child = request.runtime.process_factory.spawn(meta);
                let perms = Permissions::all();
                // TODO make this cleaner with #195
                let child_cap = child.borrow_parent().export_owned(perms);
                let child_cap = request
                    .process
                    .borrow_table()
                    .import_owned(child_cap)
                    .unwrap();
                let child_cap = request
                    .process
                    .borrow_table()
                    .wrap_handle(child_cap)
                    .unwrap();

                let label = "DirectionalLightInstance".to_string();
                let runtime = request.runtime.clone();
                tokio::spawn(async move {
                    instance.run(label, runtime, &child).await;
                });

                return ResponseInfo {
                    data: Ok(RendererSuccess::Ok),
                    caps: vec![child_cap],
                };
            }
            AddObject {
                mesh,
                skeleton,
                material,
                transform,
            } => {
                let mesh = request
                    .runtime
                    .asset_store
                    .load_asset::<MeshLoader>(mesh)
                    .await;

                let mesh = match mesh {
                    Ok(mesh) => mesh,
                    Err(err) => {
                        error!("failed to load mesh: {err:?}");
                        return RendererError::LumpError.into();
                    }
                };

                let material = request
                    .runtime
                    .asset_store
                    .load_asset::<MaterialLoader>(material)
                    .await;

                let material = match material {
                    Ok(material) => material,
                    Err(err) => {
                        error!("failed to load material: {err:?}");
                        return RendererError::LumpError.into();
                    }
                };

                let (mesh_kind, skeleton) = if let Some(skeleton) = skeleton.as_ref() {
                    let skeleton = self.renderer.add_skeleton(Skeleton {
                        joint_matrices: skeleton.to_owned(),
                        mesh: mesh.as_ref().to_owned(),
                    });

                    (ObjectMeshKind::Animated(skeleton.clone()), Some(skeleton))
                } else {
                    (ObjectMeshKind::Static(mesh.as_ref().to_owned()), None)
                };

                let object = Object {
                    mesh_kind,
                    material: material.as_ref().to_owned(),
                    transform: *transform,
                };

                let handle = self.renderer.add_object(object);

                let instance = ObjectInstance {
                    renderer: self.renderer.clone(),
                    handle,
                    skeleton,
                };

                let mut meta = cargo_process_metadata!();
                meta.name = Some("ObjectInstance".to_string());
                meta.description =
                    Some("An instance of a renderer object. Accepts ObjectUpdate.".to_string());

                let child = request.runtime.process_factory.spawn(meta);
                let perms = Permissions::all();
                // TODO make this cleaner with #195
                let child_cap = child.borrow_parent().export_owned(perms);
                let child_cap = request
                    .process
                    .borrow_table()
                    .import_owned(child_cap)
                    .unwrap();
                let child_cap = request
                    .process
                    .borrow_table()
                    .wrap_handle(child_cap)
                    .unwrap();

                let label = "ObjectInstance".to_string();
                let runtime = request.runtime.clone();
                tokio::spawn(async move {
                    instance.run(label, runtime, &child).await;
                });

                return ResponseInfo {
                    data: Ok(RendererSuccess::Ok),
                    caps: vec![child_cap],
                };
            }
            SetSkybox { texture } => {
                let handle = request
                    .runtime
                    .asset_store
                    .load_asset::<CubeTextureLoader>(texture)
                    .await;

                let handle = match handle {
                    Ok(handle) => handle,
                    Err(err) => {
                        error!("failed to load skybox texture: {err:?}");
                        return RendererError::LumpError.into();
                    }
                };

                let _ = self
                    .command_tx
                    .send(Rend3Command::SetSkybox(handle.as_ref().clone()));
            }
            SetAmbientLighting { ambient } => {
                let _ = self.command_tx.send(Rend3Command::SetAmbient(*ambient));
            }
        }

        ResponseInfo {
            data: Ok(RendererSuccess::Ok),
            caps: vec![],
        }
    }
}

impl ServiceRunner for RendererService {
    const NAME: &'static str = "hearth.Renderer";

    fn get_process_metadata() -> ProcessMetadata {
        let mut meta = cargo_process_metadata!();
        meta.description =
            Some("The native interface to the renderer. Accepts RendererRequest.".to_string());

        meta
    }
}

impl RendererService {
    pub fn new(renderer: Arc<Renderer>, command_tx: UnboundedSender<Rend3Command>) -> Self {
        Self {
            renderer,
            command_tx,
        }
    }
}

/// Initializes guest-available rendering code.
#[derive(Default)]
pub struct RendererPlugin {}

impl Plugin for RendererPlugin {
    fn build(&mut self, builder: &mut RuntimeBuilder) {
        let rend3 = builder
            .get_plugin::<Rend3Plugin>()
            .expect("rend3 plugin was not found");

        let renderer = rend3.renderer.clone();
        let command_tx = rend3.command_tx.clone();

        builder
            .add_asset_loader(MeshLoader(renderer.clone()))
            .add_asset_loader(MaterialLoader(renderer.clone()))
            .add_asset_loader(TextureLoader(renderer.clone()))
            .add_asset_loader(CubeTextureLoader(renderer.clone()))
            .add_plugin(RendererService::new(renderer, command_tx));
    }
}
