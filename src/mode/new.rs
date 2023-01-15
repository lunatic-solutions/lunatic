use std::{process::Command, env, path::Path, fs::File, io::Write};

use clap::Parser;
use anyhow::{anyhow, Result};

use crate::mode;

#[derive(Parser, Debug)]
#[command(version)]
pub struct Args {
    #[arg(index = 1)]
    pub new_args: Vec<String>,
}

fn add_lunatic_main_file() {
    let mut file = File::create("src/main.rs").expect("Could not open src/main.rs");
    file.write_all(b"use std::time::Duration;\n\n\
    use lunatic::{sleep, spawn_link};\n\n\
    fn main() {\n\
        spawn_link!(|| println!(\"Hello, world! I'm a process.\"));\n\
        sleep(Duration::from_millis(100));\n\
    }\n").expect("Could not write to src/main.rs");
}

pub(crate) fn start(args: Args) -> Result<()> {
    let project_name = &args.new_args[0];

    Command::new("cargo").args(["new", project_name]).status().expect(format!("Failed to create {} project", project_name.as_str()).as_str());

    let project_path = Path::new(project_name);
    env::set_current_dir(&project_path).expect(format!("Failed to change to the {} directory", project_name.as_str()).as_str());

    Command::new("cargo").args(["add", "lunatic"]).status().expect("Failed to add the lunatic dependency");

    match mode::init::start() {
        Ok(result) => {
            add_lunatic_main_file();
            Ok(result)
        }
        Err(error) => Err(anyhow!(
            "Could not initialize a lunatic project in {}: {}.",
            &project_name,
            error
        ))
    }
}
