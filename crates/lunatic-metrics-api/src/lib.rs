use anyhow::{Context as _, Result};
use hash_map_id::HashMapId;
use log::{Level, Record};
use lunatic_common_api::{get_memory, IntoTrap};
use lunatic_process::state::ProcessState;
use lunatic_process_api::ProcessCtx;
use opentelemetry::{
    metrics::{
        Counter, Histogram, InstrumentBuilder, Meter, MeterProvider, MetricsError, Unit,
        UpDownCounter,
    },
    trace::{Span, TraceContextExt, Tracer},
    Context, KeyValue, StringValue,
};
use serde_json::Map;
use wasmtime::{Caller, Linker};

pub const OTEL_LUNATIC_ENVIRONMENT_ID_KEY: &str = "lunatic.environment_id";
pub const OTEL_LUNATIC_PROCESS_ID_KEY: &str = "lunatic.process_id";

pub type ContextResources = HashMapId<Context>;

pub trait MetricsCtx {
    type Tracer: Tracer;
    type MeterProvider: MeterProvider;

    fn tracer(&self) -> &Self::Tracer;
    fn meter_provider(&self) -> &Self::MeterProvider;

    fn add_context(&mut self, context: Context) -> u64;
    fn get_context(&self, id: u64) -> Option<&Context>;
    fn get_last_context(&self) -> &Context;
    fn drop_context(&mut self, id: u64);

    fn add_meter(&mut self, meter: Meter) -> u64;
    fn get_meter(&self, id: u64) -> Option<&Meter>;
    fn drop_meter(&mut self, id: u64) -> Option<Meter>;

    fn log(&self, record: &Record);

    fn add_counter(&mut self, counter: Counter<f64>) -> u64;
    fn get_counter(&self, id: u64) -> Option<&Counter<f64>>;
    fn drop_counter(&mut self, id: u64) -> Option<Counter<f64>>;

    fn add_up_down_counter(&mut self, up_down_counter: UpDownCounter<f64>) -> u64;
    fn get_up_down_counter(&self, id: u64) -> Option<&UpDownCounter<f64>>;
    fn drop_up_down_counter(&mut self, id: u64) -> Option<UpDownCounter<f64>>;

    fn add_histogram(&mut self, histogram: Histogram<f64>) -> u64;
    fn get_histogram(&self, id: u64) -> Option<&Histogram<f64>>;
    fn drop_histogram(&mut self, id: u64) -> Option<Histogram<f64>>;
}

/// Links the [Metrics](https://crates.io/crates/metrics) APIs.
pub fn register<T>(linker: &mut Linker<T>) -> anyhow::Result<()>
where
    T: ProcessCtx<T> + ProcessState + MetricsCtx + 'static,
    <<T as MetricsCtx>::Tracer as Tracer>::Span: Send + Sync,
{
    linker.func_wrap("lunatic::metrics", "span_start", span_start)?;
    linker.func_wrap("lunatic::metrics", "span_drop", span_drop)?;

    linker.func_wrap("lunatic::metrics", "meter", meter)?;
    linker.func_wrap("lunatic::metrics", "meter_drop", meter_drop)?;

    linker.func_wrap("lunatic::metrics", "event", event)?;

    linker.func_wrap("lunatic::metrics", "counter", counter)?;
    linker.func_wrap("lunatic::metrics", "counter_add", counter_add)?;
    linker.func_wrap("lunatic::metrics", "counter_drop", counter_drop)?;

    linker.func_wrap("lunatic::metrics", "up_down_counter", up_down_counter)?;
    linker.func_wrap(
        "lunatic::metrics",
        "up_down_counter_add",
        up_down_counter_add,
    )?;
    linker.func_wrap(
        "lunatic::metrics",
        "up_down_counter_drop",
        up_down_counter_drop,
    )?;

    linker.func_wrap("lunatic::metrics", "histogram", histogram)?;
    linker.func_wrap("lunatic::metrics", "histogram_record", histogram_record)?;
    linker.func_wrap("lunatic::metrics", "histogram_drop", histogram_drop)?;

    Ok(())
}

/// Starts a new span of work, used for recording metrics including log events and meters such as counters, gauges, and histograms.
///
/// If parent is set to u64::MAX, then the last created span will be used.
///
/// Traps:
/// * If the name is not a valid utf8 string.
/// * If the attributes is not valid json.
/// * If the parent span does not exist.
/// * If any memory outside the guest heap space is referenced.
fn span_start<T>(
    mut caller: Caller<'_, T>,
    parent: u64,
    name_ptr: u32,
    name_len: u32,
    attributes_ptr: u32,
    attributes_len: u32,
) -> Result<u64>
where
    T: ProcessCtx<T> + ProcessState + MetricsCtx,
    <<T as MetricsCtx>::Tracer as Tracer>::Span: Send + Sync + 'static,
{
    let memory = get_memory(&mut caller)?;
    let (data, state) = memory.data_and_store_mut(&mut caller);

    let parent = if parent != u64::MAX {
        Some(parent)
    } else {
        None
    };

    let name = get_string_arg(data, name_ptr, name_len).or_trap("lunatic::metrics::span_start")?;
    let mut attributes = if attributes_len > 0 {
        let attributes_data = data
            .get(attributes_ptr as usize..(attributes_ptr + attributes_len) as usize)
            .or_trap("lunatic::metrics::span_start")?;
        let attributes_json =
            serde_json::from_slice(attributes_data).or_trap("lunatic::metrics::span_start")?;
        data_to_opentelemetry(attributes_json)
    } else {
        vec![]
    };
    inject_lunatic_attributes(state, &mut attributes);

    let parent_ctx = if let Some(id) = parent {
        state
            .get_context(id)
            .or_trap("lunatic::metrics::span_start")?
    } else {
        state.get_last_context()
    };

    let mut span = state.tracer().start_with_context(name, parent_ctx);
    span.set_attributes(attributes);

    let context = parent_ctx.with_span(span);

    let id = state.add_context(context);

    Ok(id)
}

/// Drops a span, marking it as finished.
fn span_drop<T>(mut caller: Caller<'_, T>, id: u64) -> Result<()>
where
    T: MetricsCtx,
{
    let memory = get_memory(&mut caller)?;
    let (_data, state) = memory.data_and_store_mut(&mut caller);

    state.drop_context(id);

    Ok(())
}

/// Adds a new meter with a given name.
///
/// Traps:
/// * If the name attribute is not a valid utf8 string.
/// * If any memory outside the guest heap space is referenced.
fn meter<T>(mut caller: Caller<'_, T>, name_ptr: u32, name_len: u32) -> Result<u64>
where
    T: MetricsCtx,
{
    let memory = get_memory(&mut caller)?;
    let (data, state) = memory.data_and_store_mut(&mut caller);

    let name = get_string_arg(data, name_ptr, name_len).or_trap("lunatic::metrics::meter")?;

    let meter = state.meter_provider().meter(name.into());
    let id = state.add_meter(meter);

    Ok(id)
}

/// Drops a meter.
fn meter_drop<T>(mut caller: Caller<'_, T>, id: u64) -> Result<()>
where
    T: MetricsCtx,
{
    let memory = get_memory(&mut caller)?;
    let (_data, state) = memory.data_and_store_mut(&mut caller);

    state.drop_meter(id);

    Ok(())
}

/// Adds a log event in span, containing a name and optional attributes.
///
/// If span is set to u64::MAX, then the last created span will be used.
///
/// The following attributes are optional, and used for logging to the terminal:
/// * `target`: a string describing the part of the system where the span or event that this
/// metadata describes occurred.
/// * `severityNumber`: the numerical severity of the log.
///   The following are numbers that map to differnt log levels:
///   * 1..=4 => Trace
///   * 5..=8 => Debug
///   * 9..=12 => Info
///   * 13..=16 => Warn
///   * 17..=20 => Error
///   Defaults to Info if omitted or an invalid number.
/// * `body`: the log message body. Defaults to the event name.
/// * `code.filepath`: the filepath where the log occurred.
/// * `code.lineno`: the line number where the log occurred.
/// * `code.column`: the column number where the log occurred.
/// * `code.namespace`: the module name where the log occurred.
///
/// Traps:
/// * If the name is not a valid utf8 string.
/// * If the attributes is not valid json.
/// * If the parent span does not exist.
/// * If any memory outside the guest heap space is referenced.
fn event<T>(
    mut caller: Caller<'_, T>,
    span: u64,
    name_ptr: u32,
    name_len: u32,
    attributes_ptr: u32,
    attributes_len: u32,
) -> Result<()>
where
    T: ProcessCtx<T> + ProcessState + MetricsCtx,
{
    let memory = get_memory(&mut caller)?;
    let (data, state) = memory.data_and_store_mut(&mut caller);

    let name = get_string_arg(data, name_ptr, name_len).or_trap("lunatic::metrics::event")?;

    let mut attributes = if attributes_len > 0 {
        let attributes_data = data
            .get(attributes_ptr as usize..(attributes_ptr + attributes_len) as usize)
            .or_trap("lunatic::metrics::event")?;
        let attributes_json: Map<String, serde_json::Value> =
            serde_json::from_slice(attributes_data).or_trap("lunatic::metrics::event")?;

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
    inject_lunatic_attributes(state, &mut attributes);

    let span = if span != u64::MAX {
        state
            .get_context(span)
            .or_trap("lunatic::metrics::event")?
            .span()
    } else {
        state.get_last_context().span()
    };

    span.add_event(name, attributes);

    Ok(())
}

/// Creates a counter with a given name, and optional description and unit.
///
/// Traps:
/// * If the name is not a valid utf8 string, or contains invalid characters.
/// * If the description is not a valid utf8 string.
/// * If the unit is not a valid utf8 string, or exceeds 63 characters.
/// * If the meter does not exist.
/// * If any memory outside the guest heap space is referenced.
#[allow(clippy::too_many_arguments)]
fn counter<T>(
    mut caller: Caller<'_, T>,
    meter: u64,
    name_ptr: u32,
    name_len: u32,
    description_ptr: u32,
    description_len: u32,
    unit_ptr: u32,
    unit_len: u32,
) -> Result<u64>
where
    T: MetricsCtx,
{
    let (state, counter) = create_metric(
        &mut caller,
        meter,
        name_ptr,
        name_len,
        description_ptr,
        description_len,
        unit_ptr,
        unit_len,
        |meter, name| meter.f64_counter(name),
    )
    .or_trap("lunatic::metrics::counter")?;

    let id = state.add_counter(counter);

    Ok(id)
}

/// Increments a counter.
///
/// If span is set to u64::MAX, then the last created span will be used.
///
/// Traps:
/// * If the span does not exist.
/// * If the counter does not exist.
/// * If the attributes is not valid json.
/// * If any memory outside the guest heap space is referenced.
fn counter_add<T>(
    mut caller: Caller<'_, T>,
    span: u64,
    counter: u64,
    amount: f64,
    attributes_ptr: u32,
    attributes_len: u32,
) -> Result<()>
where
    T: ProcessCtx<T> + ProcessState + MetricsCtx,
{
    let (state, cx, attributes) = update_metric(&mut caller, span, attributes_ptr, attributes_len)
        .or_trap("lunatic::metrics::counter_add")?;

    let counter = state
        .get_counter(counter)
        .or_trap("lunatic::metrics::counter_add")?;

    counter.add(cx, amount, &attributes);

    Ok(())
}

/// Drops a counter.
fn counter_drop<T>(mut caller: Caller<'_, T>, id: u64) -> Result<()>
where
    T: MetricsCtx,
{
    let memory = get_memory(&mut caller)?;
    let (_data, state) = memory.data_and_store_mut(&mut caller);

    state.drop_counter(id);

    Ok(())
}

/// Creates an up/down counter with a given name, and optional description and unit.
///
/// Traps:
/// * If the name is not a valid utf8 string, or contains invalid characters.
/// * If the description is not a valid utf8 string.
/// * If the unit is not a valid utf8 string, or exceeds 63 characters.
/// * If the meter does not exist.
/// * If any memory outside the guest heap space is referenced.
#[allow(clippy::too_many_arguments)]
fn up_down_counter<T>(
    mut caller: Caller<'_, T>,
    meter: u64,
    name_ptr: u32,
    name_len: u32,
    description_ptr: u32,
    description_len: u32,
    unit_ptr: u32,
    unit_len: u32,
) -> Result<u64>
where
    T: MetricsCtx,
{
    let (state, up_down_counter) = create_metric(
        &mut caller,
        meter,
        name_ptr,
        name_len,
        description_ptr,
        description_len,
        unit_ptr,
        unit_len,
        |meter, name| meter.f64_up_down_counter(name),
    )
    .or_trap("lunatic::metrics::up_down_counter")?;

    let id = state.add_up_down_counter(up_down_counter);

    Ok(id)
}

/// Increments an up/down counter. The amount can be negative to decrement.
///
/// If span is set to u64::MAX, then the last created span will be used.
///
/// Traps:
/// * If the span does not exist.
/// * If the counter does not exist.
/// * If the attributes is not valid json.
/// * If any memory outside the guest heap space is referenced.
fn up_down_counter_add<T>(
    mut caller: Caller<'_, T>,
    span: u64,
    counter: u64,
    amount: f64,
    attributes_ptr: u32,
    attributes_len: u32,
) -> Result<()>
where
    T: ProcessCtx<T> + ProcessState + MetricsCtx,
{
    let (state, cx, attributes) = update_metric(&mut caller, span, attributes_ptr, attributes_len)
        .or_trap("lunatic::metrics::up_down_counter_add")?;

    let counter = state
        .get_up_down_counter(counter)
        .or_trap("lunatic::metrics::up_down_counter_add")?;

    counter.add(cx, amount, &attributes);

    Ok(())
}

/// Drops an up/down counter.
fn up_down_counter_drop<T>(mut caller: Caller<'_, T>, id: u64) -> Result<()>
where
    T: MetricsCtx,
{
    let memory = get_memory(&mut caller)?;
    let (_data, state) = memory.data_and_store_mut(&mut caller);

    state.drop_up_down_counter(id);

    Ok(())
}

/// Creates a histogram with a given name, and optional description and unit.
///
/// Traps:
/// * If the name is not a valid utf8 string, or contains invalid characters.
/// * If the description is not a valid utf8 string.
/// * If the unit is not a valid utf8 string, or exceeds 63 characters.
/// * If the meter does not exist.
/// * If any memory outside the guest heap space is referenced.
#[allow(clippy::too_many_arguments)]
fn histogram<T>(
    mut caller: Caller<'_, T>,
    meter: u64,
    name_ptr: u32,
    name_len: u32,
    description_ptr: u32,
    description_len: u32,
    unit_ptr: u32,
    unit_len: u32,
) -> Result<u64>
where
    T: MetricsCtx,
{
    let (state, histogram) = create_metric(
        &mut caller,
        meter,
        name_ptr,
        name_len,
        description_ptr,
        description_len,
        unit_ptr,
        unit_len,
        |meter, name| meter.f64_histogram(name),
    )
    .or_trap("lunatic::metrics::histogram")?;

    let id = state.add_histogram(histogram);

    Ok(id)
}

/// Records a value to a histogram.
///
/// If span is set to u64::MAX, then the last created span will be used.
///
/// Traps:
/// * If the span does not exist.
/// * If the histogram does not exist.
/// * If the attributes is not valid json.
/// * If any memory outside the guest heap space is referenced.
fn histogram_record<T>(
    mut caller: Caller<'_, T>,
    span: u64,
    histogram: u64,
    value: f64,
    attributes_ptr: u32,
    attributes_len: u32,
) -> Result<()>
where
    T: ProcessCtx<T> + ProcessState + MetricsCtx,
{
    let (state, cx, attributes) = update_metric(&mut caller, span, attributes_ptr, attributes_len)
        .or_trap("lunatic::metrics::histogram_record")?;

    let histogram = state
        .get_histogram(histogram)
        .or_trap("lunatic::metrics::histogram_record")?;

    histogram.record(cx, value, &attributes);

    Ok(())
}

/// Drops a histogram.
fn histogram_drop<T>(mut caller: Caller<'_, T>, id: u64) -> Result<()>
where
    T: MetricsCtx,
{
    let memory = get_memory(&mut caller)?;
    let (_data, state) = memory.data_and_store_mut(&mut caller);

    state.drop_histogram(id);

    Ok(())
}

// === Helper functions ===

fn get_string_arg(data: &[u8], name_ptr: u32, name_len: u32) -> Result<String> {
    if name_len == 0 {
        return Ok(String::new());
    }
    let name = data
        .get(name_ptr as usize..(name_ptr + name_len) as usize)
        .context("invalid memory region")?;
    let name = String::from_utf8(name.to_vec())?;
    Ok(name)
}

#[allow(clippy::too_many_arguments)]
fn create_metric<'a, T, M, F>(
    caller: &'a mut Caller<'_, T>,
    meter: u64,
    name_ptr: u32,
    name_len: u32,
    description_ptr: u32,
    description_len: u32,
    unit_ptr: u32,
    unit_len: u32,
    builder: F,
) -> Result<(&'a mut T, M)>
where
    T: MetricsCtx,
    M: for<'b> TryFrom<InstrumentBuilder<'b, M>, Error = MetricsError>,
    F: for<'b> Fn(&'b Meter, String) -> InstrumentBuilder<'b, M>,
{
    let memory = get_memory(caller)?;
    let (data, state) = memory.data_and_store_mut(caller);

    let name = get_string_arg(data, name_ptr, name_len)?;
    let description = get_string_arg(data, description_ptr, description_len)?;
    let unit = get_string_arg(data, unit_ptr, unit_len).map(|unit| {
        if unit.is_empty() {
            None
        } else {
            Some(Unit::new(unit))
        }
    })?;

    let meter = state.get_meter(meter).context("meter does not exist")?;

    let mut metric_builder = builder(meter, name);
    if !description.is_empty() {
        metric_builder = metric_builder.with_description(description);
    }
    if let Some(unit) = unit {
        metric_builder = metric_builder.with_unit(unit);
    }

    let metric = metric_builder.try_init()?;

    Ok((state, metric))
}

fn update_metric<'a, T>(
    caller: &'a mut Caller<'_, T>,
    span: u64,
    attributes_ptr: u32,
    attributes_len: u32,
) -> Result<(&'a T, &'a Context, Vec<KeyValue>)>
where
    T: ProcessCtx<T> + ProcessState + MetricsCtx,
{
    let memory = get_memory(caller)?;
    let (data, state) = memory.data_and_store_mut(caller);

    let cx = if span != u64::MAX {
        state.get_context(span).context("span does not exist")?
    } else {
        state.get_last_context()
    };

    let mut attributes = if attributes_len > 0 {
        let attributes_data = data
            .get(attributes_ptr as usize..(attributes_ptr + attributes_len) as usize)
            .context("invalid memory region for attributes")?;
        let attributes_json: Map<String, serde_json::Value> =
            serde_json::from_slice(attributes_data)?;

        data_to_opentelemetry(attributes_json)
    } else {
        vec![]
    };
    inject_lunatic_attributes(state, &mut attributes);

    Ok((state, cx, attributes))
}

fn inject_lunatic_attributes<T: ProcessCtx<T> + ProcessState>(
    state: &T,
    attributes: &mut Vec<KeyValue>,
) {
    let environment_id = opentelemetry::Value::I64(state.environment().id() as i64);
    let process_id = opentelemetry::Value::I64(state.id() as i64);
    let lunatic_attrs = [
        (OTEL_LUNATIC_ENVIRONMENT_ID_KEY, environment_id),
        (OTEL_LUNATIC_PROCESS_ID_KEY, process_id),
    ];
    for (key, value) in lunatic_attrs {
        match attributes
            .iter_mut()
            .find(|key_value| key_value.key.as_str() == key)
        {
            Some(key_value) => key_value.value = value,
            None => {
                attributes.push(KeyValue::new(key, value));
            }
        }
    }
}

fn data_to_opentelemetry(data: Map<String, serde_json::Value>) -> Vec<KeyValue> {
    data.into_iter()
        .map(|(k, v)| KeyValue {
            key: k.into(),
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
