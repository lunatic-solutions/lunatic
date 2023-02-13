use std::{env, fs, fs::File, io::Write, path::Path, process::Command};

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

pub(crate) async fn start(args: Args) -> Result<()> {
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

    let lunatic_add_status = Command::new("cargo")
        .args(["add", "-q", "lunatic"])
        .status()
        .expect("'cargo add' should execute");

    if !lunatic_add_status.success() {
        return Err(anyhow!("Could not add the lunatic dependency"));
    }

    let lunatic_log_add_status = Command::new("cargo")
        .args(["add", "-q", "lunatic-log"])
        .status()
        .expect("'cargo add' should execute");

    if !lunatic_log_add_status.success() {
        return Err(anyhow!("Could not add the lunatic-log dependency"));
    }

    let lunatic_subms_status = Command::new("cargo")
        .args(["add", "-q", "submillisecond"])
        .status()
        .expect("'cargo add' should execute");

    if !lunatic_subms_status.success() {
        return Err(anyhow!("Could not add the lunatic-log dependency"));
    }

    match mode::init::start() {
        Ok(result) => {
            create_project(project_name)?;

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

fn create_project(project_name: &str) -> Result<()> {
    create_lib()?;
    create_supervisor()?;
    create_router()?;
    create_main(project_name)?;
    create_test()?;
    create_readme(project_name)?;
    Ok(())
}

fn create_lib() -> Result<()> {
    let text ="/*!
The `lib.rs` file defines public modules used for your project and makes interfaces easy to test.
Defined here are the main [Supervisor](https://docs.rs/lunatic/latest/lunatic/supervisor/trait.Supervisor.html) and the supervised Submillisecond http server.
*/
pub mod http_server;
pub mod main_supervisor;";

    let mut file = File::create("src/lib.rs").expect("Opened src/lib.rs");

    file.write_all(text.as_bytes())
        .expect("src/lib.rs written successfully.");
    Ok(())
}

fn create_supervisor() -> Result<()> {
    let text ="/*!
The main application supervisor. See https://docs.rs/lunatic/latest/lunatic/supervisor/trait.Supervisor.html for more information.
*/
pub mod main_supervisor {
    use lunatic::supervisor::{Supervisor, SupervisorConfig, SupervisorStrategy};
    use crate::http_server::http_server::HTTPServer;
    pub struct MainSupervisor;

    impl Supervisor for MainSupervisor {
        type Arg = ();

        // Start top-level procs.
        type Children = HTTPServer;

        fn init(config: &mut SupervisorConfig<Self>, _: ()) {
            // If a child fails, just restart it.
            config.set_strategy(SupervisorStrategy::OneForOne);
            // Setup child processes.
            config.children_args(
                ((), Some(\"GLOBAL_HTTP_SERVER\".to_owned())),
            );
        }
    }
}";

    let mut file = File::create("src/main_supervisor.rs").expect("Opened src/main_supervisor.rs");

    file.write_all(text.as_bytes())
        .expect("src/main_supervisor.rs written successfully.");
    Ok(())
}

fn create_router() -> Result<()> {
    let text ="/*!
Supervised [submillisecond](https://docs.rs/submillisecond/latest/submillisecond/index.html) router definition.
*/
pub mod http_server {
    use submillisecond::{router, Application};
    pub struct HTTPServer;
    use lunatic::{abstract_process, process::{ProcessRef}};

    fn index() -> &'static str {
        \"Hello from Submillisecond!\"
    }

    #[abstract_process]
    impl HTTPServer {
        #[init]
        fn init(_this: ProcessRef<Self>, _: ()) -> Self {

            // See https://docs.rs/submillisecond/latest/submillisecond/macro.router.html 
            // for more information on defining routes.
            Application::new(router! {
                GET \"/\" => index
            })
            .serve(\"0.0.0.0:3000\")
            .unwrap();
            HTTPServer
        }
    }
}";

    let mut file = File::create("src/http_server.rs").expect("Opened src/http_server.rs");

    file.write_all(text.as_bytes())
        .expect("src/http_server.rs written successfully.");
    Ok(())
}

fn create_main(name: &str) -> Result<()> {
    let text =format!("/*!
Application main.
*/
use lunatic::{{process::StartProcess, Mailbox}};
use lunatic_log::{{init, subscriber::fmt::FmtSubscriber, LevelFilter}};
use {name}::main_supervisor::main_supervisor::MainSupervisor;

#[lunatic::main]
fn main(_: Mailbox<()>) {{
    init(FmtSubscriber::new(LevelFilter::Info).pretty());
    MainSupervisor::start((), None);
}}", name=name);

    let mut file = File::create("src/main.rs").expect("Opened src/main.rs");

    file.write_all(text.as_bytes())
        .expect("src/main.rs written successfully.");
    Ok(())
}

fn create_test() -> Result<()> {

    fs::create_dir_all("tests/")?;
    let text = "/*!
Example Tests File.
*/
#[lunatic::test]
fn basic_test() {
    assert!(true);
}";

    let mut file = File::create("tests/test.rs").expect("Opened tests/test.rs");

    file.write_all(text.as_bytes())
        .expect("src/tests.rs written successfully.");
    Ok(())
}

fn create_readme(name: &str) -> Result<()> {
    let text =format!("# {name}
## Setup
* Install dependencies and run the application using `cargo run`.

Navigate to [`localhost:3000`](http://localhost:3000) in your browser.

## Test
* To run the test suite run `cargo test`.
", name=name);

    let mut file = File::create("README.md").expect("Opened README.md");

    file.write_all(text.as_bytes())
        .expect("README.md written successfully.");
    Ok(())
}
