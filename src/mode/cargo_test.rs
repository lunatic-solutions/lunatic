use std::{env, fs, path::Path, sync::Arc, time::Instant};

use anyhow::{Context, Result};
use clap::Parser;
use dashmap::DashMap;
use lunatic_process::{env::LunaticEnvironment, runtimes, wasm::spawn_wasm};
use lunatic_process_api::ProcessConfigCtx;
use lunatic_runtime::{DefaultProcessConfig, DefaultProcessState};
use lunatic_stdout_capture::StdoutCapture;
use lunatic_wasi_api::LunaticWasiCtx;

#[derive(Parser, Debug)]
#[command(version)]
struct Args {
    /// Entry .wasm file
    #[arg()]
    wasm: String,

    /// Run only tests that contain the filter string
    #[arg()]
    filter: Option<String>,

    /// Grant access to the given host directories
    #[arg(long, value_name = "DIRECTORY")]
    dir: Vec<String>,

    /// Run only ignored tests
    #[arg(long)]
    ignored: bool,

    /// Don't hide output from test executions
    #[arg(long)]
    nocapture: bool,

    /// Show also the output of successfull tests
    #[arg(long)]
    show_output: bool,

    /// List all tests
    #[arg(long, requires = "format")]
    list: bool,

    /// Configure formatting of output (only supported: terse)
    #[arg(long, requires = "list")]
    format: Option<String>,

    /// Exactly match filters rather than by substring
    #[arg(long)]
    exact: bool,

    /// Arguments passed to the guest
    #[arg()]
    wasm_args: Vec<String>,
}

pub(crate) async fn test() -> Result<()> {
    // Set logger level to "error" to avoid printing process failures warnings during tests.
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("error")).init();
    // Measure test duration
    let now = Instant::now();

    // Parse command line arguments
    let args = Args::parse();

    let mut config = DefaultProcessConfig::default();
    // Allow initial process to compile modules, create configurations and spawn sub-processes
    config.set_can_compile_modules(true);
    config.set_can_create_configs(true);
    config.set_can_spawn_processes(true);

    // Set correct command line arguments for the guest
    config.set_command_line_arguments(args.wasm_args);

    // Inherit environment variables
    config.set_environment_variables(env::vars().collect());

    // Always preopen the current dir
    config.preopen_dir(".");
    for dir in args.dir {
        config.preopen_dir(dir);
    }

    // Create wasmtime runtime
    let wasmtime_config = runtimes::wasmtime::default_config();
    let runtime = runtimes::wasmtime::WasmtimeRuntime::new(&wasmtime_config)?;

    // Load and compile wasm module
    let path = args.wasm;
    let path = Path::new(&path);
    let module = fs::read(path)?;
    let module = Arc::new(runtime.compile_module::<DefaultProcessState>(module.into())?);

    let filter = args.filter.unwrap_or_default();

    // Find all function exports starting with `#lunatic_test_`.
    // Functions with a name that matches `#lunatic_test_#panic_Panic message#` are expected to
    // trap with a message that contains "Panic message".
    let mut test_functions = Vec::new();
    for export in module.exports() {
        if let wasmtime::ExternType::Func(_) = export.ty() {
            let wasm_export_name = export.name();
            if wasm_export_name.starts_with("#lunatic_test_") {
                let mut name = wasm_export_name.strip_prefix("#lunatic_test_").unwrap();
                let mut ignored = false;
                if name.starts_with("#ignore_") {
                    name = name.strip_prefix("#ignore_").unwrap();
                    ignored = true;
                }
                // If --ignored flag is present, don't ignore test & filter out non-ignored ones
                if args.ignored {
                    if ignored {
                        ignored = false
                    } else {
                        // Filter out not ignored tests. The name doesn't need to be preserved,
                        // because filtered out tests don't show up in the output.
                        test_functions.push(Test {
                            filtered: true,
                            wasm_export_name: "".to_string(),
                            function_name: "".to_string(),
                            panic: None,
                            ignored: false,
                        });
                        continue;
                    }
                }
                // Check if test should panic
                let test = if name.starts_with("#panic_") {
                    let name = name.strip_prefix("#panic_").unwrap();
                    // Take all characters until `#`, but skip over escaped ones `\#`.
                    let mut prev_char = ' ';
                    let panic: String = name
                        .chars()
                        .take_while(|c| {
                            let condition = !(*c == '#' && prev_char != '\\');
                            prev_char = *c;
                            condition
                        })
                        .collect();
                    let panic_unescaped = panic.replace("\\#", "#");
                    let panic_prefix = format!("{}#", panic);
                    let function_name = name.strip_prefix(&panic_prefix).unwrap().to_string();
                    let filtered = if args.exact {
                        !function_name.eq(&filter)
                    } else {
                        !function_name.contains(&filter)
                    };
                    Test {
                        filtered,
                        wasm_export_name: wasm_export_name.to_string(),
                        function_name,
                        panic: Some(panic_unescaped),
                        ignored,
                    }
                } else {
                    let filtered = if args.exact {
                        !name.eq(&filter)
                    } else {
                        !name.contains(&filter)
                    };
                    Test {
                        filtered,
                        wasm_export_name: wasm_export_name.to_string(),
                        function_name: name.to_string(),
                        panic: None,
                        ignored,
                    }
                };
                test_functions.push(test);
            }
        }
    }

    // If --list is specified, ignore everything else and just print out the test names
    if args.list {
        let format = args.format.unwrap_or_default();
        if format != "terse" {
            return Err(anyhow::anyhow!(
                "error: argument for --format must be terse (was {})",
                format
            ));
        }
        for test_function in test_functions {
            // Skip over filtered out functions
            if test_function.filtered {
                continue;
            }
            println!("{}: test", test_function.function_name);
        }
        return Ok(());
    }

    let n = test_functions.iter().filter(|test| !test.filtered).count();
    let filtered_out = test_functions.len() - n;
    println!("\nrunning {} {}", n, if n == 1 { "test" } else { "tests" });

    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();

    let config = Arc::new(config);

    for test_function in test_functions {
        // Skip over filtered out functions
        if test_function.filtered {
            continue;
        }

        // Skip over ignored tests
        if test_function.ignored {
            sender
                .send(TestResult {
                    name: test_function.function_name,
                    status: TestStatus::Ignored,
                    stdout: StdoutCapture::new(false),
                })
                .unwrap();
            continue;
        }

        let env = Arc::new(LunaticEnvironment::new(0));
        let registry = Arc::new(DashMap::new());
        let mut state = DefaultProcessState::new(
            env.clone(),
            None,
            runtime.clone(),
            module.clone(),
            config.clone(),
            registry,
        )
        .unwrap();

        // If --nocapture is not set, use in-memory stdout & stderr to hide output in case of
        // success
        let stdout = StdoutCapture::new(args.nocapture);
        state.set_stdout(stdout.clone());
        state.set_stderr(stdout.clone());

        let (task, _) = spawn_wasm(
            env,
            runtime.clone(),
            &module,
            state,
            &test_function.wasm_export_name,
            Vec::new(),
            None,
        )
        .await
        .context(format!(
            "Failed to spawn process from {}::{}",
            path.to_string_lossy(),
            test_function.function_name
        ))?;

        let sender = sender.clone();
        let nocapture = args.nocapture;

        tokio::task::spawn(async move {
            let result = match task.await.unwrap() {
                Ok(_state) => {
                    // If we didn't expect a panic and didn't get one
                    if test_function.panic.is_none() {
                        TestResult {
                            name: test_function.function_name,
                            status: TestStatus::Ok,
                            stdout,
                        }
                    } else {
                        // If we expected a panic, but didn't get one
                        stdout.push_str("note: test did not panic as expected\n");
                        TestResult {
                            name: test_function.function_name,
                            status: TestStatus::PanicFailed,
                            stdout,
                        }
                    }
                }
                Err(_err) => {
                    // Find panic output
                    let panic_regex =
                    // Modes:
                    // * m: ^ and $ match begin/end of line (not string)
                    // * s: allow . to match \n
                    regex::Regex::new("(?ms)^thread '.*' panicked at '(.*)', ").unwrap();

                    let content = stdout.content();
                    let panic_detected = panic_regex.captures(&content);

                    // If we didn't expect a panic, but got one or were killed by a signal
                    if test_function.panic.is_none() {
                        // In case of --nocapture the regex will never match (content is empty).
                        // At this point we can't be certain if there was a panic.
                        if panic_detected.is_none() && !nocapture {
                            stdout.push_str("note: Process trapped or received kill signal\n");
                        }
                        TestResult {
                            name: test_function.function_name,
                            status: TestStatus::Failed,
                            stdout,
                        }
                    } else {
                        match panic_detected {
                            Some(panic) => {
                                // `test_function.panic` is always `Some` in this branch.
                                let expected_panic = test_function.panic.unwrap();
                                let panic_message = panic.get(1).map_or("", |m| m.as_str());
                                if panic_message.contains(&expected_panic) {
                                    TestResult {
                                        name: test_function.function_name,
                                        status: TestStatus::PanicOk,
                                        stdout,
                                    }
                                } else {
                                    let note = format!(
                                        "note: panic did not contain expected string\n      panic message: `\"{}\"`,\n expected substring: `\"{}\"`\n",
                                        panic_message,
                                        expected_panic
                                    );
                                    stdout.push_str(&note);
                                    TestResult {
                                        name: test_function.function_name,
                                        status: TestStatus::PanicFailed,
                                        stdout,
                                    }
                                }
                            }

                            // Process didn't panic, but was killed by a signal.
                            None => TestResult {
                                name: test_function.function_name,
                                // This is only considered a success if the `expected` panic string
                                // didn't contain anything.
                                status: if test_function.panic.as_ref().unwrap() == "" {
                                    TestStatus::PanicOk
                                } else {
                                    stdout.push_str(
                                        &format!(
                                            "note: Process received kill signal, but expected a panic that contains `{}`\n",
                                            test_function.panic.unwrap()
                                        )
                                    );
                                    TestStatus::PanicFailed
                                },
                                stdout,
                            },
                        }
                    }
                }
            };
            sender.send(result).unwrap();
        });
    }

    let mut ignored = 0;
    let mut successes = Vec::new();
    let mut failures = Vec::new();

    // Wait for all tests to finish
    for _ in 0..n {
        let result = receiver.recv().await.unwrap();
        let name = result.name;
        match result.status {
            TestStatus::Ok => {
                println!("test {} ... \x1b[92mok\x1b[0m", name); // green ok
                successes.push((name, result.stdout));
            }
            TestStatus::Failed => {
                println!("test {} ... \x1b[91mFAILED\x1b[0m", name); // red FAIL
                failures.push((name, result.stdout));
            }
            TestStatus::PanicOk => {
                println!("test {} - should panic ... \x1b[92mok\x1b[0m", name); // green ok
                successes.push((name, result.stdout));
            }
            TestStatus::PanicFailed => {
                println!("test {} - should panic ... \x1b[91mFAILED\x1b[0m", name); // red FAIL
                failures.push((name, result.stdout));
            }
            TestStatus::Ignored => {
                println!("test {} ... \x1b[93mignored\x1b[0m", name); // yellow ignored
                ignored += 1;
            }
        }
    }

    // If --show-output is present, print success outputs if they are not empty
    if args.show_output {
        println!("\nsuccesses:");
        // Print stdout of successes
        for (success, stdout) in successes.iter() {
            if !stdout.is_empty() {
                println!("\n---- {} stdout ----", success);
                print!("{}", stdout);
            }
        }

        println!("\nsuccesses:");
        for (success, _) in successes.iter() {
            println!("    {}", success);
        }
    }

    if !failures.is_empty() {
        println!("\nfailures:");
    }

    // Print stdout of failures if they are not empty
    for (failure, stdout) in failures.iter() {
        if !stdout.is_empty() {
            println!("\n---- {} stdout ----", failure);
            print!("{}", stdout);
        }
    }

    // List failures
    if !failures.is_empty() {
        println!("\nfailures:");
    }
    for (failure, _) in failures.iter() {
        println!("    {}", failure);
    }

    // List all failures

    let result = if failures.is_empty() {
        "\x1b[92mok\x1b[0m"
    } else {
        "\x1b[91mFAILED\x1b[0m"
    };
    println!(
        "\ntest result: {}. {} passed; {} failed; {} ignored; 0 measured; {} filtered out; finished in {:.2}s\n",
        result, successes.len(), failures.len(), ignored, filtered_out, now.elapsed().as_millis() as f64 / 1000f64
    );

    if failures.is_empty() {
        Ok(())
    } else {
        // Indicate to cargo that at least one test failed
        std::process::exit(1);
    }
}

#[derive(Debug)]
struct Test {
    wasm_export_name: String,
    function_name: String,
    panic: Option<String>,
    filtered: bool,
    ignored: bool,
}

#[derive(Debug)]
struct TestResult {
    name: String,
    stdout: StdoutCapture,
    status: TestStatus,
}

#[derive(Debug)]
enum TestStatus {
    Ok,
    Failed,
    PanicOk,
    PanicFailed,
    Ignored,
}
