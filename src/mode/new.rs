use std::{process::Command, env, path::Path, io::Write};

use clap::Parser;
use anyhow::Result;

use crate::mode;

#[derive(Parser, Debug)]
#[command(version)]
pub struct Args {
    #[arg(index = 1)]
    pub new_args: Vec<String>,
}

pub(crate) fn start(args: Args) -> Result<()> {
    Command::new("cargo").args(["new", &args.new_args[0]]).status().expect(format!("failed to create {} project", &args.new_args[0].as_str()).as_str());

    let new_directory = Path::new(&args.new_args[0]);
    env::set_current_dir(&new_directory).expect(format!("failed to change to the {} directory", &args.new_args[0].as_str()).as_str());

    Command::new("cargo").args(["add", "lunatic"]).status().expect("failed to add lunatic dependency");

    mode::init::start();

    let src_directory = Path::new("src");
    env::set_current_dir(&src_directory).expect("failed to change to the \"src\" directory");

    let mut f = std::fs::OpenOptions::new().write(true).truncate(true).open("./main.rs")?;
    f.write_all(b"use std::time::Duration;\n\n\
    use lunatic::{sleep, spawn_link};\n\n\
    fn main() {\n\
        spawn_link!(|| println!(\"Hello, world! I'm a process.\"));\n\
        sleep(Duration::from_millis(100));\n\
    }\n")?;
    f.flush()?;

    Ok(())
}
