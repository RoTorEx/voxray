# Operating Rules

These rules apply to normal work in every child project.

## Priority

1. Current user instruction.
2. Explicit safety and security boundaries.
3. Local business and product truth.
4. These shared operating rules.
5. General best practices.

## Context

- Read the root `AGENTS.md` and follow its task routing.
- Read only the source, tests, and local documentation relevant to the task.
- For product behavior or domain logic, read `BUSINESS.md` and the relevant
  `business/*.md` module before editing.
- Do not read every document or business module by default.

## Preserve user knowledge

User explanations, corrections, and clarifications are durable project input
unless the user explicitly says they are temporary.

In the same task, preserve them in the correct source of truth:

- business purpose, terminology, rules, constants, and rationale:
  `BUSINESS.md` or the relevant `business/*.md`;
- local agent behavior or workflow: the local section of `AGENTS.md`;
- commands and usage: `Makefile` and `README.md`;
- architecture and technical boundaries: the relevant local architecture doc;
- observable behavior: code, tests, and changelog when notable.

Correct outdated text instead of appending a conflicting rule. Synthesize the
durable rule and its rationale; do not paste raw conversation into project docs.
When a correction changes behavior, update its protecting tests where practical.

After repairing local truth, read `EVOLUTION.md` when the correction exposes a
reusable workflow problem, a conflict in instructions, or a possible kernel
improvement. Kernel feedback must never block completion of the local task.

## Task cycle

1. Run `git status --short` before editing.
2. If the worktree is clean and the current branch has an upstream, run
   `git pull --ff-only`. If it is dirty, do not pull automatically.
3. Preserve unrelated user changes and keep the task scope narrow.
4. Make the smallest coherent change that completes the task.
5. Run `make check`. If the project has executable code but no real
   `make check`, add one using the project's native tools.
6. Update durable documentation and `CHANGELOG.md` when behavior changed.
7. If the task produced reusable evidence about agent behavior or rules, follow
   the proposal loop in `EVOLUTION.md`.
8. Commit the completed task as one atomic commit.
9. Push the current tracked branch when it has an upstream, unless the user
   said not to push.
10. Report the outcome and checks run.

Do not commit or present incomplete, blocked, or failing work as completed.
Read-only, audit-only, and proposal-only requests do not create commits.

## Changes and Git

- Use `<type>(optional-scope): imperative summary`.
- Common types: `feat`, `fix`, `docs`, `refactor`, `test`, `build`, `chore`.
- One completed task or coherent slice per commit.
- Include only files belonging to the task.
- Never use force push or history-rewriting/destructive Git commands by default.
- Do not stash, reset, discard, or overwrite user changes without permission.

## Make command contract

Every active code project exposes `make check` as its main read-only verification
entrypoint. It must return non-zero when a required check fails and must not
rewrite source files.

Use these names when the capability exists:

- `make install` — install project dependencies;
- `make build` — build the project or distributable artifact;
- `make test` — run tests;
- `make lint` — run lint checks without rewriting files;
- `make fmt` — format files;
- `make run` — run locally;
- `make install-local` — install the built app or binary on this machine;
- `make release` and `make release-push` — follow `RELEASE.md`;
- `make vibe-pull` — refresh the local kernel copy.

Omit commands that do not apply. Do not create fake successful targets or a
forest of failing placeholders. Keep native tool configuration local; Make is
the stable human and agent interface.

For a real release, keep `make release` itself unchanged and enter the version
only at its interactive prompt, as required by `RELEASE.md`.

## Business documentation

`BUSINESS.md` is mandatory and local to the project. It describes purpose,
actors, concepts, business flows, invariants, non-goals, and a map from code
areas to detailed business modules.

Create `business/<module>.md` when a domain has enough independent rules to need
focused context. Split by business domain, not by technical layer.

Document every decision-bearing number: its value, unit, meaning, rationale,
source, change impact, edge cases, and protecting tests. Do not document ordinary
implementation literals as business rules.

## Safety

- Never commit secrets or print them in logs, errors, examples, or reports.
- Treat network, authentication, storage, deployment, and user-data changes as
  explicit boundaries; read local truth before changing them.
- Ask before weakening a documented boundary or performing a release.
- Do not refactor product code merely to satisfy a shared convention.
