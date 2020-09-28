use criterion::{criterion_group, criterion_main, Criterion};
use wasmer::{Store, Module, Instance, imports};
// use wasmtime::*;

fn lunatic_bench(c: &mut Criterion) {
    c.bench_function("wasmer instance creation", |b| {
        let store = Store::default();

        // Modules can be compiled through either the text or binary format
        let wasm = include_bytes!("start.wasm");
        let module = Module::new(&store, &wasm).unwrap();

        b.iter(move || {
            let import_object = imports! {};
            Instance::new(&module, &import_object)
        });
    });

    c.bench_function("wasmtime instance creation", |b| {
        let engine = wasmtime::Engine::default();
        let wasm = include_bytes!("start.wasm");
        let module = wasmtime::Module::new(&engine, &wasm).unwrap();

        b.iter(move || {
            let store = wasmtime::Store::new(&engine);
            let linker = wasmtime::Linker::new(&store);
            let _instance = linker.instantiate(&module);
            store
        });
    });

    c.bench_function("spawn thread", |b| {
        b.iter(move || {
            std::thread::spawn(|| {
                1 + 3
            });
        });
    });
}

criterion_group!(benches, lunatic_bench);
criterion_main!(benches);
