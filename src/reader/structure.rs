//! The structure tree of a tagged document.

use std::collections::{HashMap, HashSet};

use hayro_syntax::object::{Dict, Name, Object, ObjectIdentifier};

/// A structure tree that nests deeper than this is cut off, so that a
/// malformed tree cannot exhaust the stack.
const MAX_DEPTH: usize = 64;

/// What the structure tree says about a piece of marked content.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Structure {
    /// The types of the elements that contain the content, outermost first
    /// and separated by `/`.
    pub path: String,
    /// The table cell that contains the content: its number among the cells
    /// of the page, which are counted in the order of the tree.
    pub cell: Option<usize>,
}

impl Structure {
    /// Whether the element that the content is directly in is a figure.
    pub fn is_figure(&self) -> bool {
        self.path.rsplit('/').next() == Some("Figure")
    }
}

/// The structure of each piece of marked content of a page, by its marked
/// content identifier.
pub(super) type PageStructure = HashMap<i32, Structure>;

/// What the structure tree of a document says about the marked content of
/// its pages.
#[derive(Default)]
pub(super) struct StructureTree {
    by_page: HashMap<ObjectIdentifier, PageStructure>,
    of_no_page: PageStructure,
}

impl StructureTree {
    /// Reads the tree that the catalog of a document has, if it has one.
    pub fn read(catalog: &Dict) -> Self {
        let Some(root) = catalog.get::<Dict>("StructTreeRoot") else {
            return Self::default();
        };
        let mut reader = TreeReader {
            role_map: root.get::<Dict>("RoleMap"),
            tree: Self::default(),
            read_elements: HashSet::new(),
            cell_count: 0,
            cell_numbers_by_page: HashMap::new(),
        };
        let top = Parent {
            path: "",
            cell: None,
            page: None,
        };
        reader.read_kids(&root, &top, 0);
        reader.tree
    }

    /// The structure of the marked content of the page with the given
    /// dictionary.
    pub fn of_page(&self, page: &Dict) -> &PageStructure {
        let structure = page.obj_id().and_then(|page| self.by_page.get(&page));
        structure.unwrap_or(&self.of_no_page)
    }
}

/// What an element passes on to what is in it.
struct Parent<'p> {
    path: &'p str,
    /// The table cell that the element is or is in, as its number among
    /// the cells of the document.
    cell: Option<usize>,
    /// The page that the content of the element is on unless it says
    /// otherwise.
    page: Option<ObjectIdentifier>,
}

struct TreeReader<'a> {
    /// The standard types that the types of the document stand for.
    role_map: Option<Dict<'a>>,
    tree: StructureTree,
    /// Where the dictionaries of the elements are that have been read, so
    /// that a tree that loops is read once.
    read_elements: HashSet<usize>,
    /// How many table cells the document has so far.
    cell_count: usize,
    /// For each page, the numbers that the cells of the document have
    /// among the cells of the page.
    cell_numbers_by_page: HashMap<ObjectIdentifier, HashMap<usize, usize>>,
}

impl TreeReader<'_> {
    fn read_kids(&mut self, element: &Dict, context: &Parent, depth: usize) {
        match element.get::<Object>("K") {
            Some(Object::Array(kids)) => {
                for kid in kids.iter::<Object>() {
                    self.read_kid(kid, context, depth);
                }
            }
            Some(kid) => self.read_kid(kid, context, depth),
            None => {}
        }
    }

    /// Reads what an element has in it: a piece of marked content, as its
    /// identifier or as a reference to it, or another element.
    fn read_kid(&mut self, kid: Object, context: &Parent, depth: usize) {
        match kid {
            Object::Number(id) => {
                if let Ok(id) = i32::try_from(id.as_i64()) {
                    self.note_content(id, context.page, context);
                }
            }
            Object::Dict(kid) => match kid.get::<Name>("Type").as_deref() {
                Some(b"MCR") => {
                    // Content in a stream of its own is numbered apart
                    // from that of the page.
                    let is_in_page = !kid.contains_key("Stm");
                    let page = kid.get_ref("Pg").map(ObjectIdentifier::from);
                    if let Some(id) = kid.get::<i32>("MCID").filter(|_| is_in_page) {
                        self.note_content(id, page.or(context.page), context);
                    }
                }
                // An annotation or another object that is no content.
                Some(b"OBJR") => {}
                _ => self.read_element(&kid, context, depth + 1),
            },
            _ => {}
        }
    }

    fn read_element(&mut self, element: &Dict, parent: &Parent, depth: usize) {
        let is_new = self.read_elements.insert(element.data().as_ptr() as usize);
        if depth > MAX_DEPTH || !is_new {
            return;
        }
        let type_name = self.type_of(element);
        let cell = match type_name.as_str() {
            "TD" | "TH" => {
                self.cell_count += 1;
                Some(self.cell_count - 1)
            }
            _ => parent.cell,
        };
        let path = match parent.path {
            "" => type_name,
            parent_path => format!("{parent_path}/{type_name}"),
        };
        let page = element.get_ref("Pg").map(ObjectIdentifier::from);
        let context = Parent {
            path: &path,
            cell,
            page: page.or(parent.page),
        };
        self.read_kids(element, &context, depth);
    }

    /// The type of an element, as the standard type that the document
    /// says it stands for where it says so.
    fn type_of(&self, element: &Dict) -> String {
        let Some(own) = element.get::<Name>("S") else {
            return String::new();
        };
        let role_map = self.role_map.as_ref();
        let standard = role_map.and_then(|role_map| role_map.get::<Name>(&*own));
        standard.unwrap_or(own).as_str().to_owned()
    }

    /// Records the structure of the marked content with an identifier on a
    /// page, unless the page is not known or already has such content.
    fn note_content(&mut self, id: i32, page: Option<ObjectIdentifier>, context: &Parent) {
        let Some(page) = page else {
            return;
        };
        let cell = context.cell.map(|cell| {
            let numbers = self.cell_numbers_by_page.entry(page).or_default();
            let next_number = numbers.len();
            *numbers.entry(cell).or_insert(next_number)
        });
        let structure = self.tree.by_page.entry(page).or_default();
        structure.entry(id).or_insert_with(|| Structure {
            path: context.path.to_owned(),
            cell,
        });
    }
}

#[cfg(test)]
mod tests {
    use hayro_syntax::Pdf;

    use super::*;
    use crate::reader::testing::pdf_of;

    /// A document of two pages, which are the objects 3 and 4, and a
    /// structure tree whose root is the object 5 and has the given kids.
    fn document_with_tree(kids_of_root: &str, more_objects: &[&str]) -> Pdf {
        let mut objects = vec![
            "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 5 0 R >>".to_owned(),
            "<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 /MediaBox [0 0 200 300] >>".to_owned(),
            "<< /Type /Page /Parent 2 0 R >>".to_owned(),
            "<< /Type /Page /Parent 2 0 R >>".to_owned(),
            format!("<< /Type /StructTreeRoot /K {kids_of_root} /RoleMap << /Cell /TD >> >>"),
        ];
        objects.extend(more_objects.iter().map(|object| (*object).to_owned()));
        Pdf::new(pdf_of(&objects)).expect("the PDF opens")
    }

    /// What the tree says of the marked content of each page, as the
    /// identifier, the path and the cell of each piece in the order of the
    /// identifiers.
    fn structure_by_page(pdf: &Pdf) -> Vec<Vec<(i32, String, Option<usize>)>> {
        let cross_references = pdf.xref();
        let catalog: Dict = cross_references
            .get(cross_references.root_id())
            .expect("the document has a catalog");
        let tree = StructureTree::read(&catalog);
        let pages = pdf.pages().iter();
        pages
            .map(|page| {
                let mut content: Vec<_> = tree
                    .of_page(page.raw())
                    .iter()
                    .map(|(id, structure)| (*id, structure.path.clone(), structure.cell))
                    .collect();
                content.sort();
                content
            })
            .collect()
    }

    fn content(id: i32, path: &str, cell: Option<usize>) -> (i32, String, Option<usize>) {
        (id, path.to_owned(), cell)
    }

    #[test]
    fn content_is_under_the_types_of_the_elements_around_it() {
        let pdf = document_with_tree(
            "6 0 R",
            &[
                "<< /S /Document /K [7 0 R << /S /P /Pg 4 0 R /K 0 >>] >>",
                "<< /S /Sect /Pg 3 0 R /K [0 << /S /H1 /K [1 2] >>] >>",
            ],
        );
        assert_eq!(
            structure_by_page(&pdf),
            [
                vec![
                    content(0, "Document/Sect", None),
                    // An element without a page of its own is on that of
                    // the element around it.
                    content(1, "Document/Sect/H1", None),
                    content(2, "Document/Sect/H1", None),
                ],
                vec![content(0, "Document/P", None)],
            ]
        );
    }

    #[test]
    fn reference_to_content_can_name_another_page() {
        let pdf = document_with_tree(
            "<< /S /P /Pg 3 0 R /K [0 << /Type /MCR /MCID 0 /Pg 4 0 R >> \
             << /Type /MCR /MCID 1 >>] >>",
            &[],
        );
        assert_eq!(
            structure_by_page(&pdf),
            [
                vec![content(0, "P", None), content(1, "P", None)],
                vec![content(0, "P", None)],
            ]
        );
    }

    #[test]
    fn content_of_another_stream_and_other_objects_are_no_content_of_the_page() {
        let pdf = document_with_tree(
            "<< /S /P /Pg 3 0 R /K [<< /Type /MCR /MCID 4 /Stm 6 0 R >> \
             << /Type /OBJR /Obj 6 0 R >> 5] >>",
            &["<< >>"],
        );
        assert_eq!(structure_by_page(&pdf)[0], [content(5, "P", None)]);
    }

    #[test]
    fn cells_are_numbered_on_each_page_in_the_order_of_the_tree() {
        // The second cell goes on to the second page, where it is the
        // first one.
        let pdf = document_with_tree(
            "<< /S /Table /Pg 3 0 R /K [\
             << /S /TH /K 0 >> \
             << /S /TD /K [1 << /S /P /K 2 >> << /Type /MCR /MCID 0 /Pg 4 0 R >>] >> \
             << /S /TD /Pg 4 0 R /K 1 >> 3] >>",
            &[],
        );
        assert_eq!(
            structure_by_page(&pdf),
            [
                vec![
                    content(0, "Table/TH", Some(0)),
                    content(1, "Table/TD", Some(1)),
                    content(2, "Table/TD/P", Some(1)),
                    content(3, "Table", None),
                ],
                vec![
                    content(0, "Table/TD", Some(0)),
                    content(1, "Table/TD", Some(1)),
                ],
            ]
        );
    }

    #[test]
    fn type_of_the_document_reads_as_the_standard_type_it_stands_for() {
        let pdf = document_with_tree("<< /S /Cell /Pg 3 0 R /K 0 >>", &[]);
        assert_eq!(structure_by_page(&pdf)[0], [content(0, "TD", Some(0))]);
    }

    #[test]
    fn first_element_that_has_a_piece_of_content_keeps_it() {
        let pdf = document_with_tree(
            "[<< /S /P /Pg 3 0 R /K 0 >> << /S /H1 /Pg 3 0 R /K 0 >>]",
            &[],
        );
        assert_eq!(structure_by_page(&pdf)[0], [content(0, "P", None)]);
    }

    #[test]
    fn tree_that_loops_is_read_once() {
        let pdf = document_with_tree(
            "6 0 R",
            &[
                "<< /S /Document /Pg 3 0 R /K [0 7 0 R] >>",
                "<< /S /P /Pg 3 0 R /K [1 6 0 R] >>",
            ],
        );
        assert_eq!(
            structure_by_page(&pdf)[0],
            [content(0, "Document", None), content(1, "Document/P", None)]
        );
    }

    #[test]
    fn document_without_a_tree_says_nothing() {
        let objects = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 200 300] >>".to_owned(),
            "<< /Type /Page /Parent 2 0 R >>".to_owned(),
        ];
        let pdf = Pdf::new(pdf_of(&objects)).expect("the PDF opens");
        assert_eq!(structure_by_page(&pdf), [vec![]]);
    }

    #[test]
    fn content_directly_in_a_figure_is_figure_content() {
        let structure = |path: &str| Structure {
            path: path.to_owned(),
            cell: None,
        };
        assert!(structure("Document/Figure").is_figure());
        assert!(structure("Figure").is_figure());
        assert!(!structure("Document/Figure/Caption").is_figure());
    }
}
