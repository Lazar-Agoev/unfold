//! Short labels measured in UTF-16 code units.

/// The longest an edge's or target's text may be, in UTF-16 code units.
pub const LABEL_MAX_UTF16: usize = 120;

/// The number of UTF-16 code units `text` encodes to.
pub fn utf16_len(text: &str) -> usize {
    // Count by lead byte without decoding: a continuation byte adds nothing, a four-byte
    // (astral) lead adds a surrogate pair, and every other lead adds one unit.
    text.bytes()
        .map(|byte| match byte {
            0x80..=0xBF => 0,
            0xF0..=0xFF => 2,
            _ => 1,
        })
        .sum()
}

/// The longest prefix of `text` that is at most [`LABEL_MAX_UTF16`] UTF-16 code units. It
/// never splits a scalar value, so a surrogate pair that would straddle the limit is dropped
/// whole.
pub fn short_label(text: &str) -> &str {
    // Fast path: a string of at most 120 bytes is at most 120 UTF-16 units.
    if text.len() <= LABEL_MAX_UTF16 {
        return text;
    }
    let mut units = 0;
    for (index, character) in text.char_indices() {
        units += character.len_utf16();
        if units > LABEL_MAX_UTF16 {
            return &text[..index];
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_short_text() {
        assert_eq!(short_label("orderService.place"), "orderService.place");
        let exact = "é".repeat(120);
        assert_eq!(short_label(&exact), exact);
    }

    #[test]
    fn truncates_to_120_units() {
        let long = "a".repeat(130);
        assert_eq!(short_label(&long).len(), 120);
        let wide = "界".repeat(121);
        assert_eq!(utf16_len(short_label(&wide)), 120);
    }

    proptest::proptest! {
        #[test]
        fn utf16_len_agrees_with_encode_utf16(text in "\\PC{0,64}") {
            proptest::prop_assert_eq!(utf16_len(&text), text.encode_utf16().count());
        }
    }

    #[test]
    fn never_splits_a_surrogate_pair() {
        let text = format!("{}😀tail", "a".repeat(119));
        let label = short_label(&text);
        assert_eq!(label, "a".repeat(119));
        let fits = format!("{}😀tail", "a".repeat(118));
        assert_eq!(short_label(&fits), format!("{}😀", "a".repeat(118)));
    }
}
