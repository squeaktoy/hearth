use hearth_rpc::ProcessApi;
use hearth_wasm::WasmLinker;
use wasmtime::{Caller, Linker};

/// This contains all script-accessible process-related stuff.
pub struct Cognito<Api> {
    pub api: Api,
}

impl<Api: ProcessApi + Send + Sync> Cognito<Api> {
    pub async fn print_hello_world(&self) {
        self.api.print_hello_world().await.unwrap();
    }
}

impl<Api, T> WasmLinker<T> for Cognito<Api>
where
    Api: ProcessApi + Send + Sync + 'static,
    T: AsRef<Cognito<Api>> + Send + 'static,
{
    const MODULE_NAME: &'static str = "cognito";

    fn add_to_linker(linker: &mut Linker<T>) {
        linker
            .func_wrap0_async("cognito", "print_hello_world", |caller: Caller<'_, T>| {
                Box::new(print_hello_world(caller))
            })
            .unwrap();
    }
}

pub async fn print_hello_world<Api, T>(caller: wasmtime::Caller<'_, T>)
where
    Api: ProcessApi + Send + Sync,
    T: AsRef<Cognito<Api>> + Send,
{
    let cognito = caller.data().as_ref();
    cognito.print_hello_world().await;
}

#[cfg(test)]
mod tests {
    use super::*;

    use hearth_rpc::{remoc, CallResult};
    use remoc::rtc::async_trait;

    struct MockProcessApi;

    #[async_trait]
    impl ProcessApi for MockProcessApi {
        async fn print_hello_world(&self) -> CallResult<()> {
            println!("Hello, world!");
            Ok(())
        }
    }

    #[test]
    fn host_works() {
        let api = MockProcessApi;
        let cognito = Cognito { api };
        cognito.print_hello_world();
    }
}
