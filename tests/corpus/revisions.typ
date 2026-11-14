// Two revisions of a short document, for tests of the whole comparison.
//
//     typst compile --input revision=new revisions.typ revisions-new.pdf
//
// Each edit is marked with `EDIT`, and tests/comparing.rs lists the changes
// that they have to be reported as.
// Everything else that differs between the revisions follows from the
// edits: page numbers, section numbers and figure numbers.

#let is-new = sys.inputs.at("revision", default: "old") == "new"
#let edit(old, new) = if is-new { new } else { old }

#set document(title: "Revisions", date: none)
#set page(
  paper: "a5",
  margin: (x: 2cm, y: 2.2cm),
  header: [Operating manual #h(1fr) Revision #edit("A", "B")],
  footer: context [
    #h(1fr) Page #counter(page).display() of #counter(page).final().first()
  ],
)
#set text(size: 10pt, hyphenate: true)
#set par(justify: true)
#set heading(numbering: "1.1")
#show figure: set block(breakable: true)

#let page-of(target) = pdf.artifact[(p. #context counter(page).at(target).first())]

#let storage(room) = [
  Store the instrument in its case when it is not in use, in a #room room that
  is free of dust.
]

#outline()

= Introduction

This manual describes the operation of the instrument.
Read it completely before the first measurement, and keep it within reach of
the operator at all times.

// EDIT: a paragraph is inserted, which moves the text after it.
#if is-new [
  The second revision of the instrument adds a monitoring interface.
  It reports the internal temperature and the remaining lifetime of the light
  source, and it is described in the section on maintenance.
]

#lorem(90)

== Scope

The instrument is intended for laboratory use.
// EDIT: words are replaced.
It #edit("shall be calibrated annually", "must be verified every month") by
trained staff.
See @maintenance #page-of(<maintenance>) for the procedure.

// EDIT: a paragraph moves to the section on maintenance, where one of its
// words is another.
#edit(storage[dry], [])

#lorem(70)

// EDIT: a section is inserted, which renumbers the sections after it.
#if is-new [
  == Monitoring

  The monitoring interface is read only.
]

== Conventions

// EDIT: a word becomes bold, and its text stays as it is.
#edit([Warnings], [*Warnings*]) are set in bold type.
// EDIT: a sentence is removed.
#edit([Notes are set in italic type.], [])
Dimensions are in millimeters unless stated otherwise.

#lorem(60)

= Operation

#lorem(50)

// EDIT: two paragraphs change places.
#let checks = (
  [Check the supply voltage at the start of every series of measurements.],
  [Clean the window of the sensor with a dry cloth, and never with a solvent of any kind.],
)
#for check in edit(checks, checks.rev()) {
  par(check)
}

// EDIT: a figure is inserted, which renumbers the figure after it.
#if is-new [
  #figure(
    rect(width: 5cm, height: 1.5cm, stroke: 0.5pt)[
      #set align(center + horizon)
      Monitoring interface
    ],
    caption: [The monitoring interface],
  )
]

#figure(
  table(
    columns: (auto, 1fr, auto),
    table.header([Parameter], [Meaning], [Default]),
    ..range(18)
      .map(row => (
        [P#(row + 1)],
        [Setting number #(row + 1) of the controller],
        // EDIT: one value of the table changes.
        [#if row == 4 { edit(250, 300) } else { 100 + row * 10 }],
      ))
      .flatten(),
    // EDIT: a row is added to the table.
    ..if is-new { ([P19], [Interval of the monitoring interface], [60]) },
  ),
  caption: [Parameters of the controller],
)

#figure(
  rect(width: 5cm, height: 1.5cm, stroke: 0.5pt)[
    #set align(center + horizon)
    Front panel
  ],
  caption: [The front panel],
) <panel>

The front panel is shown in @panel.
// EDIT: a comma is removed and a full stop becomes an exclamation mark.
Press Start#edit(",", "") then wait#edit(".", "!")

= Maintenance <maintenance>

// EDIT: a long word is replaced by a short one, which changes where the
// lines of the paragraph break.
#edit("Notwithstanding", "Despite") the robustness of the instrument, the
characteristically uncomplicated recalibration procedure necessitates
extraordinarily conscientious documentation of every intermediate result.

#lorem(40)

#edit([], storage[cool])
