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
    let text = format!(
        "use std::time::Duration;
use lunatic::{{sleep, spawn_link}};
        
fn main() {{
    spawn_link!(|| println!(\"Hello, World! I'm a process.\"));
    sleep(Duration::from_millis(100));
}}"
    );

    let mut file = File::create("src/main.rs").expect("Opened src/main.rs");

    file.write_all(text.as_bytes()).expect("\"Hello, World!\" example written in src/main.rs");
}

pub(crate) fn start(args: Args) -> Result<()> {
    let project_name = &args.new_args[0];

    Command::new("cargo").args(["new", project_name]).status().expect(format!("Cargo created the {} project", project_name.as_str()).as_str());

    let project_path = Path::new(project_name);
    env::set_current_dir(&project_path).expect(format!("Current directory changed to {}", project_name.as_str()).as_str());

    Command::new("cargo").args(["add", "lunatic"]).status().expect("Cargo added the lunatic dependency");

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
