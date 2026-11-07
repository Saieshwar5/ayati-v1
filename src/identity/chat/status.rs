//! The status a person sets for their workspace (D245, `designs/chat-and-account`
//! screen 3): one of the presets or their own text, an emoji and when it clears.
//! Status text is shown to every member, so it is checked here before it is saved.
use anyhow::{Result, ensure};
use serde::Deserialize;

/// The presets, plus `custom` for the person's own text. Available is no status.
pub(crate) const KINDS: [&str; 5] = ["working", "busy", "meeting", "away", "custom"];
/// The longest status text, in characters.
pub(crate) const TEXT_LIMIT: usize = 80;
/// "This week" from Monday morning is under seven days; a day more allows for
/// time zones and daylight saving.
const LONGEST_MS: i64 = 8 * 86_400_000;

#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Status {
    pub kind: String,
    #[serde(default)]
    pub emoji: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    /// When it clears (Unix milliseconds), worked out in the person's own time
    /// zone by the browser: in an hour, at midnight, at the end of the week.
    /// None keeps it until they change it.
    #[serde(default)]
    pub clear_at: Option<i64>,
}

impl Status {
    /// The status as it is saved: trimmed, with empty text and emoji left out.
    pub(crate) fn checked(self, now_ms: i64) -> Result<Self> {
        ensure!(
            KINDS.contains(&self.kind.as_str()),
            "Choose Working, Busy, In a meeting, Away or your own status"
        );
        let text = self
            .text
            .map(|text| text.trim().to_owned())
            .filter(|text| !text.is_empty());
        if let Some(text) = &text {
            ensure!(
                text.chars().count() <= TEXT_LIMIT,
                "Keep your status to {TEXT_LIMIT} characters"
            );
            ensure!(
                !text.chars().any(char::is_control),
                "A status is one line of text"
            );
        }
        ensure!(
            self.kind != "custom" || text.is_some(),
            "Write your status or choose one of the list"
        );
        let emoji = self.emoji.filter(|emoji| !emoji.is_empty());
        ensure!(
            emoji.as_deref().is_none_or(single_emoji),
            "Choose one emoji for your status"
        );
        ensure!(
            self.clear_at
                .is_none_or(|at| at > now_ms && at <= now_ms + LONGEST_MS),
            "Choose when your status clears: within a week"
        );
        Ok(Self {
            kind: self.kind,
            emoji,
            text,
            clear_at: self.clear_at,
        })
    }
}

/// Exactly one emoji: a picture with its optional presentation and skin-tone
/// marks, several joined into one (👩‍💻), a flag or a keycap. Letters, several
/// emoji in a row and markup are refused.
pub(crate) fn single_emoji(emoji: &str) -> bool {
    let chars: Vec<char> = emoji.chars().collect();
    if emoji.len() > 64 || chars.is_empty() {
        return false;
    }
    let regional = |c: char| ('\u{1F1E6}'..='\u{1F1FF}').contains(&c);
    if chars.len() == 2 && chars.iter().all(|c| regional(*c)) {
        return true;
    }
    if let [first, rest @ ..] = chars.as_slice()
        && (first.is_ascii_digit() || matches!(first, '#' | '*'))
    {
        return matches!(rest, ['\u{20E3}'] | ['\u{FE0F}', '\u{20E3}']);
    }
    // Pictures joined by zero-width joiners, each with its own marks.
    let mut expect_picture = true;
    for c in chars {
        if expect_picture {
            if !picture(c) {
                return false;
            }
            expect_picture = false;
        } else if c == '\u{200D}' {
            expect_picture = true;
        } else if !(c == '\u{FE0F}'
            || ('\u{1F3FB}'..='\u{1F3FF}').contains(&c)
            || ('\u{E0020}'..='\u{E007F}').contains(&c))
        {
            return false;
        }
    }
    !expect_picture
}

/// Characters that are pictures on their own.
fn picture(c: char) -> bool {
    matches!(c,
        '\u{1F000}'..='\u{1F1E5}' | '\u{1F200}'..='\u{1F3FA}' | '\u{1F400}'..='\u{1FAFF}'
        | '\u{2600}'..='\u{27BF}' | '\u{2300}'..='\u{23FF}' | '\u{2B00}'..='\u{2BFF}'
        | '\u{2190}'..='\u{21FF}' | '\u{2100}'..='\u{214F}' | '\u{25A0}'..='\u{25FF}'
        | '\u{2934}' | '\u{2935}' | '\u{3030}' | '\u{303D}' | '\u{3297}' | '\u{3299}'
        | '\u{00A9}' | '\u{00AE}' | '\u{203C}' | '\u{2049}')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(
        kind: &str,
        emoji: Option<&str>,
        text: Option<&str>,
        clear_at: Option<i64>,
    ) -> Status {
        Status {
            kind: kind.into(),
            emoji: emoji.map(Into::into),
            text: text.map(Into::into),
            clear_at,
        }
    }

    #[test]
    fn one_emoji_of_any_common_shape_is_accepted() {
        for emoji in [
            "🗓️",
            "🚚",
            "🙂",
            "👍🏽",
            "👩‍💻",
            "👨‍👩‍👧‍👦",
            "🇮🇳",
            "1️⃣",
            "#⃣",
            "⛔",
            "🌙",
            "💻",
            "❤️",
            "🏴󠁧󠁢󠁳󠁣󠁴󠁿",
        ] {
            assert!(single_emoji(emoji), "{emoji}");
        }
    }

    #[test]
    fn text_markup_and_several_emoji_are_refused() {
        for emoji in [
            "",
            "a",
            "ok",
            "🙂🙂",
            "<b>",
            "🙂a",
            "\u{200D}🙂",
            "🙂\u{200D}",
            "\u{FE0F}",
            "1",
            "🇮",
            "🇮🇳🇮🇳",
            "<script>",
        ] {
            assert!(!single_emoji(emoji), "{emoji:?}");
        }
        assert!(!single_emoji(&"🙂\u{FE0F}".repeat(20)));
    }

    #[test]
    fn a_status_is_trimmed_limited_and_cleared_within_a_week() {
        let now = 1_000_000;
        let saved = status(
            "meeting",
            Some("🗓️"),
            Some("  With the Rao family  "),
            Some(now + 3_600_000),
        )
        .checked(now)
        .unwrap();
        assert_eq!(saved.text.as_deref(), Some("With the Rao family"));
        let preset = status("busy", Some(""), Some("   "), None)
            .checked(now)
            .unwrap();
        assert_eq!(
            (preset.emoji, preset.text),
            (None, None),
            "empty parts are left out"
        );
        assert!(
            status("custom", Some("🚚"), Some(&"a".repeat(80)), None)
                .checked(now)
                .is_ok()
        );
        for refused in [
            status("custom", Some("🚚"), Some(&"a".repeat(81)), None),
            status("custom", Some("🚚"), None, None),
            status("custom", Some("🚚"), Some("Two\nlines"), None),
            status("lunch", None, None, None),
            status("busy", Some("busy"), None, None),
            status("busy", None, None, Some(now)),
            status("busy", None, None, Some(now + LONGEST_MS + 1)),
        ] {
            assert!(refused.clone().checked(now).is_err(), "{refused:?}");
        }
        // 80 characters, not bytes: accented and non-Latin text fits too.
        assert!(
            status("custom", None, Some(&"é".repeat(80)), None)
                .checked(now)
                .is_ok()
        );
    }
}
