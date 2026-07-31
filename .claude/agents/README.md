# .claude/agents/

This is the place to put subagent definitions specific to this project.
Add them in the format of `agent-name.md` (with `name` / `description` / `tools` / `model` specified in frontmatter).

## Current Agents

- `designer-lite` — Consultation and creation assistant / reviewer for specifications (`docs/spec.md`). Also handles responses to questions from developer-lite and creation of detailed specifications (`docs/detailed-spec.md`).
- `developer-lite` — Reads specifications and creates implementation plans per feature (`docs/plans/<feature-name>/PLAN.md`), then implements the app (`src/`).
- `reviewer-lite` — After implementation, cross-checks the app against specification documents, runs the app, and tests it by actually operating key flows.

We operate using a 2-agent configuration (worker/reviewer) that invokes `developer-lite` → `reviewer-lite` in order.

Since this is a personal small-scale app, it does not maintain a persistent log counting round-trips (equivalent to AGENT_LOG.md). Instead, we adopt a lightweight configuration ("lite" set) that operates with "one-way escalation" (written in `docs/QA.md` and stops there).

## Workflow Philosophy

Claude Code itself (the orchestrator) basically mediates interactions between the user and subagents, and between subagents themselves. When a subagent finishes execution, it returns the result text to the caller and has no means to communicate directly with the user.

As an exception, **only `developer-lite` is equipped with the Agent tool** (`designer-lite` and `reviewer-lite` do not have it). This allows `developer-lite` to directly call `designer-lite` during implementation planning to ask questions. To avoid reciprocal loops, this invocation is **one-way only: `developer-lite` → `designer-lite`**, and `designer-lite` does not call back to `developer-lite`.

### Overall Flow

1. **Specification alignment (User ⇔ designer-lite)**: The user works directly with `designer-lite` to finalize `docs/spec.md`. The rationale for decisions and rejected options are recorded in `docs/DECISIONS.md`. This is the foundation for everything.
2. **Detailed specification creation (designer-lite)**: Based on the finalized content of `docs/spec.md`, `designer-lite` creates `docs/detailed-spec.md`, which serves as the base for implementation planning. **`docs/spec.md` itself is not edited in this step.**
3. **Implementation planning (developer-lite ⇔ designer-lite)**: `developer-lite` reads `docs/spec.md` and `docs/detailed-spec.md`, creating independent `docs/plans/<feature-name>/PLAN.md` for each feature (including function specifications, etc.). Unlike the old TabTreeNote with a single file organized by milestone order, plans are split per feature. If questions arise during planning, `developer-lite` directly calls `designer-lite` to confirm.
   - If `designer-lite` can answer within the scope of the finalized spec, it answers on the spot.
   - If `designer-lite` determines "this requires the user's own judgment," it does not make a provisional decision itself. Instead, it appends the question to `docs/QA.md` and ends the conversation there. `developer-lite` halts planning for that point and reports to the user to "please check `docs/QA.md`."
4. **Implementation (developer-lite)**: Once implementation plans are finalized for a feature, `developer-lite` begins implementation in `src/`.
5. **Review (reviewer-lite)**: After implementation, `reviewer-lite` checks consistency against `docs/spec.md`, `docs/detailed-spec.md`, and the corresponding `PLAN.md`. It runs the app and verifies major flows by actually operating them. It does not edit code or commit; it only provides feedback.

### Why This Design

- Subagents do not have a means to ask the user and wait for a response during execution, even if instructed to "confirm with the user." To avoid the problem where an agent mistakenly self-completes by assuming "the user answered," this rule is made explicit: **"If you cannot answer or need the user's judgment, write in `docs/QA.md` and stop."**
- `docs/spec.md` is kept as the authoritative version updated only through direct dialogue with the user and is not rewritten for implementation convenience. Elaboration and implementation-driven interpretation are separated into the `docs/detailed-spec.md` and `docs/plans/` layers.
- Given the assumption that the user cannot read code, **actual runtime testing by `reviewer-lite`** becomes the only safety net (the rationale for this technical choice is in `docs/DECISIONS.md`).
