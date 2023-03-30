use std::borrow::Cow;

use anyhow::Result;
use hash_map_id::HashMapId;
use log::{Level, Record};
use lunatic_common_api::{get_memory, IntoTrap};
use lunatic_process::state::ProcessState;
use lunatic_process_api::ProcessCtx;
use metrics::{counter, decrement_gauge, gauge, histogram, increment_counter, increment_gauge};
use opentelemetry::{
    trace::{SpanRef, Tracer},
    Context, KeyValue, StringValue,
};
use serde_json::Map;
use wasmtime::{Caller, Linker};

pub type SpanResources = HashMapId<Context>;
pub type TracerSpan<T> = <T as Tracer>::Span;

pub trait MetricsCtx {
    type Tracer: Tracer + Send + Sync;

    fn log(&self, record: &Record);
    fn add_span<T, I>(&mut self, parent: Option<u64>, name: T, attributes: I) -> Option<u64>
    where
        T: Into<Cow<'static, str>>,
        I: IntoIterator<Item = KeyValue>;
    fn drop_span(&mut self, id: u64);
    fn get_span(&self, id: u64) -> Option<SpanRef<'_>>;
    fn get_last_span(&self) -> SpanRef;
}

/// Links the [Metrics](https://crates.io/crates/metrics) APIs
pub fn register<T>(linker: &mut Linker<T>) -> anyhow::Result<()>
where
    T: ProcessState + ProcessCtx<T> + MetricsCtx + Send + Sync + 'static,
    <<T as MetricsCtx>::Tracer as Tracer>::Span: Send + Sync,
{
    linker.func_wrap("lunatic::metrics", "start_span", start_span)?;
    linker.func_wrap("lunatic::metrics", "add_event", add_event)?;
    linker.func_wrap("lunatic::metrics", "drop_span", drop_span)?;
    linker.func_wrap("lunatic::metrics", "counter", counter)?;
    linker.func_wrap("lunatic::metrics", "increment_counter", increment_counter)?;
    linker.func_wrap("lunatic::metrics", "gauge", gauge)?;
    linker.func_wrap("lunatic::metrics", "increment_gauge", increment_gauge)?;
    linker.func_wrap("lunatic::metrics", "decrement_gauge", decrement_gauge)?;
    linker.func_wrap("lunatic::metrics", "histogram", histogram)?;
    Ok(())
}

fn get_string_arg(
    memory_slice: &[u8],
    name_str_ptr: u32,
    name_str_len: u32,
    func_name: &str,
) -> Result<String> {
    let name = memory_slice
        .get(name_str_ptr as usize..(name_str_ptr + name_str_len) as usize)
        .or_trap(func_name)?;
    let name = String::from_utf8(name.to_vec()).or_trap(func_name)?;
    Ok(name)
}

/// TODO docs
fn start_span<T>(
    mut caller: Caller<'_, T>,
    parent_span: u64,
    name_str_ptr: u32,
    name_str_len: u32,
    attributes_ptr: u32,
    attributes_len: u32,
) -> Result<u64>
where
    T: ProcessState + ProcessCtx<T> + MetricsCtx + Send + Sync,
    <<T as MetricsCtx>::Tracer as Tracer>::Span: Send + Sync,
{
    let memory = get_memory(&mut caller)?;
    let (memory_slice, state) = memory.data_and_store_mut(&mut caller);

    let parent = if parent_span != u64::MAX {
        Some(parent_span)
    } else {
        None
    };

    let name = get_string_arg(
        memory_slice,
        name_str_ptr,
        name_str_len,
        "lunatic::metrics::start_span",
    )?;
    let attributes = if attributes_len > 0 {
        let attributes_data = memory_slice
            .get(attributes_ptr as usize..(attributes_ptr + attributes_len) as usize)
            .or_trap("lunatic::metrics::start_span")?;
        let attributes_json =
            serde_json::from_slice(attributes_data).or_trap("lunatic::metrics::start_span")?;
        data_to_opentelemetry(attributes_json)
    } else {
        vec![]
    };

    let id = state
        .add_span(parent, name, attributes)
        .or_trap("lunatic::metrics::start_span")?;

    Ok(id)
}

/// TODO docs
fn add_event<T>(
    mut caller: Caller<'_, T>,
    span_id: u64,
    name_str_ptr: u32,
    name_str_len: u32,
    attributes_ptr: u32,
    attributes_len: u32,
) -> Result<()>
where
    T: ProcessState + ProcessCtx<T> + MetricsCtx + Send + Sync,
    <<T as MetricsCtx>::Tracer as Tracer>::Span: Send + Sync,
{
    let memory = get_memory(&mut caller)?;
    let (memory_slice, state) = memory.data_and_store_mut(&mut caller);

    let name = get_string_arg(
        memory_slice,
        name_str_ptr,
        name_str_len,
        "lunatic::metrics::add_event",
    )?;

    let attributes = if attributes_len > 0 {
        let attributes_data = memory_slice
            .get(attributes_ptr as usize..(attributes_ptr + attributes_len) as usize)
            .or_trap("lunatic::metrics::add_event")?;
        let attributes_json: Map<String, serde_json::Value> =
            serde_json::from_slice(attributes_data).or_trap("lunatic::metrics::add_event")?;

        let level = attributes_json
            .get("severityNumber")
            .and_then(|level| level.as_u64())
            .and_then(|level| match level {
                1..=4 => Some(Level::Trace),
                5..=8 => Some(Level::Debug),
                9..=12 => Some(Level::Info),
                13..=16 => Some(Level::Warn),
                17..=20 => Some(Level::Error),
                _ => None,
            })
            .unwrap_or(Level::Info);
        let message = attributes_json
            .get("body")
            .and_then(|message| message.as_str())
            .unwrap_or(&name);
        let target = attributes_json
            .get("target")
            .and_then(|target| target.as_str())
            .or(state.module().module().name())
            .unwrap_or("");
        let file = attributes_json
            .get("code.filepath")
            .and_then(|file| file.as_str());
        let line = attributes_json
            .get("code.lineno")
            .and_then(|line| line.as_u64().map(|line| line as u32));
        let module_path = attributes_json
            .get("code.namespace")
            .and_then(|module_path| module_path.as_str());

        state.log(
            &Record::builder()
                .args(format_args!("{message}"))
                .level(level)
                .target(target)
                .file(file)
                .line(line)
                .module_path(module_path)
                .build(),
        );

        data_to_opentelemetry(attributes_json)
    } else {
        let message = &name;
        let target = state.module().module().name().unwrap_or("");

        state.log(
            &Record::builder()
                .args(format_args!("{message}"))
                .target(target)
                .build(),
        );

        vec![]
    };

    let span = if span_id != u64::MAX {
        state
            .get_span(span_id)
            .or_trap("lunatic::metrics::add_event")?
    } else {
        state.get_last_span()
    };

    span.add_event(name, attributes);

    Ok(())
}

/// TODO docs
fn drop_span<T>(mut caller: Caller<'_, T>, id: u64) -> Result<()>
where
    T: ProcessState + ProcessCtx<T> + MetricsCtx + Send + Sync,
    <<T as MetricsCtx>::Tracer as Tracer>::Span: Send + Sync,
{
    let memory = get_memory(&mut caller)?;
    let (_data, state) = memory.data_and_store_mut(&mut caller);

    state.drop_span(id);

    Ok(())
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
) -> Result<()> {
    let memory = get_memory(&mut caller)?;
    let memory_slice = memory.data(&mut caller);

    let name = get_string_arg(
        memory_slice,
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
) -> Result<()> {
    let memory = get_memory(&mut caller)?;
    let memory_slice = memory.data(&mut caller);

    let name = get_string_arg(
        memory_slice,
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
) -> Result<()> {
    let memory = get_memory(&mut caller)?;
    let memory_slice = memory.data(&mut caller);

    let name = get_string_arg(
        memory_slice,
        name_str_ptr,
        name_str_len,
        "lunatic::metrics::gauge",
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
) -> Result<()> {
    let memory = get_memory(&mut caller)?;
    let memory_slice = memory.data(&mut caller);

    let name = get_string_arg(
        memory_slice,
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
) -> Result<()> {
    let memory = get_memory(&mut caller)?;
    let memory_slice = memory.data(&mut caller);

    let name = get_string_arg(
        memory_slice,
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
) -> Result<()> {
    let memory = get_memory(&mut caller)?;
    let memory_slice = memory.data(&mut caller);

    let name = get_string_arg(
        memory_slice,
        name_str_ptr,
        name_str_len,
        "lunatic::metrics::histogram",
    )?;

    histogram!(name, value);
    Ok(())
}

fn data_to_opentelemetry(data: Map<String, serde_json::Value>) -> Vec<KeyValue> {
    data.into_iter()
        .map(|(k, v)| KeyValue {
            key: k.to_string().into(),
            value: json_to_opentelemetry(v),
        })
        .collect()
}

fn json_to_opentelemetry(value: serde_json::Value) -> opentelemetry::Value {
    match value {
        serde_json::Value::Null => "null".into(),
        serde_json::Value::Bool(b) => opentelemetry::Value::Bool(b),
        serde_json::Value::Number(n) => n
            .as_f64()
            .map(opentelemetry::Value::F64)
            .or_else(|| n.as_i64().map(opentelemetry::Value::I64))
            .unwrap(),
        serde_json::Value::String(s) => s.into(),
        serde_json::Value::Array(a) => {
            let first_type = a.first();
            let valid_ot_array = a.iter().skip(1).all(|value| match first_type {
                Some(serde_json::Value::Null) => false,
                Some(serde_json::Value::Bool(_)) => value.is_boolean(),
                Some(serde_json::Value::Number(n)) if n.is_f64() => value.is_f64(),
                Some(serde_json::Value::Number(n)) if n.is_i64() => value.is_i64(),
                Some(serde_json::Value::String(_)) => value.is_string(),
                Some(serde_json::Value::Array(_)) => false,
                Some(serde_json::Value::Object(_)) => false,
                _ => false,
            });
            // if the json array can be represented as a opentelemetry array, then convert
            // accoridngly. Otherwise, just convert each value to a string.
            if valid_ot_array {
                match first_type.unwrap() {
                    serde_json::Value::Bool(_) => opentelemetry::Value::Array(
                        a.into_iter()
                            .map(|value| match value {
                                serde_json::Value::Bool(_) => value.as_bool(),
                                _ => None,
                            })
                            .collect::<Option<Vec<_>>>()
                            .unwrap()
                            .into(),
                    ),
                    serde_json::Value::Number(n) if n.is_f64() => opentelemetry::Value::Array(
                        a.into_iter()
                            .map(|value| match value {
                                serde_json::Value::Number(_) => value.as_f64(),
                                _ => None,
                            })
                            .collect::<Option<Vec<_>>>()
                            .unwrap()
                            .into(),
                    ),
                    serde_json::Value::Number(n) if n.is_i64() => opentelemetry::Value::Array(
                        a.into_iter()
                            .map(|value| match value {
                                serde_json::Value::Number(_) => value.as_i64(),
                                _ => None,
                            })
                            .collect::<Option<Vec<_>>>()
                            .unwrap()
                            .into(),
                    ),
                    serde_json::Value::String(_) => opentelemetry::Value::Array(
                        a.into_iter()
                            .map(|value| match value {
                                serde_json::Value::String(_) => match value {
                                    serde_json::Value::String(s) => Some(StringValue::from(s)),
                                    _ => None,
                                },
                                _ => None,
                            })
                            .collect::<Option<Vec<_>>>()
                            .unwrap()
                            .into(),
                    ),
                    _ => unreachable!("we already checked for other types"),
                }
            } else {
                opentelemetry::Value::Array(
                    a.into_iter()
                        .map(|value| StringValue::from(value.to_string()))
                        .collect::<Vec<_>>()
                        .into(),
                )
            }
        }
        serde_json::Value::Object(o) => serde_json::to_string(&o).unwrap().into(),
    }
}
