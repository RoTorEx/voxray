# Documentation Conventions

## Repo root hygiene

Keep the repo root **minimal** and easy to scan.

Rules:

- Keep root docs minimal: `README.md` (human entrypoint) and `AGENTS.md` (agent router).
- Optional root helpers: `TASK.md` (task queue) and `CHANGELOG.md` (release progress) are allowed if the repo uses them.
- Put almost all durable docs under `docs/` (architecture, contracts, features, operations, release, etc.).
- Avoid adding extra “policy” markdown files to the root; prefer `docs/*` so routing stays explicit and conditional.
- Only keep root config files that the stack actually requires (e.g., `package.json`, `pyproject.toml`, `Cargo.toml`). Do not duplicate config across multiple tools “just in case”.

## Stable docs

Use stable docs for durable truth:

- `README.md` — human entrypoint, quickstart, docs map.
- `docs/architecture/` — current architecture/design truth and hard project boundaries (one file or many; agents choose scope).
- `docs/contracts/*` — stable behavior, safety, data, or integration contracts.
- `docs/features/*` — accepted feature behavior.

## Loose docs

Use loose docs for non-authoritative material:

- `docs/ideas/*` — raw ideas, not roadmap.
- `docs/reports/*` — implementation reports, audit notes, migration notes.
- `docs/debug/*` — project-specific debug/forensics notes.

## Rules

- Do not put speculative ideas into README or AGENTS.
- Do not create docs that have no owner or trigger.
- Do not force every project to have every doc.
- Project-specific docs remain child truth.

## Topic docs (optional)

Some projects benefit from small, task-specific “topic docs” that keep `AGENTS.md` short.

Rules:

- Do not create topic docs blindly. Create them only when they reduce `AGENTS.md` bloat or prevent repeated confusion.
- Topic docs are conditional reads; route to them by **task type** from `AGENTS.md`.
- Prefer `docs/architecture/` + contracts/features for small projects instead of many topic docs.

Suggested topic docs for larger projects (only if needed):

- `docs/architecture/` — architecture/refactor boundaries and module ownership.
- `docs/operations.md` — runtime/deploy/docker/env conventions and gotchas.
- `docs/release.md` — versioning/changelog/tag/release flow.
- `docs/testing.md` — `check`/`lint`/`test`/`typecheck` commands and expectations.
- `docs/logging.md` — logging, diagnostics, and troubleshooting entrypoints.
- `docs/integrations/*` — integration-specific contracts and workflows.

## docs/architecture/ update triggers

Update `docs/architecture/*` when a change affects any of these:

- product intent
- user flow
- major UI behavior
- architecture boundaries
- module responsibilities
- data flow
- integration boundaries
- security/network/storage assumptions
- major non-goals
- accepted design tradeoffs

Do not update `docs/architecture/*` for:

- tiny bug fixes
- internal renames with no architectural meaning
- formatting-only changes
- temporary experiments
- raw ideas
- speculative future features
