# Rust bindings for Lunatic's stdlib

This library contains higher level wrappers for low level Lunatic syscalls.

Currently it requires nightly.

<!-- [Check out the docs!](#) -->

### Example

Create 100k processes and calculate the power of numbers then send the results back to the original process.

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
lunatic target/wasm32-wasi/release/<name>.wasm
```
