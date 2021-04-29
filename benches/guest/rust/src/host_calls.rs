use easybench::bench;
use lunatic::yield_;

mod host {
    #[link(wasm_import_module = "wasi_snapshot_preview1")]
    extern "C" {
        pub fn proc_raise(_signal: u32) -> u32;
    }
}

pub fn bench_host_calls() {
    println!("Call yield_: {}", bench(|| yield_()));
    // no-op
    println!(
        "Call wasi_snapshot_preview1::proc_raise: {}",
        bench(|| unsafe { host::proc_raise(0) })
    );
}
