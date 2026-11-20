// PROTOTYPE example: words change on both sides of text that a pattern leaves out.
#let is-new = sys.inputs.at("revision", default: "old") == "new"
#let edit(old, new) = if is-new { new } else { old }
#set document(title: "Left-out text", date: none)
#set page(width: 12cm, height: 6.5cm, margin: (x: 1.2cm, top: 1.6cm, bottom: 1cm),
  header: [Controller manual #h(1fr) Revision #edit("B", "C")])
#set text(size: 10pt)

= Limits

The flow limits are #edit[shown in Figure 3 below][listed in Table 4 above], and the unit stops when one is reached.

Units in the field keep #edit[the former limits (p. 12) until][their earlier bounds (p. 31) up to] the next service.
