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
