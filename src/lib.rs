use wamr_rust_sdk::{
    function::Function, instance::Instance, module::Module, runtime::Runtime, value::WasmValue,
};

mod wasm;

#[rust2go::g2r]
pub trait G2RCall {
    fn program(wasm_bytes: Vec<u8>) -> u64 {
        let runtime = Runtime::new().expect("Failed to create runtime");

        let module =
            Module::from_vec(&runtime, wasm_bytes, "program").expect("Failed to load module");

        let instance =
            Instance::new(&runtime, &module, 1024 * 64).expect("Failed to create instance");

        let function = Function::find_export_func(&instance, "program")
            .expect("Failed to find program function");

        let wasm_result = function
            .call(&instance, &vec![])
            .expect("Failed to call program function");

        match wasm_result[0] {
            WasmValue::I64(value) => value as u64,
            _ => panic!("Unexpected return type from fibonacci function"),
        }
    }
}

impl G2RCall for G2RCallImpl {}
