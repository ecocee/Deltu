# Context Directory

This directory is Deltu's engineering knowledge base. Read the relevant files
before implementing anything. See `AGENT.md` for the required reading order
and `specs/00-build-plan.md` for the full build process.

## Contents

| File / Directory        | Purpose                                                            |
| ----------------------- | ------------------------------------------------------------------ |
| `project-overview.md`   | Product definition, goals, features, scope, success criteria.      |
| `architecture.md`       | Authoritative system architecture, boundaries, storage, invariants.|
| `ui-context.md`         | Developer interface context: CLI first, no graphical UI.           |
| `code-standards.md`     | Implementation rules and conventions for all code.                 |
| `ai-workflow-rules.md`  | Development workflow, scoping rules, delivery approach.            |
| `developer-context.md`  | Developer experience: CLI, API, SDK, configuration, logs, metrics. |
| `research/`             | Technical research that informs architecture decisions.            |
| `decisions/`            | Architecture decision records (numbered, append-only).            |
| `specs/`                | Per-unit implementation specifications (source of build order).    |
| `progress-tracker.md`   | Current phase, completed work, open questions, next steps.         |

## Reading Order for Implementation Work

1. `project-overview.md`
2. `architecture.md`
3. `ui-context.md`
4. `code-standards.md`
5. `ai-workflow-rules.md`
6. `progress-tracker.md`
7. The specification for the unit being implemented
8. Related research and decision records for that unit

## Rules

- Documentation must describe the actual implementation, never imaginary features.
- Keep this knowledge base synchronized when implementation changes behavior.
- One unit at a time, per the spec order in `specs/00-build-plan.md` §26.
