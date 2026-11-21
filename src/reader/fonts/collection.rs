//! The text of fonts that leave it to their character collection.

use hayro_cmap::{BfString, CMap, CMapName, CidFamily};
use hayro_font::CMapResolverFn;
use hayro_syntax::object::{self, Array, Dict, Object};

/// The longest character code that an encoding has, in bytes.
const MAX_CODE_BYTE_COUNT: u8 = 4;

/// What the character codes of a composite font stand for where the font
/// has no `ToUnicode` map and takes its characters from one of the
/// collections of Adobe, as fonts for Chinese, Japanese and Korean do:
/// the collection says which character each of its numbers is.
pub(super) struct CollectionText {
    /// Takes a character code to its number in the collection.
    encoding: CMap,
    /// Takes a number in the collection to its text.
    text: CMap,
}

impl CollectionText {
    /// `None` for a font that says what its codes stand for, and for one
    /// whose collection is not known.
    pub fn of(font: &Dict, cmap_resolver: &CMapResolverFn) -> Option<Self> {
        if font.contains_key("ToUnicode") {
            return None;
        }
        let descendants = font.get::<Array>("DescendantFonts")?;
        let collection = descendants
            .iter::<Dict>()
            .next()?
            .get::<Dict>("CIDSystemInfo")?;
        let registry = collection.get::<object::String>("Registry")?;
        let ordering = collection.get::<object::String>("Ordering")?;
        let family = CidFamily::from_registry_ordering(registry.as_bytes(), ordering.as_bytes());
        let encoding = match font.get::<Object>("Encoding")? {
            Object::Name(name) => named(CMapName::from_bytes(&name), cmap_resolver)?,
            Object::Stream(stream) => parse(&stream.decoded_data().ok()?, cmap_resolver)?,
            _ => return None,
        };
        Some(Self {
            encoding,
            text: named(family.ucs2_cmap()?, cmap_resolver)?,
        })
    }

    pub fn of_code(&self, code: u32) -> Option<BfString> {
        let mut byte_counts = 1..=MAX_CODE_BYTE_COUNT;
        let number =
            byte_counts.find_map(|byte_count| self.encoding.lookup_cid_code(code, byte_count))?;
        self.text.lookup_bf_string(number)
    }
}

/// One of the CMaps that every PDF reader has.
fn named(name: CMapName, cmap_resolver: &CMapResolverFn) -> Option<CMap> {
    match name {
        CMapName::IdentityH => Some(CMap::identity_h()),
        CMapName::IdentityV => Some(CMap::identity_v()),
        name => parse(cmap_resolver(name)?, cmap_resolver),
    }
}

fn parse(data: &[u8], cmap_resolver: &CMapResolverFn) -> Option<CMap> {
    let cmap_resolver = cmap_resolver.clone();
    CMap::parse(data, move |name| cmap_resolver(name))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use hayro_syntax::Pdf;
    use hayro_syntax::object::ObjectIdentifier;

    use super::*;
    use crate::reader::testing::pdf_of;

    /// The number of the hiragana letter a in the Japanese collection of
    /// Adobe, which is its code in the encoding `Identity-H`.
    const HIRAGANA_A: u32 = 843;

    /// What the codes of a composite font with the given entries stand
    /// for, as far as its collection says.
    fn text_of_font(collection: &str, more_entries: &str) -> Option<CollectionText> {
        let objects = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
            "<< /Type /Pages /Kids [] /Count 0 >>".to_owned(),
            format!(
                "<< /Type /Font /Subtype /Type0 /BaseFont /Serif /Encoding /Identity-H \
                 /DescendantFonts [<< /Type /Font /Subtype /CIDFontType0 /BaseFont /Serif \
                 /CIDSystemInfo << /Registry (Adobe) /Ordering ({collection}) /Supplement 6 >> \
                 >>] {more_entries} >>"
            ),
        ];
        let pdf = Pdf::new(pdf_of(&objects)).ok()?;
        let font: Dict = pdf.xref().get(ObjectIdentifier::new(3, 0))?;
        let cmap_resolver: CMapResolverFn = Arc::new(hayro_cmap::load_embedded);
        CollectionText::of(&font, &cmap_resolver)
    }

    #[test]
    fn collection_says_which_characters_its_numbers_are() {
        let text = text_of_font("Japan1", "").expect("the collection is known");
        assert_eq!(text.of_code(HIRAGANA_A), Some(BfString::Char('あ')));
    }

    #[test]
    fn font_that_says_what_its_codes_stand_for_is_not_asked() {
        assert!(text_of_font("Japan1", "/ToUnicode 4 0 R").is_none());
    }

    #[test]
    fn collection_of_the_font_alone_says_nothing() {
        assert!(text_of_font("Identity", "").is_none());
    }
}
