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
}

/// Exit code for a failed operation (validation/connection).
pub const EXIT_FAILURE: i32 = 1;
/// Exit code for a usage error.
pub const EXIT_USAGE: i32 = 2;

/// Runs the parsed command. Blocking; returns the process exit code.
pub fn execute(command: Command) -> i32 {
    match command {
        Command::Run { config } => run(config),
        Command::Check { config } => check(&config),
        Command::Status { url } => status(&url),
        Command::Health { url } => health(&url),
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

fn check(config: &str) -> i32 {
    match load_config(Some(config)) {
        Ok(config) => {
            if let Err(error) = config.validate() {
                eprintln!("{error}");
                return EXIT_FAILURE;
            }
            println!("ok");
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
