# Command Interface

The kernel defines command names and semantics, not implementation.

Native tool configuration remains local:

- `package.json`
- `pyproject.toml`
- `Cargo.toml`
- Docker files
- CI files

For Rust/Cargo projects, use `.vibe/kernel/examples/RUST_PROJECTS.md` for the shared Cargo target directory convention while keeping project-specific Cargo flags local.

For projects that generate `dist/`, use `.vibe/kernel/examples/DIST_ARTIFACTS.md` for the shared distribution artifact directory convention while keeping project-specific build flags local.

For GitHub Release CI/CD, use `.vibe/kernel/examples/GITHUB_RELEASES.md`.

For CLI app install/update/version/runtime layout, use `.vibe/kernel/examples/CLI_APPS.md`.

## Recommended targets

Keep the Makefile command surface stable and predictable across child projects.

Canonical target set (keep the same names, semantics, and *rough* order; omit only when truly not applicable):

1. `make install` — install dependencies
2. `make deps-update` — update dependencies (only if a real update flow exists)
3. `make build` — build artifact
4. `make install-local` — install the locally built app/binary on this machine (installable apps only)
5. `make watch` — development watch mode
6. `make typecheck` — type checking
7. `make lint` — linting if real lint exists
8. `make fmt` — formatting if real formatter exists
9. `make test` — tests if real tests exist
10. `make check` — main verification command (the “verify this repo” entrypoint)
11. `make run` — local run
12. `make release` — prepare a release
13. `make release-tag` — create only the release tag if a manual escape hatch is useful
14. `make release-push` — push the release commit and tags
15. `make release-publish` — build/publish release artifacts for CI/CD or perform a documented publish handoff
16. `make vibe-kernel-path` — print the current kernel source path (`.vibe/KERNEL_SOURCE`)
17. `make vibe-kernel-set` — set/update `.vibe/KERNEL_SOURCE` (interactive prompt, or pass `KERNEL=/abs/path/to/vibecoding-kernel`)
18. `make vibe-pull` — refresh `.vibe/kernel/*.md` from the parent kernel

## Rules

- Do not create fake passing targets.
- Treat `Makefile` as the repo’s **service command surface**: routine actions (install, lint, fmt, test, build, run, dependency updates) should be runnable via `make ...`.
- Prefer invoking `make <target>` in docs/instructions instead of raw tool commands (`npm`, `uv`, `cargo`, etc.). Make wraps the native tooling for the current repo.
- Do not create fake release targets. A project is not fully standardized until GitHub Release CI/CD is implemented.
- Do not perform version bumps, tags, publishing, or deployments without explicit user approval.
- Use the Makefile release interface (`make release`, then `make release-push`; CI/CD publishes the GitHub Release) instead of ad-hoc commands.
- `make check` should be the stable “verify this repo” command.
- Makefile should wrap native tools, not replace them.
- Keep public commands plain. Ordinary use should not require flags, environment variables, or extra arguments.
- When a human decision is required, prompt inside the command instead of asking the user to remember a variable.

## Makefile shape

Prefer one clean Makefile as the repo service panel:

- Keep common commands discoverable in one file unless the implementation is genuinely too large.
- Use small helper targets for repeated checks and native-tool orchestration.
- Keep variables for internal defaults and project-specific operations; do not make routine user commands depend on remembering variables.
- Let native scripts do complex stack-specific work when needed, but keep the public command name stable.

## Release command contract

The normal release interface is exact and flagless:

- `make release` prompts for the target `MAJOR.MINOR.PATCH` version.
- The entered version is the source of truth. Do not calculate patch/minor/major versions for the normal path.
- `make release` validates the version, refuses an existing `vMAJOR.MINOR.PATCH` tag, updates native version files and lock/release metadata, updates `CHANGELOG.md` if the repo uses one, creates a dedicated release commit, and creates an annotated tag.
- `make release` does not deploy by itself.
- `make release-push` pushes `main` and tags, normally `git push origin main --follow-tags`.
- GitHub Release CI/CD publishes the release from the pushed tag.
- `make release-publish` is the CI/CD-facing artifact build/publish target or a documented manual publish handoff when the project needs one.
- Lower-level targets such as `release-tag` may exist as implementation pieces or manual escape hatches, but docs and agents should present `make release` as the normal command.

## Git hook gates (optional)

The kernel can provide optional lint gates via git hooks.

`make vibe-pull` installs/updates kernel-managed hook scripts under `.githooks/` without deleting other hook files:

- it only overwrites hook scripts that contain the `VIBE:KERNEL_MANAGED_HOOK` sentinel;
- otherwise it leaves existing hook scripts unchanged and prints a warning.

To avoid breaking existing hook setups, `make vibe-pull` enables hooks in an append-only way:

- if `core.hooksPath` is unset and the repo does not already have non-sample hooks in `.git/hooks`, it sets `core.hooksPath` to `.githooks`;
- in all other cases it leaves git hook configuration unchanged and prints a warning.

By default the kernel-managed lint gate runs `make lint` on `git commit` and `git push` (set `VIBE_SKIP_LINT_HOOKS=1` to bypass in an emergency).

If `make lint` is still a skeleton placeholder (prints `TODO: implement lint only if real lint exists.`), the hook will **not** block commits/pushes.
