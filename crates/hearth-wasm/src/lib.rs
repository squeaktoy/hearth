pub trait WasmLinker<T: AsRef<Self>> {
    const MODULE_NAME: &'static str;
    fn add_to_linker(linker: &mut wasmtime::Linker<T>);
    fn wrap_func<Params, Args>(linker: &mut wasmtime::Linker<T>, name: &str, func: impl wasmtime::IntoFunc<T, Params, Args>) {
        linker.func_wrap(Self::MODULE_NAME, name, func).unwrap();
    }
}