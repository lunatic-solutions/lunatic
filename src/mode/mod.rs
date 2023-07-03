//! Depending on the environment that the `lunatic` binary is invoked from, it may behave
//! differently. All the different modes of working are defined in this module.

// If invoked as part of a `cargo test` command.
pub(crate) mod cargo_test;
// Default mode, if no other mode could be detected.
pub(crate) mod execution;

mod api;
mod app;
mod auth;
mod common;
mod config;
mod control;
// mod deploy;
mod init;
mod node;
mod run;
