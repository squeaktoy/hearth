use wasmtime::Caller;

pub trait WasmLinker<T: AsRef<Self>> {
    const MODULE_NAME: &'static str;
    fn add_to_linker(linker: &mut wasmtime::Linker<T>);
    fn wrap_func<Params, Args>(
        linker: &mut wasmtime::Linker<T>,
        name: &str,
        func: impl wasmtime::IntoFunc<T, Params, Args>,
    ) {
        linker.func_wrap(Self::MODULE_NAME, name, func).unwrap();
    }
}

pub struct GuestMemory<'a> {
    pub bytes: &'a mut [u8],
}

impl<'a> GuestMemory<'a> {
    pub fn from_caller<T>(caller: &mut Caller<'a, T>) -> Self {
        let memory = caller.get_export("memory").unwrap().into_memory().unwrap();
        let data_ptr = memory.data_ptr(&caller);
        let data_size = memory.data_size(&caller);
        let bytes = unsafe { std::slice::from_raw_parts_mut(data_ptr, data_size) };
        Self { bytes }
    }

    pub fn get_str(&mut self, ptr: u32, len: u32) -> &'a mut str {
        let memory = self.get_slice(ptr as usize, len as usize);
        std::str::from_utf8_mut(memory).unwrap()
    }

    pub fn get_slice(&mut self, ptr: usize, len: usize) -> &'a mut [u8] {
        if ptr + len > self.bytes.len() {
            panic!("Attempted wasm memory read is out-of-bounds!");
        }

        unsafe {
            let ptr = self.bytes.as_mut_ptr().add(ptr);
            std::slice::from_raw_parts_mut(ptr, len)
        }
    }
}
