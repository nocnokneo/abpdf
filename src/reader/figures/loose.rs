//! Drawings among the shapes of a page that nothing keeps together.
//!
//! A page draws a diagram shape by shape, the way it draws the rules of a
//! table, the line under underlined text and the box behind highlighted
//! text.
//! Nothing in a PDF says which of those shapes belong together, or which of
//! them are a drawing.
//! Both are told here from the order in which a page draws its shapes and
//! from the text that stands in them.

use std::ops::Range;

use crate::geometry::Rect;

/// A shape that is no thicker than this is a rule, in units of page space:
/// a line of a table, a line under or through text, or a bar next to it.
const MAX_RULE_THICKNESS: f32 = 4.0;

/// How far apart two shapes can be and still touch, in units of page space.
const TOUCH_DISTANCE: f32 = 1.0;

/// What tells of an object of a page whether it is part of a drawing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::reader) enum Item {
    /// A shape that nothing keeps together with others.
    Shape {
        /// The smallest upright rectangle around its outline.
        bounds: Rect,
    },
    Text {
        /// The middle of the text.
        center: (f32, f32),
        /// The table cell that the text is in, if tags say that it is in
        /// one.
        cell: Option<usize>,
    },
    /// Something that no drawing goes across, such as an object of a figure
    /// that the producer keeps together.
    Other,
}

/// The drawings among the items of a page, which are in the order in which
/// the page draws them: for each drawing, the indices of its shapes.
///
/// Shapes that a page draws one after another are one run.
/// Two runs are one if nothing is between them but text that stands in a
/// shape of either, so that the label of a box does not end a diagram.
///
/// A run is a drawing if it has more to it than rules and boxes of text,
/// where a rule is a thin shape and a box of text is any other shape that
/// text stands in:
///
/// - a shape that is neither, such as the head of an arrow, or
/// - a rule that joins a box of text to something: it touches the box and
///   does not lie in it, as the line between two boxes of a diagram does
///   and the bar at the edge of a note does not.
///
/// A run whose shapes have the text of several table cells in them is what
/// the table draws around its cells, and no drawing.
pub(in crate::reader) fn drawings(items: &[Item]) -> Vec<Vec<usize>> {
    let mut runs: Vec<Run> = Vec::new();
    for stretch in stretches(items) {
        let continues_last = runs.last().is_some_and(|run| {
            let shapes_of_both = || run.shapes.iter().copied().chain(stretch.shapes.clone());
            let mut texts_between = run.texts_after.clone();
            run.texts_after.end == stretch.shapes.start
                && texts_between.all(|text| stands_in_one_of(items, text, shapes_of_both()))
        });
        match runs.last_mut() {
            Some(run) if continues_last => {
                run.texts.extend(run.texts_after.clone());
                run.shapes.extend(stretch.shapes);
                run.texts_after = stretch.texts_after;
            }
            _ => runs.push(Run {
                shapes: stretch.shapes.collect(),
                texts: Vec::new(),
                texts_after: stretch.texts_after,
            }),
        }
    }
    let drawings = runs.into_iter().filter_map(|mut run| {
        let texts_after = run.texts_after.clone();
        let texts_in_last_shapes = texts_after
            .take_while(|&text| stands_in_one_of(items, text, run.shapes.iter().copied()));
        run.texts.extend(texts_in_last_shapes);
        is_drawing(items, &run).then_some(run.shapes)
    });
    drawings.collect()
}

/// Shapes that a page draws one after another, and the texts that it draws
/// right after them.
struct Stretch {
    shapes: Range<usize>,
    texts_after: Range<usize>,
}

/// Shapes that belong together.
struct Run {
    shapes: Vec<usize>,
    /// The texts that stand in the shapes.
    texts: Vec<usize>,
    /// The texts that the page draws right after the last of the shapes, of
    /// which the first ones may stand in the shapes.
    texts_after: Range<usize>,
}

fn stretches(items: &[Item]) -> Vec<Stretch> {
    let count_from = |start: usize, is_counted: fn(&Item) -> bool| {
        let counted = items[start..].iter().take_while(|item| is_counted(item));
        counted.count()
    };
    let mut stretches = Vec::new();
    let mut index = 0;
    while index < items.len() {
        let shapes_end = index + count_from(index, |item| matches!(item, Item::Shape { .. }));
        if shapes_end == index {
            index += 1;
            continue;
        }
        let texts_end =
            shapes_end + count_from(shapes_end, |item| matches!(item, Item::Text { .. }));
        stretches.push(Stretch {
            shapes: index..shapes_end,
            texts_after: shapes_end..texts_end,
        });
        index = texts_end;
    }
    stretches
}

/// Whether the text at an index has its middle in one of the shapes at
/// `shapes`.
fn stands_in_one_of(items: &[Item], text: usize, mut shapes: impl Iterator<Item = usize>) -> bool {
    let Item::Text { center, .. } = items[text] else {
        return false;
    };
    shapes.any(|shape| match items[shape] {
        Item::Shape { bounds } => contains(&bounds, center),
        _ => false,
    })
}

fn contains(bounds: &Rect, (x, y): (f32, f32)) -> bool {
    bounds.left <= x && x <= bounds.right && bounds.top <= y && y <= bounds.bottom
}

/// Whether the shapes of a run are a drawing.
fn is_drawing(items: &[Item], run: &Run) -> bool {
    let texts = run.texts.iter().filter_map(|&text| match items[text] {
        Item::Text { center, cell } => Some((center, cell)),
        _ => None,
    });
    let mut cells: Vec<usize> = texts.clone().filter_map(|(_, cell)| cell).collect();
    cells.sort_unstable();
    cells.dedup();
    if cells.len() > 1 {
        return false;
    }

    let shapes = run.shapes.iter().filter_map(|&shape| match items[shape] {
        Item::Shape { bounds } => Some(bounds),
        _ => None,
    });
    let is_rule = |bounds: &Rect| bounds.width().min(bounds.height()) <= MAX_RULE_THICKNESS;
    let has_text = |bounds: &Rect| texts.clone().any(|(center, _)| contains(bounds, center));
    let (rules, others): (Vec<Rect>, Vec<Rect>) = shapes.partition(is_rule);
    let (boxes_of_text, shapes_of_their_own): (Vec<Rect>, Vec<Rect>) =
        others.into_iter().partition(has_text);
    let joins_a_box = |rule: &Rect| {
        let mut boxes = boxes_of_text.iter();
        boxes.any(|text_box| touches(rule, text_box) && reaches_out_of(rule, text_box))
    };
    !shapes_of_their_own.is_empty() || rules.iter().any(joins_a_box)
}

/// Whether two rectangles are no further apart than [`TOUCH_DISTANCE`].
fn touches(rect: &Rect, other: &Rect) -> bool {
    rect.left - TOUCH_DISTANCE <= other.right
        && other.left - TOUCH_DISTANCE <= rect.right
        && rect.top - TOUCH_DISTANCE <= other.bottom
        && other.top - TOUCH_DISTANCE <= rect.bottom
}

/// Whether a rule reaches further out of a box than a rule does that runs
/// along the edge of the box, which is by its own thickness at most.
fn reaches_out_of(rule: &Rect, text_box: &Rect) -> bool {
    rule.left < text_box.left - MAX_RULE_THICKNESS
        || rule.right > text_box.right + MAX_RULE_THICKNESS
        || rule.top < text_box.top - MAX_RULE_THICKNESS
        || rule.bottom > text_box.bottom + MAX_RULE_THICKNESS
}

#[cfg(test)]
mod tests {
    use super::*;

    const NO_DRAWINGS: [Vec<usize>; 0] = [];

    /// A shape with its left top corner at `(left, top)`.
    fn shape(left: f32, top: f32, width: f32, height: f32) -> Item {
        Item::Shape {
            bounds: Rect {
                left,
                top,
                right: left + width,
                bottom: top + height,
            },
        }
    }

    /// A box of a diagram, 60 wide and 30 high.
    fn node(left: f32, top: f32) -> Item {
        shape(left, top, 60.0, 30.0)
    }

    /// A horizontal rule, 20 long.
    fn rule(left: f32, top: f32) -> Item {
        shape(left, top, 20.0, 0.0)
    }

    fn text(x: f32, y: f32) -> Item {
        text_in_cell(x, y, None)
    }

    fn text_in_cell(x: f32, y: f32, cell: Option<usize>) -> Item {
        Item::Text {
            center: (x, y),
            cell,
        }
    }

    /// Text in the middle of what [`node`] makes at the same place.
    fn label(left: f32, top: f32) -> Item {
        text(left + 30.0, top + 15.0)
    }

    #[test]
    fn boxes_with_labels_and_the_lines_between_them_are_one_drawing() {
        let items = [
            node(0.0, 0.0),
            label(0.0, 0.0),
            rule(60.0, 15.0),
            node(80.0, 0.0),
            label(80.0, 0.0),
            rule(140.0, 15.0),
            node(160.0, 0.0),
            label(160.0, 0.0),
            // A caption below the diagram.
            text(110.0, 50.0),
        ];
        assert_eq!(drawings(&items), [[0, 2, 3, 5, 6]]);
    }

    #[test]
    fn shapes_with_a_gap_between_them_are_one_drawing() {
        let items = [node(0.0, 0.0), node(200.0, 100.0)];
        assert_eq!(drawings(&items), [[0, 1]]);
    }

    #[test]
    fn text_outside_of_the_shapes_ends_a_drawing() {
        let items = [
            node(0.0, 0.0),
            node(80.0, 0.0),
            text(40.0, 60.0),
            node(0.0, 100.0),
            node(80.0, 100.0),
        ];
        assert_eq!(drawings(&items), [[0, 1], [3, 4]]);
    }

    #[test]
    fn label_of_a_box_that_is_drawn_after_it_does_not_end_a_drawing() {
        let items = [
            node(0.0, 0.0),
            label(80.0, 0.0),
            node(80.0, 0.0),
            node(160.0, 0.0),
        ];
        assert_eq!(drawings(&items), [[0, 2, 3]]);
    }

    #[test]
    fn what_is_no_loose_shape_ends_a_drawing() {
        let items = [
            node(0.0, 0.0),
            node(80.0, 0.0),
            Item::Other,
            node(160.0, 0.0),
            node(240.0, 0.0),
        ];
        assert_eq!(drawings(&items), [[0, 1], [3, 4]]);
    }

    #[test]
    fn shape_without_text_is_a_drawing_by_itself() {
        assert_eq!(drawings(&[text(0.0, 0.0), node(0.0, 20.0)]), [[1]]);
    }

    #[test]
    fn rules_are_no_drawing() {
        // The rules of a table, which a page draws before the text of its
        // cells, and the line under a word.
        let table = [rule(0.0, 0.0), rule(0.0, 20.0), shape(0.0, 0.0, 0.0, 20.0)];
        let items = [table.as_slice(), &[text(10.0, 10.0), rule(0.0, 40.0)]].concat();
        assert_eq!(drawings(&items), NO_DRAWINGS);
    }

    #[test]
    fn bar_next_to_text_is_a_rule() {
        let bar = shape(0.0, 0.0, MAX_RULE_THICKNESS, 80.0);
        assert_eq!(drawings(&[bar, text(40.0, 40.0)]), NO_DRAWINGS);
    }

    #[test]
    fn boxes_of_text_are_no_drawing() {
        // Highlighted text that goes on in the next line, and after it two
        // notes in frames.
        let line_box = |top: f32| shape(0.0, top, 100.0, 12.0);
        let note = |top: f32| shape(0.0, top, 300.0, 40.0);
        let items = [
            line_box(0.0),
            text(50.0, 6.0),
            line_box(14.0),
            text(50.0, 20.0),
            note(40.0),
            text(150.0, 60.0),
            note(90.0),
            text(150.0, 110.0),
            text(150.0, 150.0),
        ];
        assert_eq!(drawings(&items), NO_DRAWINGS);
    }

    #[test]
    fn rules_in_a_box_of_text_are_no_drawing() {
        // A note in a frame, with a bar on its left edge and a line under
        // one of its words.
        let items = [
            shape(0.0, 0.0, 200.0, 60.0),
            shape(-1.5, 0.0, 3.0, 60.0),
            text(100.0, 10.0),
            rule(80.0, 36.0),
            text(100.0, 50.0),
        ];
        assert_eq!(drawings(&items), NO_DRAWINGS);
    }

    #[test]
    fn rule_that_only_stands_near_boxes_of_text_joins_nothing() {
        // A line between two notes.
        let note = |top: f32| [shape(0.0, top, 300.0, 40.0), text(150.0, top + 20.0)];
        let items = [note(0.0).as_slice(), &[rule(0.0, 50.0)], &note(60.0)].concat();
        assert_eq!(drawings(&items), NO_DRAWINGS);
    }

    /// Two filled cells of a table and the rule under them, with their
    /// texts in the cells `cells`.
    fn filled_cells(cells: [Option<usize>; 2]) -> Vec<Item> {
        vec![
            node(0.0, 0.0),
            text_in_cell(30.0, 15.0, cells[0]),
            node(60.0, 0.0),
            text_in_cell(90.0, 15.0, cells[1]),
            shape(0.0, 30.0, 120.0, 0.0),
        ]
    }

    #[test]
    fn shapes_around_the_text_of_several_table_cells_are_no_drawing() {
        assert_eq!(drawings(&filled_cells([Some(0), Some(1)])), NO_DRAWINGS);
    }

    #[test]
    fn filled_cells_with_rules_are_a_drawing_where_nothing_says_that_they_are_cells() {
        assert_eq!(drawings(&filled_cells([None, None])), [[0, 2, 4]]);
    }

    #[test]
    fn drawing_in_one_table_cell_is_a_drawing() {
        assert_eq!(drawings(&filled_cells([Some(4), Some(4)])), [[0, 2, 4]]);
    }
}
