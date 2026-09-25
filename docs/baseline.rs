//! Baseline load driver (spec 10): sends N batches over HTTP and reports
//! achieved throughput and client-observed latency. Run manually against a
//! started engine; results are recorded in the progress tracker.

use std::io::{Read, Write};
use std::time::{Duration, Instant};

fn http_post(url: &str, body: &str) -> Result<u16, String> {
    let rest = url
        .strip_prefix("http://")
        .ok_or_else(|| "url must be http://".to_string())?;
    // Split at the FIRST slash *after* the authority: authority runs to the
    // first '/', and everything from it is the path.
    let (authority, path) = match rest.find('/') {
        Some(index) => (&rest[..index], rest[index..].to_string()),
        None => (rest, "/".to_string()),
    };
    let (host, port) = authority
        .rsplit_once(':')
        .ok_or_else(|| "url needs :port".to_string())?;
    let port: u16 = port.parse().map_err(|_| "bad port".to_string())?;

    let mut stream = std::net::TcpStream::connect((host, port))
        .map_err(|e| format!("connect: {e}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .map_err(|e| e.to_string())?;
    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: {authority}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(request.as_bytes()).map_err(|e| e.to_string())?;
    let mut response = String::new();
    stream.read_to_string(&mut response).map_err(|e| e.to_string())?;
    let status = response
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|code| code.parse::<u16>().ok())
        .ok_or_else(|| "bad response".to_string())?;
    Ok(status)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let url = args
        .get(1)
        .cloned()
        .unwrap_or_else(|| "http://127.0.0.1:8080/v1/events".to_string());
    let batches: usize = args
        .get(2)
        .and_then(|v| v.parse().ok())
        .unwrap_or(200);
    let batch_size: usize = args.get(3).and_then(|v| v.parse().ok()).unwrap_or(50);

    let mut latencies: Vec<u128> = Vec::with_capacity(batches);
    let started = Instant::now();
    let mut failures = 0u64;
    let mut events = String::with_capacity(batch_size * 128);
    for batch in 0..batches {
        events.clear();
        events.push_str("{\"events\":[");
        for i in 0..batch_size {
            if i > 0 {
                events.push(',');
            }
            events.push_str(&format!(
                "{{\"id\":\"base-{batch}-{i}\",\"source\":\"baseline/driver\",\"kind\":\"baseline.reading\",\"timestamp\":{},\"payload\":{{\"type\":\"numeric\",\"value\":{}}}}}",
                1_769_412_000_000i64 + (batch * batch_size + i) as i64,
                (batch * batch_size + i) as f64 % 100.0,
            ));
        }
        events.push_str("]}");

        let t0 = Instant::now();
        match http_post(&url, &events) {
            Ok(202) => latencies.push(t0.elapsed().as_millis()),
            Ok(status) => {
                failures += 1;
                eprintln!("batch {batch}: unexpected status {status}");
            }
            Err(error) => {
                failures += 1;
                eprintln!("batch {batch}: {error}");
            }
        }
    }
    let total = started.elapsed();

    latencies.sort();
    let p = |frac: f64| -> u128 {
        latencies
            .get(((frac * latencies.len() as f64).ceil() as usize).saturating_sub(1))
            .copied()
            .unwrap_or(0)
    };
    let total_events = batches.saturating_sub(failures as usize) * batch_size;
    println!(
        "baseline: {} batches x {} events, failures {}, wall {:.2?}, throughput {:.0} events/s, latency ms avg {:.1} p95 {} p99 {} max {}",
        batches,
        batch_size,
        failures,
        total,
        total_events as f64 / total.as_secs_f64(),
        latencies.iter().sum::<u128>() as f64 / latencies.len().max(1) as f64,
        p(0.95),
        p(0.99),
        latencies.last().copied().unwrap_or(0),
    );
}
