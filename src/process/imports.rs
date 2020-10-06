use super::creator::{spawn, FunctionLookup};
use super::ProcessEnvironment;

use tokio::task::yield_now;
use wasmtime::Linker;

/// Add all imports provided by the Lunatic runtime to this instance.
pub fn create_lunatic_imports(linker: &mut Linker, environment: ProcessEnvironment) {
    // Lunatic stdlib

    linker.define("lunatic", "memory", environment.memory());

    // Yield this process allowing other to be scheduled on same thread.
    let env = environment.clone();
    linker.func("lunatic", "yield", move || env.async_(yield_now()));

    // Spawn new process and call a fuction from the function table under the `index` and pass one i32 argument.
    let env = environment.clone();
    linker.func(
        "lunatic",
        "spawn",
        move |index: i32, argument: i32| -> i32 {
            spawn(
                env.engine.clone(),
                env.module.clone(),
                FunctionLookup::TableIndex((index, argument)),
                None,
            );
            0
        },
    );

    // Create a buffer and send it to the process with the `pid`
    linker.func(
        "lunatic",
        "send",
        |pid: i32, buffer: i32, len: i32| -> i32 { 0 },
    );

    // Receive buffer
    linker.func("lunatic", "receive", |buffer: i32, len: i32| -> i32 { 0 });
}
