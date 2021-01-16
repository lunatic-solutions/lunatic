use anyhow::Result;

fn main() -> Result<()> {
    env_logger::init();
    lunatic_runtime::run()
}
