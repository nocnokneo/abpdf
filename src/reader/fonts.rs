//! The fonts of a document.

mod collection;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use hayro_cmap::BfString;
use hayro_font::{CMapResolverFn, FontCache, FontGlyph, FontQuery, FontResolverFn, StandardFont};
use hayro_syntax::object::{Array, Dict, Name, Rect};

use self::collection::CollectionText;

/// How many units of the glyph space of a font are one unit of text space.
pub(super) const GLYPH_UNITS_PER_EM: f32 = 1000.0;

/// The font descriptor flag of an italic font.
const ITALIC_FONT_FLAG: i32 = 1 << 6;

/// How far a font without metrics reaches above and below its baseline, as
/// shares of its size: a box that is as high as the font is big.
const DEFAULT_ASCENT: f32 = 0.75;
const DEFAULT_DESCENT: f32 = -0.25;

/// What a font is called and how it looks.
#[derive(Debug, PartialEq)]
pub(super) struct FontDescription {
    /// The name of the font, without the tag that marks a subset.
    pub name: String,
    /// The weight that the font declares, or an estimate from the width of
    /// its stems where it only says that.
    pub weight: Option<u16>,
    pub is_italic: bool,
    /// How far the font reaches above its baseline, as a share of its size.
    pub ascent: f32,
    /// How far the font reaches below its baseline, as a share of its size,
    /// which is negative.
    pub descent: f32,
}

/// What a character code of a font stands for.
#[derive(Clone, Debug)]
pub(super) struct Code {
    /// The text that the glyph of the code maps to.
    /// Where the font does not say, it is the character with the number of
    /// the code, which is what a font without a mapping most often means.
    pub text: Rc<str>,
    /// How far the glyph moves the place of the next one, in glyph space:
    /// to the right in a font that is written across, and down, as a
    /// negative number, in one that is written from the top.
    pub advance: f32,
}

/// A font of a document.
pub(super) struct Font<'a> {
    pub description: Rc<FontDescription>,
    program: hayro_font::Font<'a>,
    /// What the codes stand for if the font does not say and its character
    /// collection does.
    collection_text: Option<CollectionText>,
    codes: RefCell<HashMap<u32, Code>>,
}

impl Font<'_> {
    /// The character code at `offset` of a string that is shown in the
    /// font, and how many bytes it takes.
    pub fn read_code(&self, bytes: &[u8], offset: usize) -> (u32, usize) {
        self.program.read_code(bytes, offset)
    }

    /// Whether the font is written across, and not from the top down.
    pub fn is_horizontal(&self) -> bool {
        self.program.is_horizontal()
    }

    pub fn code(&self, code: u32) -> Code {
        let mut codes = self.codes.borrow_mut();
        let known = codes.entry(code).or_insert_with(|| self.read(code));
        known.clone()
    }

    fn read(&self, code: u32) -> Code {
        let glyph = self.program.glyph(self.program.map_code(code), code);
        let unicode = match glyph {
            FontGlyph::Outline(glyph) => glyph.as_unicode(),
            FontGlyph::Type3(glyph) => glyph.as_unicode(),
        };
        let of_collection = || self.collection_text.as_ref()?.of_code(code);
        let text: Rc<str> = match unicode.or_else(of_collection) {
            Some(BfString::Char(character)) => character.to_string().into(),
            Some(BfString::String(text)) => text.into(),
            None => char::from_u32(code)
                .unwrap_or(char::REPLACEMENT_CHARACTER)
                .to_string()
                .into(),
        };
        let advance = self.program.code_advance(code);
        let advance = match self.is_horizontal() {
            true => advance.x,
            false => advance.y,
        };
        Code {
            text,
            advance: advance as f32,
        }
    }
}

/// The fonts of a document, each of which is loaded when a page first
/// uses it.
pub(super) struct Fonts<'a> {
    programs: FontCache<'a>,
    font_resolver: FontResolverFn,
    cmap_resolver: CMapResolverFn,
    /// The fonts by where their dictionaries are in the document.
    by_dictionary: RefCell<HashMap<usize, Rc<Font<'a>>>>,
}

impl<'a> Fonts<'a> {
    pub fn new() -> Self {
        Self {
            programs: FontCache::new(),
            // A font that the document does not embed is replaced by one
            // of those that every PDF reader has, which are built in.
            font_resolver: Arc::new(|query| match query {
                FontQuery::Standard(font) => Some(font.get_font_data()),
                FontQuery::Fallback(font) => Some(font.pick_standard_font().get_font_data()),
            }),
            cmap_resolver: Arc::new(hayro_cmap::load_embedded),
            by_dictionary: RefCell::new(HashMap::new()),
        }
    }

    /// The font that a font dictionary describes, or Helvetica without
    /// one, which is what text is shown in that names no font of its
    /// page.
    /// A font that cannot be loaded is read as Helvetica, too.
    /// `None` if not even that can be done.
    pub fn get(&self, dictionary: Option<&Dict<'a>>) -> Option<Rc<Font<'a>>> {
        // No dictionary is at the place 0.
        let place = dictionary.map_or(0, |dictionary| dictionary.data().as_ptr() as usize);
        if let Some(font) = self.by_dictionary.borrow().get(&place) {
            return Some(font.clone());
        }
        let program = dictionary
            .and_then(|dictionary| {
                self.programs
                    .font(dictionary, &self.font_resolver, &self.cmap_resolver)
            })
            .or_else(|| self.helvetica())?;
        let font = Rc::new(Font {
            description: Rc::new(dictionary.map_or_else(describe_helvetica, describe)),
            program,
            collection_text: dictionary
                .and_then(|dictionary| CollectionText::of(dictionary, &self.cmap_resolver)),
            codes: RefCell::new(HashMap::new()),
        });
        self.by_dictionary.borrow_mut().insert(place, font.clone());
        Some(font)
    }

    fn helvetica(&self) -> Option<hayro_font::Font<'a>> {
        self.programs
            .standard_font(StandardFont::Helvetica, &self.font_resolver)
    }
}

fn describe_helvetica() -> FontDescription {
    FontDescription {
        name: "Helvetica".to_owned(),
        weight: None,
        is_italic: false,
        ascent: DEFAULT_ASCENT,
        descent: DEFAULT_DESCENT,
    }
}

fn describe(dictionary: &Dict) -> FontDescription {
    // A composite font is described by the font that it is made of.
    let descendants = dictionary.get::<Array>("DescendantFonts");
    let descendant = descendants.and_then(|fonts| fonts.iter::<Dict>().next());
    let described = descendant.as_ref().unwrap_or(dictionary);
    let descriptor = described.get::<Dict>("FontDescriptor");
    let descriptor = descriptor.as_ref();

    let name = described
        .get::<Name>("BaseFont")
        .or_else(|| descriptor?.get::<Name>("FontName"))
        .or_else(|| dictionary.get::<Name>("Name"));
    let name = name.map_or(String::new(), |name| {
        without_subset_tag(name.as_str()).to_owned()
    });
    let number = |key: &str| descriptor?.get::<f32>(key);
    let flags = descriptor.and_then(|descriptor| descriptor.get::<i32>("Flags"));
    let (ascent, descent) = vertical_reach(
        number("Ascent"),
        number("Descent"),
        descriptor.and_then(|descriptor| descriptor.get::<Rect>("FontBBox")),
    );
    FontDescription {
        name,
        weight: number("FontWeight")
            .or_else(|| number("StemV").map(weight_of_stem_width))
            .map(|weight| weight.clamp(0.0, f32::from(u16::MAX)) as u16),
        is_italic: flags.is_some_and(|flags| flags & ITALIC_FONT_FLAG != 0),
        ascent,
        descent,
    }
}

/// How far a font reaches above and below its baseline, as shares of its
/// size, from what its descriptor says in glyph space.
///
/// Neither reaches out of the box around all glyphs of the font, and a
/// descriptor that says nothing of use gets the defaults.
fn vertical_reach(ascent: Option<f32>, descent: Option<f32>, bounds: Option<Rect>) -> (f32, f32) {
    let mut ascent = ascent.unwrap_or(0.0);
    // Some producers write how far the font descends as a positive number.
    let mut descent = -descent.unwrap_or(0.0).abs();
    if let Some(bounds) = bounds.filter(|bounds| bounds.y1 > bounds.y0) {
        ascent = ascent.min(bounds.y1 as f32);
        descent = descent.max(bounds.y0 as f32);
    }
    if ascent <= descent {
        return (DEFAULT_ASCENT, DEFAULT_DESCENT);
    }
    (ascent / GLYPH_UNITS_PER_EM, descent / GLYPH_UNITS_PER_EM)
}

/// An estimate of the weight of a font from the width of its vertical
/// stems, as PDFium makes it, which reads the width as a whole number.
fn weight_of_stem_width(stem_width: f32) -> f32 {
    let stem_width = stem_width.trunc();
    if stem_width < 140.0 {
        stem_width * 5.0
    } else {
        stem_width * 4.0 + 140.0
    }
}

/// Strips the `ABCDEF+` tag that marks a font as a subset.
fn without_subset_tag(font_name: &str) -> &str {
    match font_name.split_once('+') {
        Some((tag, name))
            if tag.len() == 6 && tag.bytes().all(|byte| byte.is_ascii_uppercase()) =>
        {
            name
        }
        _ => font_name,
    }
}

#[cfg(test)]
mod tests {
    use hayro_syntax::Pdf;
    use hayro_syntax::object::ObjectIdentifier;

    use super::*;
    use crate::reader::testing::pdf_of;

    /// The description and the text of the code 65 of the font that the
    /// given objects are, of which the first is the font dictionary.
    fn font_of(objects: &[&str]) -> (Rc<FontDescription>, Rc<str>) {
        let mut all = vec![
            "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
            "<< /Type /Pages /Kids [] /Count 0 >>".to_owned(),
        ];
        all.extend(objects.iter().map(|object| (*object).to_owned()));
        let pdf = Pdf::new(pdf_of(&all)).expect("the PDF opens");
        let dictionary: Dict = pdf
            .xref()
            .get(ObjectIdentifier::new(3, 0))
            .expect("the font is the third object");
        let fonts = Fonts::new();
        let font = fonts.get(Some(&dictionary)).expect("the font loads");
        (font.description.clone(), font.code(65).text)
    }

    #[test]
    fn simple_font_is_described_by_its_dictionary_and_its_descriptor() {
        let (description, text) = font_of(&[
            "<< /Type /Font /Subtype /Type1 /BaseFont /ABCDEF+Fancy-Italic \
             /FontDescriptor 4 0 R >>",
            "<< /Type /FontDescriptor /FontName /ABCDEF+Fancy-Italic /Flags 96 /StemV 80 \
             /Ascent 900 /Descent -200 /FontBBox [-100 -250 1000 950] >>",
        ]);
        let expected = FontDescription {
            name: "Fancy-Italic".to_owned(),
            weight: Some(400),
            is_italic: true,
            ascent: 0.9,
            descent: -0.2,
        };
        assert_eq!(*description, expected);
        assert_eq!(&*text, "A");
    }

    #[test]
    fn composite_font_is_described_by_the_font_it_is_made_of() {
        let (description, _) = font_of(&[
            "<< /Type /Font /Subtype /Type0 /BaseFont /ABCDEF+Serif-Bold-Identity-H \
             /Encoding /Identity-H /DescendantFonts [4 0 R] >>",
            "<< /Type /Font /Subtype /CIDFontType2 /BaseFont /ABCDEF+Serif-Bold \
             /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> \
             /FontDescriptor 5 0 R >>",
            "<< /Type /FontDescriptor /FontName /ABCDEF+Serif-Bold /Flags 4 /FontWeight 700 \
             /StemV 80 >>",
        ]);
        assert_eq!(description.name, "Serif-Bold");
        // The weight that the font declares counts for more than the
        // width of its stems.
        assert_eq!(description.weight, Some(700));
        assert!(!description.is_italic);
    }

    #[test]
    fn font_without_a_descriptor_says_little() {
        let (description, text) =
            font_of(&["<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>"]);
        let expected = FontDescription {
            name: "Helvetica".to_owned(),
            weight: None,
            is_italic: false,
            ascent: DEFAULT_ASCENT,
            descent: DEFAULT_DESCENT,
        };
        assert_eq!(*description, expected);
        assert_eq!(&*text, "A");
    }

    #[test]
    fn code_without_a_meaning_is_read_as_the_character_with_its_number() {
        // The font names its glyphs in a way that says nothing.
        let (_, text) = font_of(&["<< /Type /Font /Subtype /Type1 /BaseFont /Nameless \
             /Encoding << /Type /Encoding /Differences [65 /g17] >> >>"]);
        assert_eq!(&*text, "A");
    }

    #[test]
    fn subset_tag_is_stripped() {
        assert_eq!(
            without_subset_tag("YLHLOX+LibertinusSerif-Regular"),
            "LibertinusSerif-Regular"
        );
    }

    #[test]
    fn other_plus_signs_are_kept() {
        assert_eq!(without_subset_tag("Noto+Sans"), "Noto+Sans");
        assert_eq!(without_subset_tag("abcdef+Font"), "abcdef+Font");
    }

    #[test]
    fn reach_is_what_the_descriptor_says_within_the_box_of_the_font() {
        assert_eq!(
            vertical_reach(Some(900.0), Some(-250.0), None),
            (0.9, -0.25)
        );
        let bounds = Rect::new(-100.0, -200.0, 1000.0, 800.0);
        assert_eq!(
            vertical_reach(Some(900.0), Some(-250.0), Some(bounds)),
            (0.8, -0.2)
        );
    }

    #[test]
    fn descent_is_below_the_baseline_whatever_its_sign() {
        assert_eq!(vertical_reach(Some(900.0), Some(250.0), None), (0.9, -0.25));
    }

    #[test]
    fn font_without_metrics_gets_a_box_as_high_as_it_is_big() {
        assert_eq!(vertical_reach(None, None, None), (0.75, -0.25));
        assert_eq!(vertical_reach(Some(0.0), Some(0.0), None), (0.75, -0.25));
    }

    #[test]
    fn wider_stems_are_a_heavier_font() {
        assert!(weight_of_stem_width(170.0) > weight_of_stem_width(95.0));
        assert_eq!(weight_of_stem_width(80.0), 400.0);
    }
}
