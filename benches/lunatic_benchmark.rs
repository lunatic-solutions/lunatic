use criterion::{criterion_group, criterion_main, Criterion};
use wasmtime::*;

fn lunatic_bench(c: &mut Criterion) {
    c.bench_function("wasmtime instance creation", |b| {
        let store = Store::default();

        // Modules can be compiled through either the text or binary format
        let wasm = include_bytes!("start.wasm");
        let module = Module::new(store.engine(), wasm).unwrap();

        b.iter(move || {
            let handle = Instance::new(&store, &module, &[]).unwrap();
        });
    });
}

criterion_group!(benches, lunatic_bench);
criterion_main!(benches);
