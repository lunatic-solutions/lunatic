use anyhow::Result;
use wasmtime::Linker;
use wasmtime_wasi::{ambient_authority, Dir, WasiCtx, WasiCtxBuilder};

pub fn build_wasi(
    args: Option<&Vec<String>>,
    envs: Option<&Vec<(String, String)>>,
    dirs: &[String],
) -> Result<WasiCtx> {
    let mut wasi = WasiCtxBuilder::new().inherit_stdio();
    if let Some(envs) = envs {
        wasi = wasi.envs(envs)?;
    }
    if let Some(args) = args {
        wasi = wasi.args(args)?;
    }
    for preopen_dir_path in dirs {
        let preopen_dir = Dir::open_ambient_dir(preopen_dir_path, ambient_authority())?;
        wasi = wasi.preopened_dir(preopen_dir, preopen_dir_path)?;
    }
    Ok(wasi.build())
}

pub trait LunaticWasiCtx {
    fn wasi(&mut self) -> &mut WasiCtx;
}

// Register WASI APIs to the linker
pub fn register<T: LunaticWasiCtx + Send + 'static>(linker: &mut Linker<T>) -> Result<()> {
    wasmtime_wasi::sync::snapshots::preview_1::add_wasi_snapshot_preview1_to_linker(linker, |ctx| {
        ctx.wasi()
    })
}
