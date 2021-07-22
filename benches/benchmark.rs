// use std::{fs, path::Path};

use criterion::{criterion_group, criterion_main, Criterion};
use lunatic_runtime::{EnvConfig, Environment};

fn criterion_benchmark(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();

    let config = EnvConfig::default();
    let environment = Environment::new(config).unwrap();

    let raw_module = std::fs::read("./target/wasm/hello.wasm").unwrap();
    let module = rt.block_on(environment.create_module(raw_module)).unwrap();

    c.bench_function("spawn process", |b| {
        b.to_async(&rt).iter(|| async {
            module.spawn("hello", Vec::new()).await.unwrap();
        });
    });

    // TODO: Plugin has a bug when run on modules without a table
    // let path = Path::new("target/wasm/stdlib.wasm");
    // let module = fs::read(path).unwrap();
    // let mut config = EnvConfig::default();
    // config.add_plugin(module).unwrap();
    // let environment = Environment::new(config).unwrap();

    // // Reload module into modified environment (added plugin)
    // let raw_module = std::fs::read("./target/wasm/hello.wasm").unwrap();
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
