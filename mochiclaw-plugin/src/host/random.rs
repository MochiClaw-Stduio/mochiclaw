//! Host functions for rand operations

use extism::{CurrentPlugin, Function, UserData, Val, ValType};
use rand::Rng;

/// Create all rand host functions
pub fn rand_functions() -> Vec<Function> {
    vec![rand_u64_fn(), rand_bytes_fn()]
}

/// host_rand_u64: returns a random u64 as raw value (not memory offset)
pub fn rand_u64_fn() -> Function {
    Function::new(
        "host_rand_u64",
        [],
        [ValType::I64],
        UserData::Rust(std::sync::Arc::new(std::sync::Mutex::new(()))),
        |plugin: &mut CurrentPlugin,
         _inputs: &[Val],
         outputs: &mut [Val],
         _user_data: UserData<()>| {
            let val = rand::random::<u64>();
            plugin.memory_set_val(&mut outputs[0], val)
        },
    )
}

/// host_rand_bytes: allocates memory in WASM, fills with random bytes, returns offset
pub fn rand_bytes_fn() -> Function {
    Function::new(
        "host_rand_bytes",
        [ValType::I64],
        [ValType::I64],
        UserData::Rust(std::sync::Arc::new(std::sync::Mutex::new(()))),
        |plugin: &mut CurrentPlugin,
         inputs: &[Val],
         outputs: &mut [Val],
         _user_data: UserData<()>| {
            let num_bytes = match inputs.first() {
                Some(Val::I64(v)) => (*v as usize).clamp(0, 1024),
                _ => 0,
            };
            let mut buf = vec![0u8; num_bytes];
            rand::thread_rng().fill(&mut buf[..]);
            plugin.memory_set_val(&mut outputs[0], &buf)
        },
    )
}
