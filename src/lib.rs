use once_cell::sync::Lazy;
use std::{cell::RefCell, collections::HashMap};
use wamr_rust_sdk::{function::Function, instance::Instance, module::Module, runtime::Runtime};

pub mod ffi;

thread_local! {
static WASM_INSTANCES: Lazy<RefCell<HashMap<&'static [u8], &'static IntantiatedModule<'static>>>> =
    Lazy::new(|| RefCell::new(HashMap::new()));
}

pub struct IntantiatedModule<'a> {
    pub runtim: &'a Runtime,
    pub module: &'a Module<'a>,
    pub instance: &'a Instance<'a>,
    pub function: &'a Function<'a>,
}
