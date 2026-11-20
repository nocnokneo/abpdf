// PROTOTYPE example: graphics that change between two revisions.
#let is-new = sys.inputs.at("revision", default: "old") == "new"
#let edit(old, new) = if is-new { new } else { old }
#set document(title: "Figures", date: none)
#set page(paper: "a5", margin: (x: 1.6cm, top: 1.8cm, bottom: 1.4cm),
  header: [Controller manual #h(1fr) Figures #line(length: 100%, stroke: 0.4pt)])
#set text(size: 9pt)
#set par(spacing: 0.8em)
#show figure: set block(spacing: 1em)

#let photo(marked) = image(
  bytes(range(48 * 32).map(index => {
    let column = calc.rem(index, 48)
    let row = calc.quo(index, 48)
    let in-mark = marked and column > 28 and column < 40 and row > 8 and row < 20
    if in-mark { (230, 60, 40) } else { (40 + column * 4, 90 + row * 4, 170) }
  }).flatten()),
  format: (encoding: "rgb8", width: 48, height: 32),
  width: 3cm,
  alt: "A picture of the sensor",
)

#let chart(third-bar-height) = image(
  bytes(
    "<svg xmlns='http://www.w3.org/2000/svg' width='160' height='80' viewBox='0 0 160 80'>"
      + "<rect x='0.5' y='0.5' width='159' height='79' fill='white' stroke='#888'/>"
      + "<rect x='20' y='40' width='24' height='32' fill='#4477aa'/>"
      + "<rect x='60' y='24' width='24' height='48' fill='#4477aa'/>"
      + "<rect x='100' y='" + str(72 - third-bar-height) + "' width='24' height='" + str(third-bar-height) + "' fill='#4477aa'/>"
      + "<path d='M 12 72 L 148 72' stroke='black' stroke-width='1'/>"
      + "</svg>",
  ),
  format: "svg",
  width: 4.2cm,
  alt: "A bar chart",
)

#let node(dx, label, fill: white) = place(dx: dx, dy: 0.25cm, rect(width: 1.9cm, height: 0.8cm, fill: fill, stroke: 0.6pt, align(center + horizon, label)))
#let arrow(dx) = place(dx: dx, dy: 0.65cm, line(length: 0.6cm, stroke: 0.6pt))
#let diagram(valve-fill, valve-label, with-feedback) = box(width: 7.4cm, height: 1.6cm, {
  node(0cm)[Sensor]
  arrow(1.9cm)
  node(2.5cm)[Controller]
  arrow(4.4cm)
  node(5cm, fill: valve-fill, valve-label)
  if with-feedback {
    place(dx: 0.95cm, dy: 1.05cm, line(length: 0.35cm, angle: 90deg, stroke: 0.6pt))
    place(dx: 0.95cm, dy: 1.4cm, line(length: 5cm, stroke: 0.6pt))
    place(dx: 5.95cm, dy: 1.05cm, line(length: 0.35cm, angle: 90deg, stroke: 0.6pt))
  }
})

= Figures

An image whose pixels change:

#figure(photo(is-new), caption: [The sensor])

A vector graphic that is included as a file, in which one bar grows:

#figure(chart(edit(20, 44)), caption: [Flow by stage])

A diagram drawn with the shapes of the typesetting system, in which a box is filled and a feedback line is added, and no label changes:

#figure(diagram(edit(white, rgb("#ffd966")), [Valve], is-new), caption: [The control loop])

The same kind of diagram, in which only a label changes:

#figure(diagram(white, edit([Valve], [Pump]), false), caption: [The actuator])

Graphics that stay as they are:

#grid(columns: 2, gutter: 1cm, photo(false), chart(30))

Text with a #highlight[highlighted #edit[word][phrase of words]] and an #underline[underlined #edit[part][passage]] that change.
