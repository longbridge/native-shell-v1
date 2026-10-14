// The controlled-input protocol, kept free of AppKit so it can be tested on its
// own (a port of React Native's event-count scheme).
//
// The native field owns the text. Every user edit bumps `eventCount` and is
// reported with that count. The core answers with `TextValue {text, count}`
// where `count` is the latest native count it had seen when the script set the
// value. A value older than the field's latest edit is stale and dropped; a
// value arriving mid-composition (IME marked text) waits until composition
// ends, so it never clobbers half-typed pinyin or kana.

import Foundation

struct ControlledText {
    private(set) var eventCount: UInt32 = 0
    private(set) var pending: (text: String, count: UInt32)?

    enum Decision: Equatable {
        /// Write this text into the field (without reporting it back).
        case apply(String)
        /// Nothing to do: stale, or already the field's text.
        case skip
        /// Composition in progress: kept for `compositionEnded`.
        case deferred
    }

    /// A native edit happened; returns the count to report with it.
    mutating func didEdit() -> UInt32 {
        eventCount &+= 1
        return eventCount
    }

    mutating func incoming(text: String, count: UInt32, current: String, composing: Bool) -> Decision {
        guard count >= eventCount else { return .skip }
        if composing {
            pending = (text, count)
            return .deferred
        }
        pending = nil
        return text == current ? .skip : .apply(text)
    }

    /// Composition finished (committed or cancelled). The deferred value applies
    /// only if no edit happened since it was sent.
    mutating func compositionEnded(current: String) -> Decision {
        guard let p = pending else { return .skip }
        pending = nil
        return incoming(text: p.text, count: p.count, current: current, composing: false)
    }

    mutating func reset() {
        eventCount = 0
        pending = nil
    }
}

/// `MaxLength` / `AllowedChars`, enforced natively before the edit lands.
struct InputFilter {
    /// In UTF-16 units, like JavaScript's `length`. 0: unlimited.
    var maxLength = 0
    var allowed: Set<Character>?

    /// What may actually be inserted when the user replaces `range` (UTF-16) of
    /// `current` with `replacement`: the replacement itself, a shortened or
    /// filtered version, or nil to reject the edit.
    func admit(current: String, range: NSRange, replacement: String) -> String? {
        var text = replacement
        if let allowed = allowed {
            text = String(text.filter { allowed.contains($0) || $0 == "\n" && allowed.contains("\n") })
        }
        if maxLength > 0 {
            let remaining = max(0, maxLength - ((current as NSString).length - range.length))
            text = InputFilter.prefix(text, utf16: remaining)
        }
        if text.isEmpty && !replacement.isEmpty { return nil }
        return text
    }

    /// Enforces the limits on a whole string (after an IME commit, which bypasses `admit`).
    func clamp(_ s: String) -> String {
        var text = s
        if let allowed = allowed { text = String(text.filter { allowed.contains($0) }) }
        if maxLength > 0 { text = InputFilter.prefix(text, utf16: maxLength) }
        return text
    }

    /// The longest prefix of whole characters within `utf16` units.
    static func prefix(_ s: String, utf16 limit: Int) -> String {
        var used = 0
        var out = ""
        for ch in s {
            let n = ch.utf16.count
            if used + n > limit { break }
            used += n
            out.append(ch)
        }
        return out
    }
}
