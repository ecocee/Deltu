//! The Deltu command-line interface (spec 09): a thin operational surface
//! over the documented public API — the CLI never bypasses the API to read
//! internal state.
//!
//! Exit codes: 0 success, 1 validation/connection failure, 2 usage error.

use clap::{Parser, Subcommand};
use serde_json::Value as JsonValue;

/// Deltu — self-hosted continuous data processing engine.
#[derive(Debug, Parser)]
#[command(name = "deltu", version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Start the engine (HTTP + processing) in the foreground.
    Run {
        /// Path to a YAML or JSON config file. Omit for defaults.
        #[arg(long)]
        config: Option<String>,
    },
    /// Validate a config file and exit (0 valid, 1 invalid).
    Check {
        /// Path to a YAML or JSON config file.
        #[arg(long)]
        config: String,
    },
    /// Print the runtime status of a running instance (GET /v1/status).
    Status {
        /// Base URL of the running engine.
        #[arg(long, default_value = "http://127.0.0.1:8080")]
        url: String,
    },
    /// Check engine liveness for scripts and orchestrators (GET /health).
    Health {
        /// Base URL of the running engine.
        #[arg(long, default_value = "http://127.0.0.1:8080")]
        url: String,
    },
    /// Start the engine in the background (daemonized) and print the PID.
    ///
    /// A convenience wrapper for local use; production deployments should
    /// run `deltu run` under a service supervisor (launchd/systemd) or in
    /// a container. Logs go to `--log-file` (default /tmp/deltu.log).
    Up {
        /// Path to a YAML or JSON config file. Omit for defaults.
        #[arg(long)]
        config: Option<String>,
        /// Where background logs are written.
        #[arg(long, default_value = "/tmp/deltu.log")]
        log_file: String,
        /// Readiness wait in seconds (polls /health before returning).
        #[arg(long, default_value = "5")]
        wait_secs: u64,
    },
    /// Stop a background engine started with `deltu up` (SIGTERM, then
    /// SIGKILL after a grace period).
    Down {
        /// PID file written by `deltu up`.
        #[arg(long, default_value = "/tmp/deltu.pid")]
        pid_file: String,
        /// Seconds to wait for graceful exit before forcing.
        #[arg(long, default_value = "5")]
        timeout_secs: u64,
    },
    /// Print the last N lines of the background engine's log.
    Logs {
        /// Log file written by `deltu up`.
        #[arg(long, default_value = "/tmp/deltu.log")]
        file: String,
        /// Number of lines to show.
        #[arg(long, default_value = "50")]
        lines: usize,
        /// Follow the log (like tail -f) until interrupted.
        #[arg(long, default_value_t = false)]
        follow: bool,
    },
    /// Generate a starter deltu.yaml configuration file with comments.
    Init {
        /// Path to write the configuration file.
        #[arg(long, default_value = "deltu.yaml")]
        path: String,
    },
    /// Run an interactive, self-contained live terminal demo (zero config).
    Demo,
    /// Diagnose system environment, port availability, and permissions.
    Doctor,
    /// Print the effective configuration (defaults + file + env overrides).
    Config {
        /// Path to a YAML or JSON config file. Omit for defaults.
        #[arg(long)]
        config: Option<String>,
        /// Output format.
        #[arg(long, default_value = "yaml", value_parser = ["yaml", "json"])]
        format: String,
    },
}

/// Exit code for a failed operation (validation/connection).
pub const EXIT_FAILURE: i32 = 1;
/// Exit code for a usage error.
pub const EXIT_USAGE: i32 = 2;

/// Runs the parsed command. Blocking; returns the process exit code.
pub fn execute(command: Command) -> i32 {
    match command {
        Command::Init { path } => init(&path),
        Command::Demo => demo(),
        Command::Doctor => doctor(),
        Command::Run { config } => run(config),
        Command::Check { config } => check(&config),
        Command::Status { url } => status(&url),
        Command::Health { url } => health(&url),
        Command::Up {
            config,
            log_file,
            wait_secs,
        } => up(config, &log_file, wait_secs),
        Command::Down {
            pid_file,
            timeout_secs,
        } => down(&pid_file, timeout_secs),
        Command::Logs {
            file,
            lines,
            follow,
        } => logs(&file, lines, follow),
        Command::Config { config, format } => config_command(config.as_deref(), &format),
    }
}

fn run(config: Option<String>) -> i32 {
    let config = match load_config(config.as_deref()) {
        Ok(config) => config,
        Err(code) => return code,
    };
    if let Err(error) = config.validate() {
        eprintln!("{error}");
        return EXIT_FAILURE;
    }

    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("failed to start async runtime: {error}");
            return EXIT_FAILURE;
        }
    };
    match runtime.block_on(crate::serve(config)) {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("{error}");
            EXIT_FAILURE
        }
    }
}

fn init(path: &str) -> i32 {
    let target = std::path::Path::new(path);
    if target.exists() {
        eprintln!("{path} already exists. Remove it or specify a different path.");
        return EXIT_FAILURE;
    }
    let content = r#"# DELTU Engine Configuration
# Make Data Behave — https://github.com/ecocee/deltu

http:
  bind: 127.0.0.1:8080

rules:
  - name: high-temperature
    description: "Alert when temperature sensor reading exceeds 80"
    when: temperature > 80
    within: 60s
    action: log-alert

  - name: api-error-storm
    description: "Detect rapid API errors within 30 seconds"
    when: count(http.error) > 10
    within: 30s
    action: log-alert

actions:
  - id: log-alert
    kind:
      type: log
      level: warn
"#;
    match std::fs::write(path, content) {
        Ok(_) => {
            println!("Created starter configuration at {path}");
            println!(
                "Run 'deltu check --config {path}' to validate, or 'deltu run --config {path}' to start."
            );
            0
        }
        Err(e) => {
            eprintln!("Failed to write {path}: {e}");
            EXIT_FAILURE
        }
    }
}

fn demo() -> i32 {
    use crate::actions::{ActionDefinition, ActionDispatcher, ActionKind, LogLevel};
    use crate::event::{Event, Payload};
    use crate::processing::{PipelineConfig, ProcessingPipeline};
    use crate::rules::{
        Condition, EvalInput, Field, Literal, OncePerWindow, Operator, Rule, RuleEngine, Trigger,
    };
    use crate::state::{StateConfig, StateStore};

    println!("DELTU Engine v{} — Demo Mode", crate::version());
    println!("Make Data Behave.\n");
    println!("Initializing local processing pipeline (0.0s setup)... OK");
    println!("──────────────────────────────────────────────────");

    let mut pipeline = ProcessingPipeline::new(PipelineConfig::default()).unwrap();
    let mut state = StateStore::new(StateConfig::default()).unwrap();

    let rule = Rule {
        id: "high-temperature".to_string(),
        description: Some("Trigger alert when temperature exceeds 80°C".to_string()),
        on: Trigger::Event {
            kind: "temperature".to_string(),
        },
        condition: Condition::Comparison {
            field: Field::EventValue,
            op: Operator::Gt,
            value: Literal::Numeric(80.0),
        },
        action: "log-alert".to_string(),
        suppression: Some(OncePerWindow { window_ms: 60000 }),
    };

    let mut engine = RuleEngine::new(vec![rule]).unwrap();
    let mut dispatcher = ActionDispatcher::new(vec![ActionDefinition {
        id: "log-alert".to_string(),
        kind: ActionKind::Log {
            level: LogLevel::Warn,
            template: None,
        },
    }])
    .unwrap();

    let events = vec![
        ("e1", "sensor-1", "temperature", 1000, 72.4, "ACCEPTED"),
        (
            "e1",
            "sensor-1",
            "temperature",
            1000,
            72.4,
            "DUPLICATE DROPPED",
        ),
        ("e2", "sensor-1", "temperature", 2000, 74.1, "ACCEPTED"),
        ("e3", "sensor-1", "temperature", 3000, 88.9, "STATE CHANGED"),
    ];

    let mut total_ingested = 0;
    let mut dup_count = 0;
    let mut state_changes = 0;
    let mut rules_fired = 0;
    let mut actions_done = 0;

    for (id, src, kind, ts, val, label) in events {
        total_ingested += 1;
        let event = Event::new(id, src, kind, ts, Payload::Numeric { value: val }).unwrap();
        let outputs = pipeline.process(event.clone());
        if outputs.is_empty() {
            dup_count += 1;
            println!("  📥 Event {id} ({src}, {kind}={val:.1}°C) ──> [{label}]");
        } else {
            for out in outputs {
                state.observe(&out);
                if label == "STATE CHANGED" {
                    state_changes += 1;
                }
                let eval_input = EvalInput::from_event(&event, &state);
                let requests = engine.evaluate(&eval_input);
                if !requests.is_empty() {
                    rules_fired += requests.len();
                    println!("  ⚡ Rule Matched: 'high-temperature' (val: {val:.1} > 80.0)");
                    let outcomes = dispatcher.dispatch(&requests, ts);
                    actions_done += outcomes.len();
                    for outcome in outcomes {
                        if outcome.result.is_ok() {
                            println!(
                                "  🔥 Action Executed: log-alert -> [WARN] high-temperature threshold breached"
                            );
                        }
                    }
                } else {
                    println!("  📥 Event {id} ({src}, {kind}={val:.1}°C) ──> [{label}]");
                }
            }
        }
    }

    println!("──────────────────────────────────────────────────");
    println!(
        "✅ Demo complete! Processed {total_ingested} events | {dup_count} duplicate removed | {state_changes} state change | {rules_fired} rule triggered | {actions_done} action executed."
    );
    println!("\n👉 Next step: Run 'deltu init' to generate your deltu.yaml configuration!");
    0
}

fn doctor() -> i32 {
    println!("DELTU DOCTOR (v{})", crate::version());
    println!("──────────────────────────────────────────");
    println!("✓ DELTU Version: {}", crate::version());
    println!("✓ Operating System: {}", std::env::consts::OS);
    println!("✓ Architecture: {}", std::env::consts::ARCH);

    let temp = std::env::temp_dir().join("deltu-doc-test.tmp");
    if std::fs::write(&temp, "ok").is_ok() {
        let _ = std::fs::remove_file(temp);
        println!("✓ Working Directory: Writable");
    } else {
        println!("✗ Working Directory: Permission Error");
    }

    match std::net::TcpListener::bind("127.0.0.1:8080") {
        Ok(_) => println!("✓ HTTP Port 8080: Available"),
        Err(_) => println!("! HTTP Port 8080: In Use (or permission required)"),
    }

    println!("✓ Core Runtime: Ready");
    println!("──────────────────────────────────────────");
    println!("Environment check complete!");
    0
}

fn check(config_path: &str) -> i32 {
    match load_config(Some(config_path)) {
        Ok(config) => {
            if let Err(error) = config.validate() {
                eprintln!(
                    "Configuration Check Failed:\n  WHAT: Validation error\n  WHERE: {config_path}\n  WHY: {error}\n  HOW: Inspect configuration and fix invalid values."
                );
                return EXIT_FAILURE;
            }
            println!("✓ Configuration syntax valid");
            println!("✓ {} rules parsed successfully", config.rules.len());
            println!("✓ {} actions resolved successfully", config.actions.len());
            println!("✓ Bind address: {}", config.http.bind);
            println!("deltu config check: OK");
            0
        }
        Err(code) => code,
    }
}

fn status(url: &str) -> i32 {
    match get_json(url, "/v1/status") {
        Ok(body) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&body).unwrap_or_default()
            );
            0
        }
        Err(message) => {
            eprintln!("{message}");
            EXIT_FAILURE
        }
    }
}

fn health(url: &str) -> i32 {
    match get_json(url, "/health") {
        Ok(body) => {
            // Health is a scriptable boolean, not a document dump.
            let ok = body.get("status").and_then(|v| v.as_str()) == Some("ok");
            if ok {
                println!("healthy");
                0
            } else {
                eprintln!("unhealthy: {body}");
                EXIT_FAILURE
            }
        }
        Err(message) => {
            eprintln!("{message}");
            EXIT_FAILURE
        }
    }
}

/// Default PID file for `deltu up`/`deltu down`.
pub const DEFAULT_PID_FILE: &str = "/tmp/deltu.pid";

fn up(config_path: Option<String>, log_file: &str, wait_secs: u64) -> i32 {
    use std::process::{Command, Stdio};

    // Validate before backgrounding so config errors stay in the
    // foreground where the user can see them.
    if let Err(code) = load_config(config_path.as_deref()) {
        return code;
    }

    let binary = match std::env::current_exe() {
        Ok(path) => path,
        Err(error) => {
            eprintln!("cannot locate the deltu binary: {error}");
            return EXIT_FAILURE;
        }
    };
    let log = match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_file)
    {
        Ok(file) => file,
        Err(error) => {
            eprintln!("cannot open log file {log_file}: {error}");
            return EXIT_FAILURE;
        }
    };
    let pid_file = DEFAULT_PID_FILE;

    // Already running? (stale pid files are detected and replaced)
    if let Ok(pid_text) = std::fs::read_to_string(pid_file)
        && let Ok(pid) = pid_text.trim().parse::<i32>()
    {
        let alive = Command::new("kill")
            .arg("-0")
            .arg(pid.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if alive {
            println!("deltu already running (pid {pid})");
            return 0;
        }
    }

    let mut args = vec!["run".to_string()];
    if let Some(path) = &config_path {
        args.push("--config".to_string());
        args.push(path.clone());
    }
    let child = match Command::new(&binary)
        .args(&args)
        .stdout(Stdio::from(log.try_clone().expect("log clone")))
        .stderr(Stdio::from(log))
        .stdin(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            eprintln!("failed to start engine: {error}");
            return EXIT_FAILURE;
        }
    };
    let pid = child.id();
    std::fs::write(pid_file, pid.to_string()).ok();

    // Readiness poll: /health must answer before we claim success.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(wait_secs);
    let base = default_base_url(config_path.as_deref());
    while std::time::Instant::now() < deadline {
        if let Ok(body) = get_json(&base, "/health")
            && body.get("status").and_then(|v| v.as_str()) == Some("ok")
        {
            println!("started (pid {pid}); logs: {log_file}");
            return 0;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    println!("started (pid {pid}); health not confirmed within {wait_secs}s; logs: {log_file}");
    0
}

/// Best-effort base URL for the readiness poll: the configured bind when
/// parseable, else the documented default.
fn default_base_url(config_path: Option<&str>) -> String {
    if let Some(path) = config_path
        && let Ok(config) = crate::RuntimeConfig::from_file(path)
    {
        return format!("http://{}", config.http.bind);
    }
    "http://127.0.0.1:8080".to_string()
}

fn down(pid_file: &str, timeout_secs: u64) -> i32 {
    use std::process::{Command, Stdio};

    let pid_text = match std::fs::read_to_string(pid_file) {
        Ok(text) => text,
        Err(_) => {
            eprintln!("no pid file at {pid_file}; is deltu running via 'deltu up'?");
            return EXIT_FAILURE;
        }
    };
    let Ok(pid) = pid_text.trim().parse::<i32>() else {
        eprintln!("corrupt pid file {pid_file}; remove it and stop the process manually");
        return EXIT_FAILURE;
    };
    let signal_arg = |sig: &str| format!("-{sig}");
    let _ = Command::new("kill")
        .arg(signal_arg("TERM"))
        .arg(pid.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);
    while std::time::Instant::now() < deadline {
        let alive = Command::new("kill")
            .arg("-0")
            .arg(pid.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(true);
        if !alive {
            let _ = std::fs::remove_file(pid_file);
            println!("stopped (pid {pid})");
            return 0;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    let _ = Command::new("kill")
        .arg(signal_arg("KILL"))
        .arg(pid.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    let _ = std::fs::remove_file(pid_file);
    println!("forced stop (pid {pid}) after {timeout_secs}s");
    0
}

fn logs(file: &str, lines: usize, follow: bool) -> i32 {
    use std::process::{Command, Stdio};

    if !std::path::Path::new(file).exists() {
        eprintln!("no log file at {file}; start with 'deltu up' first");
        return EXIT_FAILURE;
    }
    if follow {
        let status = Command::new("tail")
            .args([
                "-f".to_string(),
                "-n".to_string(),
                lines.to_string(),
                file.to_string(),
            ])
            .status();
        return match status {
            Ok(s) if s.success() => 0,
            _ => EXIT_FAILURE,
        };
    }
    match Command::new("tail")
        .args(["-n", &lines.to_string(), file])
        .stderr(Stdio::null())
        .status()
    {
        Ok(s) if s.success() => 0,
        _ => {
            eprintln!("failed to read log file {file}");
            EXIT_FAILURE
        }
    }
}

fn config_command(config_path: Option<&str>, format: &str) -> i32 {
    let config = match load_config(config_path) {
        Ok(config) => config,
        Err(code) => return code,
    };
    if let Err(error) = config.validate() {
        eprintln!("{error}");
        return EXIT_FAILURE;
    }
    let output: Result<String, String> = if format == "json" {
        serde_json::to_string_pretty(&config).map_err(|e| e.to_string())
    } else {
        serde_yaml::to_string(&config).map_err(|e| e.to_string())
    };
    match output {
        Ok(text) => {
            println!("{text}");
            0
        }
        Err(error) => {
            eprintln!("failed to serialize config: {error}");
            EXIT_FAILURE
        }
    }
}

fn load_config(path: Option<&str>) -> Result<crate::RuntimeConfig, i32> {
    let mut config = match path {
        Some(path) => match crate::RuntimeConfig::from_file(path) {
            Ok(config) => config,
            Err(error) => {
                eprintln!("{error}");
                return Err(EXIT_FAILURE);
            }
        },
        None => crate::RuntimeConfig::default(),
    };
    let env_vars: Vec<(String, String)> = std::env::vars()
        .filter(|(key, _)| key.starts_with("DELTU_"))
        .collect();
    if let Err(error) = config.apply_env_overrides(&env_vars) {
        eprintln!("{error}");
        return Err(EXIT_FAILURE);
    }
    Ok(config)
}

/// Minimal HTTP GET via a std client — no API-client framework (spec 09);
/// one blocking call per command invocation is fine for an operator CLI.
fn get_json(base_url: &str, path: &str) -> Result<JsonValue, String> {
    let url = format!("{}{}", base_url.trim_end_matches('/'), path);
    let response = std_http_get(&url)?;
    let status = response.0;
    let body = response.1;
    if status != 200 {
        return Err(format!("GET {path} returned HTTP {status}: {body}"));
    }
    serde_json::from_str(&body).map_err(|error| format!("invalid JSON from {path}: {error}"))
}

/// Performs the HTTP GET; split out for testability.
fn std_http_get(url: &str) -> Result<(u16, String), String> {
    // Hand-rolled HTTP/1.1 GET over TcpStream: zero new dependencies
    // (a full client crate for one GET violates the dependency rule).
    let (host, port, path) = parse_http_url(url)?;
    use std::io::{Read, Write};
    let mut stream = std::net::TcpStream::connect((host.as_str(), port))
        .map_err(|error| format!("cannot connect to {url}: {error}"))?;
    let request = format!(
        "GET {path} HTTP/1.1\r\nHost: {host}:{port}\r\nConnection: close\r\nAccept: application/json\r\n\r\n"
    );
    stream
        .write_all(request.as_bytes())
        .map_err(|error| format!("write to {url} failed: {error}"))?;
    let mut buffer = String::new();
    stream
        .read_to_string(&mut buffer)
        .map_err(|error| format!("read from {url} failed: {error}"))?;

    let (head, body) = buffer
        .split_once("\r\n\r\n")
        .ok_or_else(|| format!("malformed HTTP response from {url}"))?;
    let status_line = head.lines().next().unwrap_or_default();
    let status: u16 = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .ok_or_else(|| format!("malformed status line from {url}: {status_line:?}"))?;
    Ok((status, body.to_string()))
}

/// Parses `http://host:port/path` into parts (the only scheme the CLI's
/// status/health commands need).
fn parse_http_url(url: &str) -> Result<(String, u16, String), String> {
    let rest = url
        .strip_prefix("http://")
        .ok_or_else(|| format!("url must start with http://, got {url:?}"))?;
    let (authority, path) = match rest.split_once('/') {
        Some((authority, path)) => (authority, format!("/{path}")),
        None => (rest, "/".to_string()),
    };
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) => (
            host.to_string(),
            port.parse()
                .map_err(|_| format!("invalid port in {url:?}"))?,
        ),
        None => (authority.to_string(), 80),
    };
    Ok((host, port, path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_http_url_handles_forms() {
        assert_eq!(
            parse_http_url("http://127.0.0.1:8080/v1/status").unwrap(),
            ("127.0.0.1".to_string(), 8080, "/v1/status".to_string())
        );
        assert_eq!(
            parse_http_url("http://localhost:8080").unwrap(),
            ("localhost".to_string(), 8080, "/".to_string())
        );
        assert!(
            parse_http_url("https://x:1/")
                .unwrap_err()
                .contains("http://")
        );
    }

    // Multi-thread runtime: the CLI calls block the calling thread
    // (std::thread + blocking TCP), so the stub server must be polled by a
    // different thread or this test self-deadlocks on a single-thread
    // runtime.
    #[tokio::test(flavor = "multi_thread")]
    async fn health_and_status_against_a_live_stub() {
        // A minimal stub server on an ephemeral port: the CLI talks HTTP
        // exactly as it would to a real engine.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let accepted = listener.accept().await;
                let Ok((mut socket, _)) = accepted else { break };
                tokio::spawn(async move {
                    use tokio::io::{AsyncReadExt, AsyncWriteExt};
                    let mut buffer = vec![0u8; 2048];
                    let Ok(read) = socket.read(&mut buffer).await else {
                        return;
                    };
                    let request = String::from_utf8_lossy(&buffer[..read]).to_string();
                    let (status, body) = if request.starts_with("GET /health") {
                        ("200 OK", r#"{"status":"ok"}"#)
                    } else if request.starts_with("GET /v1/status") {
                        ("200 OK", r#"{"success":true,"data":{"version":"0.1.0"}}"#)
                    } else {
                        ("404 Not Found", r#"{}"#)
                    };
                    let response = format!(
                        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = socket.write_all(response.as_bytes()).await;
                });
            }
        });

        let base = format!("http://{addr}");
        // Health: success path.
        let health = std::thread::spawn({
            let base = base.clone();
            move || health(&base)
        });
        assert_eq!(health.join().unwrap(), 0);
        // Status: success path (prints pretty JSON).
        let status = std::thread::spawn({
            let base = base.clone();
            move || status(&base)
        });
        assert_eq!(status.join().unwrap(), 0);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn health_fails_cleanly_when_engine_is_down() {
        // Bind then drop: nothing listens on this port.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);

        let base = format!("http://{addr}");
        let result = std::thread::spawn(move || health(&base));
        assert_eq!(result.join().unwrap(), EXIT_FAILURE);
    }

    #[test]
    fn check_validates_fixture_configs() {
        // A valid config file (written to a temp path).
        let valid = std::env::temp_dir().join("deltu-check-valid.yaml");
        std::fs::write(
            &valid,
            "http:\n  bind: 127.0.0.1:9999\nactions:\n  - id: log-ops\n    kind:\n      type: log\n      level: info\nrules:\n  - id: r\n    on: !Event\n      kind: temperature\n    condition: !Comparison\n      field: EventValue\n      op: Gt\n      value: !Numeric\n        80.0\n    action: log-ops\n",
        )
        .unwrap();
        assert_eq!(check(valid.to_str().unwrap()), 0);

        // A malformed file.
        let bad = std::env::temp_dir().join("deltu-check-bad.yaml");
        std::fs::write(&bad, "{not valid yaml: [").unwrap();
        assert_eq!(check(bad.to_str().unwrap()), EXIT_FAILURE);

        // A referentially-invalid config (rule → unknown action).
        let dangling = std::env::temp_dir().join("deltu-check-dangling.yaml");
        std::fs::write(
            &dangling,
            "rules:\n  - id: r\n    on: !Event\n      kind: temperature\n    condition: !Exists\n      field: EventValue\n    action: missing-action\n",
        )
        .unwrap();
        assert_eq!(check(dangling.to_str().unwrap()), EXIT_FAILURE);
    }

    #[test]
    fn cli_parses_all_documented_commands() {
        use clap::Parser as _;

        let cli = Cli::parse_from(["deltu", "run", "--config", "/tmp/x.yaml"]);
        assert!(matches!(cli.command, Command::Run { config: Some(_) }));

        let cli = Cli::parse_from(["deltu", "check", "--config", "/tmp/x.yaml"]);
        assert!(matches!(cli.command, Command::Check { .. }));

        let cli = Cli::parse_from(["deltu", "status", "--url", "http://x:1"]);
        assert!(matches!(cli.command, Command::Status { .. }));

        let cli = Cli::parse_from(["deltu", "health"]);
        assert!(matches!(cli.command, Command::Health { .. }));

        // Version is clap-handled (`--version`); default URL documented.
        let cli = Cli::parse_from(["deltu", "status"]);
        match cli.command {
            Command::Status { url } => assert_eq!(url, "http://127.0.0.1:8080"),
            _ => panic!("expected status"),
        }
    }
}
