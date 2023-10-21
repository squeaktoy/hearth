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

use hearth_rend3::{rend3, rend3_routine, wgpu, FrameRequest, Rend3Plugin};
use rend3::InstanceAdapterDevice;
use tokio::sync::{mpsc, oneshot};
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop, EventLoopBuilder, EventLoopProxy};
use winit::window::{Window as WinitWindow, WindowBuilder};

fn vertex(pos: [f32; 3]) -> glam::Vec3 {
    glam::Vec3::from(pos)
}

fn create_mesh() -> rend3::types::Mesh {
    let vertex_positions = [
        // far side (0.0, 0.0, 1.0)
        vertex([-1.0, -1.0, 1.0]),
        vertex([1.0, -1.0, 1.0]),
        vertex([1.0, 1.0, 1.0]),
        vertex([-1.0, 1.0, 1.0]),
        // near side (0.0, 0.0, -1.0)
        vertex([-1.0, 1.0, -1.0]),
        vertex([1.0, 1.0, -1.0]),
        vertex([1.0, -1.0, -1.0]),
        vertex([-1.0, -1.0, -1.0]),
        // right side (1.0, 0.0, 0.0)
        vertex([1.0, -1.0, -1.0]),
        vertex([1.0, 1.0, -1.0]),
        vertex([1.0, 1.0, 1.0]),
        vertex([1.0, -1.0, 1.0]),
        // left side (-1.0, 0.0, 0.0)
        vertex([-1.0, -1.0, 1.0]),
        vertex([-1.0, 1.0, 1.0]),
        vertex([-1.0, 1.0, -1.0]),
        vertex([-1.0, -1.0, -1.0]),
        // top (0.0, 1.0, 0.0)
        vertex([1.0, 1.0, -1.0]),
        vertex([-1.0, 1.0, -1.0]),
        vertex([-1.0, 1.0, 1.0]),
        vertex([1.0, 1.0, 1.0]),
        // bottom (0.0, -1.0, 0.0)
        vertex([1.0, -1.0, 1.0]),
        vertex([-1.0, -1.0, 1.0]),
        vertex([-1.0, -1.0, -1.0]),
        vertex([1.0, -1.0, -1.0]),
    ];

    let index_data = &[
        0, 1, 2, 2, 3, 0, // far
        4, 5, 6, 6, 7, 4, // near
        8, 9, 10, 10, 11, 8, // right
        12, 13, 14, 14, 15, 12, // left
        16, 17, 18, 18, 19, 16, // top
        20, 21, 22, 22, 23, 20, // bottom
    ];

    rend3::types::MeshBuilder::new(vertex_positions.to_vec(), rend3::types::Handedness::Left)
        .with_indices(index_data.to_vec())
        .build()
        .unwrap()
}

/// A message sent from the rest of the program to a window.
#[derive(Clone, Debug)]
pub enum WindowRxMessage {
    /// The window is requested to quit.
    Quit,
}

/// A message sent from a window to the rest of the program.
#[derive(Clone, Debug)]
pub enum WindowTxMessage {
    /// The window has been requested to quit.
    Quit,
}

/// Message sent from the window on initialization.
pub struct WindowOffer {
    /// A sender of [WindowRxMessage] to this window.
    pub incoming: EventLoopProxy<WindowRxMessage>,

    /// A receiver for [WindowTxMessage] from the window.
    pub outgoing: mpsc::UnboundedReceiver<WindowTxMessage>,

    /// A [Rend3Plugin] compatible with this window.
    pub rend3_plugin: Rend3Plugin,
}

struct Window {
    outgoing_tx: mpsc::UnboundedSender<WindowTxMessage>,
    window: WinitWindow,
    iad: InstanceAdapterDevice,
    surface: Arc<wgpu::Surface>,
    config: wgpu::SurfaceConfiguration,
    frame_request_tx: mpsc::UnboundedSender<FrameRequest>,
    _object_handle: rend3::types::ResourceHandle<rend3::types::Object>,
    _directional_handle: rend3::types::ResourceHandle<rend3::types::DirectionalLight>,
}

impl Window {
    async fn new(event_loop: &EventLoop<WindowRxMessage>) -> (Self, WindowOffer) {
        let window = WindowBuilder::new()
            .with_title("Hearth Client")
            .with_inner_size(winit::dpi::LogicalSize::new(128.0, 128.0))
            .build(event_loop)
            .unwrap();

        let size = window.inner_size();
        let swapchain_format = wgpu::TextureFormat::Bgra8UnormSrgb;
        let iad = rend3::create_iad(None, None, None, None).await.unwrap();
        let surface = unsafe { iad.instance.create_surface(&window) };
        let surface = Arc::new(surface);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: swapchain_format,
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::Immediate,
        };

        surface.configure(&iad.device, &config);
        let (outgoing_tx, outgoing_rx) = mpsc::unbounded_channel();
        let rend3_plugin = Rend3Plugin::new(iad.to_owned(), swapchain_format);
        let renderer = rend3_plugin.renderer.to_owned();
        let frame_request_tx = rend3_plugin.frame_request_tx.clone();

        let mesh = create_mesh();
        let mesh_handle = renderer.add_mesh(mesh);

        let material = rend3_routine::pbr::PbrMaterial {
            albedo: rend3_routine::pbr::AlbedoComponent::Value(glam::Vec4::new(0.0, 0.5, 0.5, 1.0)),
            ..Default::default()
        };

        let material_handle = renderer.add_material(material);

        let object = rend3::types::Object {
            mesh_kind: rend3::types::ObjectMeshKind::Static(mesh_handle),
            material: material_handle,
            transform: glam::Mat4::IDENTITY,
        };

        let object_handle = renderer.add_object(object);

        let directional_handle = renderer.add_directional_light(rend3::types::DirectionalLight {
            color: glam::Vec3::ONE,
            intensity: 10.0,
            direction: glam::Vec3::new(-1.0, -4.0, 2.0),
            distance: 400.0,
        });

        let window = Self {
            outgoing_tx,
            window,
            iad,
            surface,
            config,
            frame_request_tx,
            _object_handle: object_handle,
            _directional_handle: directional_handle,
        };

        let offer = WindowOffer {
            incoming: event_loop.create_proxy(),
            outgoing: outgoing_rx,
            rend3_plugin,
        };

        (window, offer)
    }

    pub fn on_resize(&mut self, size: winit::dpi::PhysicalSize<u32>) {
        self.config.width = size.width;
        self.config.height = size.height;
        self.surface.configure(&self.iad.device, &self.config);
        self.window.request_redraw();
    }

    pub fn on_draw(&mut self) {
        let frame = match self.surface.get_current_texture() {
            Ok(frame) => frame,
            Err(wgpu::SurfaceError::Outdated) => {
                let size = self.window.inner_size();
                self.on_resize(size);
                return;
            }
            Err(err) => {
                tracing::error!("Surface error: {:?}", err);
                return;
            }
        };

        let output_frame = rend3::util::output::OutputFrame::SurfaceAcquired {
            view: frame.texture.create_view(&Default::default()),
            surface_tex: frame,
        };

        let size = self.window.inner_size();
        let resolution = glam::UVec2::new(size.width, size.height);

        let eye = glam::Vec3::new(3.0, 3.0, 5.0);
        let center = glam::Vec3::ZERO;
        let up = glam::Vec3::Y;
        let view = glam::Mat4::look_at_rh(eye, center, up);

        let (on_complete, on_complete_rx) = oneshot::channel();

        let request = FrameRequest {
            output_frame,
            camera: rend3::types::Camera {
                projection: rend3::types::CameraProjection::Perspective {
                    vfov: 60.0,
                    near: 0.1,
                },
                view,
            },
            resolution,
            on_complete,
        };

        if self.frame_request_tx.send(request).is_err() {
            tracing::warn!("failed to request frame");
        } else {
            let _ = on_complete_rx.blocking_recv();
        }

        self.window.request_redraw();
    }
}

pub struct WindowCtx {
    event_loop: EventLoop<WindowRxMessage>,
    window: Window,
}

impl WindowCtx {
    pub async fn new() -> (Self, WindowOffer) {
        let event_loop = EventLoopBuilder::with_user_event().build();
        let (window, offer) = Window::new(&event_loop).await;
        (Self { event_loop, window }, offer)
    }

    pub fn run(self) -> ! {
        let Self {
            event_loop,
            mut window,
        } = self;

        event_loop.run(move |event, _, control_flow| {
            *control_flow = ControlFlow::Wait;

            match &event {
                Event::WindowEvent { ref event, .. } => match event {
                    WindowEvent::Resized(size) => {
                        window.on_resize(*size);
                    }
                    WindowEvent::ScaleFactorChanged { new_inner_size, .. } => {
                        window.on_resize(**new_inner_size);
                    }
                    WindowEvent::CloseRequested => {
                        *control_flow = ControlFlow::Exit;
                        window.outgoing_tx.send(WindowTxMessage::Quit).unwrap();
                    }
                    _ => {}
                },
                Event::MainEventsCleared => {
                    window.window.request_redraw();
                }
                Event::RedrawRequested(_) => {
                    window.on_draw();
                }
                Event::UserEvent(WindowRxMessage::Quit) => {
                    *control_flow = ControlFlow::Exit;
                }
                _ => (),
            }
        });
    }
}
