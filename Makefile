.PHONY: install deps-update cargo-target-dir build typecheck lint fmt test check run install-local vibe-kernel-path vibe-kernel-set vibe-pull

PROJECT_NAME := $(notdir $(CURDIR))
BIN_NAME := voxray
CONSTRUCTION_SIDE := $(HOME)/construction_side
CARGO_TARGET_DIR := $(CONSTRUCTION_SIDE)/$(PROJECT_NAME)/target
INSTALL_DIR ?= $(HOME)/.x-cli-$(PROJECT_NAME)
export CARGO_TARGET_DIR

install:
	cargo fetch --locked

deps-update:
	cargo update

cargo-target-dir:
	@mkdir -p "$(CARGO_TARGET_DIR)"

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

install-local: build
	sh scripts/install-local.sh "$(CARGO_TARGET_DIR)/release/$(BIN_NAME)" "$(INSTALL_DIR)"

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
