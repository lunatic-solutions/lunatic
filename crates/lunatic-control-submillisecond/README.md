# Lunatic Control Server

Lunatic Control Server is an HTTP server designed for managing Lunatic nodes.
It is built with Submillisecond and compiles to WebAssembly to run with Lunatic.

### Running the Server Locally

Before running the server locally, you need to build the Lunatic runtime by running the following command:

```bash
cargo build --release
```

Next, follow the steps below to build and run the control server:

1. Navigate to the `./crates/lunatic-control-submillisecond` directory.
2. Build the control server using the following command:
   ```bash
   cargo build --target wasm32-wasi
   ```
3. Finally, run the control server using the Lunatic runtime by executing the following command:
   ```bash
   ../../target/release/lunatic ./target/wasm32-wasi/debug/lunatic-control-submillisecond.wasm
   ```
   Please note that the command above assumes that you are still in the `./crates/lunatic-control-submillisecond directory.`
   If you are in a different directory, you will need to adjust the relative paths accordingly.
