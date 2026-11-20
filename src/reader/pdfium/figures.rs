//! PROTOTYPE, not for merging: the graphics of a page.
//!
//! A graphic is one of:
//!
//! - an image,
//! - what the producer keeps together: a form, or the shapes of one piece of
//!   marked content that is not an artifact,
//! - with [`Figures::All`], shapes drawn one by one that touch each other,
//!   unless all of them are rules.

use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::ffi::{c_int, c_uint, c_ulong, c_void};
use std::hash::{Hash, Hasher};

use pdfium_render::prelude::FPDF_PAGEOBJECT;

use super::handles::{LoadedPage, TextObject};
use super::library::Library;
use super::page_space::{PageSpace, UserSpaceBox};
use crate::prototype::Figures;
use crate::reader::Figure;

const TEXT: c_int = 1;
const PATH: c_int = 2;
const IMAGE: c_int = 3;
const FORM: c_int = 5;

/// How far apart two shapes can be to count as touching, in points.
const TOUCH_DISTANCE: f32 = 1.0;

/// A straight line that is no thicker than this, in points, is a rule.
const MAX_RULE_THICKNESS: f32 = 3.0;

pub(super) struct PageFigures {
    /// Each graphic with the place of its first object among the objects of
    /// the page, in ascending order.
    pub figures: Vec<(usize, Figure)>,
    /// The place of each object of the page among them, by its identity.
    pub object_orders: HashMap<usize, usize>,
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Image,
    Vector,
    Drawing,
}

struct Group {
    kind: Kind,
    marked_content_id: Option<c_int>,
    is_artifact: bool,
    members: Vec<usize>,
}

pub(super) fn read(page: &LoadedPage, page_space: &PageSpace, mode: Figures) -> PageFigures {
    let objects = page.objects();
    let object_orders = objects
        .iter()
        .enumerate()
        .map(|(order, object)| (object.identity(), order))
        .collect();
    let mut groups: Vec<Group> = Vec::new();
    let mut loose: Vec<usize> = Vec::new();
    for (order, object) in objects.iter().enumerate() {
        if mode == Figures::None {
            break;
        }
        let is_artifact = object
            .marks()
            .any(|mark| mark.name().as_deref() == Some("Artifact"));
        let single = |kind| Group {
            kind,
            marked_content_id: None,
            is_artifact,
            members: vec![order],
        };
        match (kind_of(object), object.marked_content_id()) {
            (TEXT, _) => {}
            (IMAGE, _) => groups.push(single(Kind::Image)),
            (FORM, _) => groups.push(single(Kind::Vector)),
            (_, Some(id)) if !is_artifact => match groups.last_mut() {
                Some(group) if group.marked_content_id == Some(id) => group.members.push(order),
                _ => groups.push(Group {
                    marked_content_id: Some(id),
                    ..single(Kind::Vector)
                }),
            },
            _ => loose.push(order),
        }
    }
    if mode == Figures::All {
        groups.extend(drawings(&objects, &loose));
    }
    let mut figures: Vec<(usize, Figure)> = groups
        .iter()
        .map(|group| (group.members[0], figure_of(group, &objects, page_space)))
        .collect();
    figures.sort_by_key(|(order, _)| *order);
    PageFigures {
        figures,
        object_orders,
    }
}

fn kind_of(object: &TextObject) -> c_int {
    unsafe { object.library().FPDFPageObj_GetType(object.handle()) }
}

fn bounds_of(pdfium: &Library, handle: FPDF_PAGEOBJECT) -> [f32; 4] {
    let (mut left, mut bottom, mut right, mut top) = (0.0, 0.0, 0.0, 0.0);
    unsafe { pdfium.FPDFPageObj_GetBounds(handle, &mut left, &mut bottom, &mut right, &mut top) };
    [left, bottom, right, top]
}

fn union(bounds: [f32; 4], other: [f32; 4]) -> [f32; 4] {
    [
        bounds[0].min(other[0]),
        bounds[1].min(other[1]),
        bounds[2].max(other[2]),
        bounds[3].max(other[3]),
    ]
}

fn touch(bounds: [f32; 4], other: [f32; 4]) -> bool {
    bounds[0] - TOUCH_DISTANCE <= other[2]
        && other[0] - TOUCH_DISTANCE <= bounds[2]
        && bounds[1] - TOUCH_DISTANCE <= other[3]
        && other[1] - TOUCH_DISTANCE <= bounds[3]
}

/// The shapes among `loose` that touch each other, as one drawing each.
fn drawings(objects: &[TextObject], loose: &[usize]) -> Vec<Group> {
    let bounds: Vec<[f32; 4]> = loose
        .iter()
        .map(|&order| bounds_of(objects[order].library(), objects[order].handle()))
        .collect();
    // The cluster of each shape, as the index of one of its shapes.
    let mut cluster: Vec<usize> = (0..loose.len()).collect();
    loop {
        let mut merged = false;
        for first in 0..loose.len() {
            for second in first + 1..loose.len() {
                if cluster[first] != cluster[second] && touch(bounds[first], bounds[second]) {
                    let (from, to) = (cluster[second], cluster[first]);
                    for entry in &mut cluster {
                        if *entry == from {
                            *entry = to;
                        }
                    }
                    merged = true;
                }
            }
        }
        if !merged {
            break;
        }
    }
    let mut groups: Vec<Group> = Vec::new();
    let mut group_of_cluster: HashMap<usize, usize> = HashMap::new();
    for (index, &order) in loose.iter().enumerate() {
        let group = *group_of_cluster.entry(cluster[index]).or_insert_with(|| {
            groups.push(Group {
                kind: Kind::Drawing,
                marked_content_id: None,
                is_artifact: false,
                members: Vec::new(),
            });
            groups.len() - 1
        });
        groups[group].members.push(order);
    }
    groups.retain(|group| !group.members.iter().all(|&order| is_rule(&objects[order])));
    groups
}

/// Whether a shape is a thin straight line, as the rules of a table and the
/// line under underlined text are.
fn is_rule(object: &TextObject) -> bool {
    let pdfium = object.library();
    let [left, bottom, right, top] = bounds_of(pdfium, object.handle());
    let segment_count = unsafe { pdfium.FPDFPath_CountSegments(object.handle()) };
    kind_of(object) == PATH
        && segment_count == 2
        && (right - left).min(top - bottom) <= MAX_RULE_THICKNESS
}

fn figure_of(group: &Group, objects: &[TextObject], page_space: &PageSpace) -> Figure {
    let handles: Vec<FPDF_PAGEOBJECT> = group
        .members
        .iter()
        .map(|&order| objects[order].handle())
        .collect();
    let pdfium = objects[group.members[0]].library();
    let bounds = handles
        .iter()
        .map(|&handle| bounds_of(pdfium, handle))
        .reduce(union)
        .expect("a group has a member");
    let origin = (bounds[0], bounds[3]);
    let mut hasher = DefaultHasher::new();
    for &handle in &handles {
        hash_object(pdfium, handle, origin, &mut hasher);
    }
    let description = match group.kind {
        Kind::Image => {
            let (mut width, mut height): (c_uint, c_uint) = (0, 0);
            unsafe { pdfium.FPDFImageObj_GetImagePixelSize(handles[0], &mut width, &mut height) };
            format!("image, {width} × {height} pixels")
        }
        Kind::Vector => format!("vector graphic, {}", shapes(shape_count(pdfium, &handles))),
        Kind::Drawing => format!("drawing, {}", shapes(handles.len())),
    };
    Figure {
        bounds: page_space.rect(UserSpaceBox {
            left: bounds[0].into(),
            bottom: bounds[1].into(),
            right: bounds[2].into(),
            top: bounds[3].into(),
        }),
        description,
        fingerprint: hasher.finish(),
        is_artifact: group.is_artifact,
    }
}

fn shapes(count: usize) -> String {
    match count {
        1 => "1 shape".to_owned(),
        count => format!("{count} shapes"),
    }
}

fn shape_count(pdfium: &Library, handles: &[FPDF_PAGEOBJECT]) -> usize {
    let count_of = |&handle: &FPDF_PAGEOBJECT| unsafe {
        match pdfium.FPDFPageObj_GetType(handle) {
            FORM => pdfium.FPDFFormObj_CountObjects(handle).max(0) as usize,
            _ => 1,
        }
    };
    handles.iter().map(count_of).sum()
}

/// Feeds the hasher what an object looks like, measured from `origin` and
/// rounded to tenths of a point so that where the graphic is on its page
/// does not matter.
fn hash_object(
    pdfium: &Library,
    handle: FPDF_PAGEOBJECT,
    origin: (f32, f32),
    hasher: &mut DefaultHasher,
) {
    let kind = unsafe { pdfium.FPDFPageObj_GetType(handle) };
    kind.hash(hasher);
    let [left, bottom, right, top] = bounds_of(pdfium, handle);
    let tenths = |length: f32| (length * 10.0).round() as i32;
    for length in [left - origin.0, origin.1 - top, right - left, top - bottom] {
        tenths(length).hash(hasher);
    }
    match kind {
        IMAGE => unsafe {
            let (mut width, mut height): (c_uint, c_uint) = (0, 0);
            pdfium.FPDFImageObj_GetImagePixelSize(handle, &mut width, &mut height);
            (width, height).hash(hasher);
            let length = pdfium.FPDFImageObj_GetImageDataDecoded(handle, std::ptr::null_mut(), 0);
            let mut pixels = vec![0u8; length as usize];
            pdfium.FPDFImageObj_GetImageDataDecoded(
                handle,
                pixels.as_mut_ptr() as *mut c_void,
                length,
            );
            pixels.hash(hasher);
        },
        FORM => unsafe {
            let form_origin = (left, top);
            for index in 0..pdfium.FPDFFormObj_CountObjects(handle).max(0) {
                let child = pdfium.FPDFFormObj_GetObject(handle, index as c_ulong);
                if !child.is_null() {
                    hash_object(pdfium, child, form_origin, hasher);
                }
            }
        },
        PATH => unsafe {
            let (mut fill_mode, mut is_stroked) = (0, 0);
            pdfium.FPDFPath_GetDrawMode(handle, &mut fill_mode, &mut is_stroked);
            (fill_mode, is_stroked).hash(hasher);
            pdfium.FPDFPath_CountSegments(handle).hash(hasher);
            let (mut red, mut green, mut blue, mut alpha): (c_uint, c_uint, c_uint, c_uint) =
                (0, 0, 0, 0);
            if fill_mode != 0 {
                pdfium.FPDFPageObj_GetFillColor(
                    handle, &mut red, &mut green, &mut blue, &mut alpha,
                );
                (red, green, blue, alpha).hash(hasher);
            }
            if is_stroked != 0 {
                pdfium.FPDFPageObj_GetStrokeColor(
                    handle, &mut red, &mut green, &mut blue, &mut alpha,
                );
                (red, green, blue, alpha).hash(hasher);
                let mut width = 0.0;
                pdfium.FPDFPageObj_GetStrokeWidth(handle, &mut width);
                tenths(width).hash(hasher);
            }
        },
        _ => {}
    }
}
