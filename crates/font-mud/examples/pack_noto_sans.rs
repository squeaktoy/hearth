use font_mud::glyph_atlas::GlyphAtlas;
use std::fs::File;

fn main() {
    let face = ttf_parser::Face::parse(notosans::REGULAR_TTF, 0).unwrap();
    let (glyph_atlas, _glyph_shape_errors) = GlyphAtlas::new(&face).unwrap();
    let bitmap = &glyph_atlas.bitmap;
    let output = File::create("noto-sans.png").unwrap();
    let mut encoder = png::Encoder::new(output, bitmap.width as u32, bitmap.height as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(bitmap.data_bytes()).unwrap();
}
