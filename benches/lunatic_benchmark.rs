use criterion::{criterion_group, criterion_main, Criterion};
use lunatic_vm::linker::{engine, LunaticLinker};
use lunatic_vm::process::MemoryChoice;
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

    c.bench_function("lunatic instance creation", |b| {
        let engine = engine();
        let module = Module::new(&engine, "(module)").unwrap();

        b.iter(move || {
            let linker =
                LunaticLinker::new(engine.clone(), module.clone(), 0, MemoryChoice::New(32))
                    .unwrap();
            linker.instance().unwrap()
        });
    });

    c.bench_function("lunatic instance creation multithreaded", |b| {
        use rayon::prelude::*;

        let engine = engine();
        let module = Module::new(&engine, "(module)").unwrap();

        b.iter_custom(move |iters| {
            let start = std::time::Instant::now();
            (0..iters).into_par_iter().for_each(|_i| {
                let linker =
                    LunaticLinker::new(engine.clone(), module.clone(), 0, MemoryChoice::New(32))
                        .unwrap();
                criterion::black_box(linker.instance().unwrap());
            });
            start.elapsed()
        });
    });
}

criterion_group!(benches, lunatic_bench);
criterion_main!(benches);
