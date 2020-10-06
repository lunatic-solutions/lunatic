use anyhow::Result;
use smol::{Executor, future, channel};
use easy_parallel::Parallel;
use wasmtime::{Config, Engine, Module};

use lunatic_vm::patching::patch;
use lunatic_vm::process::creator::{spawn, FunctionLookup, EXECUTOR};

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

    // Set up async runtime
    let cpus = num_cpus::get();
    let (signal, shutdown) = channel::unbounded::<()>();

    Parallel::new()
        .each(0..cpus, |_| future::block_on(EXECUTOR.run(shutdown.recv())))
        .finish(|| future::block_on(async {
            spawn(engine, module, FunctionLookup::Name("_start"), None).await;
            drop(signal);
    }));

    Ok(())
}
