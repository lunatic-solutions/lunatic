#!/usr/bin/env bash

# Compile hand written WAT file
wat2wasm start.wat --enable-all

# Create .wasm file using the Rust lunatic library
cd channel && cargo build --release --target=wasm32-wasi \
           && cp target/wasm32-wasi/release/channel.wasm ../
