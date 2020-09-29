use anyhow::Result;
use tokio::runtime::Runtime;
use wasmer::{Store, Module};

use lunatic::patching::patch;
use lunatic::process::creator::spawn_by_name;

use std::env;
use std::fs;

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    let wasm_path = args.get(1).expect("Not enough arguments passed");
    let wasm = fs::read(wasm_path).expect("Can't open WASM file");
    let (initial_memory_size, wasm) = patch(&wasm)?;
    let store = Store::default();
    let module = Module::new(&store, wasm)?;

    let mut rt = Runtime::new()?;
    rt.block_on(async {
        spawn_by_name(module, "_start", initial_memory_size).await.unwrap();
    });

    Ok(())
}
