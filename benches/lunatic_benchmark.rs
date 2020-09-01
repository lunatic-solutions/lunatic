use criterion::{criterion_group, criterion_main, Criterion};
use wasmer::{Store, Module, Instance, Value, imports};
// use wasmtime::*;

fn lunatic_bench(c: &mut Criterion) {
    c.bench_function("wasmer instance creation", |b| {
        let store = Store::default();

        // Modules can be compiled through either the text or binary format
        let wasm = include_bytes!("start.wasm");
        let module = Module::new(&store, &wasm).unwrap();
        let import_object = imports! {};

        b.iter(move || {
            let handle = Instance::new(&module, &import_object);
        });
    });
}

criterion_group!(benches, lunatic_bench);
criterion_main!(benches);
