use crate::error::{FontError, FontResult, GlyphShapeError};
use msdfgen::{Bitmap, FillRule, FontExt, MsdfGeneratorConfig, Range, Rgb};
use ttf_parser::{Face, GlyphId};

pub struct GlyphBitmap {
    pub data: Bitmap<Rgb<f32>>,
    pub width: usize,
    pub height: usize,
}

impl GlyphBitmap {
    pub fn new(
        scale: f64,
        range: Range<f64>,
        angle_threshold: f64,
        face: &Face,
        glyph: GlyphId,
    ) -> FontResult<Self> {
        let config: MsdfGeneratorConfig = MsdfGeneratorConfig::default();
        let mut shape = face
            .glyph_shape(glyph)
            .ok_or(FontError::GlyphShape(GlyphShapeError(glyph.clone())))?;
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
        let mut bitmap = Bitmap::<Rgb<f32>>::new(width as u32, height as u32);
        shape.generate_msdf(&mut bitmap, &framing, config);
        shape.correct_sign(&mut bitmap, &framing, FillRule::default());
        shape.correct_msdf_error(&mut bitmap, &framing, config);
        Ok(Self {
            data: bitmap,
            width,
            height,
        })
    }

    pub fn copy_into_bitmap(
        &self,
        bitmap: &mut Bitmap<Rgb<f32>>,
        offset_width: usize,
        offset_height: usize,
        bitmap_width: usize,
    ) {
        let pixels = bitmap.pixels_mut();
        for y in 0..self.height {
            for x in 0..self.width {
                let pixel_index = x + offset_width + (y + offset_height) * bitmap_width;
                let i_index = x + y * self.width;
                pixels[pixel_index] = self.data.pixels()[i_index];
            }
        }
    }
}
