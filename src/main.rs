use anyhow::Result;
use tokio::runtime::Runtime;
use wasmtime::{Config, Engine, Module};

use lunatic::patching::patch;
use lunatic::processes::spawn::Spawner;

use std::env;
use std::fs;

fn main() -> Result<()> {
    let mut config = Config::new();
    config.static_memory_maximum_size(0); // Disable static memory
    let engine = Engine::new(&config);

    let args: Vec<String> = env::args().collect();
    let wasm_path = args.get(1).expect("Not enough arguments passed");
    let wasm = fs::read(wasm_path).expect("Can't open WASM file");
    let (initial_memory_size, wasm) = patch(&wasm)?;
    let module = Module::new(&engine, wasm)?;

    let mut rt = Runtime::new()?;
    rt.block_on(async {
        let spawner = Spawner::new(module, engine, initial_memory_size);
        spawner.spawn_by_name("_start").await.unwrap();
    });

    Ok(())
}
