# AI Workflow

## Default cycle

1. Run the session-start Git check before editing.
2. Inspect the smallest relevant context.
3. State a short plan only when the task is non-trivial.
4. Make minimal, reversible changes.
5. Run the relevant checks.
6. Update docs only when durable behavior, commands, architecture, or workflow changed.
7. Commit and push `main` to `origin` unless the user says otherwise.
8. Report changed files and checks; include risks/docs/parent proposals only when relevant.

## Session-start Git check

At the beginning of every normal editing session:

1. Run `git status --short`.
2. If the worktree is clean, run `git pull --ff-only` before making edits.
3. If the worktree is dirty, do not pull automatically. Inspect/report the local changes and continue only when the task can be completed without hiding or overwriting them.
4. If `git pull --ff-only` fails because the branch diverged, stop and report it. Do not merge, rebase, reset, stash, or force anything unless the user explicitly asks.

In read-only/proposal-only mode, run `git status --short` but do not run `git pull` unless the user explicitly permits updating local files.

## Task queue (optional)

If the repo uses a root task file (`TASK.md`), treat it as the user-owned task queue:

- Read it at the start of the session when the user says “follow TASK.md” (do not assume it exists).
- Process tasks **in order**, one by one.
- When a task is completed, remove its section from `TASK.md` (keep the file as the current remaining queue).
- Do not turn `TASK.md` into a backlog or roadmap; move ideas to `docs/ideas/` instead.

## Agent rules

- Do not perform unrelated refactors.
- Do not invent new architecture unless the task requires it.
- Prefer existing project patterns.
- Ask for approval before weakening boundaries or changing parent rules.
- Ask for explicit approval before creating tags, publishing artifacts, or performing a release.
- For release/version work, prefer the project Makefile release interface. The normal human path is `make release`, which prompts for the exact `MAJOR.MINOR.PATCH` version, then a separate push/publish handoff.
- Always commit and push to `origin` at the end of a directive unless the user says otherwise.
- Default branch policy: assume the repo uses `main`. If the repo clearly uses a different default branch (e.g., `master`), ask before pushing.
- After any commit: push to the default branch (`git push origin main`) unless the user says otherwise.
- After creating a release tag: push commits and tags with `make release-push` if present; otherwise use `git push origin main --follow-tags` unless the user says otherwise.
- Never use history-rewriting or destructive Git commands by default:
  - forbidden: `git push --force`, `git push --force-with-lease`
  - forbidden: `git reset --hard` (use `git revert` or a new corrective commit instead)
- Do not touch files outside the task scope; treat unexpected/untracked files as suspicious and report them, but do not delete them or include them in commits unless explicitly requested.
- Do not create speculative future features in stable docs.
- Put raw ideas into `docs/ideas/`.
- Keep output compact by default: do not restate the prompt; prefer 1 outcome sentence + a small list of facts (changed files, checks, next step).
- Expand only for architecture, security, migration, release, or high-risk changes.

## Read-only / proposal-only mode

When the user says `do not code yet`, `do not change anything`, `audit only`, `proposal only`, or equivalent:

- Do not modify files, commit, push, tag, publish, or deploy.
- Inspect, reason, and report only.
- If the user explicitly asks for a parent-kernel proposal, only append that proposal to `<KERNEL_SOURCE>/PROPOSALS.md`; do not make unrelated edits.
- Resume normal editing only after the user clearly approves implementation.
