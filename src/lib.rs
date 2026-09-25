//! Deltu core engine — foundation boundary.

pub mod event;

pub use event::{Event, EventError, Payload};

/// Returns the engine version from Cargo metadata.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    #[test]
    fn version_is_set() {
        assert!(!super::version().is_empty());
    }
}
