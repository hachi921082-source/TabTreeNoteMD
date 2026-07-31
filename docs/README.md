# docs/

This directory contains specification documents (feature specifications, screen layouts, architecture review memos, etc.).

## File Structure

- `spec.md` — Finalized specification. The authoritative source for feature specifications that the user and `designer-lite` work through directly. Records only what is currently decided, concisely.
- `DECISIONS.md` — Decision rationale, comparative analysis, and rejected proposals. Detailed background is consolidated here to keep `spec.md` concise.
- `detailed-spec.md` — Detailed specification. Elaborated by `designer-lite` based on the finalized content of `spec.md` to a level where `developer-lite` can create an implementation plan.
- `plans/<feature-name>/PLAN.md` — Implementation plan independent for each feature. Design document for implementation created by `developer-lite` based on `spec.md` and `detailed-spec.md`, including function specifications and similar details. Does not follow the milestone accumulation order like the old TabTreeNote.
- `QA.md` — Questions gathered when `developer-lite` confirms with `designer-lite` during implementation planning and determines that "the user's own decision is required." The user reviews this and provides answers.
