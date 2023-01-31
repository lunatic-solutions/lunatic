<div align="center">
    <a href="https://lunatic.solutions/" target="_blank">
        <img width="60" 
             src="https://raw.githubusercontent.com/lunatic-solutions/lunatic/main/assets/logo.svg"
             alt="lunatic logo"
        >
    </a>
    <p>&nbsp;</p>
</div>

Lunatic is a universal runtime for **fast**, **robust** and **scalable** server-side applications.
It's inspired by Erlang and can be used from any language that compiles to [WebAssembly][1].
You can read more about the motivation behind Lunatic [here][2].

We currently provide libraries to take full advantage of Lunatic's features for:

- [Rust][3]
- [AssemblyScript][11]

If you would like to see other languages supported or just follow the discussions around Lunatic,
[join our discord server][4].

## Supported features

- [x] Creating, cancelling & waiting on processes
- [x] Fine-grained process permissions
- [x] Process supervision
- [x] Channel based message passing
- [x] TCP networking
- [x] Filesystem access
- [x] Distributed nodes
- [ ] Hot reloading

## Installation

If you have rust (cargo) installed, you can build and install the lunatic runtime with:

```bash
cargo install lunatic-runtime
```

---

On **macOS** you can use [Homebrew][6] too:

```bash
brew tap lunatic-solutions/lunatic
brew install lunatic
```

---

We also provide pre-built binaries for **Windows**, **Linux** and **macOS** on the
[releases page][5], that you can include in your `PATH`.

---

And as always, you can also clone this repository and build it locally. The only dependency is
[a rust compiler][7]:

```bash
# Clone the repository
git clone https://github.com/lunatic-solutions/lunatic.git
# Jump into the cloned folder
cd lunatic
# Build and install lunatic
cargo install --path .
```

## Usage

After installation, you can use the `lunatic` binary to run WASM modules.

To learn how to build modules, check out language-specific bindings:

- [Rust](https://github.com/lunatic-solutions/rust-lib)
- [AssemblyScript](https://github.com/lunatic-solutions/as-lunatic)

## Architecture

Lunatic's design is all about spawning _super lightweight_ processes, also known as green threads or
[go-routines][8] in other runtimes. Lunatic's processes are fast to create, have a small memory footprint
and a low scheduling overhead. They are designed for **massive** concurrency. It's not uncommon to have
hundreds of thousands of such processes concurrently running in your app.

Some common use cases for processes are:

- HTTP request handling
- Long running requests, like WebSocket connections
- Long running background tasks, like email sending
- Calling untrusted libraries in an sandboxed environment

### Isolation

What makes the last use case possible are the sandboxing capabilities of [WebAssembly][1]. WebAssembly was
originally developed to run in the browser and provides extremely strong sandboxing on multiple levels.
Lunatic's processes inherit these properties.

Each process has its own stack, heap, and even syscalls. If one process fails, it will not affect the rest
of the system. This allows you to create very powerful and fault-tolerant abstraction.

This is also true for some other runtimes, but Lunatic goes one step further and makes it possible to use C
bindings directly in your app without any fear. If the C code contains any security vulnerabilities or crashes,
those issues will only affect the process currently executing the code. The only requirement is that the C
code can be compiled to WebAssembly.

It's possible to give per process fine-grained access to resources (filesystem, memory, network connections, ...).
This is enforced on the syscall level.

### Scheduling

All processes running on Lunatic are preemptively scheduled and executed by a [work stealing async executor][9]. This
gives you the freedom to write simple _blocking_ code, but the runtime is going to make sure it actually never blocks
a thread if waiting on I/O.

Even if you have an infinite loop somewhere in your code, the scheduling will always be fair and not permanently block
the execution thread. The best part is that you don't need to do anything special to achieve this, the runtime will take
care of it no matter which programming language you use.

### Compatibility

We intend to eventually make Lunatic completely compatible with [WASI][10]. Ideally, you could take existing code,
compile it to WebAssembly and run on top of Lunatic; creating the best developer experience possible. We're not
quite there yet.

## License

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
[9]: https://tokio.rs
[10]: https://wasi.dev/
[11]: https://github.com/lunatic-solutions/as-lunatic
