# Examples

### Counter Plugin

The counter plugin shows an example of adding host functions to the lunatic VM which can be used by guest code.

To run the example, first build the plugin

```bash
cargo build --release -p counter-plugin
```

And then the guest wasm

```bash
cargo build --release --target wasm32-wasi -p counter-guest
```

And finally run with the plugin

```bash
lunatic --plugins ./target/release/libcounter_plugin.dylib ./target/wasm32-wasi/release/counter-guest.wasm
```
