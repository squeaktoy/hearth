use super::*;

use hearth_guest::canvas::*;

lazy_static::lazy_static! {
    /// A lazily-initialized handle to the canvas factory service.
    static ref CANVAS_FACTORY: RequestResponse<FactoryRequest, FactoryResponse> =
        RequestResponse::expect_service("hearth.canvas.CanvasFactory");
}

/// A wrapper around the canvas Capability.
pub struct Canvas {
    cap: Capability,
}

impl Canvas {
    /// Creates a new Canvas.
    ///
    /// Panics if the factory responds with an error.
    pub fn new(position: Position, pixels: Pixels, sampling: CanvasSamplingMode) -> Self {
        let resp = CANVAS_FACTORY.request(
            FactoryRequest::CreateCanvas {
                position,
                pixels,
                sampling,
            },
            &[],
        );
        let _ = resp.0.unwrap();
        Canvas {
            cap: resp.1.get(0).unwrap().clone(),
        }
    }

    /// Update this canvas with a new buffer of pixels to draw.
    pub fn update(&self, buffer: Pixels) {
        self.cap.send(&CanvasUpdate::Resize(buffer), &[]);
    }

    /// Move this canvas to a new position in 3D space.
    pub fn relocate(&self, position: Position) {
        self.cap.send(&CanvasUpdate::Relocate(position), &[])
    }

    /// Blit a recatangular buffer to a part of this canvas.
    pub fn blit(&self, blit: Blit) {
        self.cap.send(&CanvasUpdate::Blit(blit), &[])
    }
}
