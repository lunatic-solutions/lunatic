#![feature(available_concurrency)]

use anyhow::Result;
use easy_parallel::Parallel;

use clap::{crate_version, Clap};
use lunatic_runtime::module;
use lunatic_runtime::process::{FunctionLookup, MemoryChoice, Process, EXECUTOR};

use std::fs;
use std::thread;

#[cfg(feature = "vm-wasmer")]
use wasmer_vm::traphandlers::setup_unix_sigaltstack;
#[cfg(feature = "vm-wasmtime")]
use wasmtime_runtime::traphandlers::setup_unix_sigaltstack;

#[derive(Clap)]
#[clap(version = crate_version!())]
struct Opts {
    /// .wasm file
    input: String,
    /// All other arguments are forwarded to the .wasm file
    #[clap(min_values(0))]
    _args: Vec<String>,
}

pub fn run() -> Result<()> {
    let opts: Opts = Opts::parse();

    let wasm = fs::read(opts.input).expect("Can't open .wasm file");

    let module = module::LunaticModule::new(&wasm)?;

    // Set up async runtime
    let cpus = thread::available_concurrency().unwrap();
    let (signal, shutdown) = smol::channel::unbounded::<()>();

    Parallel::new()
        .each(0..cpus.into(), |_| {
            // Extend the signal stack on all execution threads
            setup_unix_sigaltstack().unwrap();
            smol::future::block_on(EXECUTOR.run(shutdown.recv()))
        })
        .finish(|| {
            smol::future::block_on(async {
                let result = Process::create(
                    None,
                    module,
                    FunctionLookup::Name("_start"),
                    MemoryChoice::New(None),
                )
                .await;
                drop(signal);
                result
            })
        })
        .1?;

    Ok(())
}

fn main() -> Result<()> {
    env_logger::init();
    run()
}
