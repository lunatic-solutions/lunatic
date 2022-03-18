use wasmtime::Linker;

/// Links the `version` APIs.
pub fn register<T>(linker: &mut Linker<T>) -> anyhow::Result<()> {
    linker.func_wrap("lunatic::version", "major", major)?;
    linker.func_wrap("lunatic::version", "minor", minor)?;
    linker.func_wrap("lunatic::version", "patch", patch)?;
    Ok(())
}

fn major() -> u32 {
    env!("CARGO_PKG_VERSION_MAJOR").parse::<u32>().unwrap()
}

fn minor() -> u32 {
    env!("CARGO_PKG_VERSION_MINOR").parse::<u32>().unwrap()
}

fn patch() -> u32 {
    env!("CARGO_PKG_VERSION_PATCH").parse::<u32>().unwrap()
}
