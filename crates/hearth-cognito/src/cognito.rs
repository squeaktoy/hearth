use wasmtime::Linker;
use hearth_wasm::WasmLinker;

/// This contains all script-accessible process-related stuff.
pub struct Cognito {

}

impl<T: AsRef<Cognito> + 'static> WasmLinker<T> for Cognito {
    const MODULE_NAME: &'static str = "cognito";

    fn add_to_linker(linker: &mut Linker<T>) {
        Self::wrap_func(linker, "print_hello_world", print_hello_world);
    }
}

fn print_hello_world<T: AsRef<Cognito>>(caller: wasmtime::Caller<'_, T>) {
    println!("hello world!");
}