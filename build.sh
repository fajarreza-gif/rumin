#!/usr/bin/env bash
# ==============================================================================
# AI-RUMIN Build & Packaging Script
# ==============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "${SCRIPT_DIR}"

APP_NAME="ai-rumin"
VERSION="$(grep '^version = ' Cargo.toml | head -n 1 | cut -d '"' -f 2)"
DIST_DIR="${SCRIPT_DIR}/dist"
ARCHIVE_NAME="ai-rumin-source.tar.zst"
MACOS_SDK="${SDKROOT:-${HOME}/.local/share/macos-sdk/MacOSX14.5.sdk}"

# Color output helpers
green() { printf "\033[1;32m%s\033[0m\n" "$*"; }
yellow() { printf "\033[1;33m%s\033[0m\n" "$*"; }
red() { printf "\033[1;31m%s\033[0m\n" "$*"; }
blue() { printf "\033[1;34m%s\033[0m\n" "$*"; }

show_help() {
    cat <<EOF
ai-rumin build script (v${VERSION})

Usage: ./build.sh [COMMAND]

Commands:
  build         Build debug binary (cargo build)
  release       Build optimized release binary (cargo build --release)
  test          Run all unit and integration tests (cargo test)
  check         Run cargo check
  dist          Build and package distribution binaries for Linux and macOS
  dist-linux    Build and package Linux x86_64 binary
  dist-macos    Cross-compile and package Apple Silicon (arm64) binary
  installer     Regenerate standalone install.sh with embedded payload
  pack          Pack source code into ${ARCHIVE_NAME} with zstd -21
  install       Run full system installer (binary, MCP, schemas, rules, hooks)
  clean         Clean target directory and packaging caches
  help          Display this help message

Environment Variables:
  PREFIX        Target installation directory prefix (default: /usr/local)
  SDKROOT       macOS SDK path for cargo-zigbuild (default: ${HOME}/.local/share/macos-sdk/MacOSX14.5.sdk)
  ZSTD_LEVEL    Compression level for pack command (default: 21)

EOF
}

cmd_build() {
    blue "==> Building ${APP_NAME} (debug)..."
    cargo build
    green "==> Debug build completed: target/debug/${APP_NAME}"
}

cmd_release() {
    blue "==> Building ${APP_NAME} (release)..."
    cargo build --release
    green "==> Release build completed: target/release/${APP_NAME}"
}

cmd_test() {
    blue "==> Running test suite..."
    cargo test
    green "==> All tests passed!"
}

cmd_check() {
    blue "==> Running cargo check..."
    cargo check
    green "==> Check passed!"
}

cmd_dist_linux() {
    cmd_release
    mkdir -p "${DIST_DIR}"
    cp "target/release/${APP_NAME}" "${DIST_DIR}/${APP_NAME}-linux-x86_64"
    green "==> Packaged Linux binary: ${DIST_DIR}/${APP_NAME}-linux-x86_64"
}

cmd_dist_macos() {
    blue "==> Cross-compiling for Apple Silicon (aarch64-apple-darwin)..."
    if [ -d "${MACOS_SDK}" ]; then
        yellow "Using macOS SDK at: ${MACOS_SDK}"
        SDKROOT="${MACOS_SDK}" cargo zigbuild --release --target aarch64-apple-darwin
    else
        yellow "No custom macOS SDK directory found at ${MACOS_SDK}; using default zigbuild environment"
        cargo zigbuild --release --target aarch64-apple-darwin
    fi
    mkdir -p "${DIST_DIR}"
    cp "target/aarch64-apple-darwin/release/${APP_NAME}" "${DIST_DIR}/${APP_NAME}-apple-m5-arm64"
    green "==> Packaged macOS Apple Silicon binary: ${DIST_DIR}/${APP_NAME}-apple-m5-arm64"
}

cmd_dist() {
    cmd_dist_linux
    cmd_dist_macos
    green "==> Distribution binaries ready in ${DIST_DIR}/:"
    ls -lh "${DIST_DIR}"
}

cmd_installer() {
    blue "==> Generating standalone install.sh with embedded payload..."
    python3 scripts/build_installer.py
    green "==> install.sh is up to date."
}

cmd_install() {
    cmd_release
    cmd_installer
    blue "==> Running system installer..."
    shift || true
    ./install.sh --yes "$@"
}

cmd_pack() {
    local level="${ZSTD_LEVEL:-21}"
    if ! command -v zstd >/dev/null 2>&1; then
        red "Error: 'zstd' command not found. Please install zstandard."
        exit 1
    fi
    blue "==> Packing ${APP_NAME} source code into ${ARCHIVE_NAME} (zstd -${level} --ultra)..."
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
        -cf - . | zstd -"${level}" --ultra -T0 -o "/tmp/${ARCHIVE_NAME}" && mv "/tmp/${ARCHIVE_NAME}" "./${ARCHIVE_NAME}"

    green "==> Archive created successfully:"
    ls -lh "${ARCHIVE_NAME}"
    blue "==> Verifying archive integrity..."
    zstd -t "${ARCHIVE_NAME}"
    green "==> Archive integrity verified: OK"
}

cmd_clean() {
    blue "==> Cleaning build artifacts..."
    cargo clean
    rm -rf "${DIST_DIR}" "${ARCHIVE_NAME}"
    find . -type d -name "__pycache__" -exec rm -rf {} + 2>/dev/null || true
    find . -type f -name "*.pyc" -delete 2>/dev/null || true
    green "==> Clean complete."
}

# Main command dispatch
COMMAND="${1:-help}"
case "${COMMAND}" in
    build)       cmd_build ;;
    release)     cmd_release ;;
    test)        cmd_test ;;
    check)       cmd_check ;;
    dist)        cmd_dist ;;
    dist-linux)  cmd_dist_linux ;;
    dist-macos)  cmd_dist_macos ;;
    installer)   cmd_installer ;;
    pack)        cmd_pack ;;
    install)     cmd_install ;;
    clean)       cmd_clean ;;
    help|--help|-h) show_help ;;
    *)
        red "Unknown command: ${COMMAND}"
        show_help
        exit 1
        ;;
esac
