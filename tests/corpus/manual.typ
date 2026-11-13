// A synthetic manual of a few hundred pages, in two revisions.
//
//     typst compile --input revision=new manual.typ manual-new.pdf
//
// The text is pseudo-random but fixed, so both revisions are the same except
// for the edits, each of which is marked with `EDIT` below.
// `--input chapters=N` makes a shorter document.

#let is-new = sys.inputs.at("revision", default: "old") == "new"
#let chapter-count = int(sys.inputs.at("chapters", default: "60"))

/// The value for the revision being built.
#let edit(old, new) = if is-new { new } else { old }

// --- Pseudo-random text ---

#let modulus = 2147483648
#let next(state) = calc.rem(calc.rem(state, modulus) * 1103515245 + 12345, modulus)
/// A number below `count`, from the better bits of `state`.
#let pick(state, count) = calc.rem(calc.quo(state, 65536), count)

#let function-words = (
  "the", "of", "and", "a", "to", "in", "is", "that", "for", "with", "as", "on",
  "by", "at", "from", "it", "be", "this", "or", "not",
)
#let onsets = (
  "b", "c", "d", "f", "g", "l", "m", "n", "p", "r", "s", "t", "v", "st", "tr",
  "pl", "ch", "br", "gr", "sp",
)
#let nuclei = ("a", "e", "i", "o", "u", "ai", "ou", "ea")
#let codas = ("", "", "", "n", "r", "s", "l", "m", "nt", "st")
#let vocabulary-size = 6000

/// The word at `index` of the vocabulary.
#let content-word(index) = {
  let state = next(index + 977)
  let word = ""
  for _ in range(1 + pick(state, 3)) {
    state = next(state)
    word += onsets.at(pick(state, onsets.len()))
    state = next(state)
    word += nuclei.at(pick(state, nuclei.len()))
    state = next(state)
    word += codas.at(pick(state, codas.len()))
  }
  word
}

/// A word, with common words common and rare words rare.
#let word(state) = {
  if pick(state, 100) < 40 {
    function-words.at(pick(next(state), function-words.len()))
  } else {
    let uniform = pick(next(state), 32768)
    content-word(calc.quo(uniform * uniform * vocabulary-size, 32768 * 32768))
  }
}

#let sentence(seed) = {
  let state = next(seed * 7919 + 13)
  let words = ()
  for position in range(8 + pick(state, 14)) {
    state = next(state)
    let separator = if position > 2 and pick(next(state + 5), 12) == 0 { "," } else { "" }
    words.push(word(state) + separator)
  }
  let text = words.join(" ").trim(",")
  upper(text.first()) + text.slice(1) + "."
}

#let paragraph(seed, sentences: 5) = {
  range(sentences).map(index => sentence(seed * 64 + index)).join(" ")
}

// --- Layout ---

#set document(title: "Synthetic manual", date: none)
#set page(
  paper: "a4",
  margin: (x: 2.5cm, y: 2.8cm),
  header: [Synthetic manual #h(1fr) Revision #edit("A", "B")],
  footer: context [
    #h(1fr) Page #counter(page).display() of #counter(page).final().first()
  ],
)
#set text(size: 10pt, hyphenate: true)
#set par(justify: true)
#set heading(numbering: "1.1")
#show heading.where(level: 1): it => {
  pagebreak(weak: true)
  it
}
#show figure: set block(breakable: true)

/// A page reference, marked as derived from pagination rather than content.
#let page-of(target) = pdf.artifact[(p. #context counter(page).at(target).first())]

// --- Content ---

#let data-table(seed, rows, changed-row: none, added-row: false) = figure(
  table(
    columns: (auto, 1fr, auto),
    table.header([Parameter], [Description], [Value]),
    ..range(rows)
      .map(row => {
        let state = next(seed * 331 + row)
        let value = 10 + pick(state, 990)
        // EDIT: one value of the table changes.
        if row == changed-row { value = edit(value, value + 1) }
        (
          [#upper(content-word(pick(state, 500)))\-#(row + 1)],
          [#sentence(seed * 53 + row)],
          [#value],
        )
      })
      .flatten(),
    // EDIT: a row is added at the end of the table.
    ..if added-row and is-new {
      ([NEW-ROW], [A parameter that the new revision adds.], [42])
    },
  ),
  caption: [Parameters of #content-word(seed)],
)

#let diagram(seed, changed: false) = figure(
  block(width: 10cm, height: 4.2cm, stroke: 0.5pt + gray, {
    for box in range(4) {
      let state = next(seed * 17 + box)
      // EDIT: one box of the diagram moves and gets another label.
      let is-changed = changed and is-new and box == 2
      place(
        dx: 0.4cm + box * 2.4cm,
        dy: 0.5cm + pick(state, 20) * 1mm + if is-changed { 8mm } else { 0mm },
        rect(width: 2cm, height: 1cm, fill: aqua.lighten(50%), stroke: 0.5pt)[
          #set align(center + horizon)
          #set text(size: 8pt)
          #if is-changed [changed] else [#content-word(pick(state, 300))]
        ],
      )
    }
    place(dx: 0.4cm, dy: 3.4cm, line(length: 9.2cm, stroke: 0.8pt))
    place(dx: 4.6cm, dy: 2.6cm, circle(radius: 0.3cm, fill: yellow))
  }),
  caption: [Structure of #content-word(seed + 40)],
)

/// A picture of 48 by 48 pixels.
#let picture(seed, changed: false) = figure(
  image(
    bytes(
      range(48 * 48)
        .map(pixel => {
          let (x, y) = (calc.rem(pixel, 48), calc.quo(pixel, 48))
          // EDIT: a square inside the picture changes color.
          let is-changed = changed and is-new and x > 16 and x < 32 and y > 16 and y < 32
          let level = calc.rem(x * 5 + y * 3 + seed, 256)
          if is-changed { (255, 64, 64) } else { (level, 255 - level, 128) }
        })
        .flatten(),
    ),
    format: (encoding: "rgb8", width: 48, height: 48),
    width: 3cm,
  ),
  caption: [Picture of #content-word(seed + 80)],
)

#outline()

#for chapter in range(1, chapter-count + 1) [
  // EDIT: the title of chapter 15 changes.
  #let title = if chapter == 15 {
    edit("Calibration of the instrument", "Calibration and verification")
  } else {
    (content-word(chapter * 3), content-word(chapter * 3 + 1)).join(" ")
  }
  #heading(level: 1)[#upper(title.first())#title.slice(1)] #label("chapter-" + str(chapter))

  #paragraph(chapter * 1000)
  #if chapter > 1 [
    See also #ref(label("chapter-" + str(chapter - 1)))
    #page-of(label("chapter-" + str(chapter - 1))).
  ]

  // EDIT: chapter 2 gains more than a page of text, which moves everything
  // after it to a later page.
  #if chapter == 2 and is-new {
    for index in range(14) {
      par(paragraph(990000 + index))
    }
  }

  #for section in range(1, 6) [
    // EDIT: chapter 17 gains a section, which renumbers the sections after it.
    #if chapter == 17 and section == 3 and is-new [
      == Inserted section

      #paragraph(980000)
    ]

    == #content-word(chapter * 50 + section) #content-word(chapter * 50 + section + 7)

    #for index in range(6) {
      let seed = chapter * 10000 + section * 100 + index
      let position = (chapter, section, index)
      if position == (5, 2, 3) {
        // EDIT: a paragraph of chapter 5 is removed.
        if not is-new { par(paragraph(seed)) }
      } else if position == (7, 1, 2) {
        // EDIT: words of a sentence in chapter 7 are replaced.
        par[
          #paragraph(seed, sentences: 2)
          The device #edit("shall be calibrated annually", "must be verified every month")
          by trained staff.
          #paragraph(seed + 1, sentences: 2)
        ]
      } else if position == (13, 2, 1) or position == (13, 2, 4) {
        // EDIT: two paragraphs of chapter 13 trade places.
        let other = if index == 1 { 4 } else { 1 }
        par(paragraph(chapter * 10000 + section * 100 + edit(index, other)))
      } else if position == (25, 1, 0) {
        // EDIT: one word of chapter 25 becomes bold and another red, with no
        // change to the text.
        par[
          The warning is #edit([important], [*important*]) and the limit is
          #edit([critical], text(fill: red)[critical]) here.
          #paragraph(seed, sentences: 4)
        ]
      } else if position == (27, 3, 2) {
        // EDIT: a comma is added and a full stop becomes a colon in chapter 27.
        par[
          Press Export#edit("", ",") then wait#edit(".", ":")
          #paragraph(seed, sentences: 4)
        ]
      } else if position == (29, 4, 5) {
        // EDIT: a long word at the start of a paragraph in chapter 29 is
        // replaced by a short one, which changes where its lines break.
        par[
          #edit("Notwithstanding", "Despite") the above,
          #paragraph(seed, sentences: 6)
        ]
      } else {
        par(paragraph(seed))
      }
    }

    #if section == 2 {
      data-table(
        chapter,
        8 + calc.rem(chapter * 7, 40),
        changed-row: if chapter == 9 { 3 },
        added-row: chapter == 11,
      )
    }
    #if section == 4 and calc.rem(chapter, 2) == 1 {
      diagram(chapter, changed: chapter == 23)
    }
    #if section == 4 and calc.rem(chapter, 4) == 0 {
      picture(chapter, changed: chapter == 20)
    }
    // EDIT: chapter 19 gains a figure, which renumbers the figures after it.
    #if section == 5 and chapter == 19 and is-new {
      diagram(970000)
    }
  ]
]
