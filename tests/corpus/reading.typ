// What a reader has to get out of a PDF.
// tests/reading.rs asserts on the text of this document.

#set document(title: "Reading", date: none)
#set page(
  paper: "a6",
  margin: (x: 1.5cm, y: 2cm),
  header: [Reading corpus #h(1fr) Draft],
  footer: context [
    #h(1fr) Page #counter(page).display() of #counter(page).final().first()
  ],
)
#set heading(numbering: "1.1")
#set text(size: 10pt)
#show figure: set block(breakable: true)

#outline()

= Broken words <words>

// The narrow column forces breaks inside the long words and after hyphens of
// the compound.
#block(width: 3cm)[
  #set par(justify: true)
  #set text(hyphenate: true)
  An internationalization of a state-of-the-art-comparison is extraordinarily
  uncharacteristic.
]

// In the bold font the first hyphen is a hard one, so the producer marks the
// soft ones instead.
#block(width: 4.2cm)[
  #set par(justify: true)
  #set text(hyphenate: true, weight: "bold")
  Well-known misunderstandings are uncharacteristically counterproductive.
]

// The italic font has only a soft hyphen, which leaves it unmarked.
#block(width: 3cm)[
  #set par(justify: true)
  #set text(hyphenate: true, style: "italic")
  Some incomprehensibilities.
]

= Characters

The ﬁrst ofﬁce “quotes” its text — with dashes, e.g. this one.
See @styles #pdf.artifact[(p. #context counter(page).at(<styles>).first())].

#figure(
  rect(width: 3cm, height: 1.2cm, fill: aqua)[
    #align(center + horizon)[Label in drawing]
  ],
  caption: [A drawing with a label],
)

= Table

#figure(
  table(
    columns: 2,
    table.header([Key], [Value]),
    ..range(30).map(i => ([Row #i], [Value number #i])).flatten(),
  ),
  caption: [A table that continues on the next page],
)

= Styles <styles>

#text(fill: red)[Red] and *bold* and _italic_ and #text(size: 14pt)[bigger].

#rotate(90deg, reflow: true)[Rotated]
