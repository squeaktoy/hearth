use hearth_guest::{renderer::*, Lump};
use kindling_host::renderer::set_skybox;

/// Helper function to append a skybox image to the cube texture data.
fn add_face(data: &mut Vec<u8>, image: &[u8]) {
    let decoded = image::load_from_memory(image).unwrap().into_rgba8();
    data.extend_from_slice(decoded.as_raw());
}

#[no_mangle]
pub extern "C" fn run() {
    let mut data = Vec::new();
    add_face(&mut data, include_bytes!("elyvisions/sh_ft.png"));
    add_face(&mut data, include_bytes!("elyvisions/sh_bk.png"));
    add_face(&mut data, include_bytes!("elyvisions/sh_up.png"));
    add_face(&mut data, include_bytes!("elyvisions/sh_dn.png"));
    add_face(&mut data, include_bytes!("elyvisions/sh_rt.png"));
    add_face(&mut data, include_bytes!("elyvisions/sh_lf.png"));

    let texture = Lump::load(&TextureData {
        label: None,
        size: (1024, 1024).into(),
        data,
    });

    set_skybox(&texture);
}
