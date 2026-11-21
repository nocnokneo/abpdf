//! The figures of a page.

mod loose;

use std::collections::HashSet;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::mem;

use super::content::{Content, Object};
use super::page_space::{PageSpace, UserSpaceBox};
use super::structure::{PageStructure, Structure};
use super::{Figure, FigureKind};
use crate::geometry::Rect;

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
pub(super) fn read(
    objects: &[Object],
    page_space: &PageSpace,
    structure: &PageStructure,
) -> Vec<FoundFigure> {
    let mut groups = kept_together(objects, structure);
    groups.extend(drawn_as_one(objects, page_space, structure, &groups));
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
fn kept_together(objects: &[Object], structure: &PageStructure) -> Vec<Group> {
    let mut groups: Vec<Group> = Vec::new();
    // The identifier of the marked content that keeps the last group
    // together, if that is what does.
    let mut content_of_last = None;
    for (index, object) in objects.iter().enumerate() {
        if matches!(object.content, Content::Text(_)) {
            continue;
        }
        let id = object.marks.marked_content_id();
        let figure_content = id.filter(|id| structure.get(id).is_some_and(Structure::is_figure));
        let is_figure_alone = matches!(object.content, Content::Image(_) | Content::Form(_));
        match groups.last_mut() {
            Some(group) if figure_content.is_some() && content_of_last == figure_content => {
                group.members.push(index);
            }
            _ if figure_content.is_some() || is_figure_alone => {
                groups.push(Group {
                    members: vec![index],
                    is_artifact: object.marks.is_artifact(),
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
    objects: &[Object],
    page_space: &PageSpace,
    structure: &PageStructure,
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
                false => loose_item_of(object, page_space, structure),
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
    object: &Object,
    page_space: &PageSpace,
    structure: &PageStructure,
) -> loose::Item {
    let shape_within = |painted: UserSpaceBox, outline: UserSpaceBox| match is_page_furniture(
        object,
        &page_space.rect(painted),
        page_space,
    ) {
        true => loose::Item::Other,
        false => loose::Item::Shape {
            bounds: page_space.rect(outline),
        },
    };
    match &object.content {
        Content::Text(run) => {
            let glyphs = run.glyphs.iter().map(|glyph| glyph.bounds);
            let Some(bounds) = glyphs.reduce(|bounds, next| bounds.union(&next)) else {
                return loose::Item::Other;
            };
            let bounds = page_space.rect(bounds);
            let id = object.marks.marked_content_id();
            let structure = id.and_then(|id| structure.get(&id));
            loose::Item::Text {
                center: (
                    (bounds.left + bounds.right) / 2.0,
                    (bounds.top + bounds.bottom) / 2.0,
                ),
                cell: structure.and_then(|structure| structure.cell),
            }
        }
        Content::Path(path) => match (path.bounds(), UserSpaceBox::bounding(path.points())) {
            (Some(painted), Some(outline)) => shape_within(painted, outline),
            _ => loose::Item::Other,
        },
        Content::Shading { bounds } => shape_within(*bounds, *bounds),
        Content::Image(_) | Content::Form(_) => loose::Item::Other,
    }
}

/// The kinds of artifact that belong to the page and not to what is on it:
/// its headers and footers, the marks that a printer cuts along and what
/// is behind everything else.
/// A shape that is a layout artifact may well be part of a drawing.
const KINDS_OF_ARTIFACT_OF_THE_PAGE: [&str; 3] = ["Pagination", "Page", "Background"];

/// How much of the width and of the height of a page a shape has to take
/// to be a frame or a background of the page, and no part of a drawing.
const MIN_SHARE_OF_PAGE_OF_A_FRAME: f32 = 0.8;

/// Whether a shape belongs to the page and not to what is on it: the
/// producer marks it as an artifact of such a kind, or it takes most of
/// the page, as a frame or a background does without a mark.
fn is_page_furniture(shape: &Object, bounds: &Rect, page_space: &PageSpace) -> bool {
    let is_of_the_page = shape.marks.artifacts().any(|mark| {
        let kind = mark.artifact_type.as_deref();
        kind.is_some_and(|kind| KINDS_OF_ARTIFACT_OF_THE_PAGE.contains(&kind))
    });
    let page = page_space.size();
    let takes_most_of_the_page = bounds.width() >= MIN_SHARE_OF_PAGE_OF_A_FRAME * page.width
        && bounds.height() >= MIN_SHARE_OF_PAGE_OF_A_FRAME * page.height;
    is_of_the_page || takes_most_of_the_page
}

/// The figure that a group of objects is, unless they draw nothing.
fn figure_of(group: &Group, objects: &[Object], page_space: &PageSpace) -> Option<Figure> {
    let mut looks = Looks::default();
    for &member in &group.members {
        looks.add(&objects[member]);
    }
    Some(Figure {
        bounds: page_space.rect(looks.bounds?),
        kind: looks.kind()?,
        is_artifact: group.is_artifact,
        content_hash: looks.content.finish(),
        points: looks
            .points
            .into_iter()
            .map(|(x, y)| page_space.point(x, y))
            .collect(),
    })
}

/// What a figure looks like, gathered from its parts.
#[derive(Default)]
struct Looks {
    /// Takes in everything about the parts that is not a position.
    content: DefaultHasher,
    /// The points that the parts are drawn through, in the user space of
    /// the page.
    points: Vec<(f64, f64)>,
    /// The smallest upright box around what the parts paint.
    bounds: Option<UserSpaceBox>,
    part_count: usize,
    /// The pixel sizes of the parts that are images.
    image_pixel_sizes: Vec<(u32, u32)>,
}

impl Looks {
    /// Takes in an object: as one part, or as the parts that it draws if it
    /// is a form.
    /// Text is no part of a figure.
    fn add(&mut self, object: &Object) {
        let point_count_before = self.points.len();
        let painted = match &object.content {
            Content::Text(_) => return,
            Content::Form(parts) => {
                for part in parts {
                    self.add(part);
                }
                return;
            }
            Content::Path(path) => {
                path.fill.hash(&mut self.content);
                let stroke = path
                    .stroke
                    .map(|stroke| (stroke.color, stroke.width.to_bits()));
                stroke.hash(&mut self.content);
                for segment in &path.segments {
                    (segment.kind, segment.closes_outline).hash(&mut self.content);
                }
                self.points.extend(path.points());
                path.bounds()
            }
            Content::Image(image) => {
                let pixel_size = (image.pixel_width, image.pixel_height);
                (pixel_size, image.data_hash).hash(&mut self.content);
                self.image_pixel_sizes.push(pixel_size);
                self.points.extend(image.corners);
                UserSpaceBox::bounding(image.corners)
            }
            Content::Shading { bounds } => {
                self.points.push((bounds.left, bounds.bottom));
                self.points.push((bounds.right, bounds.top));
                Some(*bounds)
            }
        };
        let point_count = self.points.len() - point_count_before;
        (mem::discriminant(&object.content), point_count).hash(&mut self.content);
        self.bounds = match (self.bounds, painted) {
            (Some(bounds), Some(painted)) => Some(bounds.union(&painted)),
            (bounds, painted) => bounds.or(painted),
        };
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::reader::content::{Fill, FillRule, Image, Mark, Marks, Path};
    use crate::reader::content::{Segment, SegmentKind};
    use crate::reader::testing::PAGE;

    /// A filled square with sides of 20 whose bottom left corner is at
    /// (`x`, `y`), in the given color.
    fn square_in(x: f64, y: f64, color: [u8; 3]) -> Content {
        let corners = [(x, y), (x + 20.0, y), (x + 20.0, y + 20.0), (x, y + 20.0)];
        let segments = corners.iter().enumerate().map(|(index, &point)| Segment {
            kind: match index {
                0 => SegmentKind::Move,
                _ => SegmentKind::Line,
            },
            closes_outline: index == 3,
            point,
        });
        Content::Path(Path {
            segments: segments.collect(),
            fill: Some(Fill {
                rule: FillRule::NonZero,
                color: Some(color),
            }),
            stroke: None,
        })
    }

    fn square(x: f64, y: f64) -> Content {
        square_in(x, y, [0, 0, 0])
    }

    fn image(data_hash: u64) -> Content {
        Content::Image(Image {
            pixel_width: 640,
            pixel_height: 480,
            data_hash,
            corners: [(0.0, 0.0), (64.0, 0.0), (64.0, 48.0), (0.0, 48.0)],
        })
    }

    fn unmarked(content: Content) -> Object {
        marked(content, &Marks::default())
    }

    fn marked(content: Content, marks: &Marks) -> Object {
        Object {
            marks: marks.clone(),
            content,
        }
    }

    fn mark(tag: &str, marked_content_id: Option<i32>, artifact_type: Option<&str>) -> Marks {
        Marks::default().with(Mark {
            tag: tag.to_owned(),
            marked_content_id,
            actual_text: None,
            artifact_type: artifact_type.map(str::to_owned),
        })
    }

    fn figures_of(objects: &[Object]) -> Vec<FoundFigure> {
        read(objects, &PageSpace::new(PAGE, 0), &HashMap::new())
    }

    fn kinds_of(objects: &[Object]) -> Vec<FigureKind> {
        let figures = figures_of(objects).into_iter();
        figures.map(|found| found.figure.kind).collect()
    }

    const PHOTO: FigureKind = FigureKind::Image {
        pixel_width: 640,
        pixel_height: 480,
    };

    #[test]
    fn image_is_a_figure_where_it_is_drawn() {
        let figures = figures_of(&[unmarked(square(0.0, 0.0)), unmarked(image(1))]);
        // A square alone is a drawing, too.
        let [_, found] = figures.as_slice() else {
            panic!("{} figures", figures.len());
        };
        assert_eq!((found.first_object, found.figure.kind), (1, PHOTO));
        // The page is 300 high, and the image 48.
        let bounds = found.figure.bounds;
        assert_eq!((bounds.left, bounds.top), (0.0, 252.0));
        assert_eq!((bounds.right, bounds.bottom), (64.0, 300.0));
        assert_eq!(found.figure.points.len(), 4);
    }

    #[test]
    fn form_is_one_figure_of_what_it_draws_apart_from_text() {
        let form = |parts: Vec<Content>| {
            unmarked(Content::Form(parts.into_iter().map(unmarked).collect()))
        };
        let inner = Content::Form(vec![unmarked(square(40.0, 0.0))]);
        assert_eq!(
            kinds_of(&[form(vec![square(0.0, 0.0), image(1), inner])]),
            [FigureKind::Drawing { part_count: 3 }]
        );
        assert_eq!(kinds_of(&[form(vec![image(1)])]), [PHOTO]);
        assert_eq!(kinds_of(&[form(Vec::new())]), []);
    }

    #[test]
    fn what_one_piece_of_figure_content_draws_is_one_figure() {
        let in_figure = mark("Figure", Some(4), None);
        let in_next_figure = mark("Figure", Some(5), None);
        let objects = [
            marked(square(0.0, 0.0), &in_figure),
            marked(image(1), &in_figure),
            marked(image(2), &in_next_figure),
        ];
        let figure = Structure {
            path: "Document/Figure".to_owned(),
            cell: None,
        };
        let structure = HashMap::from([(4, figure.clone()), (5, figure)]);
        let figures = read(&objects, &PageSpace::new(PAGE, 0), &structure);
        let kinds: Vec<FigureKind> = figures.iter().map(|found| found.figure.kind).collect();
        assert_eq!(kinds, [FigureKind::Drawing { part_count: 2 }, PHOTO]);
    }

    #[test]
    fn figure_looks_the_same_wherever_it_stands() {
        let figure_of = |content: Content| figures_of(&[unmarked(content)]).remove(0).figure;
        let here = figure_of(square(0.0, 0.0));
        let there = figure_of(square(50.0, 70.0));
        assert_eq!(here.content_hash, there.content_hash);
        assert_ne!(here.points, there.points);
        assert_ne!(
            here.content_hash,
            figure_of(square_in(0.0, 0.0, [255, 0, 0])).content_hash
        );
        assert_ne!(
            figure_of(image(1)).content_hash,
            figure_of(image(2)).content_hash
        );
    }

    #[test]
    fn figure_is_an_artifact_if_marks_around_it_say_so() {
        let artifact = mark("Artifact", None, None);
        let figures = figures_of(&[marked(image(1), &artifact), unmarked(image(1))]);
        let are_artifacts: Vec<bool> = figures
            .iter()
            .map(|found| found.figure.is_artifact)
            .collect();
        assert_eq!(are_artifacts, [true, false]);
    }

    #[test]
    fn shape_that_belongs_to_the_page_is_no_drawing() {
        let drawing = [FigureKind::Drawing { part_count: 1 }];
        let kinds_of_artifact = |kind: Option<&str>| {
            kinds_of(&[marked(square(0.0, 0.0), &mark("Artifact", None, kind))])
        };
        // What a producer marks as layout may be part of a diagram.
        assert_eq!(kinds_of_artifact(Some("Layout")), drawing);
        assert_eq!(kinds_of_artifact(None), drawing);
        assert_eq!(kinds_of_artifact(Some("Pagination")), []);
        assert_eq!(kinds_of_artifact(Some("Page")), []);
        assert_eq!(kinds_of_artifact(Some("Background")), []);
    }

    #[test]
    fn shape_that_takes_most_of_the_page_is_no_drawing() {
        let rectangle = |width: f64, height: f64| {
            let Content::Path(mut path) = square(0.0, 0.0) else {
                unreachable!("a square is a path");
            };
            path.segments[1].point = (width, 0.0);
            path.segments[2].point = (width, height);
            path.segments[3].point = (0.0, height);
            unmarked(Content::Path(path))
        };
        // The page is 200 wide and 300 high.
        assert_eq!(kinds_of(&[rectangle(170.0, 250.0)]), []);
        assert_eq!(kinds_of(&[rectangle(170.0, 100.0)]).len(), 1);
    }
}
