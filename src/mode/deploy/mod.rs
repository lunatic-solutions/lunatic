use std::{
    net::{SocketAddr, TcpListener},
    path::PathBuf,
};

use anyhow::{anyhow, Result};
use clap::Parser;

#[derive(Parser, Debug)]
pub(crate) struct Args {
    /// Build and deploy the specified binary. This flag may be specified multiple times and supports common Unix glob patterns.
    #[clap(short = 'd', long)]
    bin: Option<Vec<String>>,

    /// Build all binary targets.
    #[clap(long)]
    bins: bool,

    /// Build and deploy the specified example. This flag may be specified multiple times and supports common Unix glob patterns.
    #[clap(long)]
    example: Option<Vec<String>>,

    ///Build all example targets.
    #[clap(long)]
    examples: bool,

    /// Build with release optimisations
    #[clap(short = 'r', long)]
    release: bool,
}

pub(crate) async fn start(args: Args) -> Result<()> {
    let build_args: Vec<String> = if let Some(bin) = args.bin {
        let bins = bin.into_iter().flat_map(|b| ["--bin".to_owned(), b]);
        vec!["build".to_owned()].into_iter().chain(bins).collect()
    } else {
        vec!["build".to_owned()]
    };
    let build_result = std::process::Command::new("cargo")
        .args(build_args)
        .output()
        .expect("failed to execute build");

    // println!("{}", build_result.stdout);
    // eprintln!("{}", build_result.stderr);

    Ok(())

    // Err(anyhow!("No available port on 127.0.0.1. Aborting"))
}

fn get_available_localhost() -> Option<TcpListener> {
    for port in 3030..3999u16 {
        if let Ok(s) = TcpListener::bind(("127.0.0.1", port)) {
            return Some(s);
        }
    }

    for port in 1025..65535u16 {
        if let Ok(s) = TcpListener::bind(("127.0.0.1", port)) {
            return Some(s);
        }
    }

    None
}
