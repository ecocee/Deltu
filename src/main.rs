//! The Deltu executable: `deltu run [--config <path>]` (spec 07 CLI v0).
//! Flags `--version` / `--help` are handled before any runtime work.

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // Minimal flag parsing (clap arrives with the CLI unit, spec 09).
    match args.get(1).map(String::as_str) {
        Some("--version") | Some("-V") => {
            println!("deltu {}", deltu::version());
            return;
        }
        Some("--help") | Some("-h") => {
            print_help();
            return;
        }
        Some("run") => {}
        Some(other) => {
            eprintln!("unknown command: {other}\n");
            print_help();
            std::process::exit(2);
        }
        None => {
            print_help();
            return;
        }
    }

    // Parse `run` flags.
    let mut config_path: Option<String> = None;
    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--config" => {
                let Some(path) = args.get(i + 1) else {
                    eprintln!("--config requires a path argument");
                    std::process::exit(2);
                };
                config_path = Some(path.clone());
                i += 2;
            }
            other => {
                eprintln!("unknown flag: {other}");
                std::process::exit(2);
            }
        }
    }

    // Load config: explicit file or defaults.
    let config = match config_path.as_deref() {
        Some(path) => {
            let mut config = match deltu::RuntimeConfig::from_file(path) {
                Ok(config) => config,
                Err(error) => {
                    eprintln!("{error}");
                    std::process::exit(1);
                }
            };
            let env_vars: Vec<(String, String)> = std::env::vars()
                .filter(|(k, _)| k.starts_with("DELTU_"))
                .collect();
            if let Err(error) = config.apply_env_overrides(&env_vars) {
                eprintln!("{error}");
                std::process::exit(1);
            }
            config
        }
        None => deltu::RuntimeConfig::default(),
    };

    if let Err(error) = config.validate() {
        eprintln!("{error}");
        std::process::exit(1);
    }

    // Block the main thread on the async runtime.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    if let Err(error) = runtime.block_on(deltu::serve(config)) {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn print_help() {
    println!(
        "deltu {} — self-hosted continuous data processing engine

USAGE:
    deltu run [--config <path>]    start the engine (HTTP + processing)
    deltu --version                print version
    deltu --help                   print this help

ENVIRONMENT:
    DELTU_HTTP_BIND                override http.bind (e.g. 0.0.0.0:8080)
    DELTU_QUEUE_CAPACITY           override the work-queue depth",
        deltu::version()
    );
}
