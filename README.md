<div align="center">
    <a href="#">
        <img width="150" src="https://raw.githubusercontent.com/lunatic-solutions/lunatic/readme_update/assets/logo.png" alt="Lunatic logo">
    </a>
    <p>&nbsp;</p>
</div>

Lunatic is a universal runtime for **fast**, **robust** and **scalable** server-side applications.
It's inspired by Erlang and can be used from any language that compiles to [WebAssembly][1].
You can read more about the motivation behind Lunatic [here][2].

We currently support the following languages:

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
- [/] Filesystem access (partial)
- [ ] Hot reloading

## Installation

We provide pre-built binaries on the [releases page][5].

---

On MacOs you can also use [Hombrew][6]:

```bash
brew install ...
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
# Build and install lunatic
cargo +nightly install --path .
```

## Architecture

As mentioned earlier, Lunatic treats WebAssembly instances as actors. Using WebAssembly in this
context allows us to have per actor sandboxing and removes some of the most common drawbacks
in other actor implementations.

### Isolation

The failure of one process can't affect other processes running. This is also true for other actor implementations, but Lunatic goes one step further and makes it possible to use C bindings directly in your app without any fear. If the C code contains any security vulnerabilities or crashes those issues will only affect the process currently executing this code.

When spawning a process it is possible to give precise access to resources (filesystem, memory, network connections, ...). This is enforced on a syscall level. So even if you use a huge C library in your code, you don't need to read through the whole code vetting the library. You can express the permissions on the process level.

### Scheduling

Lunatic processes are intended to be really lightweight, therefore it should be possible to concurrently run millions of processes. The scheduling should always be fair. Even if you have an infinite loop somewhere in your code it will not permanently block the thread from executing other processes. And the best part is that you don't need to do anything special to achieve this.

### Compatibility

I intend to eventually make Lunatic completely compatible with [WASI](https://wasi.dev/). Ideally you could just take existing code, compile it to WebAssembly and run on top of Lunatic. Giving the best developer experience possible.

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
