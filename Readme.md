## Lunatic

> This project is still experimental and introduces breaking changes on a daily basis.
> Currently the functionality is rather limited. You will also need Rust Nightly to build it.

Lunatic is a platform for building actor systems that use [WebAssembly](https://webassembly.org/) instances as actors. It is heavily inspired by Erlang and can be targeted from any language that can compile to WebAssembly. Currently there are only [bindings for Rust](https://crates.io/crates/lunatic) available.

### Example

To get a feeling for Lunatic let's look at a simple example in Rust:

```rust
use lunatic::{Channel, Process};

fn main() {
    let channel = Channel::new(0);

    for i in 0..100_000 {
        let x = channel.clone();
        Process::spawn(move || {
            x.send((i, power(i)));
        })
        .unwrap();
    }

    for _ in 0..100_000 {
        let (i, power) = channel.receive();
        println!("Power of {} is {}", i, power);
    }
}

fn power(a: i32) -> i32 {
    a * a
}
```

Compile your app to a WebAssembly target:

```
cargo build --release --target=wasm32-wasi
```

and run it with

```
lunatic target/wasm32-wasi/release/<example>.wasm
```

This app spawns 100k processes (actors), does some calculation in them and prints out the result. If you wrote some Rust code before this should feel familiar. Similar to creating new threads with `thread::spawn`,`Process::spawn` also takes a closure, but in this case the closure can only capture `Copy` types. The reason for that is the child processes don't share any heap or stack memory with the parent one, making them completely sandboxed. The only way they can communicate is by sending messages to each other.

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

### Help wanted

Even though Lunatic is still early in development I spent a lot of time thinking about the problems in this space and feel like I'm on a good path. But without other contributors I can't get as far as I would like. If you know Rust and are interested in helping out drop me an email (me@kolobara.com) or reach out on [twitter](https://twitter.com/bkolobara).

### Future development

- [ ] Networking
- [ ] Filesystem access
- [ ] Permissions
- [ ] Process supervision
- [ ] Hot reloading
- [ ] Support for other WebAssembly languages
