#!/bin/sh

cargo fmt
cargo test

cd uptown_funk
cargo fmt
cargo test

cd uptown_funk_macro
cargo fmt
cargo test
