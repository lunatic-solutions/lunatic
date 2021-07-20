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
> Source: <https://en.wikipedia.org/wiki/Actor_model>

_Note: If you are looking to build actors in Rust and compile them to `lunatic` compatible
Wasm modules, checkout out the [lunatic crate](https://crates.io/crates/lunatic)_.

## Core Concepts

There are a number of core types and concepts that are important to be aware of when using
the `lunatic-runtime` crate:

* [`Environment`] - defines the characteristics of Processes that are spawned into it. An
  [`Environment`] is created with an [`EnvConfig`] to tweak various settings, like maximum
  memory and compute usage. An [`Environment`] can also contain `Plugins`.

* `Plugin` - raw binary data of a WebAssembly module that can export host functions for
  other modules.

* [`ProcessHandle`](process::ProcessHandle) - handle to send signals and messages to spawned
  processes.

## Plugins

TODO

## WebAssembly module requirements

TODO
*/

pub mod api;
mod config;
mod environment;
pub mod message;
pub mod plugin;
pub mod process;
pub mod state;

pub use config::EnvConfig;
pub use environment::Environment;
