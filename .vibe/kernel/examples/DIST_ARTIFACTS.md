# Distribution Artifacts

Use this file only for projects that generate distributable build output such as `dist/`.

Do not read it during tasks that do not touch build output, packaging, deployment artifacts, or generated bundles.

## When to read

Read this file when the user asks to:

- bootstrap or standardize a project that emits `dist/`;
- edit build, package, bundle, export, or deploy-artifact commands;
- change where generated distribution files are written;
- change tool config that controls output directories.

## Build artifact rule

Projects that generate `dist/` output must keep routine distribution artifacts outside the repository.

Makefile targets that generate distribution output should use this directory:

```make
PROJECT_NAME := $(notdir $(CURDIR))
CONSTRUCTION_SIDE := $(HOME)/construction_side
DIST_DIR := $(CONSTRUCTION_SIDE)/$(PROJECT_NAME)/dist
export DIST_DIR
```

The intended path is:

```text
~/construction_side/<project_name>/dist
```

Use the repository directory basename as `<project_name>` unless the child project already has one stable local name documented in its Makefile.

## Command rules

- Expose routine distribution work through plain Make targets such as `make build`, `make release-publish`, or a project-owned package target.
- Do not ask humans or agents to remember `DIST_DIR=...` on the command line.
- Do not write routine generated artifacts to project-local `dist/`.
- Configure the native build tool to write directly to `$(DIST_DIR)`; do not generate into `dist/` and move files afterward.
- Keep `dist/` in `.gitignore` anyway, because direct ad-hoc build commands can still create it.
- Prefer Makefile-owned variables over shell-profile, direnv, or global tool config for this rule.
- Use one shared `DIST_DIR` per repository unless the project has multiple independent deployable packages with documented names.
- Clean generated distribution output by deleting `$(DIST_DIR)`, not by deleting repository-local paths.

## Makefile shape

Use exact native commands for the project stack. The stable part is the Makefile-owned `DIST_DIR` and plain public target.

```make
.PHONY: dist-dir clean-dist build

PROJECT_NAME := $(notdir $(CURDIR))
CONSTRUCTION_SIDE := $(HOME)/construction_side
DIST_DIR := $(CONSTRUCTION_SIDE)/$(PROJECT_NAME)/dist
export DIST_DIR

dist-dir:
	@mkdir -p "$(DIST_DIR)"

clean-dist:
	@test -n "$(DIST_DIR)"
	@rm -rf "$(DIST_DIR)"

build: dist-dir
	# Replace this line with the project's exact native build command,
	# configured to write directly to "$(DIST_DIR)".
	@false
```

## Exact adapters

Use one of these only when it matches the child project's actual native tool.

Vite:

```make
build: dist-dir
	npm run build -- --outDir "$(DIST_DIR)"
```

TypeScript compiler:

```make
build: dist-dir
	npm exec -- tsc --outDir "$(DIST_DIR)"
```

esbuild:

```make
build: dist-dir
	npm exec -- esbuild src/index.ts --bundle --outdir="$(DIST_DIR)"
```

Rust release copy handoff:

```make
package: dist-dir
	cargo build --release
	cp "$(CARGO_TARGET_DIR)/release/<binary-name>" "$(DIST_DIR)/<binary-name>"
```

Replace placeholder names such as `<binary-name>` before using an adapter. Do not commit placeholder commands that pretend to work.

## Standardization checklist

For an existing child project with generated `dist/` output:

1. Read `.vibe/kernel/COMMAND_INTERFACE.md` and this file.
2. Update `Makefile` so build/package targets use `DIST_DIR`.
3. Update native build config only when the tool cannot accept an output directory from the Makefile command.
4. Keep public docs on `make ...` commands, not raw native commands.
5. Keep `dist/` ignored in `.gitignore`.
6. Run `make check` or the closest real verification command.
