#![feature(available_concurrency)]

pub mod channel;
pub mod linker;
pub mod module;
pub mod networking;
pub mod normalisation;
pub mod process;
pub mod wasi;

use anyhow::Result;
use easy_parallel::Parallel;

use process::{FunctionLookup, MemoryChoice, Process, EXECUTOR};

use std::env;
use std::fs;
use std::thread;

pub fn run() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    let wasm_path = args.get(1).expect("Not enough arguments passed");
    let wasm = fs::read(wasm_path).expect("Can't open WASM file");

    let module = module::LunaticModule::new(wasm)?;

    // Set up async runtime
    let cpus = thread::available_concurrency().unwrap();
    let (signal, shutdown) = smol::channel::unbounded::<()>();

    Parallel::new()
        .each(0..cpus.into(), |_| {
            smol::future::block_on(EXECUTOR.run(shutdown.recv()))
        })
        .finish(|| {
            smol::future::block_on(async {
                let result =
                    Process::spawn(module, FunctionLookup::Name("_start"), MemoryChoice::New)
                        .join()
                        .await;
                drop(signal);
                result
            })
        })
        .1?;

    Ok(())
}
