//! What one call through a connector provider is estimated to cost Cuckoo,
//! recorded on every call (schema 110) and summed in Cuckoo Control. The price
//! is each provider's `usd_per_1k_calls` in the providers file
//! (`AYATI_CONNECTOR_PROVIDERS_FILE`); without one, calls are estimated at zero
//! so Control reads $0.00 rather than a guess.

/// Estimated US dollars for one call that reached `provider`.
pub(crate) fn usd_per_call(provider: &str) -> f64 {
    super::price_per_call(provider)
}
