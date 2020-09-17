use async_wormhole::{AsyncWormhole, AsyncYielder};
use anyhow::Result;
use wasmtime::*;

use std::mem::ManuallyDrop;

use tokio::runtime::Runtime;
use tokio::task::yield_now;

use lunatic::patching::patch;

fn main() -> Result<()> {
    // All wasm objects operate within the context of a "store"
    let store = Store::default();

    // Modules can be compiled through either the text or binary format
    let test: [u8; 0] = [];
    let test = patch(&test)?;

    let module = Module::new(store.engine(), test)?;

    let mut wasm = AsyncWormhole::new(move |yielder| {
        
        let yielder_ptr = &yielder as *const AsyncYielder<()> as usize;

        let yield_ = Func::wrap(&store, move || {
            let mut yielder = unsafe {
                std::ptr::read(yielder_ptr as *const ManuallyDrop<AsyncYielder<()>>)
            };
            
            println!("Yielded");
            yielder.async_suspend( yield_now() );
        });

        
        // Instantiation of a module requires specifying its imports and then
        // afterwards we can fetch exports by name, as well as asserting the
        // type signature of the function with `get0`.
        let instance = Instance::new(&store, &module, &[yield_.into()]).unwrap();
        let hello = instance
            .get_func("hello")
            .ok_or(anyhow::format_err!("failed to find `hello`  function export")).unwrap()
            .get0::<()>().unwrap();

        // And finally we can call the wasm as if it were a Rust function!
        let now = std::time::Instant::now();
        hello().unwrap();
        println!("{}", now.elapsed().as_millis());
    })?;

    wasm.preserve_tls(&wasmtime_runtime::traphandlers::tls::PTR);

    let mut rt = Runtime::new()?;
    rt.block_on(wasm);

    Ok(())
}
