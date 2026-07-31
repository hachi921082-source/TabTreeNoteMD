# TabTreeNoteMD Decision History and Rationale

> `spec.md` follows the principle of recording only "what is currently decided" concisely, so detailed decision history, comparative studies, and rejected alternatives are consolidated here. This does not need to be read ordinarily; **refer to it only when you want to reconsider a decision**.

---

## Technology Stack: Adopting Tauri (2026-07-30)

**Decision**: Tauri (Rust backend + OS standard WebView2 frontend). Changed from the previous v1's Electron+TypeScript.

- **Goal = Reducing Size**: The previous Electron version had a distribution size of approximately 72–108 MB and memory usage of 100–300 MB. Since Tauri does not have its own Chromium and rides on WebView2, the application itself is a few MB to tens of MB, with significantly reduced memory usage. Startup delay caused by self-extraction of portable exe is also expected to be eliminated.
- **Securing Verification Means as Top Priority**: Since users lack programming knowledge and cannot verify through code review, "automatic testing by Claude itself" becomes the key safety net. Both Electron and Tauri are WebView (Chromium-based) and support automatic verification via CDP/WebDriver (the previous project's tester role has a track record of actually verifying via CDP).
- **Reasons for Rejecting Fully Native GUI (C#: WPF/WinUI3)**: The corresponding automatic UI verification tools (FlaUI/WinAppDriver, etc.) are not as mature as CDP and there is no track record. Given that users cannot read code, a verification method with weak tools is a high-risk choice. Additionally, the cost of recreating the decoration mechanism for Markdown live preview is significant.
- **Tauri preserves renderer assets**: The renderer (HTML/CSS/JS) including the decoration mechanism can be carried over largely as is.
- **Rejected Alternatives**: C# fully native (reasons above) / C#+WebView2 hybrid (dual IPC configuration between native and WebView parts becomes complex) / Electron continuation (contradicts the goal of reducing size).
- **Known Remaining Costs**: Rust has heavy compilation and cannot iterate as fast as TypeScript. A new automatic test environment for Tauri needs to be built.

## Editor Engine for Markdown Editing: CodeMirror 6 (2026-07-31)

**Decision**: **CodeMirror 6 + a custom-built decoration extension**, operating directly on the plain-text
buffer as source of truth, for the Markdown live-view editor (`spec.md` §4.2). Rejected: Milkdown/TipTap
(ProseMirror-based rich-document-model WYSIWYG libraries), Slate, and Draft.js. This resolves the open
question originally recorded as `QA.md` item #1.

- **Source-of-truth safety**: CodeMirror 6's buffer is the plain text itself — there is no parse/serialize
  round-trip on save. A ProseMirror-based library (Milkdown/TipTap) holds an internal document tree;
  saving means *regenerating* Markdown text from that tree via a serializer, not writing back the user's
  original characters with edits applied in place.
- **Custom syntax risk**: This app has several non-standard Markdown extensions with no native schema
  support in Milkdown/TipTap: the `memo:` link scheme (`[text](memo:TabName/MemoName)`), `<img src width
  height>` resize notation, and `<span style="color;font-size">` decoration notation (all from `spec.md`
  §4.2). Using a ProseMirror-based library would require hand-building both a custom parser (text→tree)
  AND a custom serializer (tree→text) for every one of these. Any gap in that custom work causes *silent*
  content mutation on save (reformatting, or literal loss of unsupported syntax) — a data-integrity risk,
  which is a more severe failure class for a note-taking app than a UI bug.
- **CodeMirror's known risk is a different, lower-severity class**: the legacy project's flagged defect
  area ("overlapping range edits, decoration widget click select" — cursor/selection mapping issues when
  decorations and hidden syntax markers occupy different visual space than their underlying text length,
  `spec.md` §8) is a rendering/interaction bug. It does not corrupt the saved file (the buffer is still
  correct plain text underneath); it is testable and fixable through careful decoration/click-handling
  implementation, unlike the silent-mutation risk above.
- **Frequent live/source switching**: The user wants to switch between live-view and raw-Markdown-source
  display frequently, not just occasionally, and additionally wanted a settings-level default for which
  mode a memo opens in (`spec.md` §3.4 `viewMode`). With CodeMirror 6, both modes operate on the exact
  same text buffer — switching is just toggling the decoration layer on/off, with zero conversion and
  zero risk. With a ProseMirror-based library, a "raw source" view would need either a second synced
  CodeMirror instance (re-running the tree↔text conversion, and its risks, on every single toggle rather
  than just at save time) or a library-specific source-mode plugin. Since the user wants to toggle often,
  this would mean hitting the round-trip risk far more frequently than just at save time.
- **Rejected Alternatives**:
  - **Milkdown / TipTap**: the only realistic representatives of "ready-made Markdown WYSIWYG library"
    considered, because they are the only ones that ship Markdown-aware parsing/serialization and
    toolbar infrastructure out of the box. Rejected for the reasons above (points 1–4).
  - **Slate**: a lower-level framework like ProseMirror, with no built-in Markdown import/export or
    toolbar. Choosing it would mean paying the same "build it yourself" cost as CodeMirror while *still*
    carrying the document-tree serialization risk described above — the worst of both approaches
    combined.
  - **Draft.js**: effectively unmaintained (Meta stopped active development years ago); not viable for a
    new project.

## Tab Deletion Semantics: Unregister-Only, No Folder-Trash Option (2026-08-01)

**Decision**: Removing a tab from `AppSettings.tabs` never touches, deletes, or trashes the underlying
folder or any files in it. It only unregisters the folder from the app's tab list (`spec.md` §2.1,
§4.1 pt.7). There is no "delete tab and folder" action anywhere in the app.

- **What was considered**: The legacy app (`TabTreeNote`, reference-only) offered two separate
  right-click actions on a tab: "remove tab, keep folder" (unregister-only) and "delete tab and
  folder" (unregister + send the folder to the OS trash).
- **What was rejected, and why**: Offering a folder-trash option at the tab level was rejected
  entirely, not just deprioritized.
  - **Data-loss risk disproportionate to convenience**: a tab is a pointer to an arbitrary,
    possibly pre-existing folder (`[spec §2.1]`, "drag-register existing folders") that may contain
    files the app doesn't manage or even know about (non-`.md` files, subfolders — `[spec §2.2]`
    excludes subdirectories from scanning but they can still exist on disk). Bulk-trashing an entire
    folder tree on a single tab-removal action risks destroying content the user did not intend to
    delete, for a convenience (skipping a manual Explorer delete) that doesn't justify the risk.
  - **Simplicity**: a folder-trash option would require designing the `TRASH_FAILED`-retry/fallback
    edge case (`[spec §5.8]`) at the tab level, in addition to the one that already exists for
    memo-level delete (`[spec §2.2]`). Dropping the option avoids that second edge-case surface
    entirely, leaving exactly one unambiguous action ("remove tab" = unregister) instead of two
    similarly-named, easily-confused actions.
- **What this does not affect**: memo-level deletion (`[spec §2.2]`) is unchanged and still goes to
  the OS trash by default, with the existing `TRASH_FAILED` → user-consent → direct-delete fallback
  (`[spec §5.8]`). Only the *tab* level dropped the trash option; individual memo files within a tab
  are still deletable (to trash) as before.
- **User-facing consequence**: to actually delete a registered folder's contents, the user must do so
  outside the app (e.g. via Windows Explorer) after removing the tab, or before/without ever
  registering it.

## Memo Identification Method: Filename = Memo Name (UUID Not Adopted)

The philosophy of following plain text (transparency, external tool compatibility, retains content even if metadata is lost) is prioritized. The adoption of internal IDs like UUID was rejected because the loss of correspondence tables would risk breaking `memo:` links and tree structures. As a trade-off, renaming = file renaming will break `memo:` links (accepted).

## History of AutoReName Discontinuation (Feature to Automatically Name Memos Based on First Non-empty Line)

In the old v1, it was a specification that "always ON with no way to disable it", so it was removed at user request (post-M7). It was confirmed not to be implemented in TabTreeNoteMD (2026-07-31, spec.md §2.2).

## Tab Lock Session Lifetime: Why "Discard Immediately When Leaving Tab"?

In the old v3's node lock, initially "not discarded when switching tabs", but there is a history of change to a more conservative standard of "discard immediately when leaving tabs" through the user's own reconsideration (there is little practical benefit in maintaining the decrypted state while not viewing, so priority is given to closing the risk of being away early). Tab lock simply follows this standard as well.

## Reference to Old Repository (Use During Implementation)

When you want to confirm implementation logic, refer to the corresponding file in the old repository (`c:\Users\m9112\vscode2\TabTreeNote`, read-only, do not edit) using Read, copy the necessary code to this repository, and then modify it.

- `spec.md` (the same content has been copied to this repository. A requirements baseline integrating DESIGN.md/DESIGNv3.md/00-core-principles.md/data-model.md/post-M7-changes.md/post-M10-changes.md/issue.md from the old repository)
- `src/main/crypto/*` (encryption and node lock implementation. Reference for implementing tab lock)
- `src/main/fs/atomicWrite.ts` and other persistence layers
- `AGENT_LOG.md` (history of past implementation and review exchanges. Keep for reference only)
