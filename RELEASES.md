# Lunatic Releases

---

## v0.3

Released 2021-01-26.

### Changes

#### 1. Created [uptown_funk](https://www.youtube.com/watch?v=OPf0YbXqDm0) (this link leads to a YouTube music video)

This is by far the biggest change in this release and one that took up most of the development time.
[uptown_funk](https://crates.io/crates/uptown_funk) is a crate that lets you elegantly define Wasm
host functions that are compatible with both [Wasmtime](https://github.com/bytecodealliance/wasmtime)
and [Wasmer](https://github.com/wasmerio/wasmer) runtimes.

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
to receive higher level Rust type (e.g. `&mut [IoSliceMut<'_>]`) and the macro is going to create appropriate
wrappers for us. And of course, it correctly works with `async` functions on Lunatic.

This was an important step forward to make Lunatic runtime agnostic. Currently we support bot Wasmer and Wasmtime,
but if we wanted, we could add support for another runtime in the future by just adding support to `uptown_funk`.

Sadly, `uptown_funk` doesn't have any documentation yet and is not that useful to other projects. But I intend to
invest more time into this in the future.

#### 2. Fixed Process canceling cleanup

This issue needs a bit context. All Lunatic processes are executed on a separate stack and if they are waiting
for some I/O they will be moved off the execution thread. Now, you can decide while you are waiting on something
just to cancel this process. Until now this would free the memory region belonging to the stack/heap and finish.
However, it can happen that the separate stack contains pointers to resources held by the runtime (channels, other
processes, etc.). Their `drop()` methods would never have been called in this case and the resources would have
been leaked.

This required [a fix](https://github.com/bkolobara/async-wormhole/commit/be7a91ba621c41b49bc834d49479f51c4487cc47)
in the [async-wormhole](https://github.com/bkolobara/async-wormhole) crate. Now, every time when a generator is
dropped and the closure didn't yet finish running, a stack unwind will be triggered on the separate stack. This
will clean up all the resources left behind.

#### 3. Updated Rust's library

The [Lunatic Rust library](https://crates.io/crates/lunatic) allows you to write Rust applications that can take
complete advantage of Lunatic's features, not to embed the Lunatic runtime in your Rust application (coming soon).
The Rust library has seen almost a complete rewrite and takes much better advantage of Rust's ownership model now.
Especially when sending host resources between processes.

#### 4. Added initial WASI filesystem support

This is still a WIP area, but the basic functionality is there for opening files, reading directories, etc.

#### 5. Added TCP support

A few APIs are still missing, but we have enough to create a TCP listener/client.

#### 6. Miscellaneous fixes

There are too many other small fixes and additions to mention here, but Lunatic is much more stable now than just
2 months ago and I have removed the experimental warning in the Readme :)
