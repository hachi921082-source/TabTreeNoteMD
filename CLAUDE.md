# TabTreeNoteMD

## Old Repository (Reference Only)

- Path: `c:\Users\m9112\vscode2\TabTreeNote`. **Reference only; do not perform Edit/Write/commits.** For details on reference destinations, see [docs/DECISIONS.md](docs/DECISIONS.md).

## Documentation for This Repository

- [docs/spec.md](docs/spec.md) — A requirements baseline that consolidates the design decisions and post-implementation change logs from the old repository. Use this as a starting point when considering the redesign.
- [docs/detailed-spec.md](docs/detailed-spec.md) — Elaboration of spec.md to a level usable for per-feature implementation planning (`docs/plans/<feature-name>/PLAN.md`).
- [docs/DECISIONS.md](docs/DECISIONS.md) — The context of decisions, comparative analysis, rejected alternatives, and detailed reference information from the old repository (normally not read; consult only when needed).

`spec.md` and `detailed-spec.md` are both long. Each has an **Index** near the top — read it first to find the relevant section number instead of reading the whole file.

## Workflow

Operated using a three-agent system: designer-lite → developer-lite → reviewer-lite (even if the user does not explicitly name the agent, work is always conducted through the corresponding agent). For details (rationale for adoption, documentation structure, escalation procedure), see [.claude/agents/README.md](.claude/agents/README.md).

## GitHub Issue Workflow

The repository's main/default branch is `V1.0.0` (renamed from `implementation`; not `master` — `master` is stale and kept only for reference; do not branch from or merge into it).

For each GitHub issue (repo `hachi921082-source/TabTreeNoteMD`): cut a branch named `fix/<short-slug>` (e.g. `fix/path-traversal-guard`) → fix on that branch (designer-lite/developer-lite/reviewer-lite flow above still applies) → Review → once the issue is resolved, open a PR and merge.

After merge, close the GitHub issue (not delete — GitHub keeps the full history), and add a short summary entry (issue number, title, PR link) to [docs/RESOLVED-ISSUES.md](docs/RESOLVED-ISSUES.md).

## Commit Convention

Commit one file per commit, with a message stating that file's role/purpose (not just what changed).

Exception: a set of new, mutually-dependent files (e.g. a group of Rust modules that reference each other and would not compile if split across separate commits) may be committed together as one logical unit. This does not relax the rule for independent/pre-existing files — split those normally.

## Language

Do not create or maintain `.ja.md` Japanese mirror files going forward. Write documentation in English only.
