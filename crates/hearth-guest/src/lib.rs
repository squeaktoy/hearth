pub fn print_hello_world() {
    unsafe { abi::print_hello_world() };
}

mod abi {
    #[link(wasm_import_module = "cognito")]
    extern "C" {
        pub fn print_hello_world();
    }
}
