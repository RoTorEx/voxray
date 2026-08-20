# GitHub Releases

Use this file when adding, auditing, or changing project release CI/CD.

Do not read it for ordinary feature work.

## Required release posture

All child projects must implement a CI/CD mechanism that publishes releases to GitHub Releases.

The durable shape is:

1. `make release` prepares the release locally and creates an annotated `vX.Y.Z` tag.
2. `make release-push` pushes `main` and the tag.
3. CI/CD runs from the pushed tag and creates or updates the GitHub Release.
4. Release assets, installers, checksums, and package outputs are uploaded by CI/CD, not by ad-hoc local commands.

Use `.vibe/kernel/CHANGE_CONVENTIONS.md` for version/changelog/tag flow and `.vibe/kernel/COMMAND_INTERFACE.md` for command names.

## GitHub Actions contract

When the project uses GitHub Actions, the release workflow must be concrete and project-owned:

- workflow path: `.github/workflows/release.yml`;
- trigger: pushed tags matching `v*.*.*`;
- permissions: `contents: write`;
- checkout the tagged commit;
- build release artifacts from the tagged source;
- create the GitHub Release for the exact tag;
- upload all project release assets required by users;
- fail loudly when an expected artifact is missing.

Do not store a personal access token in the repo. Use the workflow-provided GitHub token unless the project has a documented reason to use a scoped secret.

## Workflow shape

This is a shape, not a paste-and-forget workflow. Replace artifact commands with the child project's real release build.

```yaml
name: release

on:
  push:
    tags:
      - "v*.*.*"

permissions:
  contents: write

jobs:
  github-release:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Build release artifacts
        run: make release-publish

      - name: Publish GitHub Release
        env:
          GH_TOKEN: ${{ github.token }}
          TAG: ${{ github.ref_name }}
        run: |
          gh release create "$TAG" \
            --verify-tag \
            --title "$TAG" \
            --generate-notes \
            path/to/release/artifacts/*
```

Before committing a workflow, replace `path/to/release/artifacts/*` with the exact project artifact path. Do not commit placeholder paths.

## Rules

- GitHub Release publication must be repeatable from the pushed tag.
- Do not publish from the developer machine as the normal path.
- Do not create a release workflow that succeeds without uploading required artifacts.
- If `make release-publish` itself creates the GitHub Release, the workflow should call that target instead of duplicating release creation.
- Keep release artifacts outside the repo when generated locally; use `.vibe/kernel/examples/DIST_ARTIFACTS.md` when the project emits `dist/`.
- CLI apps must also satisfy `.vibe/kernel/examples/CLI_APPS.md`.
- README release/install docs must point to the exact install command or release page users should use.

## Standardization checklist

For an existing child project:

1. Add or repair `.github/workflows/release.yml`.
2. Ensure `make release`, `make release-push`, and `make release-publish` have real project behavior.
3. Ensure the workflow triggers on `vX.Y.Z` tags and publishes GitHub Releases.
4. Ensure expected artifacts are built and uploaded by CI/CD.
5. Run `make check` or the closest real verification command.
