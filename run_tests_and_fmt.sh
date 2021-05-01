#!/bin/sh

cargo fmt
cargo test --no-default-features --features vm-wasmtime

cd uptown_funk
cargo fmt
cargo test --no-default-features --features vm-wasmtime

cd uptown_funk_macro
cargo fmt
cargo test --no-default-features --features vm-wasmtime
