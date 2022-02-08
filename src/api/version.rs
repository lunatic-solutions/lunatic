use lunatic_common_api::link_if_match;
use wasmtime::{FuncType, Linker, Trap, ValType};

use crate::state::ProcessState;

/// Links the `version` APIs.
pub(crate) fn register(
    linker: &mut Linker<ProcessState>,
    namespace_filter: &[String],
) -> anyhow::Result<()> {
    link_if_match(
        linker,
        "lunatic::version",
        "major",
        FuncType::new([], [ValType::I32]),
        major_version,
        namespace_filter,
    )?;
    link_if_match(
        linker,
        "lunatic::version",
        "minor",
        FuncType::new([], [ValType::I32]),
        minor_version,
        namespace_filter,
    )?;
    link_if_match(
        linker,
        "lunatic::version",
        "patch",
        FuncType::new([], [ValType::I32]),
        patch_version,
        namespace_filter,
    )
}

fn major_version() -> Result<u32, Trap> {
    Ok(env!("CARGO_PKG_VERSION_MAJOR").parse::<u32>().unwrap())
}

fn minor_version() -> Result<u32, Trap> {
    Ok(env!("CARGO_PKG_VERSION_MINOR").parse::<u32>().unwrap())
}

fn patch_version() -> Result<u32, Trap> {
    Ok(env!("CARGO_PKG_VERSION_PATCH").parse::<u32>().unwrap())
}
