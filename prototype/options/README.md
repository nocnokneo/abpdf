# PROTOTYPE: the options of step 4

Throwaway code that answers one question: which of these should step 4 build?
It is not for merging.
Nothing here has tests, and `cargo test` does not compile on this branch.

Each option is switched on by an environment variable, which `src/prototype.rs` reads:

- `ABPDF_PROTOTYPE_SPLIT_AREAS=1`: the areas of a change and the outlines of the audit end where left-out text stands between their tokens.
- `ABPDF_PROTOTYPE_CELLS=1`: a replacement that runs through the same number of table cells on both sides is one change per cell.
- `ABPDF_PROTOTYPE_FIGURES=grouped`: images and the vector graphics that the producer keeps together are tokens of the flow.
- `ABPDF_PROTOTYPE_FIGURES=all`: so are shapes that are drawn one by one and touch each other.

To build the examples and the pictures of them:

```sh
cargo build --release
mise run manual
mise exec -- python3 prototype/options/build.py
```

`dashboard.html` shows the pictures, which the build leaves in `out/`.
