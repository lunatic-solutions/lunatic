use anyhow::Result;
use tokio::runtime::Runtime;
use wasmtime::{Config, Engine, Module};

use lunatic_vm::patching::patch;
use lunatic_vm::process::creator::{spawn, FunctionLookup};

use std::env;
use std::fs;

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    let wasm_path = args.get(1).expect("Not enough arguments passed");
    let wasm = fs::read(wasm_path).expect("Can't open WASM file");

    // Transfrom WASM file into a format
    let (min_memory, wasm) = patch(&wasm)?;

    let config = Config::new();
    let engine = Engine::new(&config);

    let module = Module::new(&engine, wasm)?;

    let mut rt = Runtime::new()?;
    rt.block_on(async {
        spawn(engine, module, FunctionLookup::Name("_start"), None)
            .await
            .unwrap();
    });

    Ok(())
}
