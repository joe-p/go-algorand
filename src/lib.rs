use once_cell::sync::Lazy;
use std::path::PathBuf;
use wamr_rust_sdk::{
    function::Function, instance::Instance, module::Module, runtime::Runtime, value::WasmValue,
};

struct WasmEngine {
    runtime: &'static Runtime,
    module: Module<'static>,
}

thread_local! {
    static WASM_ENGINE: Lazy<WasmEngine> = Lazy::new(|| {

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

#[derive(rust2go::R2G, Clone, Copy)]
pub struct AddResult {
    pub overflow: bool,
    pub result: u64,
}

#[rust2go::g2r]
pub trait G2RCall {
    fn add(a: u64, b: u64) -> AddResult;
    fn wasm_fibonacci(n: u64) -> u64;
}

impl G2RCall for G2RCallImpl {
    fn add(a: u64, b: u64) -> AddResult {
        if let Some(result) = a.checked_add(b) {
            AddResult {
                overflow: false,
                result,
            }
        } else {
            AddResult {
                overflow: true,
                result: 0,
            }
        }
    }

    fn wasm_fibonacci(n: u64) -> u64 {
        let mut result = 0;
        WASM_ENGINE.with(|engine| {
            let instance = Instance::new(engine.runtime, &engine.module, 1024 * 64)
                .expect("Failed to create instance");

            let function = Function::find_export_func(&instance, "fibonacci")
                .expect("Failed to find function");

            let params: Vec<WasmValue> = vec![WasmValue::I64(n as i64)];
            let wasm_result = function
                .call(&instance, &params)
                .expect("Failed to call function");

            result = match wasm_result[0] {
                WasmValue::I64(value) => value as u64,
                _ => panic!("Unexpected return type from fibonacci function"),
            }
        });

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wasm_fibonacci() {
        let result = G2RCallImpl::wasm_fibonacci(10);
        assert_eq!(result, 55);

        let result = G2RCallImpl::wasm_fibonacci(20);
        assert_eq!(result, 6765);
    }
}
