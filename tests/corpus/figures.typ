// Two revisions of a page of figures, for tests of how figures are compared.
//
//     typst compile --input revision=new figures.typ figures-new-tagged.pdf
//
// Each edit is marked with `EDIT`, and tests/comparing.rs lists the changes
// that they have to be reported as.
// The drawing that is included as a PDF is built from drawing.typ.

#let revision = sys.inputs.at("revision", default: "old")
#let is-new = revision == "new"
#let edit(old, new) = if is-new { new } else { old }

#set document(title: "Figures", date: none)
// The square in the header and the dot in the background are shapes that
// belong to the page, and are no figures where tags say so.
#set page(
  paper: "a4",
  margin: 2.5cm,
  header: [Figures #h(1fr) #box(rect(width: 8pt, height: 8pt, fill: blue))],
  background: place(top + right, dx: -1cm, dy: 1cm, circle(radius: 4pt, fill: gray)),
)
#set text(size: 10pt)
#show figure: set block(spacing: 1.2em)

// An image of a gradient, with a square in it if `has-square`.
#let photo(has-square) = image(
  bytes(
    range(24 * 16)
      .map(index => {
        let (column, row) = (calc.rem(index, 24), calc.quo(index, 24))
        let is-in-square = has-square and column >= 14 and column < 20 and row >= 4 and row < 10
        if is-in-square { (230, 60, 40) } else { (40 + column * 8, 90 + row * 8, 170) }
      })
      .flatten(),
  ),
  format: (encoding: "rgb8", width: 24, height: 16),
  width: 2.4cm,
  alt: "A picture of the sensor",
)

// A bar chart as an image in the SVG format, which is drawn as shapes.
// Its last bar is `last-bar-height` high.
#let chart(last-bar-height) = image(
  bytes(
    "<svg xmlns='http://www.w3.org/2000/svg' width='160' height='60' viewBox='0 0 160 60'>"
      + "<rect x='0.5' y='0.5' width='159' height='59' fill='white' stroke='#888888'/>"
      + "<rect x='24' y='24' width='24' height='28' fill='#4477aa'/>"
      + "<rect x='68' y='12' width='24' height='40' fill='#4477aa'/>"
      + "<rect x='112' y='" + str(52 - last-bar-height) + "' width='24' height='" + str(last-bar-height) + "' fill='#4477aa'/>"
      + "<path d='M 12 52 L 148 52' stroke='black'/>"
      + "</svg>",
  ),
  format: "svg",
  width: 4cm,
  alt: "A bar chart",
)

// A diagram of boxes and the lines between them, which the page draws shape
// by shape.
// The last box has the fill `valve-fill`.
#let diagram(valve-fill, has-feedback) = {
  let part(dx, label, fill: white) = place(
    dx: dx,
    dy: 0.2cm,
    rect(width: 1.9cm, height: 0.8cm, fill: fill, stroke: 0.6pt, align(center + horizon, label)),
  )
  let connector(dx) = place(dx: dx, dy: 0.6cm, line(length: 0.6cm, stroke: 0.6pt))
  box(width: 6.9cm, height: 1.5cm, {
    part(0cm)[Sensor]
    connector(1.9cm)
    part(2.5cm)[Controller]
    connector(4.4cm)
    part(5cm, fill: valve-fill)[Valve]
    if has-feedback {
      place(dx: 0.95cm, dy: 1cm, line(length: 0.35cm, angle: 90deg, stroke: 0.6pt))
      place(dx: 0.95cm, dy: 1.35cm, line(length: 5cm, stroke: 0.6pt))
      place(dx: 5.95cm, dy: 1cm, line(length: 0.35cm, angle: 90deg, stroke: 0.6pt))
    }
  })
}

= Figures

// EDIT: a paragraph is inserted, which moves every figure down the page.
#if is-new [
  The figures of this revision stand lower on the page than those of the
  first one.
]

// EDIT: the pixels of an image change.
#figure(photo(is-new), caption: [The sensor])

#figure(photo(false), caption: [An image that stays as it is])

// EDIT: a bar of a chart grows.
// Tags keep the shapes of the chart together, and without tags they are the
// shapes that the page draws one after another.
#figure(chart(edit(16, 36)), caption: [Flow by stage])

#figure(chart(24), caption: [A chart that stays as it is])

// EDIT: a drawing that is included as a PDF changes.
#figure(
  image("drawing-" + revision + ".pdf", width: 5.2cm),
  caption: [The pump and the tank],
)

// EDIT: a box of a diagram is filled, and lines are added to the diagram.
#figure(diagram(edit(white, rgb("#ffd966")), is-new), caption: [The control loop])

#figure(diagram(white, false), caption: [A diagram that stays as it is])

// Shapes that only rule and box text are no drawing: the frame of a note,
// the box of highlighted text and the line under underlined text.
// EDIT: a note becomes longer, and its frame higher with it.
#rect(width: 100%, stroke: 0.5pt)[
  A note in a frame #edit[stays short][needs a second line here, since this revision of the document says so much more in it than the one before did].
]

// EDIT: a highlighted word and an underlined word change.
Text with a #highlight[highlighted #edit[word][phrase]] and an #underline[underlined #edit[part][passage]] that both change in the second revision.

// EDIT: an image is added.
#if is-new {
  figure(photo(true), caption: [The sensor of the second revision])
}
