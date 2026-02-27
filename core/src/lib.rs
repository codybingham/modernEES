//! Core domain crate for Modern EES.
//!
//! Task A intentionally keeps this crate minimal. Solver/parser/units work
//! will be introduced in later roadmap tasks.

/// Returns the crate readiness marker for Task A bootstrapping.
#[must_use]
pub const fn crate_status() -> &'static str {
    "core-ready"
}

#[cfg(test)]
mod tests {
    use super::crate_status;

    #[test]
    fn crate_status_is_set() {
        assert_eq!(crate_status(), "core-ready");
    }
}
