uptown_funk is a crate that lets you elegantly define Wasm host functions that are compatible with both
[Wasmtime](https://github.com/bytecodealliance/wasmtime) and [Wasmer](https://github.com/wasmerio/wasmer)
runtimes.

It consists of a macro and a few structs/traits that let you translate from Wasm primitive types to higher
level Rust types, and back.

Lets look at an example of a host function definition using `uptown_funk`:

```rust
#[host_functions(namespace = "wasi_snapshot_preview1")]
impl WasiState {
    async fn fd_pread(&mut self, fd: u32, iovs: &mut [IoSliceMut<'_>], offset: Filesize) -> (Status, u32) {
        // ...
    }

    // .. Other functions depending on the WasiState struct.
}
```

The `host_function` macro lets us grab any host side struct and use it as state for the Wasm instances.
Instead of dealing with low level pointers + lengths passed from the WebAssembly guests, we can _pretend_
to receive higher level Rust type (e.g. `&mut [IoSliceMut<'_>]`) and the macro is going to create
appropriate wrappers for us. And of course, it correctly works with `async` functions on Lunatic.
