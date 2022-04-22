use std::sync::Arc;

use criterion::{criterion_group, criterion_main, Criterion};
use dashmap::DashMap;
// TODO: Re-export this under lunatic_runtime
use lunatic_process::{
    runtimes::wasmtime::{default_config, WasmtimeRuntime},
    state::ProcessState,
};
use lunatic_runtime::{spawn_wasm, DefaultProcessConfig, DefaultProcessState};

fn criterion_benchmark(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let config = Arc::new(DefaultProcessConfig::default());
    let wasmtime_config = default_config();
    let runtime = WasmtimeRuntime::new(&wasmtime_config).unwrap();

    let raw_module = wat::parse_file("./wat/hello.wat").unwrap();
    let module = runtime
        .compile_module::<DefaultProcessState>(raw_module)
        .unwrap();

    c.bench_function("spawn process", |b| {
        b.to_async(&rt).iter(|| async {
            let registry = Arc::new(DashMap::new());
            let state =
                DefaultProcessState::new(runtime.clone(), module.clone(), config.clone(), registry)
                    .unwrap();
            spawn_wasm(
                runtime.clone(),
                module.clone(),
                state,
                "hello",
                Vec::new(),
                None,
            )
            .await
            .unwrap()
            .0
            .await
            .unwrap();
        });
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
