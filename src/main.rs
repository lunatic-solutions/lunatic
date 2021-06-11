#![feature(available_concurrency)]

use anyhow::Result;
use easy_parallel::Parallel;

use clap::{crate_version, Clap};
use lunatic_runtime::module;
use lunatic_runtime::{
    api::heap_profiler,
    api::process::{FunctionLookup, MemoryChoice, Process, EXECUTOR},
    module::Runtime,
};
use uptown_funk::HostFunctions;

use std::fs;
use std::sync::Arc;
use std::thread;

#[derive(Clap)]
#[clap(version = crate_version!())]
struct Opts {
    /// .wasm file
    input: String,
    /// All other arguments are forwarded to the .wasm file
    #[clap(min_values(0))]
    _args: Vec<String>,
    /// Save heap profile to heap.dat
    #[clap(short, long)]
    profile: bool,
    /// Output patched/normalised wasm to normalised.wasm
    #[clap(short, long)]
    normalised_out: bool,
}

pub fn run() -> Result<()> {
    let opts: Opts = Opts::parse();
    let is_profile = opts.profile;

    let wasm = fs::read(opts.input).expect("Can't open .wasm file");

    let module =
        module::LunaticModule::new(&wasm, Runtime::default(), is_profile, opts.normalised_out)?;

    // Set up async runtime
    let cpus = thread::available_concurrency().unwrap();
    let (signal, shutdown) = smol::channel::unbounded::<()>();
    let (_, profiler) = heap_profiler::HeapProfilerState::new().split();

    Parallel::new()
        .each(0..cpus.into(), |_| {
            smol::future::block_on(EXECUTOR.run(shutdown.recv()))
        })
        .finish(|| {
            smol::future::block_on(async {
                let result = Process::create(
                    None,
                    module,
                    FunctionLookup::Name(String::from("_start")),
                    MemoryChoice::New(None),
                    profiler.clone(),
                )
                .await;
                drop(signal);
                result
            })
        })
        .1?;
    if is_profile {
        let mut profile_out = std::fs::File::create("heap.dat")?;
        let mut profiler = Arc::try_unwrap(profiler)
            .map_err(|_| {
                anyhow::Error::msg("heap_profiler: HeapProfilerState referenced multiple times")
            })?
            .into_inner()
            .unwrap();

        profiler.collect_data()?;
        profiler.write_dat(&mut profile_out)?;
    }

    Ok(())
}

fn main() -> Result<()> {
    env_logger::init();
    run()
}
