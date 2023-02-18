use hearth_macros::impl_wasm_linker;
use hearth_rpc::ProcessApi;
use hearth_wasm::{GuestMemory, WasmLinker};
use tracing::info;
use wasmtime::{Caller, Linker};

/// This contains all script-accessible process-related stuff.
pub struct Cognito {}

// Should automatically generate link_print_hello_world:
// #[impl_wasm_linker]
// should work for any struct, not just Cognito
#[impl_wasm_linker]
impl Cognito {
    pub async fn print_hello_world(&self) {
        info!("Hello, world!");
    }

    pub async fn do_number(&self, number: u32) -> u32 {
        info!("do_number({}) called", number);
        number + 1
    }

    // impl_wasm_linker should also work with non-async functions
    //
    // if a function is passed GuestMemory or GuestMemory<'_>, the macro should
    // automatically create a GuestMemory instance using the Caller's exported
    // memory extern
    //
    // it should also turn arguments in the core wasm types (u32, u64, i32, u64)
    // into arguments for the linker's closure, as well as the return type,
    // which in this example is just ().
    pub fn log_message(&self, mut memory: GuestMemory<'_>, msg_ptr: u32, msg_len: u32) {
        eprintln!("message from wasm: {}", memory.get_str(msg_ptr, msg_len));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use wasmtime::{Config, Engine, Store};

    #[tokio::test]
    async fn host_works() {
        let cognito = Cognito {};
        cognito.print_hello_world().await;
    }

    struct MockStructure {
        pub cognito: Cognito,
    }

    impl Default for MockStructure {
        fn default() -> Self {
            Self {
                cognito: Cognito {},
            }
        }
    }
    impl AsRef<Cognito> for MockStructure {
        fn as_ref(&self) -> &Cognito {
            &self.cognito
        }
    }

    fn get_wasmtime_objs() -> (Linker<MockStructure>, Store<MockStructure>) {
        let mut config = Config::new();
        config.async_support(true);
        let engine = Engine::new(&config).unwrap();
        let mut linker: wasmtime::Linker<MockStructure> = Linker::new(&engine);
        let mut store = Store::new(&engine, MockStructure::default());
        Cognito::add_to_linker(&mut linker);
        (linker, store)
    }

    #[test]
    fn print_hello_world() {
        let (linker, mut store) = get_wasmtime_objs();
        let r#extern = linker
            .get(&mut store, "cognito", "print_hello_world")
            .unwrap();
        let typed_func = r#extern
            .into_func()
            .unwrap()
            .typed::<(), ()>(&store)
            .unwrap();
    }
    #[test]
    fn do_number() {
        let (linker, mut store) = get_wasmtime_objs();
        let r#extern = linker.get(&mut store, "cognito", "do_number").unwrap();
        let typed_func = r#extern
            .into_func()
            .unwrap()
            .typed::<u32, u32>(&store)
            .unwrap();
    }
}
