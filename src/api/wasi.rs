use anyhow::Result;
use wasmtime::Linker;

use super::namespace_matches_filter;
use crate::state::State;

// Register WASI APIs to the linker
pub(crate) fn register(linker: &mut Linker<State>, namespace_filter: &[String]) -> Result<()> {
    if namespace_matches_filter("wasi_snapshot_preview1", "", namespace_filter) {
        wasmtime_wasi::sync::snapshots::preview_1::add_wasi_snapshot_preview1_to_linker(
            linker,
            |ctx| &mut ctx.wasi,
        )?;
    }
    Ok(())
}
