<div align="center">
    <a href="#">
        <img width="150" src="https://raw.githubusercontent.com/lunatic-solutions/lunatic/readme_update/assets/logo.png" alt="Lunatic logo">
    </a>
    <p>&nbsp;</p>
</div>

Lunatic is a universal runtime for **fast**, **robust** and **scalable** server-side applications.
It's inspired by Erlang and can be used from any language that compiles to [WebAssembly][1].
You can read more about the motivation behind Lunatic [here][2].

We currently provide libraries to take full advantage of Lunatic's features for:

- [Rust][3]
- ~AssemblyScript~ (coming soon)

If you would like to see other languages supported or just follow the discussions around Lunatic,
[join our discord server][4].

## Supported features

- [x] Creating, cancelling & waiting on processes
- [ ] Fine-grained process permissions
- [ ] Process supervision
- [x] Channel based message passing
- [x] TCP networking
- [x] Filesystem access (partial)
- [ ] Hot reloading

## Installation

We provide pre-built binaries for **Windows**, **Linux** and **MacOs** on the [releases page][5].

---

On **MacOs** you can also use [Hombrew][6]:

```bash
TODO
```

---

To build the project from source yuo will need to have [rustup][7] installed:

```bash
# Install Rust Nightly
rustup add nightly
# Clone the repository and all submodules
git clone --recurse-submodules https://github.com/lunatic-solutions/lunatic.git
# Jump into the cloned folder
cd lunatic
# Build and install Lunatic
cargo +nightly install --path .
```

## Architecture

Lunatic's design is all about spawning _super lightweight_ processes, also known as green threads or
[go-routines][8] in other runtimes. Lunatic's processes are fast to create, have a small memory footprint
and a low scheduling overhead. They are designed for **MASSIVE** concurrency. It's not uncommon to have
hundreds of thousands of such processes concurrently running in your app.

Some common use cases for processes are:

- HTTP request handling
- Long running background tasks like email sending
- Calling untrusted libraries in an isolated environment

### Isolation

What makes the last use case possible are the sandboxing capabilities of [WebAssembly][1]. WebAssebmly was
originally developed to run in the browser and provides extremely strong sandboxing on multiple levels.
Lunatic's processes inherit this properties.

Lunatic's processes are completely isolated from each other, they have their own stack, heap and even syscalls.
If one process fails it will not affect the rest of the system. This allows you to create very powerful and
fault-tolerant abstraction.

This is also true for some other runtimes, but Lunatic goes one step further and makes it possible to use C
bindings directly in your app without any fear. If the C code contains any security vulnerabilities or crashes
those issues will only affect the process currently executing this code.

When spawning a process it is possible to give precise access to resources (filesystem, memory, network connections, ...).
This is enforced on a syscall level. So even if you use a big C library in your code, you don't need to read through the
whole code vetting the library. You can express the permissions on the process level.

### Scheduling

All processes running on Lunatic are preemptively scheduled and executed by a [work stealing async executor][9]. This
gives you the freedom to write simple _blocking_ code, but the runtime is going to make sure it actually never blocks
a thread if waiting on I/O.

Even if you have an infinite loop somewhere in your code, the scheduling will always be fair and will not permanently block
the execution thread. The best part is that you don't need to do anything special to achieve this, the runtime will take
care of it no matter which programming language you use.

### Compatibility

We intend to eventually make Lunatic completely compatible with [WASI][10]. Ideally you could just take existing code, compile it to WebAssembly and run on top of Lunatic. Giving the best developer experience possible.

### License

Licensed under either of

- Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

[1]: https://webassembly.org/
[2]: https://kolobara.com/lunatic/index.html#motivation
[3]: https://crates.io/crates/lunatic
[4]: https://discord.gg/b7zDqpXpB4
[5]: https://github.com/lunatic-solutions/lunatic/releases
[6]: https://brew.sh/
[7]: https://rustup.rs/
[8]: https://golangbot.com/goroutines
[9]: https://docs.rs/smol
[10]: https://wasi.dev/
