use criterion::{criterion_group, criterion_main, Criterion};
use wasmtime::{Engine, Linker, Module, Store};

fn lunatic_bench(c: &mut Criterion) {
    c.bench_function("wasmtime instance creation", |b| {
        let engine = Engine::default();
        let wasm = include_bytes!("start.wasm");
        let module = Module::new(&engine, &wasm).unwrap();

        b.iter(move || {
            let store = Store::new(&engine);
            let linker = Linker::new(&store);
            let _instance = linker.instantiate(&module);
            store
        });
    });

    c.bench_function("spawn thread", |b| {
        b.iter(move || {
            std::thread::spawn(|| 1 + 3);
        });
    });
}

criterion_group!(benches, lunatic_bench);
criterion_main!(benches);
