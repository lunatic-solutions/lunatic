use std::{env, fs, path::Path, sync::Arc, time::Instant};

use anyhow::{Context, Result};
use clap::{crate_version, Arg, Command};

use lunatic_process::{runtimes, state::ProcessState};
use lunatic_process_api::ProcessConfigCtx;
use lunatic_runtime::{spawn_wasm, DefaultProcessConfig, DefaultProcessState};
use lunatic_stdout_capture::StdoutCapture;
use lunatic_wasi_api::LunaticWasiCtx;

pub(crate) async fn test() -> Result<()> {
    // Set logger level to "error" to avoid printing process failures warnings during tests.
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("error")).init();
    // Measure test duration
    let now = Instant::now();

    // Parse command line arguments
    let args = Command::new("lunatic")
        .version(crate_version!())
        .arg(
            Arg::new("wasm")
                .value_name("WASM")
                .help("Entry .wasm file")
                .required(true),
        )
        .arg(
            Arg::new("filter")
                .value_name("FILTER")
                .help("Run only tests that contain the filter string")
                .required(false),
        )
        .arg(
            Arg::new("dir")
                .long("dir")
                .value_name("DIRECTORY")
                .help("Grant access to the given host directory")
                .multiple_occurrences(true)
                .takes_value(true),
        )
        .arg(
            Arg::new("ignored")
                .long("ignored")
                .help("Run only ignored tests")
                .required(false),
        )
        .arg(
            Arg::new("nocapture")
                .long("nocapture")
                .help("Don't hide output from test executions")
                .required(false)
                .takes_value(false),
        )
        .arg(
            Arg::new("showoutput")
                .long("show-output")
                .help("Show also the output of successfull tests")
                .required(false)
                .takes_value(false),
        )
        .arg(
            Arg::new("wasm_args")
                .value_name("WASM_ARGS")
                .help("Arguments passed to the guest")
                .required(false)
                .multiple_values(true),
        )
        .get_matches();

    let mut config = DefaultProcessConfig::default();
    // Allow initial process to compile modules, create configurations and spawn sub-processes
    config.set_can_compile_modules(true);
    config.set_can_create_configs(true);
    config.set_can_spawn_processes(true);

    // Set correct command line arguments for the guest
    let wasi_args = args
        .values_of("wasm_args")
        .unwrap_or_default()
        .map(|arg| arg.to_string())
        .collect();
    config.set_command_line_arguments(wasi_args);

    // Inherit environment variables
    config.set_environment_variables(env::vars().collect());

    // Always preopen the current dir
    config.preopen_dir(".");
    if let Some(dirs) = args.values_of("dir") {
        for dir in dirs {
            config.preopen_dir(dir);
        }
    }

    // Create wasmtime runtime
    let wasmtime_config = runtimes::wasmtime::default_config();
    let runtime = runtimes::wasmtime::WasmtimeRuntime::new(&wasmtime_config)?;

    // Load and compile wasm module
    let path = args.value_of("wasm").unwrap();
    let path = Path::new(path);
    let module = fs::read(path)?;
    let module = runtime.compile_module::<DefaultProcessState>(module)?;

    let filter = args.value_of("filter").unwrap_or_default();

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
                if args.is_present("ignored") {
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
                    // TODO: Handle escaped `\#`
                    let panic: String = name.chars().take_while(|c| c.ne(&'#')).collect();
                    let panic_prefix = format!("{}#", panic);
                    let function_name = name.strip_prefix(&panic_prefix).unwrap().to_string();
                    Test {
                        filtered: !function_name.contains(filter),
                        wasm_export_name: wasm_export_name.to_string(),
                        function_name,
                        panic: Some(panic),
                        ignored,
                    }
                } else {
                    Test {
                        filtered: !name.contains(filter),
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

    let n = test_functions.iter().filter(|test| !test.filtered).count();
    let filtered_out = test_functions.len() - n;
    println!("\nrunning {} {}", n, if n == 1 { "test" } else { "tests" });

    let (sender, receiver) = async_std::channel::unbounded();

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
                    stdout: StdoutCapture::new(),
                })
                .await
                .unwrap();
            continue;
        }

        let mut state =
            DefaultProcessState::new(runtime.clone(), module.clone(), config.clone()).unwrap();
        // Use in-memory stdout & stderr to hide output in case of success.
        let stdout = StdoutCapture::new();
        state.set_stdout(stdout.clone());
        state.set_stderr(stdout.clone());

        let (task, _) = spawn_wasm(
            runtime.clone(),
            module.clone(),
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
        async_std::task::spawn(async move {
            let result = match task.await {
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
                    // If we didn't expect a panic, but got one
                    if test_function.panic.is_none() {
                        TestResult {
                            name: test_function.function_name,
                            status: TestStatus::Failed,
                            stdout,
                        }
                    } else {
                        // Find panic output
                        let panic_regex =
                            // Modes:
                            // * m: ^ and $ match begin/end of line (not string)
                            // * s: allow . to match \n
                            regex::Regex::new("(?ms)^thread '.*' panicked at '(.*)', ").unwrap();

                        let content = stdout.content();
                        let panic = panic_regex.captures(&content);
                        match panic {
                            Some(panic) => {
                                let expected_panic = match test_function.panic {
                                    Some(text) => text,
                                    None => String::from(""),
                                };
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
                                status: TestStatus::PanicFailed,
                                stdout,
                            },
                        }
                    }
                }
            };
            sender.send(result).await.unwrap();
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
                // If --nocapture is present, print all output
                if args.is_present("nocapture") {
                    print!("{}", result.stdout);
                }
                println!("test {} ... \x1b[92mok\x1b[0m", name); // green ok
                successes.push((name, result.stdout));
            }
            TestStatus::Failed => {
                // If --nocapture is present, print all output
                if args.is_present("nocapture") {
                    print!("{}", result.stdout);
                }
                println!("test {} ... \x1b[91mFAILED\x1b[0m", name); // red FAIL
                failures.push((name, result.stdout));
            }
            TestStatus::PanicOk => {
                // If --nocapture is present, print all output
                if args.is_present("nocapture") {
                    print!("{}", result.stdout);
                }
                println!("test {} - should panic ... \x1b[92mok\x1b[0m", name); // green ok
                successes.push((name, result.stdout));
            }
            TestStatus::PanicFailed => {
                // If --nocapture is present, print all output
                if args.is_present("nocapture") {
                    print!("{}", result.stdout);
                }
                println!("test {} - should panic ... \x1b[91mFAILED\x1b[0m", name); // red FAIL
                failures.push((name, result.stdout));
            }
            TestStatus::Ignored => {
                println!("test {} ... \x1b[93mignored\x1b[0m", name); // yellow ignored
                ignored += 1;
            }
        }
    }

    // If --show-output is present, print success outputs
    if args.is_present("showoutput") {
        println!("\nsuccesses:");
        if !args.is_present("nocapture") {
            // Print stdout of failures
            for (success, stdout) in successes.iter() {
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

    if !args.is_present("nocapture") {
        // Print stdout of failures
        for (failure, stdout) in failures.iter() {
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
