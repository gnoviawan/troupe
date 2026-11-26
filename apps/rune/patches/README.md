# Troupe's patches to Rune

Troupe builds Rune at one pinned rev (`apps/rune/rune.pin`). When Troupe needs
a change in Rune before upstream has it, the change lives here as a numbered
patch series on top of that rev.

| Patch | What it changes | Upstream issue |
|---|---|---|
| `0001-web-a11y-menu-tooltip-combobox-roles.patch` | The web mirror gives menus and tooltips their ARIA role and container, and mirrors a combobox trigger as `role="combobox"` with its name | Drafted, not filed |
| `0002-web-a11y-terminal-grid-rows.patch` | The web mirror reads a `grid` (a terminal screen) as one text line per non-blank row, as the native tree's text runs | Drafted below, not filed |

Cargo still builds the pinned rev until a fork carries the series: CI checks that the
series applies, and local runs use it through a `[patch]` of the scratch clone.

## Rules

- One change per patch, made with `git format-patch` against the pinned rev
  and named `NNNN-short-name.patch` (`0001-...`, `0002-...`). The number sets
  the apply order.
- Upstream takes issues, not pull requests. File an issue on
  `anomalyco/rune` for every patch and put its URL in the patch's commit
  message, so the patch can be dropped when upstream fixes it.
- Bumping the pin means checking the series again: drop patches upstream
  absorbed, then rebase the rest.

## Check and build the series

```sh
node scripts/rune/apply-patches.mjs
```

The script fetches the pinned rev, applies the patches in order with
`git am`, and exits 1 at the first patch that does not apply. Pass
`--out DIR` to keep the result: a clone with the branch `troupe/<rev>`, ready
to push to a fork that `apps/rune/Cargo.toml` can point at. With no patches it
does nothing and passes.

## Upstream issue drafts

### 0002: the web mirror drops a terminal grid's text

At `9d4b463`, `runtime/web/a11y.rs` `web_a11y_items` has no case for an element with
`vis.grid`. A `grid` has no `TextSpec` of its own, so the mirror emits nothing for it: a
terminal's screen never reaches a screen reader or the browser's Find on the web, while the
native tree reads each row as a `TextRun` (`a11y/text.rs` `a11y_grid_rows`). Repro: a
`grid(20, 8.0, 19.0, rows)` with rows `"$ ls"` and `"hello  world"`; `web_a11y_items()` is
empty. Suggested fix, which Troupe carries as a patch: mirror each non-blank row the grid shows
as a `text` item at its row's rect in the row's font (`GridRow::reading` gives the text), with
the test `mirrors_a_terminal_grids_rows_as_lines_of_text`. Rune sources such as `a11y.rs` are
CRLF, so `scripts/rune/apply-patches.mjs` runs `git am --keep-cr` and this folder's
`.gitattributes` keeps patches byte for byte.
