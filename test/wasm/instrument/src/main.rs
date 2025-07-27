use radix_wasm_instrument::{
    gas_metering::{ConstantCostRules, host_function, inject},
    inject_stack_limiter,
    utils::module_info::ModuleInfo,
};

fn main() {
    let backend = host_function::Injector::new("algorand", "host_gas_check");

    let rel_path = std::env::args().nth(1).expect("Module path not provided");
    let module_path = std::env::current_dir()
        .expect("Failed to get current directory")
        .join(&rel_path);

    let module_bytes = std::fs::read(&module_path).expect("Failed to read module file");
    let mut module =
        ModuleInfo::new(&module_bytes).expect("Failed to create ModuleInfo from bytes");

    let gas_metered_module_bytes =
        inject(&mut module, backend, &ConstantCostRules::new(1, 10_000, 1)).unwrap();

    println!(
        "Gas Metering: {} bytes -> {} bytes",
        module_bytes.len(),
        gas_metered_module_bytes.len()
    );

    let mut gas_metered_module = ModuleInfo::new(&gas_metered_module_bytes)
        .expect("Failed to create ModuleInfo from gas-metered bytes");

    let stack_limited_and_gas_metered_module_bytes =
        inject_stack_limiter(&mut gas_metered_module, 1000)
            .expect("Failed to inject stack limiter");

    println!(
        "Stack Limited: {} bytes -> {} bytes",
        module_bytes.len(),
        stack_limited_and_gas_metered_module_bytes.len()
    );

    std::fs::write(module_path, stack_limited_and_gas_metered_module_bytes)
        .expect("Failed to write injected module to file");
}
