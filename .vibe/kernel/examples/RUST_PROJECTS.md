# Rust Projects

Use this file only for Rust/Cargo project work.

Do not read it during non-Rust tasks.

## When to read

Read this file when the user asks to:

- bootstrap or standardize a Rust project;
- edit Rust Makefile targets;
- change Cargo build, check, lint, format, test, or run commands;
- change local binary installation behavior;
- change Cargo workspace layout or build artifact behavior.

## Build artifact rule

Rust projects must keep routine Cargo build artifacts outside the repository.

All Makefile targets that invoke Cargo must export this target directory:

```make
PROJECT_NAME := $(notdir $(CURDIR))
CONSTRUCTION_SIDE := $(HOME)/construction_side
CARGO_TARGET_DIR := $(CONSTRUCTION_SIDE)/$(PROJECT_NAME)/target
export CARGO_TARGET_DIR
```

The intended path is:

```text
~/construction_side/<project_name>/target
```

Use the repository directory basename as `<project_name>` unless the child project already has one stable local name documented in its Makefile.

## Local binary install rule

Rust projects that produce an installable binary must expose:

```text
make install-local
```

`make install-local` installs the locally built release binary on this machine. It is for developer/local machine installation, not dependency installation.

Use this Makefile shape:

```make
PROJECT_NAME := $(notdir $(CURDIR))
BIN_NAME := $(PROJECT_NAME)
INSTALL_HOME_KIND ?= x-cli
INSTALL_DIR ?= $(HOME)/.$(INSTALL_HOME_KIND)-$(PROJECT_NAME)
```

Rules:

- choose `INSTALL_HOME_KIND` locally and document it; CLI tools usually use `x-cli`, while worker-style binaries may use a project-owned kind such as `x-worker`;
- for a worker named `figma-render-bridge`, the expected default would be `$(HOME)/.x-worker-figma-render-bridge`;
- install the executable at `$(INSTALL_DIR)/$(BIN_NAME)` for single-binary apps unless the project has multiple binaries and documents a `bin/` subdirectory;
- support `INSTALL_DIR=/absolute/path make install-local` for tests and one-off installs;
- install through a temporary file and atomic rename where possible;
- make the installed executable runnable from the shell when humans invoke it directly, usually by adding an idempotent PATH block for `$(INSTALL_DIR)` or the documented binary subdirectory;
- keep runtime state, caches, logs, and update downloads under the app home, not inside the repository;
- do not install routine binaries into `.cargo/bin`, `/usr/local/bin`, `.local/bin`, or the project repo by default.

## Command rules

- Expose routine Cargo work through plain Make targets: `make build`, `make test`, `make lint`, `make fmt`, `make check`, and `make run` when applicable.
- For installable Rust binaries, expose `make install-local`.
- Do not ask humans or agents to remember `CARGO_TARGET_DIR=...` on the command line.
- Do not use project-local `target/` for routine commands.
- Keep `target/` in `.gitignore` anyway, because direct ad-hoc Cargo commands can still create it.
- Prefer Makefile-owned variables over shell-profile, direnv, or global Cargo config for this rule.
- Do not add `.cargo/config.toml` only to set `target-dir` unless the child project already uses Cargo config for other real project needs.
- Use one shared target directory per repository or Cargo workspace. Do not create per-crate target directories inside a workspace unless the project has separate independent workspaces.

## Makefile example

This is the expected shape for a Rust child project. Adapt native Cargo flags locally, but keep the target directory rule and plain public targets.

```make
.PHONY: cargo-target-dir build test lint fmt check run install-local

PROJECT_NAME := $(notdir $(CURDIR))
BIN_NAME := $(PROJECT_NAME)
CONSTRUCTION_SIDE := $(HOME)/construction_side
CARGO_TARGET_DIR := $(CONSTRUCTION_SIDE)/$(PROJECT_NAME)/target
INSTALL_HOME_KIND ?= x-cli
INSTALL_DIR ?= $(HOME)/.$(INSTALL_HOME_KIND)-$(PROJECT_NAME)
export CARGO_TARGET_DIR

cargo-target-dir:
	@mkdir -p "$(CARGO_TARGET_DIR)"

build: cargo-target-dir
	cargo build --release

test: cargo-target-dir
	cargo test

lint: cargo-target-dir
	cargo clippy --all-targets --all-features -- -D warnings

fmt:
	cargo fmt --all

check: fmt lint test

run: cargo-target-dir
	cargo run

install-local: build
	@mkdir -p "$(INSTALL_DIR)"
	@tmp="$(INSTALL_DIR)/.$(BIN_NAME).tmp.$$$$"; \
	trap 'rm -f "$$tmp"' EXIT HUP INT TERM; \
	cp "$(CARGO_TARGET_DIR)/release/$(BIN_NAME)" "$$tmp"; \
	chmod 0755 "$$tmp"; \
	mv -f "$$tmp" "$(INSTALL_DIR)/$(BIN_NAME)"; \
	trap - EXIT HUP INT TERM; \
	printf "Installed %s\n" "$(INSTALL_DIR)/$(BIN_NAME)"
```

## Standardization checklist

For an existing Rust child project:

1. Read `.vibe/kernel/COMMAND_INTERFACE.md` and this file.
2. Update `Makefile` so every Cargo-invoking target receives the exported `CARGO_TARGET_DIR`.
3. If the project builds an installable binary, add or repair `make install-local` with a documented `INSTALL_DIR` default.
4. Keep public docs on `make ...` commands, not raw Cargo commands.
5. Keep `target/` ignored in `.gitignore`.
6. Run `make check` or the closest real verification command.
