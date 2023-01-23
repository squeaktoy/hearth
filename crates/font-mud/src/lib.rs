pub mod error;
pub mod glyph_atlas;
pub mod glyph_bitmap;

#[cfg(test)]
mod tests {
    use crate::glyph_atlas::GlyphAtlas;
    use std::fs::File;

    #[test]
    fn test() {
        let face = ttf_parser::Face::parse(notosans::REGULAR_TTF, 0).unwrap();
        let (glyph_atlas, _glyph_shape_errors) = GlyphAtlas::new(&face).unwrap();
        save_bitmap_and_preview("lets", "go", "gamers", &glyph_atlas.bitmap.data);
    }

    fn save_bitmap_and_preview<T>(pfx: &str, name: &str, sfx: &str, bitmap: &msdfgen::Bitmap<T>)
    where
        T: msdfgen::PngColorType + Copy,
        T::PngPixelType: From<T>,
        msdfgen::Gray<f32>: msdfgen::RenderTarget<T>,
    {
        let mut bitmap = bitmap.clone();
        bitmap.flip_y();

        let mut output = File::create(&format!("{}-{}-{}.png", pfx, name, sfx)).unwrap();
        bitmap.write_png(&mut output).unwrap();
    }
}
