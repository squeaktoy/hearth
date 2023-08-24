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

use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{channel, Sender},
        Arc,
    },
    thread::JoinHandle,
};

use alacritty_terminal::{
    ansi::{Color, CursorShape, NamedColor},
    config::PtyConfig,
    event::{Event, EventListener},
    event_loop::{EventLoop, Msg, State},
    grid::Indexed,
    sync::FairMutex,
    term::{
        cell::{Cell, Flags},
        color::{Colors, Rgb, COUNT},
        RenderableContent, RenderableCursor,
    },
    tty::Pty,
    Term,
};
use glam::Vec2;
use mio_extras::channel::Sender as MioSender;
use owned_ttf_parser::AsFaceRef;
use wgpu::{
    self, BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, Buffer, BufferAddress,
    BufferDescriptor, BufferUsages, Device,
};

use crate::{
    gpu::{CameraUniform, DynamicMesh, GlyphVertex, SolidVertex},
    text::{FaceAtlas, FontSet, FontStyle},
};

pub struct Listener {
    sender: Sender<Event>,
}

impl Listener {
    pub fn new(sender: Sender<Event>) -> Self {
        Self { sender }
    }
}

impl EventListener for Listener {
    fn send_event(&self, event: Event) {
        self.sender.send(event).unwrap();
    }
}

/// Configuration for the initialization of a terminal.
#[derive(Clone)]
pub struct TerminalConfig {
    pub fonts: FontSet<Arc<FaceAtlas>>,
    pub colors: Colors,
}

/// A CPU-side wrapper around terminal functionality.
pub struct Terminal {
    config: TerminalConfig,
    term: Arc<FairMutex<Term<Listener>>>,
    _term_loop: JoinHandle<(EventLoop<Pty, Listener>, State)>,
    term_channel: Arc<FairMutex<MioSender<Msg>>>,
    should_quit: AtomicBool,
}

impl Terminal {
    pub fn new(config: TerminalConfig) -> Arc<Self> {
        let term_size =
            alacritty_terminal::term::SizeInfo::new(100.0, 75.0, 1.0, 1.0, 0.0, 0.0, false);

        let (sender, term_events) = channel();

        let shell = alacritty_terminal::config::Program::Just("/usr/bin/fish".into());

        let term_config = alacritty_terminal::config::Config {
            pty_config: PtyConfig {
                shell: Some(shell),
                working_directory: None,
                hold: false,
            },
            ..Default::default()
        };

        let term_listener = Listener::new(sender.clone());

        let term = Term::new(&term_config, term_size, term_listener);
        let term = FairMutex::new(term);
        let term = Arc::new(term);

        let pty = alacritty_terminal::tty::new(&term_config.pty_config, &term_size, None).unwrap();

        let term_listener = Listener::new(sender);
        let term_loop = EventLoop::new(term.clone(), term_listener, pty, false, false);
        let term_channel = term_loop.channel();

        let term = Self {
            config,
            term,
            _term_loop: term_loop.spawn(),
            term_channel: Arc::new(FairMutex::new(term_channel)),
            should_quit: AtomicBool::new(false),
        };

        let term = Arc::new(term);

        let event_term = term.to_owned();
        std::thread::spawn(move || {
            while let Ok(event) = term_events.recv() {
                event_term.on_event(event);
            }
        });

        term
    }

    pub fn update_draw_state(&self, draw: &mut TerminalDrawState) {
        let colors = self.config.colors.clone();
        let mut canvas = TerminalCanvas::new(colors, self.config.fonts.clone());
        let term = self.term.lock();
        let content = term.renderable_content();
        canvas.update_from_content(content);
        drop(term); // get off the mutex
        canvas.apply_to_state(draw);
    }

    pub fn should_quit(&self) -> bool {
        self.should_quit.load(Ordering::Relaxed)
    }

    pub fn send_input(&self, input: &str) {
        let bytes = input.as_bytes();
        let cow = std::borrow::Cow::Owned(bytes.to_owned());
        let sender = self.term_channel.lock();
        sender.send(Msg::Input(cow)).unwrap();
    }

    fn on_event(&self, event: Event) {
        match event {
            Event::ColorRequest(index, format) => {
                let color = self.config.colors[index].unwrap_or(Rgb {
                    r: 0xff,
                    g: 0xff,
                    b: 0xff,
                });

                self.send_input(&format(color));
            }
            Event::PtyWrite(text) => self.send_input(&text),
            Event::Exit => self.should_quit.store(true, Ordering::Relaxed),
            _ => {}
        }
    }
}

/// A ready-to-render terminal state.
pub struct TerminalDrawState {
    pub device: Arc<Device>,
    pub camera_buffer: Buffer,
    pub camera_bind_group: BindGroup,
    pub bg_mesh: DynamicMesh<SolidVertex>,
    pub glyph_meshes: FontSet<DynamicMesh<GlyphVertex>>,
    pub overlay_mesh: DynamicMesh<SolidVertex>,
}

impl TerminalDrawState {
    pub fn new(device: Arc<Device>, camera_bgl: &BindGroupLayout) -> Self {
        let camera_buffer = device.create_buffer(&BufferDescriptor {
            label: Some("Alacritty terminal camera buffer"),
            size: std::mem::size_of::<CameraUniform>() as BufferAddress,
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let camera_bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("Alacritty terminal camera bind group"),
            layout: camera_bgl,
            entries: &[BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });

        Self {
            camera_buffer,
            camera_bind_group,
            bg_mesh: DynamicMesh::new(&device),
            glyph_meshes: FontSet {
                regular: DynamicMesh::new(&device),
                italic: DynamicMesh::new(&device),
                bold: DynamicMesh::new(&device),
                bold_italic: DynamicMesh::new(&device),
            },
            overlay_mesh: DynamicMesh::new(&device),
            device,
        }
    }
}

/// An in-progress terminal draw state.
pub struct TerminalCanvas {
    colors: Colors,
    fonts: FontSet<Arc<FaceAtlas>>,
    bg_vertices: Vec<SolidVertex>,
    bg_indices: Vec<u32>,
    overlay_vertices: Vec<SolidVertex>,
    overlay_indices: Vec<u32>,
    glyphs: Vec<(Vec2, FontStyle, u16, u32)>,
}

impl TerminalCanvas {
    pub fn new(colors: Colors, fonts: FontSet<Arc<FaceAtlas>>) -> Self {
        Self {
            colors,
            fonts,
            bg_vertices: Vec::new(),
            bg_indices: Vec::new(),
            overlay_vertices: Vec::new(),
            overlay_indices: Vec::new(),
            glyphs: Vec::new(),
        }
    }

    pub fn update_from_content(&mut self, content: RenderableContent) {
        for index in 0..COUNT {
            if let Some(color) = content.colors[index] {
                self.colors[index] = Some(color);
            }
        }

        for cell in content.display_iter {
            self.draw_cell(cell);
        }

        self.draw_cursor(content.cursor);
    }

    pub fn apply_to_state(&self, state: &mut TerminalDrawState) {
        let mut touched = FontSet::<Vec<u16>>::default();
        let mut glyph_meshes = FontSet::<(Vec<GlyphVertex>, Vec<u32>)>::default();

        let scale = 1.0 / 37.5;
        for (offset, style, glyph, color) in self.glyphs.iter().copied() {
            let (vertices, indices) = &mut glyph_meshes.get_mut(style);

            let index = vertices.len() as u32;
            let atlas = &self.fonts.get(style).atlas;
            let bitmap = match atlas.glyphs[glyph as usize].as_ref() {
                Some(b) => b,
                None => continue,
            };

            touched.get_mut(style).push(glyph);

            vertices.extend(bitmap.vertices.iter().map(|v| GlyphVertex {
                position: v.position * scale + offset,
                tex_coords: v.tex_coords,
                color,
            }));

            indices.extend_from_slice(&[
                index,
                index + 1,
                index + 2,
                index + 2,
                index + 1,
                index + 3,
            ]);
        }

        self.fonts
            .as_ref()
            .zip(touched)
            .for_each(|(font, touched)| {
                font.touch(&touched);
            });

        state
            .glyph_meshes
            .as_mut()
            .zip(glyph_meshes)
            .for_each(|(mesh, (vertices, indices))| {
                mesh.update(&state.device, &vertices, &indices)
            });

        state
            .bg_mesh
            .update(&state.device, &self.bg_vertices, &self.bg_indices);

        state
            .overlay_mesh
            .update(&state.device, &self.overlay_vertices, &self.overlay_indices);
    }

    pub fn draw_cell(&mut self, cell: Indexed<&Cell>) {
        if cell.flags.contains(Flags::HIDDEN) {
            return;
        }

        let col = cell.point.column.0 as i32;
        let row = cell.point.line.0;
        let pos = self.grid_to_pos(col, row);
        let mut fg = cell.fg;
        let mut bg = cell.bg;

        let is_full_block = cell.c == '▀';

        if cell.flags.contains(Flags::INVERSE) ^ is_full_block {
            std::mem::swap(&mut fg, &mut bg);
        }

        if !is_full_block {
            let style = FontStyle::from_cell_flags(cell.flags);
            let face = self.fonts.get(style).face.as_face_ref();
            if let Some(glyph) = face.glyph_index(cell.c) {
                let fg = self.color_to_u32(fg);
                self.glyphs.push((pos, style, glyph.0, fg));
            }
        }

        /*if bg == Color::Named(NamedColor::Background) {
            return;
        }*/

        let bg = self.color_to_u32(bg);
        let tl = self.grid_to_pos(col, row - 1);
        let br = self.grid_to_pos(col + 1, row);
        self.draw_solid_rect(tl, br, bg);
    }

    pub fn draw_cursor(&mut self, cursor: RenderableCursor) {
        let cursor_color = Color::Named(NamedColor::Foreground);
        let cursor_color = self.color_to_u32(cursor_color);
        let col = cursor.point.column.0 as i32;
        let row = cursor.point.line.0;
        match cursor.shape {
            CursorShape::Hidden => {}
            _ => {
                let tl = self.grid_to_pos(col, row - 1);
                let br = self.grid_to_pos(col + 1, row);
                self.draw_solid_rect(tl, br, cursor_color);
            }
        }
    }

    pub fn draw_solid_rect(&mut self, tl: Vec2, br: Vec2, color: u32) {
        let index = self.bg_vertices.len() as u32;
        self.bg_vertices.extend_from_slice(&[
            SolidVertex {
                position: tl,
                color,
            },
            SolidVertex {
                position: Vec2::new(br.x, tl.y),
                color,
            },
            SolidVertex {
                position: Vec2::new(tl.x, br.y),
                color,
            },
            SolidVertex {
                position: br,
                color,
            },
        ]);

        self.bg_indices.extend_from_slice(&[
            index,
            index + 1,
            index + 2,
            index + 2,
            index + 1,
            index + 3,
        ]);
    }

    pub fn grid_to_pos(&self, x: i32, y: i32) -> Vec2 {
        let col = x as f32 / 50.0 - 1.0;
        let row = (y as f32 + 1.0) / -37.5 + 1.0;
        Vec2::new(col, row)
    }

    pub fn color_to_rgb(&self, color: Color) -> Rgb {
        match color {
            Color::Named(name) => self.colors[name].unwrap(),
            Color::Spec(rgb) => rgb,
            Color::Indexed(index) => {
                if let Some(color) = self.colors[index as usize] {
                    color
                } else if let Some(gray) = index.checked_sub(232) {
                    let value = gray * 10 + 8;
                    Rgb {
                        r: value,
                        g: value,
                        b: value,
                    }
                } else if let Some(cube_idx) = index.checked_sub(16) {
                    let r = cube_idx / 36;
                    let g = (cube_idx / 6) % 6;
                    let b = cube_idx % 6;

                    let c = |c| {
                        if c == 0 {
                            0
                        } else {
                            c * 40 + 55
                        }
                    };

                    Rgb {
                        r: c(r),
                        g: c(g),
                        b: c(b),
                    }
                } else {
                    Rgb {
                        r: 0xff,
                        g: 0x00,
                        b: 0xff,
                    }
                }
            }
        }
    }

    pub fn color_to_u32(&self, color: Color) -> u32 {
        let rgb = self.color_to_rgb(color);
        0xff000000 | ((rgb.b as u32) << 16) | ((rgb.g as u32) << 8) | (rgb.r as u32)
    }
}
