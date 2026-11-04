#!/usr/bin/env python3
"""Builds the standalone install.sh script for ai-rumin by embedding the integrations/ payload."""

import base64
import io
import os
import tarfile
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
INTEGRATIONS_DIR = REPO_ROOT / "integrations"
OUTPUT_INSTALLER = REPO_ROOT / "install.sh"

INSTALLER_TEMPLATE = r'''#!/usr/bin/env bash
# ==============================================================================
# AI-RUMIN & RUMIN-FIRST AGENT SYSTEM INSTALLER
# ==============================================================================
# Comprehensive installer for:
#   1. ai-rumin & rumin binary (Rust MCP code memory & graph engine)
#   2. Antigravity & AGY MCP Server configuration (ai-rumin serve)
#   3. Full 25 MCP tool schemas & operational instructions
#   4. Global agent rules (Rumin-First protocol in GEMINI.md & AGENTS.md)
#   5. PreToolUse Token Circuit Breaker hook (intercept_read.py in hooks.json)
#   6. Optional Claude Desktop & Cursor IDE MCP integrations
# ==============================================================================
set -euo pipefail

# Visual formatting
BOLD="\\033[1m"
GREEN="\\033[1;32m"
BLUE="\\033[1;34m"
YELLOW="\\033[1;33m"
RED="\\033[1;31m"
RESET="\\033[0m"

info()    { printf "${BLUE}==>${RESET} %s\\n" "$*"; }
success() { printf "${GREEN}[✓]${RESET} %s\\n" "$*"; }
warn()    { printf "${YELLOW}[!]${RESET} %s\\n" "$*"; }
error()   { printf "${RED}[✗]${RESET} %s\\n" "$*" >&2; }

show_help() {
    printf "%b\\n" "${BOLD}ai-rumin Installer${RESET}

Usage:
  ./install.sh [OPTIONS]
  curl -fsSL <URL>/install.sh | bash

Options:
  -p, --prefix <DIR>       Install binary to <DIR>/bin (default: ~/.local or /usr/local)
  -b, --bin-dir <DIR>      Install binary directly into <DIR>
  -y, --yes                Accept defaults automatically (non-interactive)
      --no-binary          Skip installing the ai-rumin binary
      --no-mcp             Skip registering MCP server in client configs
      --no-rules           Skip installing global Rumin-First agent rules
      --no-hook            Skip installing token circuit breaker pre-tool hook
      --claude             Configure Claude Desktop MCP configuration
      --cursor             Configure Cursor IDE MCP configuration
      --all                Configure all supported clients (Antigravity, Claude, Cursor)
      --dry-run            Simulate operations without writing changes
  -h, --help               Show this help message

Environment Variables:
  PREFIX                   Alternative to --prefix
  BINDIR                   Alternative to --bin-dir
  SKIP_BINARY              Set to 1 to skip binary install
  SKIP_MCP                 Set to 1 to skip MCP config
  SKIP_RULES               Set to 1 to skip rules install
  SKIP_HOOK                Set to 1 to skip hook install
"
}

# Defaults
NON_INTERACTIVE=0
DRY_RUN=0
OPT_NO_BINARY="${SKIP_BINARY:-0}"
OPT_NO_MCP="${SKIP_MCP:-0}"
OPT_NO_RULES="${SKIP_RULES:-0}"
OPT_NO_HOOK="${SKIP_HOOK:-0}"
OPT_CLAUDE=0
OPT_CURSOR=0
OPT_ALL=0

PREFIX="${PREFIX:-}"
BIN_DIR="${BINDIR:-}"

# Parse command line flags
while [ $# -gt 0 ]; do
    case "$1" in
        -h|--help)
            show_help
            exit 0
            ;;
        -y|--yes)
            NON_INTERACTIVE=1
            shift
            ;;
        -p|--prefix)
            PREFIX="$2"
            shift 2
            ;;
        -b|--bin-dir)
            BIN_DIR="$2"
            shift 2
            ;;
        --no-binary)
            OPT_NO_BINARY=1
            shift
            ;;
        --no-mcp)
            OPT_NO_MCP=1
            shift
            ;;
        --no-rules)
            OPT_NO_RULES=1
            shift
            ;;
        --no-hook)
            OPT_NO_HOOK=1
            shift
            ;;
        --claude)
            OPT_CLAUDE=1
            shift
            ;;
        --cursor)
            OPT_CURSOR=1
            shift
            ;;
        --all)
            OPT_ALL=1
            shift
            ;;
        --dry-run)
            DRY_RUN=1
            shift
            ;;
        *)
            error "Unknown argument: $1"
            show_help
            exit 1
            ;;
    esac
done

if [ "$OPT_ALL" -eq 1 ]; then
    OPT_CLAUDE=1
    OPT_CURSOR=1
fi

# Detect OS and Architecture
OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
ARCH="$(uname -m)"

info "Detected System: ${OS} (${ARCH})"

# Determine Script / Repo Location
SCRIPT_DIR=""
if [ -n "${BASH_SOURCE[0]:-}" ] && [ -f "${BASH_SOURCE[0]}" ]; then
    SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
fi

# Resolve payload directory (local repository integrations/ or unpacked embedded payload)
CLEANUP_TMP=""
INTEGRATIONS_DIR=""

if [ -n "${SCRIPT_DIR}" ] && [ -d "${SCRIPT_DIR}/integrations/mcp" ]; then
    INTEGRATIONS_DIR="${SCRIPT_DIR}/integrations"
    info "Using local integrations repository: ${INTEGRATIONS_DIR}"
else
    info "Extracting embedded integrations payload..."
    TMP_PAYLOAD_DIR="$(mktemp -d -t rumin-payload-XXXXXX)"
    CLEANUP_TMP="${TMP_PAYLOAD_DIR}"
    
    # Cross-platform base64 decoding helper
    decode_b64() {
        if command -v base64 >/dev/null 2>&1; then
            base64 -d 2>/dev/null || base64 -D 2>/dev/null || openssl base64 -d 2>/dev/null
        elif command -v python3 >/dev/null 2>&1; then
            python3 -m base64 -d
        else
            error "Neither base64, openssl, nor python3 available for payload decoding."
            exit 1
        fi
    }

    echo "$EMBEDDED_PAYLOAD" | decode_b64 | tar -xzf - -C "${TMP_PAYLOAD_DIR}"
    INTEGRATIONS_DIR="${TMP_PAYLOAD_DIR}/integrations"
    success "Embedded integrations payload extracted."
fi

cleanup() {
    if [ -n "${CLEANUP_TMP}" ] && [ -d "${CLEANUP_TMP}" ]; then
        rm -rf "${CLEANUP_TMP}"
    fi
}
trap cleanup EXIT

# ------------------------------------------------------------------------------
# STEP 1: Determine Binary Destination Directory
# ------------------------------------------------------------------------------
if [ -z "${BIN_DIR}" ]; then
    if [ -n "${PREFIX}" ]; then
        BIN_DIR="${PREFIX}/bin"
    elif [ "$(id -u)" -eq 0 ]; then
        BIN_DIR="/usr/local/bin"
    else
        BIN_DIR="${HOME}/.local/bin"
    fi
fi

# ------------------------------------------------------------------------------
# STEP 2: Install ai-rumin Binary
# ------------------------------------------------------------------------------
if [ "$OPT_NO_BINARY" -eq 0 ]; then
    info "Installing ai-rumin executable to ${BIN_DIR}..."
    
    SRC_BINARY=""
    
    # 1. Check local dist/ directory
    if [ -n "${SCRIPT_DIR}" ]; then
        if [ -f "${SCRIPT_DIR}/dist/ai-rumin" ]; then
            SRC_BINARY="${SCRIPT_DIR}/dist/ai-rumin"
        elif [ "${OS}" = "linux" ] && [ "${ARCH}" = "x86_64" ] && [ -f "${SCRIPT_DIR}/dist/ai-rumin-linux-x86_64" ]; then
            SRC_BINARY="${SCRIPT_DIR}/dist/ai-rumin-linux-x86_64"
        elif [ "${OS}" = "darwin" ] && [ "${ARCH}" = "arm64" ] && [ -f "${SCRIPT_DIR}/dist/ai-rumin-apple-m5-arm64" ]; then
            SRC_BINARY="${SCRIPT_DIR}/dist/ai-rumin-apple-m5-arm64"
        elif [ -f "${SCRIPT_DIR}/target/release/ai-rumin" ]; then
            SRC_BINARY="${SCRIPT_DIR}/target/release/ai-rumin"
        fi
    fi

    # 2. Build with cargo if source repo available and no binary pre-compiled
    if [ -z "${SRC_BINARY}" ] && [ -n "${SCRIPT_DIR}" ] && [ -f "${SCRIPT_DIR}/Cargo.toml" ]; then
        if command -v cargo >/dev/null 2>&1; then
            info "Building optimized release binary using cargo in ${SCRIPT_DIR}..."
            if [ "$DRY_RUN" -eq 0 ]; then
                (cd "${SCRIPT_DIR}" && cargo build --release)
                SRC_BINARY="${SCRIPT_DIR}/target/release/ai-rumin"
            else
                info "[DRY-RUN] Would run: cargo build --release"
                SRC_BINARY="target/release/ai-rumin"
            fi
        fi
    fi

    # 3. Check if ai-rumin is already on PATH
    if [ -z "${SRC_BINARY}" ]; then
        if command -v ai-rumin >/dev/null 2>&1; then
            CURRENT_BIN="$(command -v ai-rumin)"
            warn "No source binary found in current directory, but ai-rumin is already installed at: ${CURRENT_BIN}"
            SRC_BINARY="${CURRENT_BIN}"
        else
            error "No pre-compiled ai-rumin binary found and cargo is not installed."
            error "Please build ai-rumin via 'cargo build --release' or obtain a pre-compiled binary from release."
            exit 1
        fi
    fi

    if [ "$DRY_RUN" -eq 0 ]; then
        mkdir -p "${BIN_DIR}"
        if [ "${SRC_BINARY}" != "${BIN_DIR}/ai-rumin" ]; then
            cp -f "${SRC_BINARY}" "${BIN_DIR}/ai-rumin"
        fi
        chmod 755 "${BIN_DIR}/ai-rumin"
        ln -sf "ai-rumin" "${BIN_DIR}/rumin"
        success "Binary installed: ${BIN_DIR}/ai-rumin (symlinked as ${BIN_DIR}/rumin)"
    else
        info "[DRY-RUN] Would copy ${SRC_BINARY} -> ${BIN_DIR}/ai-rumin"
        info "[DRY-RUN] Would symlink ${BIN_DIR}/rumin -> ai-rumin"
    fi

    # Check PATH presence
    case ":${PATH}:" in
        *:"${BIN_DIR}":*) ;;
        *)
            warn "${BIN_DIR} is NOT currently in your PATH environment variable."
            RC_FILE=""
            if [ -n "${SHELL:-}" ]; then
                case "${SHELL}" in
                    */zsh)  RC_FILE="${HOME}/.zshrc" ;;
                    */bash) RC_FILE="${HOME}/.bashrc" ;;
                    *)      RC_FILE="${HOME}/.profile" ;;
                esac
            else
                RC_FILE="${HOME}/.profile"
            fi

            if [ -f "${RC_FILE}" ]; then
                if [ "$NON_INTERACTIVE" -eq 1 ]; then
                    DO_ADD="y"
                else
                    printf "Would you like to automatically add '${BIN_DIR}' to %s? [Y/n]: " "${RC_FILE}"
                    read -r DO_ADD || DO_ADD="y"
                fi
                if [ -z "${DO_ADD}" ] || [ "${DO_ADD}" = "y" ] || [ "${DO_ADD}" = "Y" ]; then
                    if [ "$DRY_RUN" -eq 0 ]; then
                        printf "\\n# Added by ai-rumin installer\\nexport PATH=\\"%s:\\$PATH\\"\\n" "${BIN_DIR}" >> "${RC_FILE}"
                        success "Appended PATH export to ${RC_FILE}"
                        info "Run 'source ${RC_FILE}' or restart your shell to activate PATH."
                    else
                        info "[DRY-RUN] Would append export PATH=\\"${BIN_DIR}:\\$PATH\\" to ${RC_FILE}"
                    fi
                fi
            else
                warn "Please manually add: export PATH=\\"${BIN_DIR}:\\$PATH\\" to your shell configuration."
            fi
            ;;
    esac
else
    info "Skipping binary installation (--no-binary requested)."
fi

# ------------------------------------------------------------------------------
# STEP 3: Configure Antigravity MCP Server & Tool Schemas
# ------------------------------------------------------------------------------
if [ "$OPT_NO_MCP" -eq 0 ]; then
    info "Configuring Antigravity MCP Server & Tool Schemas..."

    GEMINI_CONFIG_DIR="${HOME}/.gemini/config"
    ANTIGRAVITY_MCP_DIR="${HOME}/.gemini/antigravity-cli/mcp/rumin"
    
    if [ "$DRY_RUN" -eq 0 ]; then
        mkdir -p "${GEMINI_CONFIG_DIR}"
        mkdir -p "${ANTIGRAVITY_MCP_DIR}"

        # 1. Update ~/.gemini/config/mcp_config.json safely via Python
        python3 - "${GEMINI_CONFIG_DIR}/mcp_config.json" << 'PYEOF'
import json, sys
from pathlib import Path

target_file = Path(sys.argv[1])
data = {}
if target_file.exists():
    try:
        data = json.loads(target_file.read_text(encoding="utf-8"))
    except Exception:
        data = {}

mcp_servers = data.setdefault("mcpServers", {})
mcp_servers["rumin"] = {
    "command": "ai-rumin",
    "args": ["serve"]
}

target_file.parent.mkdir(parents=True, exist_ok=True)
target_file.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")
PYEOF
        success "Registered 'rumin' in ${GEMINI_CONFIG_DIR}/mcp_config.json"

        # 2. Copy 25 tool schema files and instructions.md
        if [ -d "${INTEGRATIONS_DIR}/mcp/rumin" ]; then
            cp -r "${INTEGRATIONS_DIR}/mcp/rumin/"* "${ANTIGRAVITY_MCP_DIR}/"
            SCHEMA_COUNT="$(find "${ANTIGRAVITY_MCP_DIR}" -name "*.json" | wc -l)"
            success "Installed ${SCHEMA_COUNT} MCP tool schemas and instructions in ${ANTIGRAVITY_MCP_DIR}/"
        else
            warn "Integrations schema directory not found in ${INTEGRATIONS_DIR}/mcp/rumin"
        fi
    else
        info "[DRY-RUN] Would update ${GEMINI_CONFIG_DIR}/mcp_config.json"
        info "[DRY-RUN] Would install schemas to ${ANTIGRAVITY_MCP_DIR}/"
    fi
else
    info "Skipping MCP server & schema configuration (--no-mcp requested)."
fi

# ------------------------------------------------------------------------------
# STEP 4: Install Global Agent Rules (Rumin-First Protocol)
# ------------------------------------------------------------------------------
if [ "$OPT_NO_RULES" -eq 0 ]; then
    info "Installing Global Agent Navigation Rules (Rumin-First Protocol)..."

    RULES_SRC="${INTEGRATIONS_DIR}/rules/GEMINI.md"
    GEMINI_RULES="${HOME}/.gemini/config/GEMINI.md"
    AGENTS_RULES="${HOME}/.gemini/config/AGENTS.md"

    if [ -f "${RULES_SRC}" ]; then
        if [ "$DRY_RUN" -eq 0 ]; then
            mkdir -p "${HOME}/.gemini/config"
            
            install_rule() {
                local target="$1"
                local src="$2"
                local header="# Global Agent Guidelines: Rumin-First Code Intelligence & Navigation"
                
                if [ -f "${target}" ]; then
                    if grep -qF "${header}" "${target}"; then
                        info "Rule header already present in ${target}; updating section..."
                        # Safe replace or rewrite
                        cp "${src}" "${target}"
                    else
                        cp "${target}" "${target}.bak.$(date +%s)"
                        cat "${src}" "${target}" > "${target}.tmp" && mv "${target}.tmp" "${target}"
                        success "Prepended Rumin-First rules to ${target} (backup created)"
                        return
                    fi
                else
                    cp "${src}" "${target}"
                fi
                success "Installed rules to ${target}"
            }

            install_rule "${GEMINI_RULES}" "${RULES_SRC}"
            install_rule "${AGENTS_RULES}" "${RULES_SRC}"
        else
            info "[DRY-RUN] Would install Rumin-First rules to ${GEMINI_RULES} and ${AGENTS_RULES}"
        fi
    else
        warn "Rules source file not found: ${RULES_SRC}"
    fi
else
    info "Skipping global rules installation (--no-rules requested)."
fi

# ------------------------------------------------------------------------------
# STEP 5: Install PreToolUse Token Circuit Breaker Hook
# ------------------------------------------------------------------------------
if [ "$OPT_NO_HOOK" -eq 0 ]; then
    info "Configuring Token Circuit Breaker Hook (read-interceptor)..."

    HOOK_SCRIPT_SRC="${INTEGRATIONS_DIR}/scripts/intercept_read.py"
    HOOKS_CONFIG_SRC="${INTEGRATIONS_DIR}/hooks/hooks.json"
    
    TARGET_SCRIPTS_DIR="${HOME}/.gemini/config/scripts"
    TARGET_SCRIPT="${TARGET_SCRIPTS_DIR}/intercept_read.py"
    TARGET_HOOKS="${HOME}/.gemini/config/hooks.json"

    if [ -f "${HOOK_SCRIPT_SRC}" ]; then
        if [ "$DRY_RUN" -eq 0 ]; then
            mkdir -p "${TARGET_SCRIPTS_DIR}"
            cp "${HOOK_SCRIPT_SRC}" "${TARGET_SCRIPT}"
            chmod 755 "${TARGET_SCRIPT}"
            success "Installed interceptor script: ${TARGET_SCRIPT}"

            # Safely merge into hooks.json via Python
            python3 - "${TARGET_HOOKS}" << 'PYEOF'
import json, sys
from pathlib import Path

target_file = Path(sys.argv[1])
data = {}
if target_file.exists():
    try:
        data = json.loads(target_file.read_text(encoding="utf-8"))
    except Exception:
        data = {}

interceptor = data.setdefault("read-interceptor", {})
pre_tool = interceptor.setdefault("PreToolUse", [])

found = False
for item in pre_tool:
    matcher = item.get("matcher", "")
    if "read_file" in matcher or "view_file" in matcher:
        item["matcher"] = "read_file|view_file|run_command"
        item["hooks"] = [{"type": "command", "command": "python3 scripts/intercept_read.py"}]
        found = True
        break

if not found:
    pre_tool.append({
        "matcher": "read_file|view_file|run_command",
        "hooks": [
            {"type": "command", "command": "python3 scripts/intercept_read.py"}
        ]
    })

target_file.parent.mkdir(parents=True, exist_ok=True)
target_file.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")
PYEOF
            success "Configured PreToolUse circuit breaker hook in ${TARGET_HOOKS}"
        else
            info "[DRY-RUN] Would install ${TARGET_SCRIPT} and configure ${TARGET_HOOKS}"
        fi
    else
        warn "Hook script source not found: ${HOOK_SCRIPT_SRC}"
    fi
else
    info "Skipping token circuit breaker hook (--no-hook requested)."
fi

# ------------------------------------------------------------------------------
# STEP 6: Optional Claude Desktop Integration
# ------------------------------------------------------------------------------
CLAUDE_CONFIG=""
if [ "${OS}" = "darwin" ]; then
    CLAUDE_CONFIG="${HOME}/Library/Application Support/Claude/claude_desktop_config.json"
elif [ "${OS}" = "linux" ]; then
    CLAUDE_CONFIG="${HOME}/.config/Claude/claude_desktop_config.json"
fi

if [ -n "${CLAUDE_CONFIG}" ] && { [ "$OPT_CLAUDE" -eq 1 ] || [ -d "$(dirname "${CLAUDE_CONFIG}")" ]; }; then
    info "Configuring Claude Desktop integration (${CLAUDE_CONFIG})..."
    if [ "$DRY_RUN" -eq 0 ]; then
        python3 - "${CLAUDE_CONFIG}" << 'PYEOF'
import json, sys
from pathlib import Path

target_file = Path(sys.argv[1])
data = {}
if target_file.exists():
    try:
        data = json.loads(target_file.read_text(encoding="utf-8"))
    except Exception:
        data = {}

mcp_servers = data.setdefault("mcpServers", {})
mcp_servers["rumin"] = {
    "command": "ai-rumin",
    "args": ["serve"]
}

target_file.parent.mkdir(parents=True, exist_ok=True)
target_file.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")
PYEOF
        success "Registered 'rumin' in Claude Desktop configuration."
    else
        info "[DRY-RUN] Would configure Claude Desktop at ${CLAUDE_CONFIG}"
    fi
fi

# ------------------------------------------------------------------------------
# STEP 7: Optional Cursor IDE Integration
# ------------------------------------------------------------------------------
CURSOR_DIR="${HOME}/.cursor"
if [ "$OPT_CURSOR" -eq 1 ] || [ -d "${CURSOR_DIR}" ]; then
    CURSOR_MCP="${CURSOR_DIR}/mcp.json"
    info "Configuring Cursor IDE integration (${CURSOR_MCP})..."
    if [ "$DRY_RUN" -eq 0 ]; then
        python3 - "${CURSOR_MCP}" << 'PYEOF'
import json, sys
from pathlib import Path

target_file = Path(sys.argv[1])
data = {}
if target_file.exists():
    try:
        data = json.loads(target_file.read_text(encoding="utf-8"))
    except Exception:
        data = {}

mcp_servers = data.setdefault("mcpServers", {})
mcp_servers["rumin"] = {
    "command": "ai-rumin",
    "args": ["serve"]
}

target_file.parent.mkdir(parents=True, exist_ok=True)
target_file.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")
PYEOF
        success "Registered 'rumin' in Cursor IDE configuration."
    else
        info "[DRY-RUN] Would configure Cursor IDE at ${CURSOR_MCP}"
    fi
fi

# ------------------------------------------------------------------------------
# STEP 8: Verification & Health Check
# ------------------------------------------------------------------------------
info "Running self-test and verification..."

if [ "$DRY_RUN" -eq 0 ]; then
    EXECUTABLE="${BIN_DIR}/ai-rumin"
    if [ -x "${EXECUTABLE}" ]; then
        VERSION_OUTPUT="$("${EXECUTABLE}" --version 2>&1 || true)"
        success "Executable verified: ${VERSION_OUTPUT}"

        # Test stdio JSON-RPC 2.0 initialize handshake
        HANDSHAKE_TEST='{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"installer-check","version":"1.0"}}}'
        RPC_RESPONSE="$(printf "%s\\n" "${HANDSHAKE_TEST}" | "${EXECUTABLE}" serve 2>/dev/null | head -n 1 || true)"
        
        if echo "${RPC_RESPONSE}" | grep -q '"protocolVersion"'; then
            success "JSON-RPC 2.0 Stdio MCP Handshake verified successfully!"
        else
            warn "Handshake test did not return expected protocolVersion (response: ${RPC_RESPONSE})"
        fi
    else
        warn "${EXECUTABLE} is not directly executable in current shell; check permissions or PATH."
    fi
fi

# ------------------------------------------------------------------------------
# Installation Summary
# ------------------------------------------------------------------------------
printf "%b\\n" "
${BOLD}========================================================================${RESET}
${GREEN}${BOLD}       AI-RUMIN & RUMIN-FIRST AGENT SYSTEM INSTALLED SUCCESSFULLY!       ${RESET}
${BOLD}========================================================================${RESET}

${BOLD}Installed Components:${RESET}
  • Binary:            ${BIN_DIR}/ai-rumin (symlinked as ${BIN_DIR}/rumin)
  • MCP Configuration: ~/.gemini/config/mcp_config.json
  • Tool Schemas (25): ~/.gemini/antigravity-cli/mcp/rumin/
  • MCP Instructions:  ~/.gemini/antigravity-cli/mcp/rumin/instructions.md
  • Global Rules:      ~/.gemini/config/GEMINI.md & AGENTS.md
  • Circuit Breaker:   ~/.gemini/config/hooks.json & scripts/intercept_read.py

${BOLD}How to verify:${RESET}
  $ ai-rumin --version
  $ ai-rumin digest
  $ ai-rumin query "VisualBasicParser"

${BOLD}Enjoy lightning-fast, token-efficient code intelligence with Rumin!${RESET}
"
'''


def build_installer():
    print(f"Building installer from {INTEGRATIONS_DIR}...")
    if not INTEGRATIONS_DIR.exists():
        raise FileNotFoundError(f"Missing integrations directory: {INTEGRATIONS_DIR}")

    # Create in-memory tar.gz
    buf = io.BytesIO()
    with tarfile.open(fileobj=buf, mode="w:gz") as tar:
        tar.add(str(INTEGRATIONS_DIR), arcname="integrations")
    tar_bytes = buf.getvalue()
    b64_payload = base64.b64encode(tar_bytes).decode("ascii")

    # Format base64 payload nicely with 80 columns
    b64_lines = [b64_payload[i : i + 80] for i in range(0, len(b64_payload), 80)]
    formatted_b64 = "\n".join(b64_lines)

    # Insert payload into script
    payload_def = f'EMBEDDED_PAYLOAD="{formatted_b64}"\n'
    content = INSTALLER_TEMPLATE.replace(
        'echo "$EMBEDDED_PAYLOAD"',
        f"{payload_def}\n    echo \"$EMBEDDED_PAYLOAD\"",
    )

    OUTPUT_INSTALLER.write_text(content, encoding="utf-8")
    OUTPUT_INSTALLER.chmod(0o755)
    print(f"Generated {OUTPUT_INSTALLER} ({len(content)} bytes, executable 0755)")


if __name__ == "__main__":
    build_installer()
