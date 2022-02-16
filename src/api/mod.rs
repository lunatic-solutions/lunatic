mod process;

use anyhow::Result;
use wasmtime::Linker;

use crate::state::ProcessState;

// Registers all sub-APIs to the `Linker`.
pub(crate) fn register(
    linker: &mut Linker<ProcessState>,
    namespace_filter: &[String],
) -> Result<()> {
    lunatic_error_api::register(linker, namespace_filter)?;
    process::register(linker, namespace_filter)?;
    lunatic_messaging_api::register(linker, namespace_filter)?;
    lunatic_networking_api::register(linker, namespace_filter)?;
    lunatic_version_api::register(linker, namespace_filter)?;
    lunatic_wasi_api::register(linker, namespace_filter)?;
    Ok(())
}

mod tests {
    #[async_std::test]
    async fn import_filter_signature_matches() {
        use crate::{EnvConfig, Environment};

        // The default configuration includes both, the "lunatic::*" and "wasi_*" namespaces.
        let config = EnvConfig::default();
        let environment = Environment::local(config).unwrap();
        let raw_module = wat::parse_file("./wat/all_imports.wat").unwrap();
        let module = environment.create_module(raw_module).await.unwrap();
        module.spawn("hello", Vec::new(), None).await.unwrap();

        // This configuration should still compile, even all host calls will trap.
        let config = EnvConfig::new(0, None);
        let environment = Environment::local(config).unwrap();
        let raw_module = wat::parse_file("./wat/all_imports.wat").unwrap();
        let module = environment.create_module(raw_module).await.unwrap();
        module.spawn("hello", Vec::new(), None).await.unwrap();
    }
}
