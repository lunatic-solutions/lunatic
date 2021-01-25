<div align="center">
    <img width="200" src="https://raw.githubusercontent.com/lunatic-solutions/lunatic/readme_update/assets/logo.png" alt="Lunatic logo">
</div>

Lunatic is a platform for building actor systems that use [WebAssembly](https://webassembly.org/) instances as actors. It is heavily inspired by Erlang and can be targeted from any language that can compile to WebAssembly. Currently there are only [bindings for Rust](https://crates.io/crates/lunatic) available.

[Read more about the motivation behind Lunatic.](https://kolobara.com/lunatic/index.html#motivation)

[Join our discord server!](https://discord.gg/b7zDqpXpB4)

### Example

To get a feeling for Lunatic let's look at a simple example in Rust:

```rust
use lunatic::{Channel, Process};

fn main() {
    let channel = Channel::new(0);
    let vec: Vec<i32> = (0..1_000).collect();

    for i in vec.iter() {
        Process::spawn((*i, vec.clone(), channel.clone()), child).unwrap();
    }

    for _ in vec.iter() {
        let (i, sum) = channel.receive();
        println!("Sum until {}: {}", i, sum);
    }
}

// Child process calculates the sum of numbers of context.1 until context.0 index.
fn child(context: (i32, Vec<i32>, Channel<(i32, i32)>)) {
    let i = context.0;
    let vec = context.1;
    let channel = context.2;
    let sum_until_i: i32 = vec[..=i as usize].iter().sum();
    channel.send((i, sum_until_i));
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

This app spawns `1000` child processes and calculates the sum of numbers from 0 to i in each child process,
then sends the result back to the parent process and prints it. If you wrote some Rust code before this should feel familiar. [Check out the docs for more examples.](https://docs.rs/lunatic/0.2.0/lunatic/)

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

### License

Licensed under either of

- Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.
