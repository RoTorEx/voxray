.PHONY: install deps-update cargo-target-dir dist-dir build typecheck lint fmt test check run version install-local release release-tag release-push release-publish vibe-kernel-path vibe-kernel-set vibe-pull

PROJECT_NAME := $(notdir $(CURDIR))
BIN_NAME := voxray
CONSTRUCTION_SIDE := $(HOME)/construction_side
CARGO_TARGET_DIR := $(CONSTRUCTION_SIDE)/$(PROJECT_NAME)/target
DIST_DIR ?= $(CONSTRUCTION_SIDE)/$(PROJECT_NAME)/dist
INSTALL_DIR ?= $(HOME)/.x-cli-$(PROJECT_NAME)
export CARGO_TARGET_DIR
export DIST_DIR

install:
	cargo fetch --locked

deps-update:
	cargo update

cargo-target-dir:
	@mkdir -p "$(CARGO_TARGET_DIR)"

dist-dir:
	@mkdir -p "$(DIST_DIR)"

build: cargo-target-dir
	cargo build --release --locked

typecheck: cargo-target-dir
	cargo check --locked --all-targets

lint: cargo-target-dir
	cargo clippy --locked --all-targets --all-features -- -D warnings

fmt:
	cargo fmt --all

test: cargo-target-dir
	cargo test --locked --all-targets

check: cargo-target-dir
	cargo fmt --all -- --check
	cargo check --locked --all-targets
	cargo clippy --locked --all-targets --all-features -- -D warnings
	cargo test --locked --all-targets
	cargo build --release --locked

run: cargo-target-dir
	cargo run --locked --

version: cargo-target-dir
	cargo run --locked -- --version

install-local: build
	sh scripts/install-local.sh "$(CARGO_TARGET_DIR)/release/$(BIN_NAME)" "$(INSTALL_DIR)"

release:
	sh scripts/release.sh

release-tag:
	@branch="$$(git branch --show-current)"; \
	test "$$branch" = "main" || { echo "ERROR: release tag must be created from main, not $$branch" >&2; exit 1; }; \
	test -z "$$(git status --porcelain)" || { echo "ERROR: commit or remove local changes before tagging" >&2; exit 1; }; \
	version="$$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)"; \
	test -n "$$version" || { echo "ERROR: could not read version from Cargo.toml" >&2; exit 1; }; \
	! git rev-parse --verify "refs/tags/v$$version" >/dev/null 2>&1 || { echo "ERROR: tag v$$version already exists" >&2; exit 1; }; \
	git tag -a "v$$version" -m "Release $$version"; \
	echo "Created annotated tag v$$version"

release-push:
	@set -eu; \
	branch="$$(git branch --show-current)"; \
	test "$$branch" = "main" || { echo "ERROR: releases must be pushed from main, not $$branch." >&2; exit 1; }; \
	version="$$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)"; \
	tag="v$$version"; \
	git rev-parse -q --verify "refs/tags/$$tag" >/dev/null || { echo "ERROR: missing $$tag. Run make release." >&2; exit 1; }; \
	git push origin main --follow-tags

release-publish: cargo-target-dir dist-dir
	@set -e; \
	test "$$(uname -s)" = "Darwin" || { echo "ERROR: release artifacts are supported on macOS only" >&2; exit 1; }; \
	case "$$(uname -m)" in arm64) arch="aarch64" ;; x86_64) arch="x86_64" ;; *) echo "ERROR: unsupported architecture $$(uname -m)" >&2; exit 1 ;; esac; \
	archive="$(BIN_NAME)-macos-$$arch.tar.gz"; \
	cargo build --release --locked; \
	tar -czf "$(DIST_DIR)/$$archive" -C "$(CARGO_TARGET_DIR)/release" "$(BIN_NAME)" -C "$(CURDIR)" config.example.toml; \
	cd "$(DIST_DIR)"; \
	shasum -a 256 "$$archive" > "$$archive.sha256"; \
	echo "Created $(DIST_DIR)/$$archive"

vibe-kernel-path:
	@test -f .vibe/KERNEL_SOURCE || { echo "Missing .vibe/KERNEL_SOURCE" >&2; exit 1; }
	@sed -n '1p' .vibe/KERNEL_SOURCE

vibe-kernel-set:
	@mkdir -p .vibe; \
	if [ -n "$(KERNEL)" ]; then kernel_root="$(KERNEL)"; else printf "Kernel path: "; read -r kernel_root; fi; \
	test -f "$$kernel_root/tools/vibe-pull" || { echo "Invalid kernel path" >&2; exit 1; }; \
	printf "%s\n" "$$kernel_root" > .vibe/KERNEL_SOURCE

vibe-pull:
	@test -f .vibe/KERNEL_SOURCE || { echo "Missing .vibe/KERNEL_SOURCE" >&2; exit 1; }
	@kernel_root="$$(sed -n '1p' .vibe/KERNEL_SOURCE)"; python3 "$$kernel_root/tools/vibe-pull" .
