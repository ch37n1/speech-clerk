SHELL := /bin/sh

CARGO ?= cargo
TAPLO ?= taplo
BIOME ?= biome
SWIFT ?= swift
SWIFT_FORMAT ?= xcrun swift-format
SWIFTLINT ?= swiftlint
CALL_ARGS ?=

HAS_CARGO_WORKSPACE := $(shell test -f Cargo.toml && printf yes)
HAS_SWIFT_PACKAGE := $(shell test -f apps/macos/Package.swift && printf yes)
HAS_WEB := $(shell find apps -maxdepth 3 \( -name package.json -o -name biome.json -o -name biome.jsonc \) 2>/dev/null | head -1)
SWIFT_PACKAGE_PATH := apps/macos
SWIFT_SCRATCH_PATH := $(CURDIR)/.build/swiftpm/macos
SWIFT_MODULE_CACHE_PATH := $(CURDIR)/.build/swiftpm/clang-module-cache
SWIFT_SOURCES := $(shell find apps/macos \
	-path '*/.build/*' -prune -o \
	-path '*/.swiftpm/*' -prune -o \
	-path '*/Generated/UniFFI/*' -prune -o \
	-name '*.swift' -print 2>/dev/null)

.PHONY: help init install-tools install-swift-tools fmt fmt-check toml-fmt toml-check swift-fmt swift-fmt-check swift-lint swift-build swift-test swift-check macos-agent-build macos-agent-launch macos-agent-smoke macos-agent-screenshot macos-agent-stop fix check clippy test deny machete bacon web-check c clean

help: ## Show available make targets
	@awk 'BEGIN {FS = ":.*## "}; /^[a-zA-Z0-9_.-]+:.*## / {printf "  %-16s %s\n", $$1, $$2}' $(MAKEFILE_LIST)

init: install-tools ## Install local quality tools

install-tools: ## Install optional Rust quality tools
	$(CARGO) install cargo-deny
	$(CARGO) install cargo-machete
	$(CARGO) install taplo-cli
	$(CARGO) install bacon

install-swift-tools: ## Install optional Swift quality tools when Homebrew is available
	@if command -v brew >/dev/null 2>&1; then brew install swiftlint; else echo "Homebrew not installed; install SwiftLint manually if desired"; fi

fmt: ## Format Rust, TOML, and Swift files
	@if [ "$(HAS_CARGO_WORKSPACE)" = "yes" ]; then $(CARGO) fmt --all; else echo "No Cargo workspace found; skipping rustfmt"; fi
	@if command -v $(TAPLO) >/dev/null 2>&1; then $(TAPLO) fmt; else echo "taplo not installed; skipping TOML format"; fi
	@$(MAKE) swift-fmt

fmt-check: ## Check Rust and Swift formatting
	@if [ "$(HAS_CARGO_WORKSPACE)" = "yes" ]; then $(CARGO) fmt --all -- --check; else echo "No Cargo workspace found; skipping rustfmt"; fi
	@$(MAKE) swift-fmt-check

toml-fmt: ## Format TOML files
	@if command -v $(TAPLO) >/dev/null 2>&1; then $(TAPLO) fmt; else echo "taplo not installed; run make install-tools"; fi

toml-check: ## Check TOML formatting when taplo is installed
	@if command -v $(TAPLO) >/dev/null 2>&1; then $(TAPLO) fmt --check; else echo "taplo not installed; skipping TOML check"; fi

swift-fmt: ## Format first-party Swift files
	@if [ "$(HAS_SWIFT_PACKAGE)" != "yes" ]; then echo "No Swift package found; skipping Swift format"; \
	elif [ -z "$(SWIFT_SOURCES)" ]; then echo "No first-party Swift sources found; skipping Swift format"; \
	elif $(SWIFT_FORMAT) --version >/dev/null 2>&1; then $(SWIFT_FORMAT) format --configuration .swift-format -i $(SWIFT_SOURCES); \
	else echo "swift-format not found; install Xcode 16+ or set SWIFT_FORMAT"; exit 1; fi

swift-fmt-check: ## Check first-party Swift formatting
	@if [ "$(HAS_SWIFT_PACKAGE)" != "yes" ]; then echo "No Swift package found; skipping Swift format check"; \
	elif [ -z "$(SWIFT_SOURCES)" ]; then echo "No first-party Swift sources found; skipping Swift format check"; \
	elif $(SWIFT_FORMAT) --version >/dev/null 2>&1; then $(SWIFT_FORMAT) lint --strict --configuration .swift-format $(SWIFT_SOURCES); \
	else echo "swift-format not found; install Xcode 16+ or set SWIFT_FORMAT"; exit 1; fi

swift-lint: ## Run SwiftLint when installed
	@if [ "$(HAS_SWIFT_PACKAGE)" != "yes" ]; then echo "No Swift package found; skipping SwiftLint"; \
	elif command -v $(SWIFTLINT) >/dev/null 2>&1; then $(SWIFTLINT) lint --config .swiftlint.yml --strict; \
	else echo "SwiftLint not installed; skipping SwiftLint"; fi

swift-build: ## Build the macOS Swift package
	@if [ "$(HAS_SWIFT_PACKAGE)" = "yes" ]; then \
		$(CARGO) build -p ffi && \
		CLANG_MODULE_CACHE_PATH=$(SWIFT_MODULE_CACHE_PATH) $(SWIFT) build --package-path $(SWIFT_PACKAGE_PATH) --scratch-path $(SWIFT_SCRATCH_PATH); \
	else echo "No Swift package found; skipping Swift build"; fi

swift-test: ## Test the macOS Swift package
	@if [ "$(HAS_SWIFT_PACKAGE)" = "yes" ]; then \
		CLANG_MODULE_CACHE_PATH=$(SWIFT_MODULE_CACHE_PATH) $(SWIFT) run --package-path $(SWIFT_PACKAGE_PATH) --scratch-path $(SWIFT_SCRATCH_PATH) SpeechClerkMacUnitTests; \
	else echo "No Swift package found; skipping Swift tests"; fi

swift-check: swift-fmt-check swift-lint swift-build swift-test ## Run the Swift quality gate

macos-agent-build: ## Build the macOS app for agent manual testing
	@sh tools/macos-agent.sh build

macos-agent-launch: ## Launch the macOS app for agent manual testing
	@sh tools/macos-agent.sh launch

macos-agent-smoke: ## Launch, inspect, and screenshot the macOS app
	@sh tools/macos-agent.sh smoke

macos-agent-screenshot: ## Capture a macOS agent testing screenshot
	@sh tools/macos-agent.sh screenshot

macos-agent-stop: ## Stop the macOS app launched for agent testing
	@sh tools/macos-agent.sh stop

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

c: fmt-check toml-check check clippy test deny machete swift-check web-check ## Run the full local quality gate

clean: ## Remove local build artifacts
	@if [ "$(HAS_CARGO_WORKSPACE)" = "yes" ]; then $(CARGO) clean; else echo "No Cargo workspace found; skipping cargo clean"; fi
