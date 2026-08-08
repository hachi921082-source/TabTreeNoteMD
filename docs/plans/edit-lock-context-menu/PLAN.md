# PLAN: Edit-Lock Context Menu Item

> Status: implemented. Frontend only: `src/components/Tree/TreeView.tsx`
> (`buildContextMenuItems`). No backend/state changes — `state/useTree.ts`'s `setEditLock`,
> `ipc/tree.ts`'s `memoSetEditLock`, `TreeRow.tsx`'s locked-row indicator, and
> `EditorPane.tsx`'s read-only status banner were all already implemented by `memo-tree/PLAN.md`
> and `tab-lock/PLAN.md` (as the distinct, no-password "Edit Lock" concept — see
> `tab-lock/PLAN.md` §1 for the explicit disambiguation from Tab Lock). This plan closes the one
> remaining gap: nothing in the UI called `setEditLock`.

Sources: `docs/detailed-spec.md` §5.2 "Memo Tree (Left Pane) — Elaboration" (edit-lock menu item,
added for GitHub issue #64); `docs/plans/memo-tree/PLAN.md` (`memo_set_edit_lock` backend command,
`node.locked` field, `TreeRow` locked indicator, `EditorPane` read-only banner — all pre-existing).

## 1. Scope

Add a right-click context-menu item on a tree row that toggles that memo's `locked` flag. This is
the only change; the lock's effects (read-only editor, row indicator) already work today for any
memo that has `locked: true` on disk — there was simply no UI path to set it.

## 2. Menu Item Spec

In `TreeView.tsx`'s `buildContextMenuItems(path)`:

- Look up the target node via `findNode(nodes, path)` (from `shared/treeOps.ts`, already exports
  this — `locateNode`'s single-node wrapper) to read its current `node.locked` state.
- Two-state item, same single-target-only pattern as `"名前を変更"` / `"子として作成"` /
  `"プロパティ"` in the same function (`disabled: !singleSelection` — a mixed multi-selection has
  no unambiguous single label/action):
  - Currently unlocked (`locked` falsy) → label `"編集ロック"` → `onSelect: () => void
    tree.setEditLock(path, true)`.
  - Currently locked (`locked === true`) → label `"編集ロック解除"` → `onSelect: () => void
    tree.setEditLock(path, false)`.
- Placement: immediately after `"プロパティ"` (last item in the array) — groups it with the other
  single-target-only, non-destructive items rather than next to `"削除"`/`"メモを作成"` which are
  multi-target-capable.
- If `findNode` returns `null` (path not found — should not normally happen for a row that has a
  context menu, but defensive), treat as unlocked (fall back to the `"編集ロック"` label/action);
  do not throw.

## 3. Non-Goals

- No change to `state/useTree.ts`, `ipc/tree.ts`, `TreeRow.tsx`, or `EditorPane.tsx` — all already
  correct per `memo-tree/PLAN.md`.
- No new dialog/confirmation — locking/unlocking a memo is a low-stakes, instantly-reversible
  toggle (unlike Tab Lock, which is password-gated); matches existing "アイコン" submenu items'
  no-confirmation pattern.
