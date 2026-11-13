//! Strings that PDFium writes into a caller's buffer.

use std::ffi::c_ulong;

const INLINE_CAPACITY: usize = 64;

/// Reads a UTF-16 string.
///
/// `fill` is given a buffer and its size in bytes, and returns the size in
/// bytes of the whole string with its terminator, or `None` if there is no
/// string.
pub(super) fn utf16(fill: impl FnMut(*mut u16, c_ulong) -> Option<c_ulong>) -> Option<String> {
    let units = read(fill)?;
    Some(String::from_utf16_lossy(without_terminator(&units, 0)))
}

/// Reads a UTF-8 string.
///
/// `fill` is given a buffer and its size in bytes, and returns the size in
/// bytes of the whole string with its terminator.
pub(super) fn utf8(mut fill: impl FnMut(*mut u8, c_ulong) -> c_ulong) -> String {
    let bytes = read(|buffer, byte_capacity| Some(fill(buffer, byte_capacity))).unwrap_or_default();
    String::from_utf8_lossy(without_terminator(&bytes, 0)).into_owned()
}

/// Reads a PDF text string from its bytes.
///
/// `fill` is given a buffer and its size in bytes, and returns the size in
/// bytes of the whole string, or `None` if there is no string.
pub(super) fn pdf_text(fill: impl FnMut(*mut u8, c_ulong) -> Option<c_ulong>) -> Option<String> {
    Some(decode_pdf_text(&read(fill)?))
}

/// Calls `fill` until the buffer it is given holds the whole value.
fn read<T: Clone + Default>(
    mut fill: impl FnMut(*mut T, c_ulong) -> Option<c_ulong>,
) -> Option<Vec<T>> {
    let mut buffer = vec![T::default(); INLINE_CAPACITY];
    loop {
        let byte_capacity = buffer.len() * size_of::<T>();
        let byte_length = fill(buffer.as_mut_ptr(), byte_capacity as c_ulong)? as usize;
        let length = byte_length.div_ceil(size_of::<T>());
        if byte_length <= byte_capacity {
            buffer.truncate(length);
            return Some(buffer);
        }
        buffer.resize(length, T::default());
    }
}

fn without_terminator<T: PartialEq>(units: &[T], terminator: T) -> &[T] {
    units.strip_suffix(&[terminator]).unwrap_or(units)
}

/// Decodes a PDF text string, which is UTF-16BE or UTF-8 if it starts with
/// the matching byte order mark, and PDFDocEncoding otherwise.
fn decode_pdf_text(bytes: &[u8]) -> String {
    if let Some(utf16) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        let units: Vec<u16> = utf16
            .chunks(2)
            .map(|pair| u16::from_be_bytes([pair[0], *pair.get(1).unwrap_or(&0)]))
            .collect();
        String::from_utf16_lossy(&units)
    } else if let Some(utf8) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        String::from_utf8_lossy(utf8).into_owned()
    } else {
        bytes
            .iter()
            .map(|&byte| decode_pdf_doc_encoding(byte))
            .collect()
    }
}

/// PDFDocEncoding is Latin-1 except in two ranges.
fn decode_pdf_doc_encoding(byte: u8) -> char {
    const FROM_0X18: [char; 8] = [
        '\u{02D8}', '\u{02C7}', '\u{02C6}', '\u{02D9}', '\u{02DD}', '\u{02DB}', '\u{02DA}',
        '\u{02DC}',
    ];
    const FROM_0X80: [char; 33] = [
        '\u{2022}', '\u{2020}', '\u{2021}', '\u{2026}', '\u{2014}', '\u{2013}', '\u{0192}',
        '\u{2044}', '\u{2039}', '\u{203A}', '\u{2212}', '\u{2030}', '\u{201E}', '\u{201C}',
        '\u{201D}', '\u{2018}', '\u{2019}', '\u{201A}', '\u{2122}', '\u{FB01}', '\u{FB02}',
        '\u{0141}', '\u{0152}', '\u{0160}', '\u{0178}', '\u{017D}', '\u{0131}', '\u{0142}',
        '\u{0153}', '\u{0161}', '\u{017E}', '\u{FFFD}', '\u{20AC}',
    ];
    match byte {
        0x18..=0x1F => FROM_0X18[usize::from(byte - 0x18)],
        0x80..=0xA0 => FROM_0X80[usize::from(byte - 0x80)],
        _ => char::from(byte),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Behaves like a PDFium getter for a value of `units`.
    fn getter<T: Copy>(units: Vec<T>) -> impl FnMut(*mut T, c_ulong) -> Option<c_ulong> {
        move |buffer, byte_capacity| {
            let byte_length = units.len() * size_of::<T>();
            if byte_length <= byte_capacity as usize {
                // SAFETY: the buffer holds at least `byte_length` bytes.
                unsafe { buffer.copy_from_nonoverlapping(units.as_ptr(), units.len()) };
            }
            Some(byte_length as c_ulong)
        }
    }

    fn utf16_getter(text: &str) -> impl FnMut(*mut u16, c_ulong) -> Option<c_ulong> {
        getter(text.encode_utf16().chain([0]).collect())
    }

    #[test]
    fn reads_a_string_that_fits() {
        assert_eq!(utf16(utf16_getter("Artifact")).as_deref(), Some("Artifact"));
    }

    #[test]
    fn retries_with_a_larger_buffer() {
        let long = "x".repeat(3 * INLINE_CAPACITY);
        assert_eq!(utf16(utf16_getter(&long)), Some(long));
    }

    #[test]
    fn reports_a_missing_string() {
        assert_eq!(utf16(|_, _| None), None);
    }

    #[test]
    fn reads_utf8() {
        let mut fill = getter(b"Libertinus\0".to_vec());
        let text = utf8(|buffer, byte_capacity| fill(buffer, byte_capacity).unwrap());
        assert_eq!(text, "Libertinus");
    }

    #[test]
    fn pdf_text_with_a_utf16_byte_order_mark() {
        let soft_hyphen = pdf_text(getter(vec![0xFE, 0xFF, 0x00, 0xAD]));
        assert_eq!(soft_hyphen.as_deref(), Some("\u{AD}"));
    }

    #[test]
    fn pdf_text_with_a_utf8_byte_order_mark() {
        let text = pdf_text(getter(vec![0xEF, 0xBB, 0xBF, 0xC3, 0xA9]));
        assert_eq!(text.as_deref(), Some("é"));
    }

    #[test]
    fn pdf_text_in_pdf_doc_encoding() {
        let text = pdf_text(getter(vec![b'-', 0x84, 0xE9, 0xA0]));
        assert_eq!(text.as_deref(), Some("-—é€"));
    }
}
