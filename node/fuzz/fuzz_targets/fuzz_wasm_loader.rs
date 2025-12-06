//! Y10.2: Fuzz target for Wasm module loader
//!
//! This fuzzer tests the Wasm module loader with arbitrary byte sequences
//! to find crashes, panics, or memory safety issues in module validation.
//!
//! Note: We test wasmtime's Module::validate directly since creating a full
//! WasmRuntime for each fuzz iteration would be too slow.
//!
//! Run with: cargo +nightly fuzz run fuzz_wasm_loader

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Create a simple wasmtime engine for validation
    // This tests that arbitrary bytes don't crash the Wasm validator
    let engine = wasmtime::Engine::default();

    // Try to validate arbitrary bytes as a Wasm module
    // This should never panic, only return Ok/Err
    let _ = wasmtime::Module::validate(&engine, data);

    // If validation succeeded, try to compile
    // This exercises more code paths
    if wasmtime::Module::validate(&engine, data).is_ok() {
        let _ = wasmtime::Module::new(&engine, data);
    }
});
