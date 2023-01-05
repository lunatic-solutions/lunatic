use std::future::Future;

use anyhow::Result;
use lunatic_common_api::IntoTrap;
use wasmtime::{Caller, Linker, Val};

// Register the trap APIs to the linker
pub fn register<T: Send + 'static>(linker: &mut Linker<T>) -> Result<()> {
    linker.func_wrap2_async("lunatic::trap", "catch", catch_trap::<T>)?;
    Ok(())
}

// Can be used as a trampoline to catch traps inside of guest by jumping
// through the host.
//
// WebAssembly doesn't have unwinding support, this means that traps can't
// be caught by just guest code. To work around that, this function can be
// used to jump back into the guest.
//
// If the guest code invoked by this function fails, it will return `0`,
// otherwise it will return whatever the guest export `_lunatic_catch_trap`
// returns.
//
// This function will expect a `_lunatic_catch_trap` function export. This
// export will get the parameters `function` and `pointer` forwarded to it.
//
// Traps:
// * If export `_lunatic_catch_trap` doesn't exist or is not a function.
fn catch_trap<T: Send>(
    mut caller: Caller<T>,
    function: i32,
    pointer: i32,
) -> Box<dyn Future<Output = Result<i32>> + Send + '_> {
    Box::new(async move {
        let lunatic_catch_trap = caller
            .get_export("_lunatic_catch_trap")
            .or_trap("lunatic::trap::catch: No export `_lunatic_catch_trap` defined in module")?
            .into_func()
            .or_trap("lunatic::trap::catch: Export `_lunatic_catch_trap` is not a function")?;

        let params = [Val::I32(function), Val::I32(pointer)];
        let mut result = [Val::I32(0)];
        let execution_result = lunatic_catch_trap
            .call_async(caller, &params, &mut result)
            .await;
        match execution_result {
            Ok(()) => Ok(result.get(0).unwrap().i32().unwrap()),
            Err(_) => Ok(0),
        }
    })
}
