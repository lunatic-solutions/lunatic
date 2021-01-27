use criterion::{criterion_group, criterion_main, Criterion};
use lunatic_runtime::linker::LunaticLinker;
use lunatic_runtime::module::LunaticModule;
use lunatic_runtime::process::{DefaultApi, MemoryChoice};

fn lunatic_bench(c: &mut Criterion) {
    #[cfg(feature = "vm-wasmtime")]
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

    #[cfg(feature = "vm-wasmer")]
    c.bench_function("wasmer instance creation", |b| {
        let store = wasmer::Store::default();
        let wasm = include_bytes!("start.wasm");
        let module = wasmer::Module::new(&store, &wasm).unwrap();
        let import_object = wasmer::imports! {};

        b.iter(move || wasmer::Instance::new(&module, &import_object).unwrap());
    });

    c.bench_function("spawn thread", |b| {
        b.iter(move || {
            std::thread::spawn(|| 1 + 3);
        });
    });

    c.bench_function("lunatic instance creation", |b| {
        let wasm = include_bytes!("start.wasm");
        let module = LunaticModule::new(wasm.as_ref().into()).unwrap();

        b.iter(move || {
            let mut linker = LunaticLinker::new(module.clone(), 0, MemoryChoice::New).unwrap();
            linker.add_api(DefaultApi::new(None, module.clone()));
            criterion::black_box(linker.instance().unwrap())
        });
    });

    c.bench_function("lunatic multithreaded instance creation", |b| {
        use rayon::prelude::*;
        let wasm = include_bytes!("start.wasm");
        let module = LunaticModule::new(wasm.as_ref().into()).unwrap();

        b.iter_custom(move |iters| {
            let start = std::time::Instant::now();
            (0..iters).into_par_iter().for_each(|_i| {
                let mut linker = LunaticLinker::new(module.clone(), 0, MemoryChoice::New).unwrap();
                linker.add_api(DefaultApi::new(None, module.clone()));
                criterion::black_box(linker.instance().unwrap());
            });
            start.elapsed()
        });
    });
}

criterion_group!(benches, lunatic_bench);
criterion_main!(benches);
