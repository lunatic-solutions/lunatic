use lunatic_common_api::{get_memory, IntoTrap};
use metrics::{counter, decrement_gauge, gauge, histogram, increment_counter, increment_gauge};
use wasmtime::{Caller, Linker, Trap};

/// Links the [Metrics](https://crates.io/crates/metrics) APIs
pub fn register<T: 'static>(linker: &mut Linker<T>) -> anyhow::Result<()> {
    linker.func_wrap("lunatic::metrics", "counter", counter)?;
    linker.func_wrap("lunatic::metrics", "increment_counter", increment_counter)?;
    linker.func_wrap("lunatic::metrics", "gauge", gauge)?;
    linker.func_wrap("lunatic::metrics", "increment_gauge", increment_gauge)?;
    linker.func_wrap("lunatic::metrics", "decrement_gauge", decrement_gauge)?;
    linker.func_wrap("lunatic::metrics", "histogram", histogram)?;
    Ok(())
}

fn get_string_arg<T>(
    caller: &mut Caller<T>,
    name_str_ptr: u32,
    name_str_len: u32,
    func_name: &str,
) -> Result<String, Trap> {
    let memory = get_memory(caller)?;
    let memory_slice = memory.data(caller);
    let name = memory_slice
        .get(name_str_ptr as usize..(name_str_ptr + name_str_len) as usize)
        .or_trap(func_name)?;
    let name = String::from_utf8(name.to_vec()).or_trap(func_name)?;
    Ok(name)
}

/// Sets a counter.
///
/// Traps:
/// * If the name is not a valid utf8 string.
/// * If any memory outside the guest heap space is referenced.
fn counter<T>(
    mut caller: Caller<'_, T>,
    name_str_ptr: u32,
    name_str_len: u32,
    value: u64,
) -> Result<(), Trap> {
    let name = get_string_arg(
        &mut caller,
        name_str_ptr,
        name_str_len,
        "lunatic::metrics::counter",
    )?;

    counter!(name, value);
    Ok(())
}

/// Increments a counter.
///
/// Traps:
/// * If the name is not a valid utf8 string.
/// * If any memory outside the guest heap space is referenced.
fn increment_counter<T>(
    mut caller: Caller<'_, T>,
    name_str_ptr: u32,
    name_str_len: u32,
) -> Result<(), Trap> {
    let name = get_string_arg(
        &mut caller,
        name_str_ptr,
        name_str_len,
        "lunatic::metrics::increment_counter",
    )?;

    increment_counter!(name);
    Ok(())
}

/// Sets a gauge.
///
/// Traps:
/// * If the name is not a valid utf8 string.
/// * If any memory outside the guest heap space is referenced.
fn gauge<T>(
    mut caller: Caller<'_, T>,
    name_str_ptr: u32,
    name_str_len: u32,
    value: f64,
) -> Result<(), Trap> {
    let name = get_string_arg(
        &mut caller,
        name_str_ptr,
        name_str_len,
        "lunatic::metrics::increment_counter",
    )?;

    gauge!(name, value);
    Ok(())
}

/// Increments a gauge.
///
/// Traps:
/// * If the name is not a valid utf8 string.
/// * If any memory outside the guest heap space is referenced.
fn increment_gauge<T>(
    mut caller: Caller<'_, T>,
    name_str_ptr: u32,
    name_str_len: u32,
    value: f64,
) -> Result<(), Trap> {
    let name = get_string_arg(
        &mut caller,
        name_str_ptr,
        name_str_len,
        "lunatic::metrics::increment_gauge",
    )?;

    increment_gauge!(name, value);
    Ok(())
}

/// Decrements a gauge.
///
/// Traps:
/// * If the name is not a valid utf8 string.
/// * If any memory outside the guest heap space is referenced.
fn decrement_gauge<T>(
    mut caller: Caller<'_, T>,
    name_str_ptr: u32,
    name_str_len: u32,
    value: f64,
) -> Result<(), Trap> {
    let name = get_string_arg(
        &mut caller,
        name_str_ptr,
        name_str_len,
        "lunatic::metrics::decrement_gauge",
    )?;

    decrement_gauge!(name, value);
    Ok(())
}

/// Sets a histogram.
///
/// Traps:
/// * If the name is not a valid utf8 string.
/// * If any memory outside the guest heap space is referenced.
fn histogram<T>(
    mut caller: Caller<'_, T>,
    name_str_ptr: u32,
    name_str_len: u32,
    value: f64,
) -> Result<(), Trap> {
    let name = get_string_arg(
        &mut caller,
        name_str_ptr,
        name_str_len,
        "lunatic::metrics::histogram",
    )?;

    histogram!(name, value);
    Ok(())
}
