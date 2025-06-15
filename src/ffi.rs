use wamr_rust_sdk::{
    function::Function, instance::Instance, module::Module, runtime::Runtime, value::WasmValue,
};

use crate::{IntantiatedModule, WASM_INSTANCES};

#[rust2go::g2r]
pub trait G2RCall {
    fn program(wasm_bytes: Vec<u8>) -> u64 {
        let mut ret_val = 0;

        WASM_INSTANCES.with(|instances| {
            let mut instances = instances.borrow_mut();

            let instance = match instances.get(wasm_bytes.as_slice()) {
                Some(instance) => instance,
                None => {
                    let leaked_bytes = wasm_bytes.clone().leak();
                    let runtime = Runtime::new().expect("Failed to create runtime");
                    let leaked_runtime = Box::leak(Box::new(runtime));

                    let module = Module::from_vec(leaked_runtime, wasm_bytes, "program")
                        .expect("Failed to load module");

                    let leaked_module = Box::leak(Box::new(module));

                    let instance = Instance::new(leaked_runtime, leaked_module, 1024 * 64)
                        .expect("Failed to create instance");

                    let leaked_instance = Box::leak(Box::new(instance));

                    let function = Function::find_export_func(leaked_instance, "program")
                        .expect("Failed to find program function");

                    let instantiated_module = IntantiatedModule {
                        runtim: leaked_runtime,
                        module: leaked_module,
                        instance: leaked_instance,
                        function: Box::leak(Box::new(function)),
                    };

                    let leaked_instantiated_module = Box::leak(Box::new(instantiated_module));

                    instances.insert(leaked_bytes, leaked_instantiated_module);

                    &*leaked_instantiated_module
                }
            };

            let result = instance
                .function
                .call(&instance.instance, &vec![])
                .expect("Failed to call program function");

            ret_val = match result[0] {
                WasmValue::I64(value) => value as u64,
                _ => panic!("Unexpected return type from fibonacci function"),
            };
        });

        ret_val
    }
}

impl G2RCall for G2RCallImpl {}
