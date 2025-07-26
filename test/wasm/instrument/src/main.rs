use radix_wasm_instrument::{
    gas_metering::{ConstantCostRules, host_function, inject},
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

    let injected_module =
        inject(&mut module, backend, &ConstantCostRules::new(1, 10_000, 1)).unwrap();

    println!(
        "Instrumented: {} bytes -> {} bytes",
        module_bytes.len(),
        injected_module.len()
    );

    std::fs::write(module_path, injected_module).expect("Failed to write injected module to file");
}
