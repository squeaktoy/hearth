use font_mud::glyph_atlas::GlyphAtlas;
use std::fs::File;

fn main() {
    let face = ttf_parser::Face::parse(notosans::REGULAR_TTF, 0).unwrap();
    let (glyph_atlas, _glyph_shape_errors) = GlyphAtlas::new(&face).unwrap();
    let mut bitmap = glyph_atlas.bitmap.data.clone();
    bitmap.flip_y();
    let mut output = File::create("noto-sans.png").unwrap();
    bitmap.write_png(&mut output).unwrap();
}
