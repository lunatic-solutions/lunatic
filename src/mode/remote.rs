use anyhow::Result;
use clap::Parser;

#[derive(Parser, Debug, Clone)]
#[clap(rename_all = "kebab_case")]
pub enum RemoteArgs {
    Add {
        /// The name of the remote
        name: String,
        /// The URL of the remote
        url: String,
    },
    Rename {
        /// The old name of the remote
        old_name: String,
        /// The new name of the remote
        new_name: String,
    },
    Remove {
        /// The name of the remote
        name: String,
    },
    Get {
        /// The name of the remote
        remote_name: String,
    },
}

#[derive(Parser, Debug)]
pub struct Args {
    #[command(subcommand)]
    remote: RemoteArgs,
}

pub(crate) async fn start(args: Args) -> Result<()> {
    // match args.
    Ok(())

    // Err(anyhow!("No available port on 127.0.0.1. Aborting"))
}
