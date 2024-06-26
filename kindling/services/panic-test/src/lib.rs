hearth_guest::export_metadata!();

// Currently just a stub for testing purposes
#[no_mangle]
pub extern "C" fn run() {
    panic!("panic handler works!");
}
