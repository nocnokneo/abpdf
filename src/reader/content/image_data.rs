//! The data of an image as it is compared.
//!
//! An image is compared by its data without its filters, so that a picture
//! is the same however its producer compressed it.
//! A few bytes of compressed data can stand for gigabytes, though, and the
//! decoder keeps all of what it decodes, so data is only decoded where it
//! cannot decode to more than [`MAX_DECODED_IMAGE_DATA_BYTES`].
//! Other data is compared as it is in the file: the same picture compressed
//! in another way then differs, and the same data still does not.

use std::collections::hash_map::DefaultHasher;
use std::hash::Hash;
use std::io::{self, Read};

use flate2::read::DeflateDecoder;
use hayro_syntax::Filter;
use hayro_syntax::object::Stream;

/// The most that the data of an image is decoded to, to be compared.
/// An image of a page scanned in color at 400 dpi is under 50 MiB.
const MAX_DECODED_IMAGE_DATA_BYTES: u64 = 64 << 20;

/// Takes in the data of an image, or of its mask.
pub(super) fn hash(stream: &Stream, hasher: &mut DefaultHasher) {
    hash_within(stream, hasher, MAX_DECODED_IMAGE_DATA_BYTES);
}

/// Takes in the data of a stream as it is without its filters if it cannot
/// decode to more than `max_decoded_bytes`, and as it is in the file if it
/// can.
/// What only an image decoder can undo is taken in as it is in the file as
/// well.
fn hash_within(stream: &Stream, hasher: &mut DefaultHasher, max_decoded_bytes: u64) {
    let raw = stream.raw_data();
    let most_bytes = most_decoded_bytes(&raw, &stream.filters(), max_decoded_bytes);
    let decoded = match most_bytes <= max_decoded_bytes {
        true => stream.decoded_data().ok(),
        false => None,
    };
    match decoded {
        Some(data) => data.hash(hasher),
        None => raw.hash(hasher),
    }
}

/// The most bytes that data can decode to through its filters, which is
/// more than `max_bytes` wherever that is more.
///
/// What a first Flate filter inflates the data to is measured, by
/// inflating it without keeping anything and only as far as just past
/// `max_bytes`.
/// Every other filter counts as expanding the data as far as it can.
fn most_decoded_bytes(data: &[u8], filters: &[Filter], max_bytes: u64) -> u64 {
    let (measured_bytes, other_filters) = match filters {
        [Filter::FlateDecode, other_filters @ ..] => {
            let inflated_bytes = inflated_bytes_up_to(data, max_bytes + 1);
            let at_most = || data.len() as u64 * most_bytes_decoded_per_byte(Filter::FlateDecode);
            (inflated_bytes.unwrap_or_else(at_most), other_filters)
        }
        _ => (data.len() as u64, filters),
    };
    let factors = other_filters
        .iter()
        .map(|&filter| most_bytes_decoded_per_byte(filter));
    factors.fold(measured_bytes, u64::saturating_mul)
}

/// How many bytes Flate data inflates to, counted up to `limit_bytes`.
/// `None` if it is no Flate data that can be read to its end, which leaves
/// the decoder of `hayro-syntax`, which is more lenient, to make of it what
/// it can.
fn inflated_bytes_up_to(data: &[u8], limit_bytes: u64) -> Option<u64> {
    // As the decoder of `hayro-syntax` does, a zlib header is skipped, and
    // without one the data is taken to be raw deflate data.
    let deflate_data = match data {
        [method, flags, rest @ ..] if is_zlib_header(*method, *flags) => rest,
        _ => data,
    };
    let mut inflated = DeflateDecoder::new(deflate_data).take(limit_bytes);
    io::copy(&mut inflated, &mut io::sink()).ok()
}

/// Whether two bytes are the header of zlib data that needs no preset
/// dictionary.
fn is_zlib_header(method: u8, flags: u8) -> bool {
    let is_deflate = method & 0x0f == 8;
    let is_checked = (u16::from(method) << 8 | u16::from(flags)).is_multiple_of(31);
    let needs_dictionary = flags & 0x20 != 0;
    is_deflate && is_checked && !needs_dictionary
}

/// The most bytes that a filter decodes one byte of data to.
fn most_bytes_decoded_per_byte(filter: Filter) -> u64 {
    match filter {
        // A copy of 258 bytes takes two codes of at least one bit each.
        Filter::FlateDecode => 1032,
        // A code of at least 9 bits stands for at most 4096 bytes.
        Filter::LzwDecode => 3641,
        // Two bytes repeat a byte at most 128 times.
        Filter::RunLengthDecode => 64,
        // `z` stands for four zero bytes.
        Filter::Ascii85Decode => 4,
        Filter::AsciiHexDecode | Filter::Crypt => 1,
        // Data that only an image decoder can undo is not decoded.
        Filter::DctDecode | Filter::JpxDecode | Filter::Jbig2Decode | Filter::CcittFaxDecode => 1,
    }
}

#[cfg(test)]
mod tests {
    use std::hash::Hasher;
    use std::io::Write;

    use flate2::Compression;
    use flate2::write::ZlibEncoder;
    use hayro_syntax::Pdf;
    use hayro_syntax::object::ObjectIdentifier;

    use super::*;
    use crate::reader::testing::{page_pdf, stream};

    #[test]
    fn data_is_decoded_unless_it_can_decode_to_more_than_a_limit() {
        // The same two pixels, as they are and written in hexadecimal
        // digits, which are more bytes than the pixels.
        let image = |entries: &str, data: &str| {
            let entries = format!(
                "/Type /XObject /Subtype /Image /Width 2 /Height 1 \
                 /ColorSpace /DeviceGray /BitsPerComponent 8 {entries}"
            );
            stream(&entries, data)
        };
        let images = [image("", "ab"), image("/Filter /ASCIIHexDecode", "6162>")];
        let pdf = Pdf::new(page_pdf("", "", &images)).expect("the PDF opens");
        let hashes_within = |max_decoded_bytes| {
            [5, 6].map(|object_number| {
                let id = ObjectIdentifier::new(object_number, 0);
                let image: Stream = pdf.xref().get(id).expect("the image is there");
                let mut hasher = DefaultHasher::new();
                hash_within(&image, &mut hasher, max_decoded_bytes);
                hasher.finish()
            })
        };
        let [plain, hexadecimal] = hashes_within(5);
        assert_eq!(plain, hexadecimal);
        let [plain, hexadecimal] = hashes_within(4);
        assert_ne!(plain, hexadecimal);
    }

    fn zlib(data: &[u8]) -> Vec<u8> {
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::best());
        encoder.write_all(data).expect("the data is compressed");
        encoder.finish().expect("the data is compressed")
    }

    #[test]
    fn flate_data_is_measured_as_far_as_the_limit() {
        let data = zlib(&[0; 5000]);
        let filters = [Filter::FlateDecode];
        assert_eq!(most_decoded_bytes(&data, &filters, 5000), 5000);
        assert_eq!(most_decoded_bytes(&data, &filters, 4000), 4001);
    }

    #[test]
    fn flate_data_without_a_zlib_header_is_raw_deflate_data() {
        let data = zlib(b"a picture");
        let deflate_data = &data[2..];
        assert_eq!(
            most_decoded_bytes(deflate_data, &[Filter::FlateDecode], 100),
            9
        );
    }

    #[test]
    fn data_that_is_no_flate_data_counts_as_expanding_as_far_as_flate_can() {
        // A block of the type that deflate does not have.
        let data = [0b111; 10];
        let most_bytes = most_decoded_bytes(&data, &[Filter::FlateDecode], 100_000);
        assert_eq!(most_bytes, 10 * 1032);
    }

    #[test]
    fn filters_after_the_first_count_as_expanding_as_far_as_they_can() {
        let data = zlib(&[0; 100]);
        let filters = [Filter::FlateDecode, Filter::RunLengthDecode];
        assert_eq!(most_decoded_bytes(&data, &filters, 1000), 100 * 64);
        let filters = [Filter::AsciiHexDecode, Filter::FlateDecode];
        assert_eq!(
            most_decoded_bytes(&data, &filters, 1000),
            data.len() as u64 * 1032
        );
    }
}
