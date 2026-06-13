SHELL := /bin/sh

CARGO ?= cargo
TAPLO ?= taplo
BIOME ?= biome
CALL_ARGS ?=

HAS_CARGO_WORKSPACE := $(shell test -f Cargo.toml && printf yes)
HAS_WEB := $(shell find apps -maxdepth 3 \( -name package.json -o -name biome.json -o -name biome.jsonc \) 2>/dev/null | head -1)

.PHONY: help init install-tools fmt fmt-check toml-fmt toml-check fix check clippy test deny machete bacon web-check c clean

help: ## Show available make targets
	@awk 'BEGIN {FS = ":.*## "}; /^[a-zA-Z0-9_.-]+:.*## / {printf "  %-16s %s\n", $$1, $$2}' $(MAKEFILE_LIST)

init: install-tools ## Install local quality tools

install-tools: ## Install optional Rust quality tools
	$(CARGO) install cargo-deny
	$(CARGO) install cargo-machete
	$(CARGO) install taplo-cli
	$(CARGO) install bacon

fmt: ## Format Rust and TOML files
	@if [ "$(HAS_CARGO_WORKSPACE)" = "yes" ]; then $(CARGO) fmt --all; else echo "No Cargo workspace found; skipping rustfmt"; fi
	@if command -v $(TAPLO) >/dev/null 2>&1; then $(TAPLO) fmt; else echo "taplo not installed; skipping TOML format"; fi

fmt-check: ## Check Rust formatting
	@if [ "$(HAS_CARGO_WORKSPACE)" = "yes" ]; then $(CARGO) fmt --all -- --check; else echo "No Cargo workspace found; skipping rustfmt"; fi

toml-fmt: ## Format TOML files
	@if command -v $(TAPLO) >/dev/null 2>&1; then $(TAPLO) fmt; else echo "taplo not installed; run make install-tools"; fi

toml-check: ## Check TOML formatting when taplo is installed
	@if command -v $(TAPLO) >/dev/null 2>&1; then $(TAPLO) fmt --check; else echo "taplo not installed; skipping TOML check"; fi

fix: ## Apply safe Rust compiler fixes
	@if [ "$(HAS_CARGO_WORKSPACE)" = "yes" ]; then $(CARGO) fix --workspace --all-targets --all-features --allow-dirty; else echo "No Cargo workspace found; skipping cargo fix"; fi

check: ## Run cargo check
	@if [ "$(HAS_CARGO_WORKSPACE)" = "yes" ]; then $(CARGO) check --locked --workspace --all-targets --all-features; else echo "No Cargo workspace found; skipping cargo check"; fi

clippy: ## Run strict Rust linting
	@if [ "$(HAS_CARGO_WORKSPACE)" = "yes" ]; then $(CARGO) clippy --locked --workspace --all-targets --all-features -- -D warnings; else echo "No Cargo workspace found; skipping clippy"; fi

test: ## Run Rust tests
	@if [ "$(HAS_CARGO_WORKSPACE)" = "yes" ]; then $(CARGO) test --locked --workspace --all-targets --all-features $(CALL_ARGS); else echo "No Cargo workspace found; skipping tests"; fi

deny: ## Run dependency and security policy checks when cargo-deny is installed
	@if command -v cargo-deny >/dev/null 2>&1; then cargo deny check; else echo "cargo-deny not installed; skipping dependency audit"; fi

machete: ## Detect unused Rust dependencies when cargo-machete is installed
	@if command -v cargo-machete >/dev/null 2>&1; then cargo machete; else echo "cargo-machete not installed; skipping dead dependency check"; fi

bacon: ## Run the fast Rust feedback loop when bacon is installed
	@if command -v bacon >/dev/null 2>&1; then bacon $(CALL_ARGS); else echo "bacon not installed; run make install-tools"; fi

web-check: ## Run Biome only when web-style project files exist
	@if [ -n "$(HAS_WEB)" ] && command -v $(BIOME) >/dev/null 2>&1; then $(BIOME) check .; else echo "No Biome-managed web app found; skipping web check"; fi

c: fmt-check toml-check check clippy test deny machete web-check ## Run the full local quality gate

clean: ## Remove local build artifacts
	@if [ "$(HAS_CARGO_WORKSPACE)" = "yes" ]; then $(CARGO) clean; else echo "No Cargo workspace found; skipping cargo clean"; fi
