# Implementation Plans Index

Each feature has its own independent, self-contained implementation plan under
`docs/plans/<feature-name>/PLAN.md`. Unlike the old `TabTreeNote` repository's single milestone-ordered
`PLAN.md`, these are **not** meant to be read or built in a fixed sequence — each one can be read,
planned against, and (mostly) implemented on its own. Two plans are foundational and are referenced by
name from the others rather than repeated: `project-architecture` (module layout, naming conventions)
and `persistence-fs` (atomic write, self-write registry, conflict detection, corruption recovery,
app-settings persistence) — read those two first, since nearly everything else assumes them.

A feature folder normally holds a single `PLAN.md`, but may hold a second file when one sub-algorithm
is large and self-contained enough to warrant separate reading (currently: `tab-lock/CRASH-RECOVERY.md`
alongside `tab-lock/PLAN.md`, and `tab-lock-permanent-unlock/CRASH-RECOVERY.md` alongside
`tab-lock-permanent-unlock/PLAN.md`) — the primary `PLAN.md` always points to any companion file. Table
of Contents was similarly split out of `markdown-editing/PLAN.md` into its own feature folder,
`table-of-contents/`, because it is referenced from three other plans (`markdown-editing`, `link`,
`tab-lock`), not because of size alone.

`tab-lock-permanent-unlock` is a separate feature folder from `tab-lock` (not a subsection of it)
because it is its own distinct, separate, irreversible operation with its own command family, marker
file, and recovery flow (`tab-lock-permanent-unlock/PLAN.md`'s own rationale) — large enough to warrant
the same PLAN.md/CRASH-RECOVERY.md split the original feature used.

`markdown/markdown-preview/PLAN.md`, `markdown/pdf-export/PLAN.md`, and `markdown/image-handling/PLAN.md`
live together under a nested `docs/plans/markdown/` parent because they are jointly-planned sub-parts
of one originally combined, oversized plan (split-pane preview, PDF export, and live-view real image
rendering) split apart by feature rather than kept as one combined document. The nested
`markdown/image-handling/PLAN.md` there is intentionally a distinct file from the top-level
`image-handling/PLAN.md` above (write path vs. read-back path — see that file's own disambiguation
note), not a duplicate or a relocation of it.

## Feature Plans

| Plan | Covers |
|---|---|
| [`project-architecture/PLAN.md`](project-architecture/PLAN.md) | Tech stack, directory/module layout, `Result<T>` mapping, Tauri command & event naming conventions (foundational) |
| [`persistence-fs/PLAN.md`](persistence-fs/PLAN.md) | Atomic write, self-write registry, file watching/conflict classification, metadata corruption recovery, App Settings JSON, name validation, Windows lock retry (foundational) |
| [`tab-management/PLAN.md`](tab-management/PLAN.md) | Tab create/register/rename/reorder/remove, pin, open-location (excludes lock, see `tab-lock`) |
| [`memo-tree/PLAN.md`](memo-tree/PLAN.md) | Tree reconciliation, memo create/rename/delete/move/DND, labels, edit lock, properties |
| [`editor-save/PLAN.md`](editor-save/PLAN.md) | Plain-text editing, encoding/EOL, save triggers, autosave, conflict resolution UI, in-memo search/replace |
| [`markdown-editing/PLAN.md`](markdown-editing/PLAN.md) | CodeMirror 6 decoration extension, live/source toggle, 16-slot toolbar, sanitization allowlist |
| [`table-of-contents/PLAN.md`](table-of-contents/PLAN.md) | TOC heading scan/slugification/marker-block generation, triggered from the markdown-editing toolbar |
| [`image-handling/PLAN.md`](image-handling/PLAN.md) | Image paste/insert storage, filename generation, resize markup, no-orphan-cleanup policy |
| [`markdown/markdown-preview/PLAN.md`](markdown/markdown-preview/PLAN.md) | Split-pane Markdown preview (`paneMode` tri-state toggle, `markdown-it` rendering pipeline, scroll-sync, checkbox/link interaction) |
| [`markdown/pdf-export/PLAN.md`](markdown/pdf-export/PLAN.md) | PDF export via hidden-iframe print flow, reusing the preview pane's renderer |
| [`markdown/image-handling/PLAN.md`](markdown/image-handling/PLAN.md) | Image read-back pipeline (`image_read_bytes`, Blob-URL cache) and live-view real `<img>` rendering — distinct from the top-level `image-handling/PLAN.md` above, which covers the write path only |
| [`search-index/PLAN.md`](search-index/PLAN.md) | Cross-tab full-text index build/update/query, locked-tab exclusion |
| [`link/PLAN.md`](link/PLAN.md) | Auto-link recognition, `memo:` resolution, external link opening |
| [`tab-lock/PLAN.md`](tab-lock/PLAN.md) + [`tab-lock/CRASH-RECOVERY.md`](tab-lock/CRASH-RECOVERY.md) | Tab-level password lock/unlock, session lifecycle, cross-tab DND onto locked tabs; crash-mid-lock recovery |
| [`tab-lock-permanent-unlock/PLAN.md`](tab-lock-permanent-unlock/PLAN.md) + [`tab-lock-permanent-unlock/CRASH-RECOVERY.md`](tab-lock-permanent-unlock/CRASH-RECOVERY.md) | Irreversibly decrypting a whole locked tab back into an ordinary unencrypted tab (distinct from ending a session, `tab-lock/PLAN.md`); crash-mid-permanent-unlock recovery |
| [`appearance-theme/PLAN.md`](appearance-theme/PLAN.md) | Light/dark theme, UI font scale, basic color/font/spacing customization |
| [`window-operation/PLAN.md`](window-operation/PLAN.md) | Frameless window, pane layout/splitter, shortcuts, tray/autostart, multi-launch prevention, settings-screen shell |

## Independent Implementation, Mandatory Final Integration Pass

Per-feature plans and their implementation may proceed **independently** — a feature can be planned,
built, and functionally spot-checked on its own without waiting for every other feature to exist first.
This independence is the whole point of splitting the plans this way.

However, this independence creates a real risk: a feature that looks correct in isolation can still
break at the seams where it touches another feature, and per-feature verification alone will not catch
that. Concrete examples already visible from the plans above:

- Tab Lock + Search: does a search performed while a session is active *actually* still exclude that
  tab (`tab-lock/PLAN.md` §6, `search-index/PLAN.md` §1)?
- Memo Tree DND + Tab Lock: dropping a memo onto a locked tab mid-drag (`memo-tree/PLAN.md` §3,
  `tab-lock/PLAN.md` §5) — does the whole drop-to-password-to-session-start-to-insert chain work as one
  interaction, not just each half in isolation?
- Table of Contents + Tab Lock: does clicking the toolbar's "Update" button work correctly for a memo
  inside an active Tab Lock session (`table-of-contents/PLAN.md`, `tab-lock/PLAN.md` §3), and does
  the resulting `#slug` anchor link still resolve/scroll correctly (`link/PLAN.md` §5) for that
  locked-then-unlocked memo?
- Link + Tab Lock: does `memo:` resolution correctly flip from "unresolved" to "resolved" the moment a
  session starts for the target tab, and back on session end (`link/PLAN.md` §2, `tab-lock/PLAN.md` §3)?
- Editor/Save + Persistence-FS: does the conflict-resolution dialog (`editor-save/PLAN.md` §3) actually
  fire correctly when an external change lands on the currently-open memo, end to end through the
  watcher pipeline (`persistence-fs/PLAN.md` §5)?
- Window/Operation + everything: does the "flush all pending saves before exit" chain
  (`editor-save/PLAN.md` §6, `window-operation/PLAN.md` §1) actually complete before the frameless
  window's close handler lets the process die, across ordinary memos *and* an active tab-lock session's
  pending re-encrypted saves?

**Therefore**: `reviewer-lite`'s sign-off is not complete after checking each feature against its own
`PLAN.md` in isolation. Once implementation of **all** features listed above is complete,
`reviewer-lite` must perform one comprehensive, whole-app integration test pass — actually running the
app and exercising cross-feature interaction paths like the examples above, not just each feature's own
happy path — before giving final sign-off. This final pass is a distinct, additional step, not a
relabeling of the per-feature checks already performed along the way.
