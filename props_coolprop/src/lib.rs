//! CoolProp-backed properties provider stub.
//!
//! Task A intentionally provides only a placeholder API surface. Real CoolProp
//! integration arrives in a later roadmap task.

/// Placeholder provider type for future CoolProp integration.
#[derive(Debug, Default, Clone, Copy)]
pub struct CoolPropProvider;

impl CoolPropProvider {
    /// Returns whether the provider is wired to a real backend.
    #[must_use]
    pub const fn is_stub(self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::CoolPropProvider;

    #[test]
    fn provider_is_stub() {
        assert!(CoolPropProvider.is_stub());
    }
}
