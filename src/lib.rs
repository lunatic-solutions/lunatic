/*!
The [lunatic vm](https://lunatic.solutions/) is a system for creating actors from WebAssembly
modules. This `lunatic-runtime` library allows you to embed the `lunatic vm` inside your Rust
code.

> _The actor model in computer science is a mathematical model of concurrent computation that
treats actor as the universal primitive of concurrent computation. In response to a message it
receives, an actor can: make local decisions, create more actors, send more messages, and
determine how to respond to the next message received. Actors may modify their own private
state, but can only affect each other indirectly through messaging (removing the need for
lock-based synchronization)._
>
> Source: <https://en.wikipedia.org/wiki/Actor_model>

_**Note:** If you are looking to build actors in Rust and compile them to `lunatic` compatible
Wasm modules, checkout out the [lunatic crate](https://crates.io/crates/lunatic)_.

## Core Concepts

* [`Environment`] - defines the characteristics of Processes that are spawned into it. An
  [`Environment`] is created with an [`EnvConfig`] to tweak various settings, like maximum
  memory and compute usage.

* [`WasmProcess`](process::WasmProcess) - a handle to send signals and messages to spawned
  Wasm processes. It implements the [`Process`](process::Process) trait.

## Plugins

TODO

## WebAssembly module requirements

TODO
*/

pub(crate) mod api;
pub mod async_map;
mod config;
mod environment;
pub mod message;
pub(crate) mod module;
pub mod node;
pub mod registry;
pub(crate) mod state;

use async_std::sync::RwLock;
use lazy_static::lazy_static;

pub use config::EnvConfig;
pub use environment::Environment;
pub use lunatic_process::{spawn, Finished, Process, Signal, WasmProcess};
use node::Node;

// Global node singleton.
lazy_static! {
    pub static ref NODE: RwLock<Option<Node>> = RwLock::new(None);
}
