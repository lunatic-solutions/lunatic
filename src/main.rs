#![feature(available_concurrency)]

use anyhow::Result;
use easy_parallel::Parallel;
use smol::{channel, future};
use wasmtime::Module;

use lunatic_vm::normalisation::patch;
use lunatic_vm::process::{FunctionLookup, MemoryChoice, Process, EXECUTOR};
use lunatic_vm::wasmtime::engine;

use std::env;
use std::fs;
use std::thread;

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    let wasm_path = args.get(1).expect("Not enough arguments passed");
    let wasm = fs::read(wasm_path).expect("Can't open WASM file");

    // Transfrom WASM file into a format
    let (min_memory, wasm) = patch(&wasm)?;

    let engine = engine();

    let module = Module::new(&engine, wasm)?;

    // Set up async runtime
    let cpus = thread::available_concurrency().unwrap();
    let (signal, shutdown) = channel::unbounded::<()>();

    Parallel::new()
        .each(0..cpus.into(), |_| {
            future::block_on(EXECUTOR.run(shutdown.recv()))
        })
        .finish(|| {
            future::block_on(async {
                let result = Process::spawn(
                    engine,
                    module,
                    FunctionLookup::Name("_start"),
                    MemoryChoice::New(min_memory),
                )
                .take_task()
                .unwrap()
                .await;
                drop(signal);
                result
            })
        })
        .1?;

    Ok(())
}
