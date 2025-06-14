use once_cell::sync::Lazy;
use std::path::PathBuf;
use wamr_rust_sdk::{
    function::Function, instance::Instance, module::Module, runtime::Runtime, value::WasmValue,
};

pub struct WasmEngine {
    pub runtime: &'static Runtime,
    pub module: Module<'static>,
}

thread_local! {
    pub static WASM_ENGINE: Lazy<WasmEngine> = Lazy::new(|| {

        let  runtime_ref = Box::leak(Box::new(
            Runtime::new().expect("Failed to create runtime")
        ));

        let mut d = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        d.push("fibo.aot");
        let wasm_bytes = std::fs::read(d).expect("Failed to read AOT file");

        let module = Module::from_vec(runtime_ref, wasm_bytes.clone(), "fibo")
            .expect("Failed to load module");

        WasmEngine { runtime: runtime_ref, module }
    });
}
