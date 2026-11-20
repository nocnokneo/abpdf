//! The figures of a page.

mod loose;

use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::ffi::c_int;
use std::hash::{Hash, Hasher};

use super::handles::{Mark, ObjectKind, PageObject};
use super::matrix::Matrix;
use super::objects::PageObjects;
use super::page_space::{PageSpace, UserSpaceBox};
use super::structure::Structure;
use crate::geometry::Rect;
use crate::reader::{Figure, FigureKind};

/// A figure, and the index among the objects of its page of the first
/// object that draws it.
pub(super) struct FoundFigure {
    pub first_object: usize,
    pub figure: Figure,
}

/// The figures of a page, in the order in which the page draws them.
///
/// A figure is what the producer keeps together:
///
/// - what one piece of marked content draws that the structure tree has as
///   a `Figure`, apart from its text,
/// - an image or a form anywhere else,
///
/// or what the page draws as one of the shapes that nothing keeps
/// together: see [`loose`].
/// `structures` has the structure of marked content by its identifier.
pub(super) fn read(
    objects: &PageObjects,
    page_space: &PageSpace,
    structures: &HashMap<c_int, Structure>,
) -> Vec<FoundFigure> {
    let mut groups = kept_together(objects, structures);
    groups.extend(drawn_as_one(objects, page_space, structures, &groups));
    groups.sort_by_key(|group| group.members[0]);
    let figures = groups.iter().filter_map(|group| {
        Some(FoundFigure {
            first_object: group.members[0],
            figure: figure_of(group, objects, page_space)?,
        })
    });
    figures.collect()
}

/// Objects of a page that are one figure.
struct Group {
    /// The indices of the objects among those of the page.
    members: Vec<usize>,
    /// Whether the producer marks the objects as an artifact.
    is_artifact: bool,
}

/// The objects that the producer keeps together, as a group for each
/// figure.
fn kept_together(objects: &PageObjects, structures: &HashMap<c_int, Structure>) -> Vec<Group> {
    let mut groups: Vec<Group> = Vec::new();
    // The identifier of the marked content that keeps the last group
    // together, if that is what does.
    let mut content_of_last = None;
    for (index, object) in objects.iter().enumerate() {
        let kind = object.kind();
        if kind == ObjectKind::Text {
            continue;
        }
        let id = object.marked_content_id();
        let figure_content = id.filter(|id| structures.get(id).is_some_and(Structure::is_figure));
        let is_figure_alone = matches!(kind, ObjectKind::Image | ObjectKind::Form);
        match groups.last_mut() {
            Some(group) if figure_content.is_some() && content_of_last == figure_content => {
                group.members.push(index);
            }
            _ if figure_content.is_some() || is_figure_alone => {
                groups.push(Group {
                    members: vec![index],
                    is_artifact: artifact_marks(object).next().is_some(),
                });
                content_of_last = figure_content;
            }
            _ => {}
        }
    }
    groups
}

/// The shapes that nothing keeps together and that the page draws as one
/// drawing, as a group for each drawing.
/// `kept_together` has the objects that are figures already.
fn drawn_as_one(
    objects: &PageObjects,
    page_space: &PageSpace,
    structures: &HashMap<c_int, Structure>,
    kept_together: &[Group],
) -> Vec<Group> {
    let members_of_figures: HashSet<usize> = kept_together
        .iter()
        .flat_map(|group| group.members.iter().copied())
        .collect();
    let items: Vec<loose::Item> = objects
        .iter()
        .enumerate()
        .map(
            |(index, object)| match members_of_figures.contains(&index) {
                true => loose::Item::Other,
                false => loose_item_of(object, page_space, structures),
            },
        )
        .collect();
    let drawings = loose::drawings(&items).into_iter();
    drawings
        .map(|members| Group {
            members,
            is_artifact: false,
        })
        .collect()
}

/// What an object that is in no figure is to a drawing of loose shapes.
fn loose_item_of(
    object: &PageObject,
    page_space: &PageSpace,
    structures: &HashMap<c_int, Structure>,
) -> loose::Item {
    let bounds = object.bounds().map(|bounds| page_space.rect(bounds));
    match (object.kind(), bounds) {
        (ObjectKind::Text, Some(bounds)) => {
            let id = object.marked_content_id();
            let structure = id.and_then(|id| structures.get(&id));
            loose::Item::Text {
                center: (
                    (bounds.left + bounds.right) / 2.0,
                    (bounds.top + bounds.bottom) / 2.0,
                ),
                cell: structure.and_then(|structure| structure.cell),
            }
        }
        (ObjectKind::Path | ObjectKind::Shading, Some(bounds))
            if !is_page_furniture(object, &bounds, page_space) =>
        {
            let outline = outline_of(object, page_space);
            loose::Item::Shape {
                bounds: Rect::bounding(outline).unwrap_or(bounds),
            }
        }
        _ => loose::Item::Other,
    }
}

/// The points of the outline of a path of a page, in page space.
fn outline_of(path: &PageObject, page_space: &PageSpace) -> Vec<(f32, f32)> {
    let to_page = path.matrix();
    let points = path.path_segments().map(|segment| {
        let (x, y) = to_page.apply(segment.point.0.into(), segment.point.1.into());
        page_space.point(x, y)
    });
    points.collect()
}

/// How much of the width and of the height of a page a shape has to take
/// to be a frame or a background of the page, and no part of a drawing.
const MIN_SHARE_OF_PAGE_OF_A_FRAME: f32 = 0.8;

/// Whether a shape belongs to the page and not to what is on it.
///
/// Such a shape is marked as an artifact that says where on the page it is
/// attached, what it is a part of the page for or which area it takes.
/// A shape that the producer only marks as layout has none of these, and
/// may well be part of a drawing: PDFium does not tell the type of an
/// artifact, which would say it outright.
/// A shape that takes most of the page is its frame or its background
/// without a mark, too.
fn is_page_furniture(shape: &PageObject, bounds: &Rect, page_space: &PageSpace) -> bool {
    let says_what_it_is_to_the_page = |mark: &Mark| {
        ["Subtype", "Attached", "BBox"]
            .iter()
            .any(|key| mark.has_property(key))
    };
    let page = page_space.size();
    let takes_most_of_the_page = bounds.width() >= MIN_SHARE_OF_PAGE_OF_A_FRAME * page.width
        && bounds.height() >= MIN_SHARE_OF_PAGE_OF_A_FRAME * page.height;
    artifact_marks(shape).any(|mark| says_what_it_is_to_the_page(&mark)) || takes_most_of_the_page
}

/// The marks of an object that say that it is an artifact.
fn artifact_marks<'a>(object: &PageObject<'a>) -> impl Iterator<Item = Mark<'a>> {
    let marks = object.marks();
    marks.filter(|mark| mark.name().as_deref() == Some("Artifact"))
}

/// The figure that a group of objects is, unless they draw nothing.
fn figure_of(group: &Group, objects: &PageObjects, page_space: &PageSpace) -> Option<Figure> {
    let mut looks = Looks::default();
    for &member in &group.members {
        objects.visit_parts_of(member, &mut |part, to_page| {
            looks.add(part, to_page, page_space);
        });
    }
    let members = group.members.iter().map(|&member| objects.get(member));
    let bounds = members
        .clone()
        .filter_map(|member| Some(page_space.rect(member.bounds()?)))
        .reduce(|bounds, next| bounds.union(&next))?;
    Some(Figure {
        bounds,
        kind: looks.kind()?,
        is_artifact: group.is_artifact,
        content_hash: looks.content.finish(),
        points: looks.points,
    })
}

/// What a figure looks like, gathered from its parts.
#[derive(Default)]
struct Looks {
    /// Takes in everything about the parts that is not a position.
    content: DefaultHasher,
    points: Vec<(f32, f32)>,
    part_count: usize,
    /// The pixel sizes of the parts that are images.
    image_pixel_sizes: Vec<(u32, u32)>,
}

impl Looks {
    /// Takes in a part, which is no form.
    /// `to_page` takes the space that the part is drawn in to that of its
    /// page.
    fn add(&mut self, part: &PageObject, to_page: &Matrix, page_space: &PageSpace) {
        let kind = part.kind();
        let own_to_page = part.matrix().then(to_page);
        let mut points = Vec::new();
        match kind {
            ObjectKind::Path => {
                let paint = part.path_paint();
                paint.hash(&mut self.content);
                if paint.fill_mode != 0 {
                    part.fill_color().hash(&mut self.content);
                }
                if paint.is_stroked {
                    part.stroke_color().hash(&mut self.content);
                    part.stroke_width().to_bits().hash(&mut self.content);
                }
                for segment in part.path_segments() {
                    (segment.kind, segment.closes_outline).hash(&mut self.content);
                    let (x, y) = segment.point;
                    points.push(own_to_page.apply(x.into(), y.into()));
                }
            }
            ObjectKind::Image => {
                let pixel_size = part.image_pixel_size();
                pixel_size.hash(&mut self.content);
                part.image_data().hash(&mut self.content);
                self.image_pixel_sizes.push(pixel_size);
                // An image fills the unit square of its own space.
                let corners = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)];
                points.extend(corners.map(|(x, y)| own_to_page.apply(x, y)));
            }
            ObjectKind::Shading => {
                if let Some(bounds) = part.bounds() {
                    let UserSpaceBox {
                        left,
                        bottom,
                        right,
                        top,
                    } = bounds;
                    points.push(to_page.apply(left, bottom));
                    points.push(to_page.apply(right, top));
                }
            }
            ObjectKind::Text | ObjectKind::Form | ObjectKind::Other => return,
        }
        (kind, points.len()).hash(&mut self.content);
        let in_page_space = points.into_iter().map(|(x, y)| page_space.point(x, y));
        self.points.extend(in_page_space);
        self.part_count += 1;
    }

    /// What kind of figure the parts are, if there are any.
    fn kind(&self) -> Option<FigureKind> {
        match (self.part_count, self.image_pixel_sizes.as_slice()) {
            (0, _) => None,
            (1, &[(pixel_width, pixel_height)]) => Some(FigureKind::Image {
                pixel_width,
                pixel_height,
            }),
            (part_count, _) => Some(FigureKind::Drawing { part_count }),
        }
    }
}
