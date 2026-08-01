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

## Commit Convention

Commit one file per commit, with a message stating that file's role/purpose (not just what changed).

Exception: a set of new, mutually-dependent files (e.g. a group of Rust modules that reference each other and would not compile if split across separate commits) may be committed together as one logical unit. This does not relax the rule for independent/pre-existing files — split those normally.

## Language

Do not create or maintain `.ja.md` Japanese mirror files going forward. Write documentation in English only.
