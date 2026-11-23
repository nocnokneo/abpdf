//! The text strings of a PDF, which are not in one encoding.

/// The characters of `PDFDocEncoding` from 0x18 to 0x1F, which are accents.
const ACCENTS: [char; 8] = [
    '\u{02D8}', '\u{02C7}', '\u{02C6}', '\u{02D9}', '\u{02DD}', '\u{02DB}', '\u{02DA}', '\u{02DC}',
];

/// The characters of `PDFDocEncoding` from 0x80 to 0xA0.
/// The bytes that it leaves undefined are U+FFFD.
const PUNCTUATION_AND_LETTERS: [char; 33] = [
    '\u{2022}', '\u{2020}', '\u{2021}', '\u{2026}', '\u{2014}', '\u{2013}', '\u{0192}', '\u{2044}',
    '\u{2039}', '\u{203A}', '\u{2212}', '\u{2030}', '\u{201E}', '\u{201C}', '\u{201D}', '\u{2018}',
    '\u{2019}', '\u{201A}', '\u{2122}', '\u{FB01}', '\u{FB02}', '\u{0141}', '\u{0152}', '\u{0160}',
    '\u{0178}', '\u{017D}', '\u{0131}', '\u{0142}', '\u{0153}', '\u{0161}', '\u{017E}', '\u{FFFD}',
    '\u{20AC}',
];

/// The text that the bytes of a text string stand for.
///
/// A byte order mark says that the string is in UTF-16 or in UTF-8, and
/// without one it is in `PDFDocEncoding`.
pub(super) fn decode(bytes: &[u8]) -> String {
    match bytes {
        [0xFE, 0xFF, rest @ ..] => decode_utf16(rest, u16::from_be_bytes),
        [0xFF, 0xFE, rest @ ..] => decode_utf16(rest, u16::from_le_bytes),
        [0xEF, 0xBB, 0xBF, rest @ ..] => String::from_utf8_lossy(rest).into_owned(),
        _ => bytes.iter().map(|&byte| decode_pdf_doc(byte)).collect(),
    }
}

fn decode_utf16(bytes: &[u8], unit_of: fn([u8; 2]) -> u16) -> String {
    let (pairs, _) = bytes.as_chunks::<2>();
    let units = pairs.iter().map(|&pair| unit_of(pair));
    char::decode_utf16(units)
        .map(|character| character.unwrap_or(char::REPLACEMENT_CHARACTER))
        .collect()
}

fn decode_pdf_doc(byte: u8) -> char {
    match byte {
        0x18..=0x1F => ACCENTS[usize::from(byte - 0x18)],
        0x80..=0xA0 => PUNCTUATION_AND_LETTERS[usize::from(byte - 0x80)],
        // Everything else is where Unicode has it.
        _ => char::from(byte),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_without_a_byte_order_mark_is_in_pdf_doc_encoding() {
        assert_eq!(decode(b"Caf\xE9 \x85 \x93n \xA0"), "Café – ﬁn €");
        assert_eq!(decode(b"\x1A"), "\u{02C6}");
    }

    #[test]
    fn byte_order_mark_tells_utf_16_in_either_order() {
        assert_eq!(decode(b"\xFE\xFF\x00\xAD\x20\x13"), "\u{AD}\u{2013}");
        assert_eq!(decode(b"\xFF\xFE\xAD\x00\x13\x20"), "\u{AD}\u{2013}");
    }

    #[test]
    fn utf_16_takes_two_units_for_a_character_outside_the_basic_plane() {
        assert_eq!(decode(b"\xFE\xFF\xD8\x3D\xDE\x00"), "\u{1F600}");
        // Half of such a character is no character.
        assert_eq!(decode(b"\xFE\xFF\xD8\x3D"), "\u{FFFD}");
    }

    #[test]
    fn byte_order_mark_tells_utf_8() {
        assert_eq!(decode(b"\xEF\xBB\xBF\xE2\x80\x93"), "\u{2013}");
    }
}
