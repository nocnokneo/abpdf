// A drawing that figures.typ includes as a PDF, in two revisions.
//
//     typst compile --input revision=new drawing.typ drawing-new.pdf

#let is-new = sys.inputs.at("revision", default: "old") == "new"

#set document(title: "Drawing", date: none)
#set page(width: 5.2cm, height: 2cm, margin: 0pt)
#set text(size: 8pt)

#let part(dx, width, label) = place(
  dx: dx,
  dy: 0.4cm,
  rect(width: width, height: 1.2cm, stroke: 0.5pt, align(center + horizon, label)),
)

#part(0.3cm, 1.6cm)[Pump]
#place(dx: 1.9cm, dy: 1cm, line(length: 1.2cm, stroke: 0.5pt))
// EDIT: the tank becomes wider.
#part(3.1cm, if is-new { 1.8cm } else { 1.4cm })[Tank]
