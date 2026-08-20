# Change Conventions

## Commits

Use compact conventional commits:

```text
<type>(optional-scope): imperative summary
```

Common types:

- `feat`
- `fix`
- `docs`
- `refactor`
- `test`
- `build`
- `chore`
- `ui`
- `style`

## Atomicity

- One completed slice per commit.
- Do not mix unrelated changes.
- Keep release/version bump commits separate when possible.

## Changelog (if present)

If the repo uses `CHANGELOG.md`:

- Keep new notable changes under `## [Unreleased]` as they land.
- On a release, move the Unreleased entries under a new version header like:
  - `## [X.Y.Z] - YYYY-MM-DD`
  leaving `## [Unreleased]` empty for new work.
- Update the changelog deterministically (no speculation; only what actually shipped).

## Tags and releases

Default tag format:

```text
vX.Y.Z
```

All child projects must implement a CI/CD mechanism that publishes releases to GitHub Releases.

Release CI/CD is project-owned and should follow `.vibe/kernel/examples/GITHUB_RELEASES.md`.

Recommended flow (language-agnostic; Makefile wraps native tooling):

1. Commit all normal work first (clean working tree).
2. Ask for explicit release approval. If the user has not already provided the target version, `make release` prompts for the exact `MAJOR.MINOR.PATCH` version.
3. Run the repo release command:
   - `make release`
4. `make release` should validate and apply the exact version, update native version files, update lock/release metadata when required, move `CHANGELOG.md` Unreleased entries when present, create a dedicated release commit, and create an annotated `vX.Y.Z` tag.
5. Push the release commit and tag:
   - `make release-push`
6. CI/CD publishes the GitHub Release from the pushed tag.
7. Publish/deploy non-GitHub artifacts only if the project has a separate publish/deploy handoff:
   - `make release-publish`

Notes:
- Ask for explicit user approval before any version bump, tagging, or publishing.
- Do not use patch/minor/major calculations, flags, or environment variables as the normal human release interface.
- Do not publish GitHub Releases from a developer machine as the normal path; local commands prepare and trigger CI/CD.
- The release command may have lower-level helpers, but the public path should be plain: `make release`, then `make release-push`.
- Never use destructive Git overrides: no `git push --force*`, no `git reset --hard`.

## Reports

For agent work, prefer a minimal report:

- Changed
- Checks

Include only when relevant:

- Risks
- Docs (only if durable docs changed)
- Parent proposal

Do not create giant reports for small changes.
