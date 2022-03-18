use std::sync::Arc;

use criterion::{criterion_group, criterion_main, Criterion};
// TODO: Re-export this under lunatic_runtime
use lunatic_process::runtimes::wasmtime::{default_config, WasmtimeRuntime};
use lunatic_runtime::{spawn_wasm, state::DefaultProcessState, DefaultProcessConfig};

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
            spawn_wasm(
                runtime.clone(),
                module.clone(),
                config.clone(),
                "hello",
                Vec::new(),
                None,
            )
            .await
            .unwrap()
            .0
            .await;
        });
    });

    // TODO: Plugin has a bug when run on modules without a table
    // let path = Path::new("target/wasm/stdlib.wasm");
    // let module = fs::read(path).unwrap();
    // let mut config = EnvConfig::default();
    // config.add_plugin(module).unwrap();
    // let environment = Environment::new(config).unwrap();

    // // Reload module into modified environment (added plugin)
    // let raw_module =  wat::parse_file("./wat/hello.wat").unwrap();
    // let module = rt.block_on(environment.create_module(raw_module)).unwrap();

    // c.bench_function("spawn process with stdlib plugin", |b| {
    //     b.to_async(&rt).iter(|| async {
    //         environment
    //             .spawn(&module, "hello", Vec::new())
    //             .await
    //             .unwrap();
    //     });
    // });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
