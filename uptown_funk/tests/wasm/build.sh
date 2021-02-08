#!/usr/bin/env bash
rustc add.rs -C link-args=--import-memory --target=wasm32-unknown-unknown --crate-type=cdylib -C opt-level=3
rustc multivalue.rs -C link-args=--import-memory --target=wasm32-unknown-unknown --crate-type=cdylib -C opt-level=3
rustc ref_str.rs -C link-args=--import-memory --target=wasm32-unknown-unknown --crate-type=cdylib -C opt-level=3
rustc ioslices.rs -C link-args=--import-memory --target=wasm32-unknown-unknown --crate-type=cdylib -C opt-level=3
rustc custom_types.rs -C link-args=--import-memory --target=wasm32-unknown-unknown --crate-type=cdylib -C opt-level=3
rustc custom_types_return.rs -C link-args=--import-memory --target=wasm32-unknown-unknown --crate-type=cdylib -C opt-level=3
rustc mutable_state.rs -C link-args=--import-memory --target=wasm32-unknown-unknown --crate-type=cdylib -C opt-level=3
rustc trap.rs -C link-args=--import-memory --target=wasm32-unknown-unknown --crate-type=cdylib -C opt-level=3
rustc pointer.rs -C link-args=--import-memory --target=wasm32-unknown-unknown --crate-type=cdylib -C opt-level=3
rustc multiple_namespaces.rs -C link-args=--import-memory --target=wasm32-unknown-unknown --crate-type=cdylib -C opt-level=3
