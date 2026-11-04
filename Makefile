# AI-RUMIN Build & Distribution Makefile
# High-performance Rust MCP code-memory engine with Python semantic sidecar

NAME       := ai-rumin
VERSION    := $(shell grep '^version = ' Cargo.toml | head -n 1 | cut -d '"' -f 2)
CARGO      ?= cargo
PREFIX     ?= /usr/local
BINDIR     ?= $(PREFIX)/bin
DIST_DIR   := dist
MACOS_SDK  ?= $(HOME)/.local/share/macos-sdk/MacOSX14.5.sdk
ZSTD_LEVEL ?= 21
ARCHIVE    := ai-rumin-source.tar.zst

.PHONY: all build release test check dist dist-linux dist-macos install installer pack clean help

all: release

help:
	@echo "AI-RUMIN Build System ($(VERSION))"
	@echo ""
	@echo "Available targets:"
	@echo "  build         Build debug binary (cargo build)"
	@echo "  release       Build optimized release binary (cargo build --release)"
	@echo "  test          Run full test suite (cargo test)"
	@echo "  check         Run static cargo checks (cargo check)"
	@echo "  dist-linux    Build Linux x86_64 release binary -> $(DIST_DIR)/ai-rumin-linux-x86_64"
	@echo "  dist-macos    Cross-compile Apple Silicon arm64 binary -> $(DIST_DIR)/ai-rumin-apple-m5-arm64"
	@echo "  dist          Build all distributable binaries (Linux + macOS)"
	@echo "  installer     Regenerate standalone install.sh with embedded payload"
	@echo "  install       Run full system installer (binary, MCP, schemas, rules, hooks)"
	@echo "  pack          Package source code to $(ARCHIVE) with zstd level $(ZSTD_LEVEL)"
	@echo "  clean         Remove build artifacts and temporary files"
	@echo ""

build:
	$(CARGO) build

release:
	$(CARGO) build --release

test:
	$(CARGO) test

check:
	$(CARGO) check

$(DIST_DIR):
	mkdir -p $(DIST_DIR)

dist-linux: $(DIST_DIR) release
	cp target/release/$(NAME) $(DIST_DIR)/ai-rumin-linux-x86_64
	@echo "Linux binary packaged: $(DIST_DIR)/ai-rumin-linux-x86_64"

dist-macos: $(DIST_DIR)
	@if [ -d "$(MACOS_SDK)" ]; then \
		SDKROOT="$(MACOS_SDK)" $(CARGO) zigbuild --release --target aarch64-apple-darwin; \
	else \
		$(CARGO) zigbuild --release --target aarch64-apple-darwin; \
	fi
	cp target/aarch64-apple-darwin/release/$(NAME) $(DIST_DIR)/ai-rumin-apple-m5-arm64
	@echo "macOS Apple Silicon binary packaged: $(DIST_DIR)/ai-rumin-apple-m5-arm64"

dist: dist-linux dist-macos

installer:
	python3 scripts/build_installer.py

install: release installer
	@./install.sh --yes $(if $(PREFIX),--prefix $(PREFIX),)

pack:
	@echo "Packing ai-rumin source code into $(ARCHIVE) with zstd -$(ZSTD_LEVEL) (ultra)..."
	tar --exclude="./target" \
	    --exclude="./.git" \
	    --exclude="./dist" \
	    --exclude="./.rumin" \
	    --exclude="./.aiedit" \
	    --exclude="./.aimem" \
	    --exclude="**/__pycache__" \
	    --exclude="**/*.pyc" \
	    --exclude="*.sock" \
	    --exclude="*.tar.zst*" \
	    -cf - . | zstd -$(ZSTD_LEVEL) --ultra -T0 -o /tmp/$(ARCHIVE) && mv /tmp/$(ARCHIVE) ./$(ARCHIVE)
	@echo "Successfully created $(ARCHIVE)"
	@ls -lh $(ARCHIVE)

clean:
	$(CARGO) clean
	rm -rf $(DIST_DIR) $(ARCHIVE)
	find . -type d -name "__pycache__" -exec rm -rf {} +
	find . -type f -name "*.pyc" -delete
