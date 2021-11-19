#!/bin/sh
SCRIPT=$(readlink -f "$0")
BASEDIR=$(dirname "$SCRIPT")

cargo build --release

PATH="$BASEDIR/target/release:$PATH"
echo "Using lunatic: $(which lunatic)"

cd $BASEDIR/rust-lib
cargo build --release
cargo test
