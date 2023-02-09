use crate::error::{FontError, FontResult, GlyphShapeError};
use crate::glyph_bitmap::{GlyphBitmap, GlyphMtsdf};
use glam::{UVec2, Vec2};
use msdfgen::Range;
use rect_packer::Packer;
use ttf_parser::{Face, GlyphId};

#[derive(Copy, Clone, Debug)]
pub struct GlyphVertex {
    pub position: Vec2,
    pub tex_coords: Vec2,
}

pub struct GlyphInfo {
    pub position: UVec2,
    pub size: UVec2,
    pub anchor: Vec2,
    pub vertices: [GlyphVertex; 4],
}

pub struct GlyphAtlas {
    pub bitmap: GlyphBitmap,
    pub glyphs: Vec<Option<GlyphInfo>>,
}

impl GlyphAtlas {
    pub const PX_PER_EM: f64 = 48.0;
    pub const RANGE: Range<f64> = Range::Px(2.0);
    pub const ANGLE_THRESHOLD: f64 = 3.0;

    /// turns a face into a glyph atlas.
    /// all fonts have some glyph shape errors for some reason, we pass those through, as we treat them as non-fatal errors.
    pub fn new(face: &Face) -> FontResult<(GlyphAtlas, Vec<GlyphShapeError>)> {
        let mut glyphs = vec![];
        let mut glyph_shape_errors = vec![];
        for c in 0..face.number_of_glyphs() {
            let glyph = GlyphMtsdf::generate(
                face.units_per_em() as f64,
                Self::PX_PER_EM,
                Self::RANGE,
                Self::ANGLE_THRESHOLD,
                face,
                GlyphId(c),
            );

            match glyph {
                Ok(glyph) => {
                    glyphs.push(Some(glyph));
                }
                Err(err) => {
                    match err {
                        FontError::GlyphShape(glyph_shape_error) => {
                            glyph_shape_errors.push(glyph_shape_error);
                        }
                        error => return Err(error),
                    }
                    glyphs.push(None);
                }
            }
        }
        let mut packer = Self::generate_packer(&glyphs);
        let width = packer.config().width as u32;
        let height = packer.config().height as u32;
        let texture_size = Vec2::new(width as f32, height as f32);
        let mut bitmap = GlyphBitmap::new(width, height);
        let mut glyph_info = vec![];
        for glyph in glyphs {
            match glyph {
                None => {
                    glyph_info.push(None);
                }
                Some(glyph) => {
                    if let Some(rect) = packer.pack(glyph.width as i32, glyph.height as i32, false)
                    {
                        glyph.copy_to(&mut bitmap, rect.x as u32, rect.y as u32);

                        let scale = (1.0 / glyph.px_per_em) as f32;
                        let offset = glyph.anchor;
                        println!("{} {:?}", scale, offset);

                        let position = Vec2::new(rect.x as f32, rect.y as f32);
                        let position = position / texture_size;

                        let size = Vec2::new(glyph.width as f32, glyph.height as f32);
                        let v1 = Vec2::ZERO;
                        let v2 = Vec2::new(size.x, 0.0);
                        let v3 = Vec2::new(0.0, size.y);
                        let v4 = size;

                        glyph_info.push(Some(GlyphInfo {
                            position: UVec2::new(rect.x as u32, rect.y as u32),
                            size: UVec2::new(glyph.width, glyph.height),
                            anchor: offset,
                            vertices: [
                                GlyphVertex {
                                    position: v1 * scale - offset,
                                    tex_coords: v1 / texture_size + position,
                                },
                                GlyphVertex {
                                    position: v2 * scale - offset,
                                    tex_coords: v2 / texture_size + position,
                                },
                                GlyphVertex {
                                    position: v3 * scale - offset,
                                    tex_coords: v3 / texture_size + position,
                                },
                                GlyphVertex {
                                    position: v4 * scale - offset,
                                    tex_coords: v4 / texture_size + position,
                                },
                            ],
                        }))
                    }
                }
            }
        }

        Ok((
            GlyphAtlas {
                bitmap,
                glyphs: glyph_info,
            },
            glyph_shape_errors,
        ))
    }

    fn generate_packer(glyphs: &Vec<Option<GlyphMtsdf>>) -> Packer {
        let mut config = rect_packer::Config {
            width: 256,
            height: 256,
            border_padding: 0,
            rectangle_padding: 0,
        };

        let mut packer = Packer::new(config);
        let mut last_switched_width = false;
        loop {
            let mut flag = true;
            for glyph in glyphs {
                match glyph {
                    None => {}
                    Some(glyph) => {
                        match packer.pack(glyph.width as i32, glyph.height as i32, false) {
                            None => {
                                match last_switched_width {
                                    true => {
                                        last_switched_width = false;
                                        config.height *= 2;
                                    }
                                    false => {
                                        last_switched_width = true;
                                        config.width *= 2;
                                    }
                                }
                                packer = Packer::new(config);
                                flag = false;
                                break;
                            }
                            Some(_) => {}
                        }
                    }
                }
            }
            if flag {
                return Packer::new(config);
            }
        }
    }
}
