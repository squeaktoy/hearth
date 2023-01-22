use hearth_wasm::WasmLinker;
use wasmtime::Linker;

/// This contains all script-accessible process-related stuff.
pub struct Cognito {}

impl Cognito {
    pub fn print_hello_world(&self) {
        println!("Hello, world!");
    }
}

impl<T: AsRef<Cognito> + 'static> WasmLinker<T> for Cognito {
    const MODULE_NAME: &'static str = "cognito";

    fn add_to_linker(linker: &mut Linker<T>) {
        Self::wrap_func(
            linker,
            "print_hello_world",
            move |caller: wasmtime::Caller<'_, T>| {
                caller.data().as_ref().print_hello_world();
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_works() {
        let cognito = Cognito {};
        cognito.print_hello_world();
    }
}