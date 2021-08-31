# Lunatic Releases

---

## v0.6.0

Released 2021-08-31.

### Changes

This release contains mostly internal changes that should improve the developer experience of
people working on the VM, but also adds some cool new features.

#### VM

- Processes now have a more general abstraction, the [`Process`][0] trait. It allows us to treat
  anything that can receive a "message" as a process. At the moment this can only be a WebAssembly
  process or [native Rust closures][1], but it could be extended in the future with other resources
  that act as processes.

- Tags were added to messages, allowing for selective receives. A common use case for them is to
  make a request to a process and ignore all other messages until the response arrives. This can
  be now done by giving the request message a specific tag (`i64` value) and waiting for a response
  on that tag with [`lunatic::message::receive`][2]. The `receive` function will first search the
  existing mailbox for the first message matching the tag or block until a message with the
  specified tag arrives. If we know that such a tag can't yet exist in the mailbox, we can use the
  atomic send and receive operation ([`send_receive_skip_search`][3]) that will not look through
  the mailbox.

- Messages are now just a [special kind of signals][4] that a process can receive. Other signals
  are `Kill`, `Link`, `Unlink`, ...

- A [test][5] was added for catching signature changes of host functions.

- The messaging API was extended, including functions [`write_data`][6] and [`read_data`][7] that
  allow for streaming zero-copy message de/serialization.

- The `Environment` was extended with a concept of a `registry` and 3 host functions:
  [`register`][8], [`unregister`][9] and [`lookup`][10]. Processes can now be registered inside the
  `Environment` under a well known name and version number. When looking up processes inside the
  `Environment` with a query, the lookup will follow semantic versioning rules for the version.
  If we have a process under the name "test" and version "1.2.3", a lookup query with the name
  "test" and version "^1.2" will match it.

- Fixed [an issue][11] around async Rust cancellation safety and receives with timeouts.

- [Improved handling][12] of command line arguments and environment variables.

#### Rust library

- The `Message` trait was removed and we now solely rely on serde's `Serialize` & `Deserialize`
  traits to define what can be a message. Originally I was thinking that this is going to be an
  issue once we get support for Rust's native `TcpStream` and we can't define serde's traits for
  it, but this can be solved with [remote derives][13] in the future. This removes a really big
  and complex macro from the library and allows us to use the new [`write_data`][6] and
  [`read_data`][7] host functions for zero-copy de/serialization.

- [MessagePack][14] is now used as the default message serialization format.

- A [request/replay][15] API was added, that was built on the new selective receive functionality.

- The [`Environment`][16] struct was extended with the new `registry` functionality.

- New `lunatic::main` & `lunatic::test` macros were added to improve developer experiences.

- [`lunatic::process::this_env`][17] was added to get the environment that the process was spawned
  in.

[0]: https://github.com/lunatic-solutions/lunatic/blob/main/src/process.rs#L21
[1]: https://github.com/lunatic-solutions/lunatic/blob/main/src/process.rs#L195
[2]: https://github.com/lunatic-solutions/lunatic/blob/main/src/api/mailbox.rs#L526
[3]: https://github.com/lunatic-solutions/lunatic/blob/main/src/api/mailbox.rs#L474
[4]: https://github.com/lunatic-solutions/lunatic/blob/main/src/process.rs#L48
[5]: https://github.com/lunatic-solutions/lunatic/blob/main/wat/all_imports.wat
[6]: https://github.com/lunatic-solutions/lunatic/blob/main/src/api/mailbox.rs#L216
[7]: https://github.com/lunatic-solutions/lunatic/blob/main/src/api/mailbox.rs#L246
[8]: https://github.com/lunatic-solutions/lunatic/blob/main/src/api/process.rs#L886
[9]: https://github.com/lunatic-solutions/lunatic/blob/main/src/api/process.rs#L946
[10]: https://github.com/lunatic-solutions/lunatic/blob/main/src/api/process.rs#L1019
[11]: https://github.com/lunatic-solutions/lunatic/commit/a7188fed4b88484a9eb3874a082e1a0a163e916b
[12]: https://github.com/lunatic-solutions/lunatic/commit/0c693985265ea00d7537e9cb62ec9b9390599915
[13]: https://serde.rs/remote-derive.html
[14]: https://msgpack.org/index.html
[15]: https://docs.rs/lunatic/0.6.0/lunatic/index.html#requestreplay-architecture
[16]: https://docs.rs/lunatic/0.6.0/lunatic/struct.Environment.html#
[17]: https://docs.rs/lunatic/0.6.0/lunatic/process/fn.this_env.html

## v0.5.0

Released 2021-07-29.

### Changes

Lunatic was completely re-written from scratch. Now it's built on top of
[Wasmtime's](https://github.com/bytecodealliance/wasmtime) `async` support and doesn't contain any
**unsafe** code.

The architecture of the runtime was changed to closer mirror Erlang's features. Processes now only
contain **one mailbox** and the channels API was removed. Processes can also be linked together to
propagate failure, so that [supervisor](https://erlang.org/doc/man/supervisor.html) like tools can
be built on top of them.

#### Environments

Environments allow you to specify some characteristics of the execution, like how much memory or
CPU processes can use. They can also define host function namespaces that are allowed to be called.
Processes that are spawned into an environment inherit theses characteristics, allowing you to
dynamically create execution contexts for new processes.

#### Dynamic module loading

WebAssembly modules can be loaded from other WebAssembly modules during runtime. Combined together
with `Environments` this can be used to load untrusted code and run it inside a sandbox.

#### Libraries

The Rust library was also completely re-written to support the new abstractions.
[Check out the new docs!](https://docs.rs/lunatic/0.5.0/lunatic/)

The AssemblyScript library is still WIP and doesn't support the new features yet.

---

## v0.3.1

Released 2021-02-22.

### Changes

Miscellaneous bug fixes and stability improvements.

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
