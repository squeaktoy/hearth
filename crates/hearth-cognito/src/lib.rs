use hearth_macros::impl_wasm_linker;
use hearth_rpc::ProcessApi;
use hearth_wasm::{GuestMemory, WasmLinker};
use wasmtime::{Caller, Linker};

/// This contains all script-accessible process-related stuff.
pub struct Cognito {
    pub api: Box<dyn ProcessApi + Send + Sync>,
}

// Should automatically generate link_print_hello_world:
// #[impl_wasm_linker]
// should work for any struct, not just Cognito
#[impl_wasm_linker]
impl Cognito {
    pub async fn print_hello_world(&self) {
        self.api.print_hello_world().await.unwrap();
    }
    pub async fn do_number(&self, number: u32) -> u32 {
        self.api.do_number(number).await.unwrap()
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

    use hearth_rpc::{remoc, CallResult};
    use remoc::rtc::async_trait;
    use wasmtime::{Config, Engine, Instance, Store};

    struct MockProcessApi;

    #[async_trait]
    impl ProcessApi for MockProcessApi {
        async fn print_hello_world(&self) -> CallResult<()> {
            println!("Hello, world!");
            Ok(())
        }

        async fn do_number(&self, number: u32) -> CallResult<u32> {
            Ok(number)
        }
    }

    #[test]
    fn host_works() {
        let api = Box::new(MockProcessApi);
        let cognito = Cognito { api };
        cognito.print_hello_world();
    }

    struct MockStructure {
        pub cognito: Cognito,
    }
    impl Default for MockStructure {
        fn default() -> Self {
            Self {
                cognito: Cognito {
                    api: Box::new(MockProcessApi),
                },
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
        let api = Box::new(MockProcessApi);
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
