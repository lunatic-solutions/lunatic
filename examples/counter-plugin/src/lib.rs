use anyhow::Result;
use lunatic_plugin::{DefaultProcessState, LoadState, Plugin};
use wasmtime::{Caller, Linker, Trap};

#[derive(Debug)]
struct CounterPlugin {
    count: i32,
}

impl Plugin for CounterPlugin {
    fn init() -> Self {
        CounterPlugin { count: 0 }
    }

    fn register(linker: &mut Linker<DefaultProcessState>) -> Result<()> {
        linker.func_wrap("lunatic::counter", "increment", increment)?;
        linker.func_wrap("lunatic::counter", "decrement", decrement)?;
        linker.func_wrap("lunatic::counter", "count", count)?;
        Ok(())
    }
}

lunatic_plugin::register_plugin!(CounterPlugin);

fn increment(mut caller: Caller<DefaultProcessState>, amount: i32) -> Result<i32, Trap> {
    let state = caller.data_mut().load_state_mut::<CounterPlugin>().unwrap();
    state.count += amount;
    Ok(state.count)
}

fn decrement(mut caller: Caller<DefaultProcessState>, amount: i32) -> Result<i32, Trap> {
    let state = caller.data_mut().load_state_mut::<CounterPlugin>().unwrap();
    state.count -= amount;
    Ok(state.count)
}

fn count(caller: Caller<DefaultProcessState>) -> Result<i32, Trap> {
    let state = caller.data().load_state::<CounterPlugin>().unwrap();
    Ok(state.count)
}
