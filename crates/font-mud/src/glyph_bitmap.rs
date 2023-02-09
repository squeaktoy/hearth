use std::ops::Deref;

use crate::error::{FontError, FontResult, GlyphShapeError};
use glam::Vec2;
use msdfgen::{Bitmap, FillRule, FontExt, MsdfGeneratorConfig, Range, Rgba};
use ttf_parser::{Face, GlyphId};

pub struct GlyphMtsdf {
    pub bitmap: GlyphBitmap,
    pub anchor: Vec2,
    pub scale: f32,
}

impl Deref for GlyphMtsdf {
    type Target = GlyphBitmap;

    fn deref(&self) -> &GlyphBitmap {
        &self.bitmap
    }
}

impl GlyphMtsdf {
    pub fn generate(
        scale: f64,
        range: Range<f64>,
        angle_threshold: f64,
        face: &Face,
        glyph: GlyphId,
    ) -> FontResult<Self> {
        let config: MsdfGeneratorConfig = MsdfGeneratorConfig::default();
        let mut shape = face
            .glyph_shape(glyph)
            .ok_or(FontError::GlyphShape(GlyphShapeError(glyph)))?;
        shape.edge_coloring_simple(angle_threshold, 0);
        let bounds = shape.get_bound();
        let width = (bounds.width() * scale).ceil() as usize + 2;
        let height = (bounds.height() * scale).ceil() as usize + 2;
        let framing = bounds
            .autoframe(width as u32, height as u32, range, None)
            .ok_or(FontError::AutoFraming {
                glyph,
                width,
                height,
                range,
            })?;
        println!("{:#?}", framing);
        let mut bitmap = Bitmap::<Rgba<f32>>::new(width as u32, height as u32);
        shape.generate_mtsdf(&mut bitmap, framing, config);
        shape.correct_sign(&mut bitmap, framing, FillRule::default());
        shape.correct_msdf_error(&mut bitmap, framing, config);

        Ok(Self {
            bitmap: GlyphBitmap {
                data: bitmap
                    .pixels()
                    .iter()
                    .map(|p| {
                        fn conv(f: f32) -> u32 {
                            (f * 256.0).round() as u8 as _
                        }

                        (conv(p.r) << 24) | (conv(p.g) << 16) | (conv(p.b) << 8) | conv(p.a)
                    })
                    .collect(),
                width,
                height,
            },
            anchor: Vec2::ZERO,
            scale: 0.0,
        })
    }
}

pub struct GlyphBitmap {
    pub data: Vec<u32>,
    pub width: usize,
    pub height: usize,
}

impl GlyphBitmap {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            data: vec![0; width * height],
            width,
            height,
        }
    }

    pub fn data_bytes(&self) -> &[u8] {
        unsafe {
            let ptr = self.data.as_ptr();
            let ptr: *const u8 = std::mem::transmute(ptr);
            let len = self.data.len() * 4;
            std::slice::from_raw_parts(ptr, len)
        }
    }

    pub fn copy_to(&self, dst: &mut GlyphBitmap, x: usize, y: usize) {
        if self.width + x > dst.width || self.height + y > dst.height {
            panic!("copy_to out-of-bounds");
        }

        let mut cursor = y * dst.width + x;
        for y in 0..self.height {
            let src_range = (y * self.width)..((y + 1) * self.width);
            let dst_range = cursor..(cursor + self.width);
            dst.data[dst_range].copy_from_slice(&self.data[src_range]);
            cursor += dst.width;
        }
    }
}
