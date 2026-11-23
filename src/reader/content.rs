//! What a page draws.
//!
//! The content stream of a page is a program: operators set a state and
//! draw with it.
//! [`objects_of`] runs it and reports what it draws as [`Object`]s in the
//! user space of the page, with nothing left of the state that drew them.
//! What a page shows of them is not looked at: nothing is clipped, hidden
//! by optional content or painted over.

mod color;
mod image_data;
mod marks;
mod path;
mod state;
#[cfg(test)]
mod tests;
mod text;

use std::cell::RefCell;
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::Hasher;

use hayro_syntax::content::TypedIter;
use hayro_syntax::content::ops::TypedInstruction;
use hayro_syntax::object::{
    Array, Dict, Name, Number, Object as PdfObject, ObjectIdentifier, Rect, Stream,
};
use hayro_syntax::page::{Page, Resources};

use self::color::ColorSpace;
pub(super) use self::marks::{Mark, Marks};
use self::path::PathBuilder;
pub(super) use self::path::{Fill, FillRule, Path, Stroke};
#[cfg(test)]
pub(super) use self::path::{Segment, SegmentKind};
use self::state::{GraphicsState, Paint};
pub(super) use self::text::{Glyph, TextRun};
use self::text::{TextPosition, TextShow};
use super::fonts::Fonts;
use super::matrix::Matrix;
use super::page_space::UserSpaceBox;
use super::{Color, PageLimit};

/// How much content a page may run, counting a form each time it is drawn,
/// because a form that draws another form several times multiplies what
/// the page runs with each level.
/// Every object that a page draws takes a few bytes of content, so this
/// also bounds the memory that the objects of a page take.
pub(super) const MAX_CONTENT_BYTES_PER_PAGE: usize = 16 << 20;

/// How deep forms may be drawn inside forms, so that a form that draws
/// itself cannot exhaust the stack.
pub(super) const MAX_FORM_DEPTH: usize = 16;

/// How many graphics states may be saved at once, each of which is a copy.
pub(super) const MAX_SAVED_STATE_DEPTH: usize = 1024;

/// How deep marked content may be inside marked content, each level of
/// which copies the levels around it.
pub(super) const MAX_MARKED_CONTENT_DEPTH: usize = 64;

/// Something that a page draws.
#[derive(Clone, Debug)]
pub(super) struct Object {
    /// The marked content that the object is drawn in.
    pub marks: Marks,
    pub content: Content,
}

#[derive(Clone, Debug)]
pub(super) enum Content {
    Text(TextRun),
    Path(Path),
    Image(Image),
    /// A shading that fills what is not clipped away.
    Shading {
        /// The smallest upright box around what is not clipped away.
        bounds: UserSpaceBox,
    },
    /// A form, which stands for the objects that it draws in turn.
    Form(Vec<Object>),
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Image {
    pub pixel_width: u32,
    pub pixel_height: u32,
    /// A hash of the data of the image and of the mask that makes parts of
    /// it see-through.
    pub data_hash: u64,
    /// The corners of the image in the user space of the page, from its
    /// bottom left corner around it.
    pub corners: [(f64, f64); 4],
}

/// What the pages of a document share, which is read when a page first
/// uses it: the fonts, and the hashes of the data of the images.
pub(super) struct Shared<'a> {
    fonts: Fonts<'a>,
    /// By the identifier of the stream of an image, which an image that a
    /// content stream holds itself does not have.
    image_data_hashes: RefCell<HashMap<ObjectIdentifier, u64>>,
}

impl Shared<'_> {
    pub fn new() -> Self {
        Self {
            fonts: Fonts::new(),
            image_data_hashes: RefCell::new(HashMap::new()),
        }
    }

    /// A hash of the data of an image and of the mask that makes parts of
    /// it see-through.
    fn image_data_hash(&self, image: &Stream) -> u64 {
        let id = image.dict().obj_id();
        let known = id.and_then(|id| self.image_data_hashes.borrow().get(&id).copied());
        known.unwrap_or_else(|| {
            let mut data = DefaultHasher::new();
            image_data::hash(image, &mut data);
            if let Some(mask) = image.dict().get::<Stream>("SMask") {
                image_data::hash(&mask, &mut data);
            }
            let hash = data.finish();
            if let Some(id) = id {
                self.image_data_hashes.borrow_mut().insert(id, hash);
            }
            hash
        })
    }
}

/// The objects that a page draws, in the order in which it draws them, or
/// the limit that the page goes beyond.
/// `visible` is the part of the page that a viewer shows.
pub(super) fn objects_of<'a>(
    page: &Page<'a>,
    shared: &Shared<'a>,
    visible: UserSpaceBox,
) -> Result<Vec<Object>, PageLimit> {
    objects_within(page, shared, visible, MAX_CONTENT_BYTES_PER_PAGE)
}

/// What [`objects_of`] is, with how much content the page may run.
fn objects_within<'a>(
    page: &Page<'a>,
    shared: &Shared<'a>,
    visible: UserSpaceBox,
    max_content_bytes: usize,
) -> Result<Vec<Object>, PageLimit> {
    let mut interpreter = Interpreter {
        shared,
        state: GraphicsState::initial(visible),
        saved_states: Vec::new(),
        marks: Marks::default(),
        text_position: TextPosition::default(),
        path: PathBuilder::default(),
        path_is_clip: false,
        form_depth: 0,
        content_bytes_left: max_content_bytes,
    };
    let content = page.page_stream().unwrap_or_default();
    interpreter.run(content, page.resources())
}

struct Interpreter<'s, 'a> {
    shared: &'s Shared<'a>,
    state: GraphicsState<'a>,
    saved_states: Vec<GraphicsState<'a>>,
    /// The marked content that is open.
    marks: Marks,
    text_position: TextPosition,
    /// The path that is being constructed.
    path: PathBuilder,
    /// Whether the path that is being constructed is clipped to once it is
    /// painted.
    path_is_clip: bool,
    /// How many forms are being drawn, one inside the other.
    form_depth: usize,
    /// How much more content the page may run.
    content_bytes_left: usize,
}

impl<'a> Interpreter<'_, 'a> {
    /// Runs a content stream and returns what it draws, or stops at the
    /// first limit that the page goes beyond.
    ///
    /// A stream cannot end what the stream around it began: it brings back
    /// no more states than it saved and ends no more marked content than
    /// it began, and what it leaves open ends with it.
    fn run(&mut self, content: &[u8], resources: &Resources<'a>) -> Result<Vec<Object>, PageLimit> {
        let content_bytes_left = self.content_bytes_left.checked_sub(content.len());
        self.content_bytes_left = content_bytes_left.ok_or(PageLimit::ContentBytes)?;
        let own = OwnOfStream {
            saved_state_count: self.saved_states.len(),
            mark_count: self.marks.count(),
        };
        let mut objects = Vec::new();
        let mut instructions = TypedIter::new(content);
        while let Some(instruction) = instructions.next() {
            self.execute(instruction, resources, &own, &mut objects)?;
        }
        while self.saved_states.len() > own.saved_state_count {
            self.restore_state(&own);
        }
        while self.marks.count() > own.mark_count {
            self.marks = self.marks.without_innermost();
        }
        self.path = PathBuilder::default();
        self.path_is_clip = false;
        Ok(objects)
    }

    /// Executes an instruction and adds what it draws to `objects`.
    fn execute(
        &mut self,
        instruction: TypedInstruction<'_, '_>,
        resources: &Resources<'a>,
        own: &OwnOfStream,
        objects: &mut Vec<Object>,
    ) -> Result<(), PageLimit> {
        use TypedInstruction as I;
        let (non_zero, even_odd) = (Some(FillRule::NonZero), Some(FillRule::EvenOdd));
        let drawn = match instruction {
            // The painting of a path.
            I::EndPath(_) => self.paint(None, false),
            I::StrokePath(_) => self.paint(None, true),
            I::CloseAndStrokePath(_) => self.close_and_paint(None),
            I::FillPathNonZero(_) | I::FillPathNonZeroCompatibility(_) => {
                self.paint(non_zero, false)
            }
            I::FillPathEvenOdd(_) => self.paint(even_odd, false),
            I::FillAndStrokeNonZero(_) => self.paint(non_zero, true),
            I::FillAndStrokeEvenOdd(_) => self.paint(even_odd, true),
            I::CloseFillAndStrokeNonZero(_) => self.close_and_paint(non_zero),
            I::CloseFillAndStrokeEvenOdd(_) => self.close_and_paint(even_odd),

            // Shadings, images and forms.
            I::Shading(_) => Some(Content::Shading {
                bounds: self.state.clip,
            }),
            I::XObject(name) => match resources.get_x_object(name.0) {
                Some(x_object) => self.draw(&x_object, resources)?,
                None => None,
            },
            I::InlineImage(image) => Some(Content::Image(self.image_of(image.0))),

            // The showing of text.
            I::ShowText(text) => self.show([TextPart::String(text.0.as_bytes())]),
            I::NextLineAndShowText(text) => {
                self.start_next_line();
                self.show([TextPart::String(text.0.as_bytes())])
            }
            I::ShowTextWithParameters(text) => {
                self.state.text.word_spacing = text.0.as_f32();
                self.state.text.character_spacing = text.1.as_f32();
                self.start_next_line();
                self.show([TextPart::String(text.2.as_bytes())])
            }
            I::ShowTexts(texts) => self.show_all(texts.0),

            // Everything else draws nothing.
            other => {
                self.set(other, resources, own)?;
                None
            }
        };
        objects.extend(drawn.map(|content| Object {
            marks: self.marks.clone(),
            content,
        }));
        Ok(())
    }

    /// Executes an instruction that draws nothing: one that constructs a
    /// path, or that sets what the instructions after it draw with.
    fn set(
        &mut self,
        instruction: TypedInstruction<'_, '_>,
        resources: &Resources<'a>,
        own: &OwnOfStream,
    ) -> Result<(), PageLimit> {
        use TypedInstruction as I;
        let text = &mut self.state.text;
        match instruction {
            // The graphics state.
            I::SaveState(_) => self.save_state()?,
            I::RestoreState(_) => self.restore_state(own),
            I::Transform(matrix) => {
                let numbers = [matrix.0, matrix.1, matrix.2, matrix.3, matrix.4, matrix.5];
                self.state.to_page = matrix_of(numbers).then(&self.state.to_page);
            }
            I::LineWidth(width) => self.state.line_width = width.0.as_f32(),
            I::SetGraphicsState(name) => {
                if let Some(parameters) = resources.get_ext_g_state(name.0) {
                    self.set_parameters(&parameters);
                }
            }

            // The construction of a path.
            I::MoveTo(point) => self.path.move_to(self.on_page(point.0, point.1)),
            I::LineTo(point) => self.path.line_to(self.on_page(point.0, point.1)),
            I::CubicTo(curve) => {
                let first = self.on_page(curve.0, curve.1);
                let second = self.on_page(curve.2, curve.3);
                self.path
                    .curve_to([first, second], self.on_page(curve.4, curve.5));
            }
            // The first control point is where the curve starts.
            I::CubicStartTo(curve) => {
                let second = self.on_page(curve.0, curve.1);
                let first = self.path.current_point().unwrap_or(second);
                self.path
                    .curve_to([first, second], self.on_page(curve.2, curve.3));
            }
            // The second control point is where the curve ends.
            I::CubicEndTo(curve) => {
                let end = self.on_page(curve.2, curve.3);
                self.path
                    .curve_to([self.on_page(curve.0, curve.1), end], end);
            }
            I::ClosePath(_) => self.path.close(),
            I::RectPath(rectangle) => {
                let (x, y) = (rectangle.0.as_f64(), rectangle.1.as_f64());
                let (right, top) = (x + rectangle.2.as_f64(), y + rectangle.3.as_f64());
                let corners = [(x, y), (right, y), (right, top), (x, top)];
                let to_page = self.state.to_page;
                self.path
                    .rectangle(corners.map(|(x, y)| to_page.apply(x, y)));
            }
            I::ClipNonZero(_) | I::ClipEvenOdd(_) => self.path_is_clip = true,

            // The state of text.
            I::CharacterSpacing(spacing) => text.character_spacing = spacing.0.as_f32(),
            I::WordSpacing(spacing) => text.word_spacing = spacing.0.as_f32(),
            I::HorizontalScaling(percent) => text.horizontal_scale = percent.0.as_f32() / 100.0,
            I::TextLeading(leading) => text.leading = leading.0.as_f32(),
            I::TextRise(rise) => text.rise = rise.0.as_f32(),
            I::TextFont(font) => {
                let dictionary = resources.get_font(font.0);
                text.font = self.shared.fonts.get(dictionary.as_ref());
                text.font_size = font.1.as_f32();
            }

            // The place of text.
            I::BeginText(_) => self.text_position.set(Matrix::IDENTITY),
            I::SetTextMatrix(matrix) => {
                let numbers = [matrix.0, matrix.1, matrix.2, matrix.3, matrix.4, matrix.5];
                self.text_position.set(matrix_of(numbers));
            }
            I::NextLine(offset) => {
                self.text_position
                    .start_line(offset.0.as_f64(), offset.1.as_f64());
            }
            I::NextLineAndSetLeading(offset) => {
                text.leading = -offset.1.as_f32();
                self.text_position
                    .start_line(offset.0.as_f64(), offset.1.as_f64());
            }
            I::NextLineUsingLeading(_) => self.start_next_line(),

            // Marked content.
            I::BeginMarkedContent(mark) => self.begin_marked_content(mark.0, None)?,
            I::BeginMarkedContentWithProperties(mark) => {
                let properties = match mark.1 {
                    PdfObject::Dict(properties) => Some(properties.clone()),
                    PdfObject::Name(name) => resources.properties.get::<Dict>(name),
                    _ => None,
                };
                self.begin_marked_content(mark.0, properties.as_ref())?;
            }
            I::EndMarkedContent(_) => {
                if self.marks.count() > own.mark_count {
                    self.marks = self.marks.without_innermost();
                }
            }

            other => self.set_color(other, resources),
        }
        Ok(())
    }

    /// Executes an instruction that sets the color of fills or of strokes.
    /// Any other instruction changes nothing of what is read here.
    fn set_color(&mut self, instruction: TypedInstruction<'_, '_>, resources: &Resources<'a>) {
        use TypedInstruction as I;
        let (fill, stroke) = (&mut self.state.fill, &mut self.state.stroke);
        let gray = |gray: Number| Paint::of_color(ColorSpace::Gray, &color::numbers(&[gray]));
        let rgb = |rgb: [Number; 3]| Paint::of_color(ColorSpace::Rgb, &color::numbers(&rgb));
        let cmyk = |cmyk: [Number; 4]| Paint::of_color(ColorSpace::Cmyk, &color::numbers(&cmyk));
        match instruction {
            I::ColorSpaceStroke(name) => {
                *stroke = Paint::of_space(ColorSpace::named(name.0, resources));
            }
            I::ColorSpaceNonStroke(name) => {
                *fill = Paint::of_space(ColorSpace::named(name.0, resources));
            }
            I::StrokeColor(color) => stroke.set(&color.0, false),
            I::StrokeColorNamed(color) => stroke.set(&color.0, color.1.is_some()),
            I::NonStrokeColor(color) => fill.set(&color.0, false),
            I::NonStrokeColorNamed(color) => fill.set(&color.0, color.1.is_some()),
            I::StrokeColorDeviceGray(color) => *stroke = gray(color.0),
            I::NonStrokeColorDeviceGray(color) => *fill = gray(color.0),
            I::StrokeColorDeviceRgb(color) => *stroke = rgb([color.0, color.1, color.2]),
            I::NonStrokeColorDeviceRgb(color) => *fill = rgb([color.0, color.1, color.2]),
            I::StrokeColorCmyk(color) => *stroke = cmyk([color.0, color.1, color.2, color.3]),
            I::NonStrokeColorCmyk(color) => *fill = cmyk([color.0, color.1, color.2, color.3]),
            _ => {}
        }
    }

    fn save_state(&mut self) -> Result<(), PageLimit> {
        if self.saved_states.len() >= MAX_SAVED_STATE_DEPTH {
            return Err(PageLimit::SavedStateDepth);
        }
        self.saved_states.push(self.state.clone());
        Ok(())
    }

    fn restore_state(&mut self, own: &OwnOfStream) {
        if self.saved_states.len() > own.saved_state_count
            && let Some(state) = self.saved_states.pop()
        {
            self.state = state;
        }
    }

    /// Sets what a graphics state parameter dictionary has of the state
    /// that is read here.
    fn set_parameters(&mut self, parameters: &Dict<'a>) {
        if let Some(line_width) = parameters.get::<f32>("LW") {
            self.state.line_width = line_width;
        }
        if let Some(fill_opacity) = parameters.get::<f32>("ca") {
            self.state.fill_opacity = fill_opacity;
        }
        if let Some(font_and_size) = parameters.get::<Array>("Font") {
            let mut parts = font_and_size.iter::<PdfObject>();
            let font = parts.next().and_then(PdfObject::into_dict);
            let size = parts.next().and_then(PdfObject::into_f32);
            if let (Some(font), Some(size)) = (font, size) {
                self.state.text.font = self.shared.fonts.get(Some(&font));
                self.state.text.font_size = size;
            }
        }
    }

    /// A point of the space that the content is drawn in, in the user
    /// space of the page.
    fn on_page(&self, x: Number, y: Number) -> (f64, f64) {
        self.state.to_page.apply(x.as_f64(), y.as_f64())
    }

    /// Closes the outline that is being drawn, and strokes the path.
    fn close_and_paint(&mut self, fill_rule: Option<FillRule>) -> Option<Content> {
        self.path.close();
        self.paint(fill_rule, true)
    }

    /// Ends the path that is being constructed, and returns it if it is
    /// painted.
    fn paint(&mut self, fill_rule: Option<FillRule>, is_stroked: bool) -> Option<Content> {
        if std::mem::take(&mut self.path_is_clip)
            && let Some(bounds) = self.path.bounds()
        {
            self.state.clip = self.state.clip.intersection(&bounds);
        }
        let segments = self.path.take();
        if segments.is_empty() || (fill_rule.is_none() && !is_stroked) {
            return None;
        }
        let fill = fill_rule.map(|rule| Fill {
            rule,
            color: self.state.fill.color,
        });
        let stroke = is_stroked.then(|| Stroke {
            color: self.state.stroke.color,
            width: self.state.line_width,
            width_on_page: f64::from(self.state.line_width) * self.state.scale_to_page(),
        });
        Some(Content::Path(Path {
            segments,
            fill,
            stroke,
        }))
    }

    /// What an image or a form draws.
    fn draw(
        &mut self,
        x_object: &Stream<'a>,
        resources: &Resources<'a>,
    ) -> Result<Option<Content>, PageLimit> {
        match x_object.dict().get::<Name>("Subtype").as_deref() {
            Some(b"Image") => Ok(Some(Content::Image(self.image_of(x_object)))),
            Some(b"Form") => self.draw_form(x_object, resources),
            _ => Ok(None),
        }
    }

    fn image_of(&self, image: &Stream) -> Image {
        let dictionary = image.dict();
        // An image in a content stream may shorten its keys.
        let pixels = |key: &str, short_key: &str| {
            let count = dictionary.get::<u32>(key);
            count.or_else(|| dictionary.get::<u32>(short_key))
        };
        // An image fills the unit square of the space that it is drawn in.
        let corners = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)];
        Image {
            pixel_width: pixels("Width", "W").unwrap_or(0),
            pixel_height: pixels("Height", "H").unwrap_or(0),
            data_hash: self.shared.image_data_hash(image),
            corners: corners.map(|(x, y)| self.state.to_page.apply(x, y)),
        }
    }

    /// The objects that a form draws, as one object.
    /// `resources` are those of the stream that draws the form, which the
    /// form uses if it has none of its own.
    fn draw_form(
        &mut self,
        form: &Stream<'a>,
        resources: &Resources<'a>,
    ) -> Result<Option<Content>, PageLimit> {
        if self.form_depth >= MAX_FORM_DEPTH {
            return Err(PageLimit::FormDepth);
        }
        let dictionary = form.dict();
        let Ok(content) = form.decoded_data() else {
            return Ok(None);
        };
        let own_resources = dictionary.get::<Dict>("Resources").map(Resources::new);
        let state_around = self.state.clone();
        if let Some(matrix) = dictionary.get::<[f64; 6]>("Matrix") {
            self.state.to_page = Matrix::from(matrix).then(&self.state.to_page);
        }
        if let Some(bounds) = dictionary.get::<Rect>("BBox") {
            let to_page = self.state.to_page;
            let corners = [
                (bounds.x0, bounds.y0),
                (bounds.x1, bounds.y0),
                (bounds.x1, bounds.y1),
                (bounds.x0, bounds.y1),
            ];
            let on_page = corners.map(|(x, y)| to_page.apply(x, y));
            let bounds = UserSpaceBox::bounding(on_page).expect("a box has corners");
            self.state.clip = self.state.clip.intersection(&bounds);
        }
        self.form_depth += 1;
        let objects = self.run(&content, own_resources.as_ref().unwrap_or(resources))?;
        self.form_depth -= 1;
        self.state = state_around;
        Ok(Some(Content::Form(objects)))
    }

    fn start_next_line(&mut self) {
        let leading = f64::from(self.state.text.leading);
        self.text_position.start_line(0.0, -leading);
    }

    /// Shows the strings and makes the adjustments of one operator, and
    /// returns the glyphs that it shows, if it shows any.
    fn show<'p>(&mut self, parts: impl IntoIterator<Item = TextPart<'p>>) -> Option<Content> {
        let fill_color = self.fill_color();
        let mut text = TextShow::new(
            &mut self.text_position,
            &self.state.text,
            self.state.to_page,
        )?;
        for part in parts {
            match part {
                TextPart::String(bytes) => text.string(bytes, fill_color),
                TextPart::Adjustment(thousandths) => text.adjust(thousandths),
            }
        }
        text.finish().map(Content::Text)
    }

    /// Shows an array of strings and of the adjustments between them.
    fn show_all(&mut self, texts: &Array) -> Option<Content> {
        let items: Vec<PdfObject> = texts.iter::<PdfObject>().collect();
        let parts = items.iter().filter_map(|item| match item {
            PdfObject::String(text) => Some(TextPart::String(text.as_bytes())),
            PdfObject::Number(number) => Some(TextPart::Adjustment(number.as_f32())),
            _ => None,
        });
        self.show(parts)
    }

    /// The color that text is filled with, which is black for a pattern.
    fn fill_color(&self) -> Color {
        let [red, green, blue] = self.state.fill.color.unwrap_or(color::BLACK);
        let opacity = self.state.fill_opacity.clamp(0.0, 1.0);
        Color {
            red,
            green,
            blue,
            alpha: (opacity * 255.0).round() as u8,
        }
    }

    fn begin_marked_content(
        &mut self,
        tag: &Name,
        properties: Option<&Dict>,
    ) -> Result<(), PageLimit> {
        if self.marks.count() >= MAX_MARKED_CONTENT_DEPTH {
            return Err(PageLimit::MarkedContentDepth);
        }
        let is_in_form = self.form_depth > 0;
        self.marks = self.marks.with(Mark::new(tag, properties, is_in_form));
        Ok(())
    }
}

/// What a content stream may end, because it is the stream that began it.
struct OwnOfStream {
    /// How many graphics states were saved when the stream started.
    saved_state_count: usize,
    /// How much marked content was open when the stream started.
    mark_count: usize,
}

/// A part of what an operator that shows text is given.
enum TextPart<'p> {
    String(&'p [u8]),
    /// How far the next glyph is moved back, in thousandths of the font
    /// size.
    Adjustment(f32),
}

fn matrix_of(numbers: [Number; 6]) -> Matrix {
    Matrix::from(numbers.map(|number| number.as_f64()))
}
