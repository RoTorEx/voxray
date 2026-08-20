# Setup

Use this directive only for one-time bootstrap, migration, or standardization tasks.

Do not read it during normal feature work, bug fixes, or routine releases.

## When to read

Read this file when the user asks to:

- migrate a repo to Vibecoding standards;
- standardize an existing repo;
- bootstrap or repair kernel setup;
- audit whether a child repo follows the kernel command/routing surface.

## Checklist

Keep the setup pass small and local. Preserve project reality.

1. Confirm `.vibe/KERNEL_SOURCE` exists, contains one absolute parent-kernel path, and is ignored by git.
2. Run or repair `make vibe-pull` so `.vibe/kernel/*.md` exists as local read-only copies.
3. Keep `AGENTS.md` as a router. It should contain the managed `VIBE:KERNEL_ROUTING` block and avoid duplicating the same kernel routing lists elsewhere in the file.
4. Preserve child truth. Do not move product-specific architecture, deployment, feature, or safety rules into the kernel.
5. Ensure `Makefile` exposes the common service surface that applies to the repo: install, build, install-local for installable apps, watch, typecheck, lint, fmt, test, check, run, release, and kernel sync targets.
6. Read `.vibe/kernel/examples/GITHUB_RELEASES.md` and ensure GitHub Release CI/CD exists.
7. For CLI apps, read `.vibe/kernel/examples/CLI_APPS.md` and ensure one-command install, `-V`, `update`, PATH setup, and `~/.x-cli-<project-name>` layout exist.
8. For repos that generate `dist/`, read `.vibe/kernel/examples/DIST_ARTIFACTS.md` and ensure routine distribution output goes under `~/construction_side/<project_name>/dist`.
9. For Rust/Cargo repos, read `.vibe/kernel/examples/RUST_PROJECTS.md` and ensure routine Cargo targets place build artifacts under `~/construction_side/<project_name>/target`; if the repo builds an installable binary, ensure `make install-local` installs to a documented `INSTALL_DIR` such as `~/.x-cli-<project-name>` or a project-owned equivalent.
10. Remove or keep failing placeholders for unsupported commands; never create fake passing targets.
11. Make the public release path plain: `make release` prompts for the exact version, `make release-push` pushes `main` and tags, and CI/CD publishes the GitHub Release.
12. If `.githooks/` is used, keep kernel-managed hooks sentinel-based and refresh them through `make vibe-pull`; do not clobber non-kernel hooks.
13. Keep `TASK.md` and `CHANGELOG.md` only when the repo uses them. Avoid half-configured docs that no workflow owns.
14. Run `make check` or the closest existing verification command after setup changes.

## Output

Report only:

- what was standardized;
- what was intentionally left project-specific;
- checks run;
- any parent-kernel proposal appended.
