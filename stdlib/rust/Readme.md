# Rust bindings for Lunatic's stdlib

This library contains higher level wrappers for low level Lunatic syscalls.

[Check out the docs!](https://docs.rs/lunatic/0.2.0/lunatic/)

### Example

Create 1_000 child processes and calculate the sum of numbers from 0 to i in each child process,
then send the result back to the parent process and print it.

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
lunatic target/wasm32-wasi/release/<name>.wasm
```
