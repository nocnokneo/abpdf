//! The figures of a page.

use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::ffi::c_int;
use std::hash::{Hash, Hasher};

use super::handles::{ObjectKind, PageObject};
use super::matrix::Matrix;
use super::objects::PageObjects;
use super::page_space::{PageSpace, UserSpaceBox};
use super::structure::Structure;
use crate::reader::{Figure, FigureKind};

/// A figure, and the index among the objects of its page of the first
/// object that draws it.
pub(super) struct FoundFigure {
    pub first_object: usize,
    pub figure: Figure,
}

/// The figures of a page, in the order in which the page draws them.
///
/// A figure is something that the producer keeps together:
///
/// - what one piece of marked content draws that the structure tree has as
///   a `Figure`, apart from its text,
/// - an image or a form anywhere else.
///
/// Shapes that a page draws one by one outside of these are no figure,
/// because nothing says which of them belong together.
/// `structures` has the structure of marked content by its identifier.
pub(super) fn read(
    objects: &PageObjects,
    page_space: &PageSpace,
    structures: &HashMap<c_int, Structure>,
) -> Vec<FoundFigure> {
    let mut groups: Vec<Group> = Vec::new();
    for (index, object) in objects.iter().enumerate() {
        let kind = object.kind();
        if kind == ObjectKind::Text {
            continue;
        }
        let id = object.marked_content_id();
        let figure_content = id.filter(|id| structures.get(id).is_some_and(Structure::is_figure));
        let is_figure_alone = matches!(kind, ObjectKind::Image | ObjectKind::Form);
        match groups.last_mut() {
            Some(group) if figure_content.is_some() && group.figure_content == figure_content => {
                group.members.push(index);
            }
            _ if figure_content.is_some() || is_figure_alone => groups.push(Group {
                figure_content,
                members: vec![index],
            }),
            _ => {}
        }
    }
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
    /// The identifier of the marked content that keeps the objects
    /// together, if that is what does.
    figure_content: Option<c_int>,
    /// The indices of the objects among those of the page.
    members: Vec<usize>,
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
    let first = objects.get(group.members[0]);
    Some(Figure {
        bounds,
        kind: looks.kind()?,
        is_artifact: first
            .marks()
            .any(|mark| mark.name().as_deref() == Some("Artifact")),
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
