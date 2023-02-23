// Copyright (c) 2023 the Hearth contributors.
// SPDX-License-Identifier: Apache-2.0

use msdfgen::Range;
use std::fmt;
use std::fmt::Formatter;
use ttf_parser::GlyphId;

#[derive(Debug, Clone)]
pub enum FontError {
    GlyphShape(GlyphShapeError),
    AutoFraming {
        glyph: GlyphId,
        width: usize,
        height: usize,
        range: Range<f64>,
    },
    PackingError(GlyphId),
}

#[derive(Debug, Clone)]
pub struct GlyphShapeError(pub GlyphId);

impl fmt::Display for GlyphShapeError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "glyph shape formation error for glyph id: {}", self.0 .0)
    }
}

impl fmt::Display for FontError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            FontError::GlyphShape(glyph_shape_error) => {
                write!(f, "{}", glyph_shape_error)
            }
            FontError::AutoFraming {
                glyph,
                width,
                height,
                range,
            } => {
                write!(f, "failed to autoframe glyph: {}, with dimensions:: width: {}, height: {}, px_range: {:?}", glyph.0, width, height, range)
            }
            FontError::PackingError(glyph) => {
                write!(f, "packing error for glyph: {}", glyph.0)
            }
        }
    }
}

pub type FontResult<T> = Result<T, FontError>;
