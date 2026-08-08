# PLAN: Tree Guide Lines (Indent Connector Lines)

Sources: `docs/detailed-spec.md` §5.2 "Memo Tree (Left Pane) — Elaboration" (added for GitHub issue
#60); `docs/plans/memo-tree/PLAN.md` (existing `TreeRow`/`TreeView` structure this replaces the
indent mechanism of).

## 1. Scope

Replace `TreeRow`'s plain `paddingLeft: depth * 16 + 6` indent with VSCode-explorer-style rendered
guide-line segments (continuing vertical lines for ancestors that still have following siblings,
plus a `├─`/`└─` branch connector at the row's own depth). Presentational only — no interaction,
state, or data-model change.

## 2. Root-Level Decision (not spec'd to the letter)

Root-level rows (`depth === 0`) render **no** guide/branch segment at all — same as VSCode's
explorer, which draws no connector for top-level items even in a multi-root forest. Guide segments
only start appearing from `depth === 1` onward (a depth-1 row renders its own branch connector;
it has zero ancestor segments since its parent, the root, contributes no line).

## 3. Data Threading Through Recursion

`TreeRow` currently recurses on itself with just a `depth` prop. Add two props:

- `ancestorGuides: boolean[]` — one entry per ancestor at depths `1..depth-1`; `true` means that
  ancestor has a following sibling (so its column should render a continuing vertical line through
  this row), `false` means blank (that ancestor was a last child, its line already terminated
  above). Length is always `depth - 1` for `depth >= 1`, and unused (ignored) at `depth === 0`.
- `isLastChild: boolean` — whether this row is the last child among its own siblings. Drives this
  row's own connector shape (`├─` if false, `└─` corner-only if true) and is folded into the
  `ancestorGuides` array passed to this row's children.

Recursion in `TreeRow`, when mapping `node.children`:

```ts
node.children.map((child, i) => {
  const childIsLast = i === node.children.length - 1;
  const childAncestorGuides = depth === 0 ? [] : [...ancestorGuides, !isLastChild];
  return <TreeRow ... depth={depth + 1} ancestorGuides={childAncestorGuides} isLastChild={childIsLast} .../>;
});
```

`depth === 0 ? [] : [...]` is what implements the §2 decision — the root never contributes a slot
to its children's ancestor-guide column list, so depth-1 rows always start with an empty
`ancestorGuides` (their only segment is their own branch connector).

Top-level call site in `TreeView.tsx` (`nodes.map(...)`): pass `depth={0}`, `ancestorGuides={[]}`,
`isLastChild={i === nodes.length - 1}` (value unused for rendering at `depth === 0` per §2, but
threaded through for type consistency and so the root-mapping code mirrors the child-mapping code
in `TreeRow`).

## 4. Rendering

Inside `.ttn-tree-row`, before the existing toggle/spacer + icon + name group:

- If `depth > 0`, render a `.ttn-tree-guides` container holding, in order:
  - One `.ttn-tree-guide` span per `ancestorGuides` entry: `.ttn-tree-guide-continue` (vertical
    line, full row height) if `true`, `.ttn-tree-guide-blank` (empty) if `false`.
  - One final `.ttn-tree-guide.ttn-tree-guide-branch` span, with modifier
    `.ttn-tree-guide-branch-mid` (`├─`: vertical line full height + horizontal stub) if
    `!isLastChild`, or `.ttn-tree-guide-branch-last` (`└─`: vertical line only top half + same
    horizontal stub) if `isLastChild`.
- The toggle/spacer + `FileIcon` + name/rename-input group is wrapped in a new
  `.ttn-tree-row-content` flex container (`gap: 4px`, `flex: 1 1 auto`, `min-width: 0`) so the
  guides container and the content group don't inherit an unwanted `gap` between them (the row's
  own `gap` moves from `.ttn-tree-row` into `.ttn-tree-row-content`).

## 5. Sizing (no shift vs. old `paddingLeft` indent)

`.ttn-tree-row`'s `paddingLeft` becomes a fixed `6px` (was `depth * 16 + 6`). Each `.ttn-tree-guide`
segment is `16px` wide, matching the old per-depth unit. Segment count for a depth-`D` row is
`D` (`D - 1` ancestor segments + 1 own-branch segment, or `0` segments at `D === 0`), so total
horizontal offset to the toggle button is `6 + 16 * D` in both the old and new scheme — unchanged.

`.ttn-tree-guides` uses `align-self: stretch` (row's `align-items: center` would otherwise size it
to content) so its `100%`-height line segments span the full row height set by
`--ttn-tree-row-line-height`.

## 6. CSS (`Tree.css`)

New classes: `.ttn-tree-guides`, `.ttn-tree-guide`, `.ttn-tree-guide-continue`,
`.ttn-tree-guide-blank`, `.ttn-tree-guide-branch`, `.ttn-tree-guide-branch-mid`,
`.ttn-tree-guide-branch-last`, `.ttn-tree-row-content`. Line color: `var(--ttn-border)`, `1px`
solid, vertical line centered at `left: 7px` of the 16px cell; horizontal branch stub from
`left: 7px` to the cell's right edge (`width: 9px`) at vertical center.

## 7. Non-Goals

- No change to click/selection/DND/rename/context-menu behavior or to `TreeNode`/data model.
- No change to `.ttn-tree-row-drop-*` indicators, `.ttn-tree-toggle`, `FileIcon`, or rename input
  behavior beyond moving them one level deeper into `.ttn-tree-row-content`.
