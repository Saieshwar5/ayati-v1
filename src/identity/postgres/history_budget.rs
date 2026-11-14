//! How much of a conversation an agent's prompt carries, from its model's
//! context window (the model catalog's `context`). One budget sizes the recent
//! history in each prompt and decides when the automatic summarizer runs: when
//! the messages since the last summary would no longer fit, it summarizes them.
//! Models outside the catalog keep the original 14,000-byte window.
use super::*;

/// The window for models the catalog doesn't describe, and the least any gets.
pub(crate) const FLOOR: usize = 14_000;
/// The most any prompt carries: more costs every request and slows answers.
const CAP: usize = 64_000;
/// A rough count of characters per token for English text and JSON.
const BYTES_PER_TOKEN: usize = 4;

/// History bytes for a model whose window is `tokens` (0 when unknown): a fifth
/// of the window, leaving the rest for instructions, notes, documents, tool
/// results and the answer.
pub(crate) fn bytes(tokens: u32) -> usize {
    if tokens == 0 {
        return FLOOR;
    }
    (tokens as usize * BYTES_PER_TOKEN / 5).clamp(FLOOR, CAP)
}

/// The most messages read for that many bytes; 60 at the floor.
pub(crate) fn messages(bytes: usize) -> i64 {
    ((bytes / 230) as i64).clamp(60, 280)
}

impl PgStore {
    /// The model's context window in tokens, from the catalog; 0 when the
    /// catalog isn't configured or doesn't list the model.
    pub(crate) fn model_window(&self, model: &str) -> u32 {
        self.model_catalog
            .as_ref()
            .and_then(|files| files.load().ok())
            .and_then(|catalog| catalog.find(model).map(|m| m.context))
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_history_window_follows_the_model_within_limits() {
        assert_eq!(bytes(0), FLOOR, "unknown models keep today's window");
        assert_eq!(bytes(8_000), FLOOR, "small models never get less");
        assert_eq!(bytes(32_000), 25_600);
        assert_eq!(bytes(128_000), CAP);
        assert_eq!(bytes(1_000_000), CAP);
        assert_eq!(messages(FLOOR), 60);
        assert_eq!(messages(CAP), 278);
    }
}
