use std::{env, fs::File, io::Write, path::Path, process::Command};

use anyhow::{anyhow, Result};
use clap::Parser;

use crate::mode;

#[derive(Parser, Debug)]
#[command(version)]
pub struct Args {
    /// Name of new Lunatic project
    #[arg(index = 1)]
    pub name: String,
}

fn add_lunatic_main_file() {
    let text = "use std::time::Duration;
use lunatic::{{sleep, spawn_link}};
        
fn main() {{
    spawn_link!(|| println!(\"Hello, World! I'm a process.\"));
    sleep(Duration::from_millis(100));
}}";

    let mut file = File::create("src/main.rs").expect("Opened src/main.rs");

    file.write_all(text.as_bytes())
        .expect("\"Hello, World!\" example written in src/main.rs");
}

pub(crate) fn start(args: Args) -> Result<()> {
    let project_name = &args.name;

    let new_status = Command::new("cargo")
        .args(["new", project_name])
        .status()
        .expect("'cargo new' should execute");

    if !new_status.success() {
        return Err(anyhow!("Could not create a new Lunatic project"));
    }

    let project_path = Path::new(project_name);
    env::set_current_dir(project_path).expect("Current directory is changed to the new project");

    let add_status = Command::new("cargo")
        .args(["add", "-q", "lunatic"])
        .status()
        .expect("'cargo add' should execute");

    if !add_status.success() {
        return Err(anyhow!("Could not add the Lunatic dependency"));
    }

    match mode::init::start() {
        Ok(result) => {
            add_lunatic_main_file();

            println!("\nYour Lunatic project is ready 🚀");
            Ok(result)
        }
        Err(error) => Err(anyhow!(
            "Could not initialize a Lunatic project in {}: {}",
            &project_name,
            error
        )),
    }
}
