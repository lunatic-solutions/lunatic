use async_wormhole::{AsyncWormhole, AsyncYielder};
use anyhow::Result;
use wasmtime::*;
use std::mem::ManuallyDrop;

use smol::Timer;
use std::time::{Duration, Instant};

use tokio::prelude::*;
use tokio::runtime::Runtime;
use tokio::time::delay_for;

fn main() -> Result<()> {
    // All wasm objects operate within the context of a "store"
    let store = Store::default();

    // Modules can be compiled through either the text or binary format
    let wat = r#"
        (module
            (import "" "" (func $minus_42 (param i32)))

            (func (export "hello")
                i32.const 45
                call $minus_42)
        )
    "#;
    let module = Module::new(store.engine(), wat)?;

    let mut wasm = AsyncWormhole::new(move |yielder| {
        
        let yielder_ptr = &yielder as *const AsyncYielder<()> as usize;

        let minus_42 = Func::wrap(&store, move |_param: i32| {
            let mut yielder = unsafe {
                std::ptr::read(yielder_ptr as *const ManuallyDrop<AsyncYielder<()>>)
            };
            println!("Now wait");
            let now = Instant::now();
            yielder.async_suspend( delay_for(Duration::from_secs(5)) );
            println!("{}", now.elapsed().as_nanos());
        });

        
        // Instantiation of a module requires specifying its imports and then
        // afterwards we can fetch exports by name, as well as asserting the
        // type signature of the function with `get0`.
        let instance = Instance::new(&store, &module, &[minus_42.into()]).unwrap();
        let hello = instance
            .get_func("hello")
            .ok_or(anyhow::format_err!("failed to find `hello`  function export")).unwrap()
            .get0::<()>().unwrap();

        // And finally we can call the wasm as if it were a Rust function!
        hello().unwrap();
    })?;

    wasm.preserve_tls(&wasmtime_runtime::traphandlers::tls::PTR);

    let mut rt = Runtime::new()?;
    rt.block_on(wasm);

    Ok(())
}
