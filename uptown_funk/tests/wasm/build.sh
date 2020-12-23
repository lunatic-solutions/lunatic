#!/usr/bin/env bash
rustc add.rs --target=wasm32-unknown-unknown --crate-type=cdylib -C opt-level=3
rustc multivalue.rs --target=wasm32-unknown-unknown --crate-type=cdylib -C opt-level=3
rustc ref_str.rs --target=wasm32-unknown-unknown --crate-type=cdylib -C opt-level=3
rustc ioslices.rs --target=wasm32-unknown-unknown --crate-type=cdylib -C opt-level=3
rustc custom_types.rs --target=wasm32-unknown-unknown --crate-type=cdylib -C opt-level=3
rustc custom_types_return.rs --target=wasm32-unknown-unknown --crate-type=cdylib -C opt-level=3
rustc custom_types_ref.rs --target=wasm32-unknown-unknown --crate-type=cdylib -C opt-level=3