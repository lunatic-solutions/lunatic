use anyhow::Result;
use tokio::runtime::Runtime;
use wasmer::{Store, Module, Features, JITEngine, Cranelift, Target, CompilerConfig};

use lunatic_vm::patching::patch;
use lunatic_vm::process::creator::spawn_by_name;

use std::env;
use std::fs;

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    let wasm_path = args.get(1).expect("Not enough arguments passed");
    let wasm = fs::read(wasm_path).expect("Can't open WASM file");
    
    // Transfrom WASM file into a format 
    let (min_memory, wasm) = patch(&wasm)?;

    // Enable all experimental features
    let features = Features {
        threads: true,
        reference_types: true,
        simd: true,
        bulk_memory: true,
        multi_value: true
    };
    let mut cranelift = Cranelift::new();
    cranelift.enable_simd(true);
    let engine = JITEngine::new(cranelift.compiler(), Target::default(), features);

    let store = Store::new(&engine);
    let module = Module::new(&store, wasm)?;

    let mut rt = Runtime::new()?;
    rt.block_on(async {
        spawn_by_name(module, "_start", min_memory).await.unwrap();
    });

    Ok(())
}
