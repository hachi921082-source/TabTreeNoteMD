# TabTreeNoteMD

@~/.claude/shared/dev-workflow.md

## Old Repository (Reference Only)

- Path: `c:\Users\m9112\vscode2\TabTreeNote`. **Reference only; do not perform Edit/Write/commits.** For details on reference destinations, see [docs/DECISIONS.md](docs/DECISIONS.md).

## Documentation for This Repository

- [docs/spec.md](docs/spec.md) — A requirements baseline that consolidates the design decisions and post-implementation change logs from the old repository. Use this as a starting point when considering the redesign.
- [docs/detailed-spec.md](docs/detailed-spec.md) — Elaboration of spec.md to a level usable for per-feature implementation planning (`docs/plans/<feature-name>/PLAN.md`).
- [docs/DECISIONS.md](docs/DECISIONS.md) — The context of decisions, comparative analysis, rejected alternatives, and detailed reference information from the old repository (normally not read; consult only when needed).

`spec.md` and `detailed-spec.md` are both long. Each has an **Index** near the top — read it first to find the relevant section number instead of reading the whole file.

- Future version bumps (e.g. a `v1.2.0` branch) follow the same version-suffixed-file convention started by `spec.v1.1.0.md`: new/changed spec content goes into its own `docs/spec.v<version>.md` rather than being appended into `docs/spec.md`, and this separation extends to `docs/detailed-spec.v<version>.md` and version-labeled plan content under `docs/plans/<feature-name>/` — never append version-specific content into the frozen baseline or an earlier version's file. See [docs/README.md](docs/README.md)'s File Structure section for the current file list and what each one covers.

## Workflow

Operated using a three-agent system: designer-lite → developer-lite → reviewer-lite (even if the user does not explicitly name the agent, work is always conducted through the corresponding agent). For details (rationale for adoption, documentation structure, escalation procedure), see [.claude/agents/README.md](.claude/agents/README.md).

## GitHub Issue Workflow

The repository's main/default branch is `V1.0.0` (renamed from `implementation`; not `master` — `master` is stale and kept only for reference; do not branch from or merge into it).

For each GitHub issue (repo `hachi921082-source/TabTreeNoteMD`): cut a branch named `fix/<short-slug>` (e.g. `fix/path-traversal-guard`) → fix on that branch (designer-lite/developer-lite/reviewer-lite flow above still applies) → Review → once the issue is resolved, open a PR and merge.

After merge, close the GitHub issue (not delete — GitHub keeps the full history), and add a short summary entry (issue number, title, PR link) to [docs/RESOLVED-ISSUES.md](docs/RESOLVED-ISSUES.md).

## PLAN Implementation Workflow

Repo-specific detail for the shared "PLAN Implementation Flow" standard (see the imported `dev-workflow.md` above for the generic procedure). Established by precedent during `v1.2.0/preview-link-encoding-fix`'s implementation:

- Branch naming: `feature/<version>-<feature-name>` (e.g. `feature/v1.2.0-preview-link-encoding-fix`), mirroring v1.1.0's `feature/v1.1.0-<name>` pattern.
- Merge target: the current version branch (e.g. `V1.2.0`), via a GitHub PR (updated 2026-08-22 — superseded the original no-PR direct-merge precedent: a PR gives a visible reviewable diff, an explicit merge action as its own checkpoint, and a durable record on GitHub, all valued for the same reversibility reasons `fix/<slug>` branches are used for GitHub-issue-driven work). Commit message on merge: `Merge feature/<version>-<feature-name>: <description>`.
- Confirm with the user before merging, pushing, or deleting the branch — reviewer-lite's approval alone is not authorization for these steps (some of these git operations may also require the user's own execution or explicit permission grant, since Claude Code's Auto Mode permission classifier blocks certain git operations that touch shared/remote state, independent of any prior chat confirmation).
- After merge, write or update [docs/HANDOFF.md](docs/HANDOFF.md) summarizing what changed, what remains open, and the recommended next PLAN.md to implement.
