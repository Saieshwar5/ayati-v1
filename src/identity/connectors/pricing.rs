//! What one call through a connector provider is estimated to cost Cuckoo,
//! recorded on every call (schema 110) and summed in Cuckoo Control.
//!
//! The price comes from the providers file (`AYATI_CONNECTOR_PROVIDERS_FILE`,
//! each provider's `usd_per_1k_calls`), which the provider routing work adds.
//! Until it is wired in here every call is estimated at zero, so Control's cost
//! reads $0.00 rather than a guess.

/// Estimated US dollars for one call that reached `provider`.
pub(crate) fn usd_per_call(_provider: &str) -> f64 {
    0.0
}
