#let is-new = sys.inputs.at("revision", default: "old") == "new"
#let edit(old, new) = if is-new { new } else { old }
#set document(title: "Tables", date: none)
#set page(paper: "a5", margin: 2cm)
#set text(size: 10pt)

= Rows

Text before the table that stays as it is in both revisions of the document.

#table(
  columns: (auto, 1fr, auto),
  table.header([Parameter], [Meaning], [Default]),
  ..range(12)
    .map(row => {
      let cells = (
        [P#(row + 1)],
        [Setting number #(row + 1) of the controller],
        [#(100 + row * 10)],
      )
      // A row is inserted in the middle, which is like its neighbors.
      if row == 5 and is-new {
        cells = ([P5a], [Setting number 5a of the controller], [155], ..cells)
      }
      // Two cells that are next to each other change.
      if row == 2 { cells = ([P3], [#edit("Setting number 3 of the controller", "Threshold of the detector")], [#edit(120, 125)]) }
      // A row is removed.
      if row == 8 and is-new { cells = () }
      // The last cell of a row and the first of the next change.
      if row == 9 { cells.at(2) = [#edit(190, 195)] }
      if row == 10 { cells.at(0) = [#edit("P11", "Q11")] }
      cells
    })
    .flatten(),
)

Text between the tables that stays as it is in both revisions.

= Columns

#table(
  columns: if is-new { 4 } else { 3 },
  table.header([Name], [Unit], ..if is-new { ([Range],) }, [Note]),
  ..(
    ("Pressure", "bar", "0 to 10", "Measured at the inlet"),
    ("Temperature", "K", "250 to 400", "Measured at the outlet"),
    ("Flow", "l/min", "0 to 50", "Measured in the return line"),
    ("Speed", "rpm", "0 to 3000", "Measured at the shaft"),
  ).map(row => if is-new { row } else { (row.at(0), row.at(1), row.at(3)) }).flatten().map(cell => [#cell]),
)

Text after the tables that stays as it is in both revisions.
