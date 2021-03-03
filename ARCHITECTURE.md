# Architecture

Lunatic is a runtime for [WebAssembly][wasm] that runs Wasm modules as lightweight
processes with its own heap/stack and preemptively schedules them on a multi-threaded executor.
A running Wasm module can spawn new Lunatic processes using a guest library currently
available for [Rust][lunatic-rust] and [AssemblyScript][lunatic-as].
Scheduling is implemented by modifying a Wasm module and adding
"gas"/"reduction counts" (similar to [Erlang]).

Lunatic uses [Wasmtime] or [Wasmer] to JIT compile and run Wasm. Both support [Cranelift] for code generation and Wasmer also supports [LLVM].

[wasm]: https://webassembly.org/
[erlang]: https://www.erlang.org/
[lunatic-rust]: https://github.com/lunatic-solutions/rust-lib
[lunatic-as]: https://github.com/lunatic-solutions/as-lunatic
[wasmtime]: https://github.com/bytecodealliance/wasmtime
[wasmer]: https://github.com/wasmerio/wasmer
[cranelift]: https://github.com/bytecodealliance/wasmtime/tree/main/cranelift
[llvm]: https://llvm.org/

## Main components

 - `LunaticModule` represents [a Wasm module][wasm-module].
   It is created from a Wasm binary `&[u8]`. Before creating a Wasmtime/Wasmer module, it modifies
   it using [walrus] by adding reduction counts and other (see `module::normalisation`).
 - `HostFunctions` provide guest APIs for spawning processes, channels,
   WebAssembly System Interface ([WASI]), and networking (until WASI gets support for networking).
   An API implements `uptown_funk::HostFunctions` trait. This trait is usually implemented
   automatically using `uptown_funk_macro`.
 - `LunaticLinker` which abstracts Wasmtime/Wasmer linking.
   It takes a Lunatic module and an API (`impl HostFunctions`) and returns
   an ready to execute instance.
 - `Process` (found in `api::process` module) takes `LunaticModule` and `HostFunctions`, links them,
   creates an `AsyncWormhole` (see [AsyncWormhole crate]) and runs the Wasm instance inside it.
   AsyncWormhole implements the `Future` trait.
 
[wasm-module]: https://webassembly.github.io/spec/core/syntax/modules.html
[walrus]: https://github.com/rustwasm/walrus
[wasi]: https://github.com/WebAssembly/WASI
[asyncwormhole crate]: https://crates.io/crates/async-wormhole

## Guest APIs and Uptown Funk

## AsyncWormhole and Switcheroo
