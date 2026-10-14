//! The controlled-input protocol, kept free of GTK so it can be tested on its
//! own (a port of React Native's event-count scheme, as on iOS and Android).
//!
//! The native editor owns the text. Every user edit bumps the event count and
//! is reported with it. The core answers with `TextValue {text, count}` where
//! `count` is the latest native count it had seen when the script set the
//! value. A value older than the editor's latest edit is stale and dropped; a
//! value arriving mid-composition (IME preedit) waits until composition ends,
//! so it never clobbers half-typed pinyin or kana.

use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// Write this text into the editor (without reporting it back).
    Apply(String),
    /// Nothing to do: stale, or already the editor's text.
    Skip,
    /// Composition in progress: kept for [`ControlledText::composition_ended`].
    Deferred,
}

#[derive(Debug, Default)]
pub struct ControlledText {
    event_count: u32,
    pending: Option<(String, u32)>,
}

impl ControlledText {
    /// A native edit happened; returns the count to report with it.
    pub fn did_edit(&mut self) -> u32 {
        self.event_count = self.event_count.wrapping_add(1);
        self.event_count
    }

    pub fn incoming(&mut self, text: &str, count: u32, current: &str, composing: bool) -> Decision {
        if count < self.event_count {
            return Decision::Skip;
        }
        if composing {
            self.pending = Some((text.to_owned(), count));
            return Decision::Deferred;
        }
        self.pending = None;
        if text == current {
            Decision::Skip
        } else {
            Decision::Apply(text.to_owned())
        }
    }

    /// Composition finished (committed or cancelled). The deferred value
    /// applies only if no edit happened since it was sent.
    pub fn composition_ended(&mut self, current: &str) -> Decision {
        match self.pending.take() {
            Some((text, count)) => self.incoming(&text, count, current, false),
            None => Decision::Skip,
        }
    }

    pub fn reset(&mut self) {
        *self = ControlledText::default();
    }
}

/// `MaxLength` / `AllowedChars`, enforced natively before an edit lands.
#[derive(Debug, Default, Clone)]
pub struct InputFilter {
    /// In UTF-16 units, like JavaScript's `length`. 0: unlimited.
    pub max_length: usize,
    pub allowed: Option<HashSet<char>>,
}

impl InputFilter {
    pub fn is_active(&self) -> bool {
        self.max_length > 0 || self.allowed.is_some()
    }

    /// What may actually be inserted into `current` (whose selection GTK has
    /// already deleted): the insertion itself, a shortened or filtered
    /// version, or `None` to reject the edit.
    pub fn admit(&self, current: &str, insertion: &str) -> Option<String> {
        let mut text: String = match &self.allowed {
            Some(allowed) => insertion.chars().filter(|c| allowed.contains(c)).collect(),
            None => insertion.to_owned(),
        };
        if self.max_length > 0 {
            let remaining = self.max_length.saturating_sub(utf16_len(current));
            text = prefix_utf16(&text, remaining).to_owned();
        }
        if text.is_empty() && !insertion.is_empty() {
            None
        } else {
            Some(text)
        }
    }
}

pub fn utf16_len(s: &str) -> usize {
    s.chars().map(char::len_utf16).sum()
}

/// The longest prefix of whole characters within `limit` UTF-16 units.
pub fn prefix_utf16(s: &str, limit: usize) -> &str {
    let mut used = 0;
    for (i, c) in s.char_indices() {
        used += c.len_utf16();
        if used > limit {
            return &s[..i];
        }
    }
    s
}

/// The UTF-16 offset of the `chars`-th character of `s` (GTK counts characters).
pub fn char_to_utf16(s: &str, chars: usize) -> usize {
    s.chars().take(chars).map(char::len_utf16).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits_bump_the_count() {
        let mut c = ControlledText::default();
        assert_eq!(c.did_edit(), 1);
        assert_eq!(c.did_edit(), 2);
    }

    #[test]
    fn stale_values_are_dropped() {
        let mut c = ControlledText::default();
        c.did_edit();
        c.did_edit();
        assert_eq!(c.incoming("a", 1, "ab", false), Decision::Skip);
        assert_eq!(c.incoming("AB", 2, "ab", false), Decision::Apply("AB".into()));
        assert_eq!(c.incoming("AB", 3, "ab", false), Decision::Apply("AB".into()));
    }

    #[test]
    fn equal_text_is_not_rewritten() {
        let mut c = ControlledText::default();
        c.did_edit();
        assert_eq!(c.incoming("ab", 1, "ab", false), Decision::Skip);
    }

    #[test]
    fn values_wait_for_composition_and_apply_if_still_current() {
        let mut c = ControlledText::default();
        c.did_edit();
        assert_eq!(c.incoming("x", 1, "n", true), Decision::Deferred);
        assert_eq!(c.composition_ended("n"), Decision::Apply("x".into()));
        assert_eq!(c.composition_ended("n"), Decision::Skip);
    }

    #[test]
    fn deferred_values_go_stale_when_the_user_keeps_typing() {
        let mut c = ControlledText::default();
        c.did_edit();
        assert_eq!(c.incoming("n", 1, "", true), Decision::Deferred);
        c.did_edit(); // preedit "ni"
        c.did_edit(); // commit "你"
        assert_eq!(c.composition_ended("你"), Decision::Skip);
    }

    #[test]
    fn reset_forgets_count_and_pending() {
        let mut c = ControlledText::default();
        c.did_edit();
        c.incoming("x", 1, "", true);
        c.reset();
        assert_eq!(c.composition_ended(""), Decision::Skip);
        assert_eq!(c.incoming("y", 0, "", false), Decision::Apply("y".into()));
        assert_eq!(c.did_edit(), 1);
    }

    #[test]
    fn filter_limits_length_in_utf16_units() {
        let f = InputFilter { max_length: 4, allowed: None };
        assert_eq!(f.admit("ab", "cdef").as_deref(), Some("cd"));
        assert_eq!(f.admit("abc", "😀").as_deref(), None, "a surrogate pair does not fit one unit");
        assert_eq!(f.admit("ab", "😀x").as_deref(), Some("😀"));
        assert_eq!(f.admit("abcd", "e"), None);
        assert_eq!(f.admit("abcd", "").as_deref(), Some(""));
    }

    #[test]
    fn filter_keeps_allowed_chars_only() {
        let f = InputFilter { max_length: 0, allowed: Some("0123456789".chars().collect()) };
        assert_eq!(f.admit("", "a1b2").as_deref(), Some("12"));
        assert_eq!(f.admit("", "abc"), None);
        assert!(f.is_active());
        assert!(!InputFilter::default().is_active());
    }

    #[test]
    fn utf16_helpers() {
        assert_eq!(utf16_len("a😀é"), 4);
        assert_eq!(prefix_utf16("a😀é", 2), "a");
        assert_eq!(prefix_utf16("a😀é", 3), "a😀");
        assert_eq!(char_to_utf16("a😀é", 2), 3);
        assert_eq!(char_to_utf16("a😀é", 9), 4);
    }
}
