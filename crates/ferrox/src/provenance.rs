//! Ferrox's `ProvenanceSource` marker.
//!
//! Migrated to the [`converge_pack::ProvenanceSource`] trait. Public
//! surface is unchanged at call sites:
//! `FERROX_PROVENANCE.proposed_fact(...)` reads the same.
//!
//! The `converge-core` engine emits the uniform `suggestor.execute`
//! tracing span automatically around every `Suggestor::execute`
//! call. Suggestors override `Suggestor::provenance()` to return
//! `FERROX_PROVENANCE.as_str()` so the engine's span carries the
//! right origin.

use converge_pack::ProvenanceSource;

/// Marker type identifying ferrox-emitted facts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Ferrox;

impl ProvenanceSource for Ferrox {
    fn as_str(&self) -> &'static str {
        "ferrox"
    }
}

/// Canonical provenance constant for ferrox.
pub const FERROX_PROVENANCE: Ferrox = Ferrox;

#[cfg(test)]
mod tests {
    use super::*;
    use converge_pack::ContextKey;

    #[test]
    fn provenance_string_is_stable() {
        assert_eq!(FERROX_PROVENANCE.as_str(), "ferrox");
    }

    #[test]
    fn proposed_fact_uses_canonical_source_string() {
        let fact = FERROX_PROVENANCE.proposed_fact(
            ContextKey::Diagnostic,
            "diagnostic",
            converge_pack::TextPayload::new("content"),
        );
        assert_eq!(fact.provenance(), "ferrox");
    }
}
