# AGENTS.md

This project uses local copies of the Vibecoding Kernel instructions under `.vibe/kernel/`.

This file should be a router, not an encyclopedia.

Do not read the parent kernel repo outside this repository during normal work.

## Kernel routing

<!-- VIBE:KERNEL_ROUTING_START -->

This project uses committed local copies of the Vibecoding Kernel.

- Always read `.vibe/kernel/OPERATING.md` before normal project work.
- For version, tag, publish, or release work, also read
  `.vibe/kernel/RELEASE.md`.
- For user corrections, instruction conflicts, reusable agent-workflow lessons,
  kernel proposals, or a newly pulled kernel version, also read
  `.vibe/kernel/EVOLUTION.md`.
- For product behavior or domain logic, read `BUSINESS.md` and only the relevant
  module from `business/*`.
- Do not edit `.vibe/kernel/*` manually or read the parent kernel during normal
  work. Refresh the local copy with `make vibe-pull`.
- When a reusable workflow improvement belongs in the parent, run
  `make vibe-propose`; it appends a reviewable proposal without changing rules.

<!-- VIBE:KERNEL_ROUTING_END -->

## Local routing

Read these only when the task requires them:

- `TASK.md` — task queue (process tasks in order; remove completed task sections).
- `CHANGELOG.md` — release progress (update on releases).
- `README.md` — quickstart, docs map, current product surface.
- `docs/architecture/*` — design truth (agents choose scope; keep schemas/diagrams/relationships up to date).
- `docs/contracts/*` — stable contracts and boundaries.
- `docs/features/*` — accepted feature behavior.
- `docs/ideas/*` — idea triage only.
- `docs/reports/*` — reports/history only when relevant.

## Release execution

When the user explicitly requests a release, start `make release` unchanged,
enter the requested version through the running command's stdin, then run
`make release-push`. Do not pipe the version into the shell command, repeat the
script's checks manually, or wait for GitHub Actions unless the user asks.

## Rule priority

1. Current user instruction
2. Hard safety/security/boundary constraints
3. Project truth docs
4. `.vibe/kernel/*.md`
5. General best practices
