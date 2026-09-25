//! The Deltu runtime: configuration, the processing worker, and the HTTP
//! service boundary. This is where decision 004 (deferred async runtime)
//! is retired — continuous network processing now justifies tokio.

pub mod config;
pub mod http;
#[cfg(test)]
mod http_tests;
pub mod worker;

pub use config::{ConfigError, HttpConfig, RuntimeConfig};
pub use http::serve;
pub use worker::{EngineCore, now_unix_ms};
