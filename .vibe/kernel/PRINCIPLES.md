# Principles

The kernel centralizes reusable workflow rules, not product reality.

## Core principles

- Preserve local project reality.
- Extract before designing.
- Centralize only what repeats or clearly should be shared.
- Keep child projects flexible.
- Prefer small, reversible changes.
- Do not refactor product code to satisfy the kernel.
- Distinguish stable contracts from ideas and speculative notes.
- Child projects keep full local copies of kernel instructions under `.vibe/kernel/*.md` (do not edit).
- Keep the repo root minimal; put most docs under `docs/` and route to them by task.

## Non-goals

- This kernel does not own product architecture.
- This kernel does not define feature behavior.
- This kernel does not replace project-specific docs.
- This kernel does not force the same file tree on every project.
