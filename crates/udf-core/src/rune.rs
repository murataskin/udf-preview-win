//! Rune (Unicode scalar) offset arithmetic.
//!
//! **CRITICAL:** element offsets in `content.xml` (`startOffset` / `length`) are RUNE
//! offsets — counts of Unicode scalars — not UTF-16 code units and not bytes. Rust's
//! `char` is a Unicode scalar value, so `str::chars()` matches Swift's `unicodeScalars`
//! exactly. Mirrors `RuneHelpers.swift` in the reference app.
//!
//! Correctness rule (slice-raw-then-clean): callers must slice the *raw* CDATA by scalar
//! offset **first**, and only then decode entities / trim / transform the sliced text.
//! Decoding before slicing shifts indices and corrupts every offset downstream.

/// Total number of Unicode scalars (runes) in `s`.
#[inline]
pub fn rune_len(s: &str) -> usize {
    s.chars().count()
}

/// Return the substring of `s` spanning scalars `[start, start + len)`.
///
/// Returns `None` if the requested range extends past the end of `s` (out-of-range
/// offsets indicate a malformed document; the caller decides how to recover — never panic).
pub fn rune_slice(s: &str, start: usize, len: usize) -> Option<String> {
    let end = start.checked_add(len)?;
    let mut out = String::new();
    let mut seen = 0usize;
    for (i, ch) in s.chars().enumerate() {
        seen = i + 1;
        if i < start {
            continue;
        }
        if i >= end {
            // We found a scalar past the requested range, so the whole range is in bounds.
            return Some(out);
        }
        out.push(ch);
    }
    // Reached the end of `s` without overshooting. The range is valid iff it ends at or
    // before the string boundary; `end > seen` means `start` (or `start + len`) ran past
    // the end — a zero-length slice past the end is out of range too.
    if end <= seen {
        Some(out)
    } else {
        None
    }
}

/// Lenient rune substring for the parser: clamps the range into bounds instead of failing.
///
/// The reference's Swift `runeSubstring` traps on an out-of-range offset; real files never
/// trigger that, but a corrupt file must not crash a shell surrogate. Negative `start`/`len`
/// are treated as 0, `start` is clamped to the string length, and the end is clamped to the
/// string end — returning whatever valid text is available.
pub fn rune_substring_clamped(s: &str, start: i64, len: i64) -> String {
    let total = rune_len(s);
    let start = start.max(0) as usize;
    let len = len.max(0) as usize;
    let start = start.min(total);
    let end = start.saturating_add(len).min(total);
    s.chars().skip(start).take(end - start).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_slice() {
        assert_eq!(rune_slice("hello world", 0, 5).as_deref(), Some("hello"));
        assert_eq!(rune_slice("hello world", 6, 5).as_deref(), Some("world"));
    }

    #[test]
    fn slice_to_exact_end() {
        assert_eq!(rune_slice("abc", 1, 2).as_deref(), Some("bc"));
        assert_eq!(rune_slice("abc", 0, 3).as_deref(), Some("abc"));
        assert_eq!(rune_slice("abc", 3, 0).as_deref(), Some(""));
    }

    #[test]
    fn out_of_range_is_none() {
        assert_eq!(rune_slice("abc", 1, 5), None);
        assert_eq!(rune_slice("abc", 4, 0), None);
        assert_eq!(rune_slice("abc", usize::MAX, 1), None); // checked_add overflow
    }

    #[test]
    fn turkish_multibyte_is_one_rune_each() {
        // Each of these is one Unicode scalar but multiple UTF-8 bytes. Offsets are by rune.
        let s = "çğışöü";
        assert_eq!(rune_len(s), 6);
        assert_eq!(rune_slice(s, 0, 1).as_deref(), Some("ç"));
        assert_eq!(rune_slice(s, 5, 1).as_deref(), Some("ü"));
    }

    #[test]
    fn astral_scalars_count_as_one_rune() {
        // U+1D6FC MATHEMATICAL ITALIC SMALL ALPHA is a single scalar (>U+FFFF), but two
        // UTF-16 code units. Rune slicing must treat it as ONE position, matching Swift.
        let s = "x𝛼y"; // 'x', U+1D6FC, 'y' => 3 runes
        assert_eq!(rune_len(s), 3);
        assert_eq!(rune_slice(s, 1, 1).as_deref(), Some("𝛼"));
        assert_eq!(rune_slice(s, 0, 2).as_deref(), Some("x𝛼"));
        assert_eq!(rune_slice(s, 2, 1).as_deref(), Some("y"));
    }

    #[test]
    fn clamped_in_range_matches_strict() {
        assert_eq!(rune_substring_clamped("hello", 1, 3), "ell");
        assert_eq!(rune_substring_clamped("x𝛼y", 1, 1), "𝛼");
    }

    #[test]
    fn clamped_out_of_range_does_not_panic() {
        assert_eq!(rune_substring_clamped("abc", 1, 99), "bc"); // end clamps
        assert_eq!(rune_substring_clamped("abc", 9, 3), ""); // start past end
        assert_eq!(rune_substring_clamped("abc", -5, 2), "ab"); // negative start -> 0
        assert_eq!(rune_substring_clamped("abc", 1, -1), ""); // negative len -> 0
    }

    #[test]
    fn slice_then_decode_ordering() {
        // The raw CDATA contains an entity `&amp;` as 5 literal scalars. Slicing must happen
        // on the raw text (so offsets line up); decoding is the caller's *next* step.
        let raw = "A &amp; B";
        let sliced = rune_slice(raw, 2, 5).unwrap(); // "&amp;" — 5 raw scalars
        assert_eq!(sliced, "&amp;");
        // Decoding the *sliced* fragment is correct; decoding `raw` first would have made
        // this a 1-scalar "&" and shifted every later offset by 4.
    }
}
