#!/usr/bin/env bash
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

    EMBEDDED_PAYLOAD="H4sIAJcT62oC/+19e3fbOJJv/tanwLL3JFJGkiX5Ne1t972K7aS969hu25meXMcrUyQkYUKRbIK0rXb8
3W9VAXxIlCzn6ZMJ0KdjigQLIB71KxQKVc2V5sr/PbZvfuO2y6MnXyW1VFr0t9VaXcuv8X671Wl3nrCb
J98gJTK2Iyj+yY+ZOn9n41iM+XZ78+fO5vrPrdZGc/3ntU7r57XKE5P+7ZPwYz6M7FgEvlx58vXm/+bm
Ov5tb663i3+zOd9e72xsbKx3Vlc3Yf53OuvrT9j6t5z/rvD69+Vb9vw7TU3D/w3/n+H/G60N4P/rhv//
aPx/FATvvwIKfDz/X12DW4b/G/5v+L/h/yZ9W/5P/zb/JQP/C87/jY21hfx/rb05w//XYQH6hLW+5fz/
Qfn/bYUxKwLm38CREDk8jIPI2mJ4H54cR/wsCLw3ksO9c7rH9DN6PrZjZ8TxBSLSGwiPf7gS/FpdRYnf
c4Lx2PZdq56/RSOsQHCaKGWJJyEWaZXfpsfpbcgRTuJR4K8y6UQijOVK9h09rFEznFiFV++y64tK8Q7+
uqvc/Xgsz+C/wX+j/zP4T/g/dsKVrzT/P3L9hyzBrP8M/zf8/1HWfx3g/6uG//+I/D9KxsJf+eLz/+P4
/1p7Y8Pwf8P/Df9/HPn/70b+/7H5v/BlHCUO3W6O3c+f//fo/zrra7P6v80OZDf6v2+QfmIn2OHs9c4x
O+XRFY/YfqHv2VN2FHI1PmyPveAyZseRDQ8dLisV9a6QLB5xFkZibEcT5gQu79uSMxxbnieG3Hd4nXVP
z9h7P7j2uDvkDMZcOKoz23eZHTkjEXMnTiIoIuK2DHzhD5mk6jQrXe/ankgkH0QiFn9xlkh8nlc8DgJP
sgArP7b9BKig9hEIIGkWRKwfJTFvDILI4SzmN7F6o1mp/PQTexV4LvfZoX0lhvSd7KUXXFcq7SZ7/vwg
cOyYs9PJuI9FPGU7gY+6Rfn8+RYqDhtsx/Y8dkmzpvdnwqNJlf7dtn5xVFYsX9L7v1p11k/g6+Pt9nqr
VbtsKhJnATSV4yUuZ66IoCGoraSHbQxPqHEjLkPoD2hHyWN2yW9CaLpeP3AFl1sM+osXqEHJ0IU2vhow
GXJHDITDUKUqWZU3h0126Xi2lJd1djlIfOpqvI7tvscva3VoYA6/MP8WO7coL9TdSvNaF1BYpYMNBIMl
xBrv+TH0DZ/fLi5H5WyfVznmmvR8GwDH+iVtldol1pPf2JAXmiBwoPLQwbLOxjweBS5cDAT38C+OFwco
80iyawEPk5i5yTik8RIk2L/Y9di1q1i9MxirXNXlNwHjGIcax348DrzJ0AtgNNvxaH6toQVhREleVfXs
CbdQ57ruKmiNbUvXyPqgrjg2lgtdP9puN5urWT8XiQ8EdF8IZVdVtXuKMJZAv7EEYEwwVvIn6nfaXjF9
mhMFUjZwqsBowaE2iAKYdr7biING33bewyXWBf7ANJywCFqMmmcNm6ebuCJmL6B/Y3ZiuyKRMMVhmkCT
jWx/iK26A7M5a57uwR/dt6csSvz0O8Q4BG5QzWuf92lfUeJQBBKCurnc43StckndLiccdwxYEkK/c3uc
dTAxh8EAWpm7LPGhplD1GCu/jpV/DQM8xkH+CnkJewlTZORzmXdmdxADR3CAZlqBtC40RupUUPYh0EA3
tGtRxW6BT8E/9CHwos4kJ75T1cPVlwl8Hc5N4mUwQwceVFXxQs/GqjIHWxGb28j/Rv438r9J34v8r9id
7bq96yB6Dyyz5wcx/0SjgGX7/+trM/qfTnujtWnk/2+Rbi0Uxqwta36PW3VLCW8hyX1bVjdEUYLZuN2O
e/+h7TL9AsMXECxpB34MWIuic2zL9wzaGC8DfA8QGkg1gXJoR1B2jJLT1q0VRgGsNFCGxF8OSTExXk5X
4BAL0U9LhaFUhZIjEFcGBJYSJa27uqULBjGuTPSMJKu0bmx/t0wAKET8zwSEPnj/vEitnlX2Insr6P8L
ZAF4y+j/DP4b/Dfp+8R/XB5+viXgMvzvdNZn9X9k/23w/9vjP/Z4GfRJTTCtpxP+lR0J24/VOl3pITx7
AituIQNPadKiBHUx90P93fcHmv9GyeC/wX+D/wb/F+N/zxNjEX/OgYDl9v9rM+v/1uZG2+D/Y+G/7vEF
YkCU+Mgw2BD323xYgavcSgoIfCeJItppGNl/2ZFLG1jdkN+suHZs07Yg7g+y6unR7wfwTAr44QVBKOts
9zXeSX+hcj5IYlm4lfj9IPFd7jLcY8ONHJAxRmI4ajhQlPBtT8QTRrtYsmbEDoP/Bv8N/pv0yfjvjLjz
Pgwgz2cpAZbq/zdm9f+ttc6qwf9HwP+8x0vov3cTBhGp3CMuY9zZtxX2ypBzhH6yBGB9gGE0AsoIsWBA
W+Iztj/sStiYGaWBZZsBtrI6Kantd/JCdJa6xf1kTAp6+wp3L7zAdgt6+Xw7AHf351CE4RCMi7UnSyLM
zKpBqMygakt3B3RtvoP9AIP/Bv8N/hv8n4//cDPwBKzc+OcIAMvwf3W1PYv/65vG/vdR8D/v8TkCAHeS
GGDfG6Ih7mgsHDbm44BMftPXUOtflR7nIQtHsNqvbbEwSnxYqruAMGi0CEt+3Lr3AfeV0qCfCM+F50LG
wgMJgY0TLxaNURAyOQKRAwo1ewgG/w3+G/w36Zvif2q3/5kmAMvwf3NzYxb/WxvrBv8fAf/THi+B//HJ
/uvuyVt2+vb1i6MDtn94dnJ0ery3c7Z/dMjOjo4OttjrN6dnrI+ncwDDYzzToo6G2D4ICHiEJD3PUWfp
QZK6Mt8b2HhASJ06qqOGgVT46QECdOOD5oWRfa0Pd7ATHieRL6fOiqRHRLIzI3R+RR8x8tBgwQngE4cr
QRIPAySY2u0rYwU5EqFkVX34oI72/Bx3MpCG8EccRB4bfi3bV7AKZ1zK+gV1kIjhQ/xOFILY/q46lFNn
z7qOEyR+/AwuT5WB4Wvbt4c8wjt2Eo+QOJ5I6kEjR8+WKyKKlZmrjTD4b/Df4L/B//n4LwaDL+AIcPn+
f6d0/tfY/z0O/kOPl7B/JxiHCdnvh4EXDAGAPDzKF9usarsuBzwfB/CiwKuIj4MrwH+uz4MS/KLWX9aY
PbTxXDmzC/r1pecAspy9BRr7eap6kD7wS9IS58KkURiY9b/Bf4P/Bv/vwX/g259/AGCp/f9aexb/0SWI
wf/HwH/s8ZIEoNbbUwLA9GkA9R6BvYSnHFfhkXBkuvmf+gQxenyD/wb/Df6b9D3g/4C76PuJ9yIeBp8u
BizV/3dK5/9bmxsG/x8B/6d6vCQGvNRPAecZvwEE98lhVxhIEaMZQBCxscCTgNodEQyugOAf7eGueGot
oPTuaMgfMZt04TJEX0a2J2xZZ9y3+x7q59WZwgJ9XNvrI/6znsN4eiRgqS0hFjLHrUBWizDiA3GjKpMp
5h07hhuSNPN94WH15mnfUfkeBgs0Fd2+DDzUo6QqisKXwa+06Zfq9PMivoB9ocF/g/8G/w3+z8f/1End
56kAluA/zPbV2f3/jrH/fxz8T3t8oQFA92Tnt/2zvZ2zNyfdA3bcPfuNvdw/3N0/fKXNAHaFdPBooAQR
AQ0GaZsfD/8NvOBabccX/BFiWRLtB310pgi4Hl8HqXtAcmeIG/4gP6AowMLUaSMdA7QfAvdj+6ZHrhjL
ePzavhHjZKzgWPsKpays6vKBnXgxWy9gPM0UHpEroaLLxjmb/MoRpZy71z9PaJhy9LjYO9ED6c16Kpqq
7GxhF0bjYvQ/Rv4z8p+R/4z8Nyv/6SP8n7sBtNT+Y302/sfmZsfofx5D/tM9Xpb+eDQIojHrk6voSLmK
tn3bm0ihbDwigX4enSAi/87QpsrRchSMM+uQkly3THjT+csS0UkQZPLQ/m5GUEmTQFTEfEyvlUUtdcOO
IntSFpV0eT+QUGTw3+C/wX+D/3PxXzn3/+r2H5vrGyX7jw1j//kY+K96fI7yZ6+x81v38NUe23993N05
Y93D7sHb0/3TmbMfdHrCTU9uEPBPdOyFiA+AtJIPyOkD00KFZGuNPxPbjWy/5GRS+ZTIBI1qFiPizf5K
usuUndhwg2u/GEKCQjygKgqLzONH1LWZyoiPbTYASSKEDPHScx1LtEg6WgiZxhZ1SFuss0CJtEh7NP+I
CDrPxmb4a/kG0ceodgz+G/w3+G/wfz7+Y0ycb+D/edb/Y3tzddPYfz4K/mOPl+D/LBLDIRlrDBLPY4iS
AK9o+tGgFwjhBzpOWWrTYEw9zfrf4L/Bf5O+b/ynmHhf1/9jq2T/YfD/EfGferwkBJwmEZ388CYZ7mM4
J8B+TwXdzM09Z109Cp/JpN9Yb43zqJ1AQzq2T5Fe8R08MBpxiiVFdpjLluNY4gJDyxPy6HBFy2d71ugS
y6LakgFmIxV47l9R54X9u20NGPw3+D+L/xutDuD/qsF/g/94qq9HkYa/YvyHUvz3Tmtz3ej/Hwf/8x4v
SQD79JBip+e2nSoOtWTV/z49OjyoKVsAPhQSYy8rP8hsgFjKfUeoeObqGMjSzf974jUeaU/MhYiNpCCX
MnDE3DMUcAPruUBiONbCAX3D7LcxLQ7dLyIUyH9PMoLBf4P/Bv8N/s/FfzxSN/nq+v9Oe620/7+2Zs5/
PAb+U4+XHUCd7J/t73QP2ApLj4HsHO3uscPuP/Zfdee4gNRmAC/2Xh6d7JEDSFpya0+OdTaMeEiOHtWx
iyZ7iWaFKXzj0l0b9+mgUjG/idMjGgmqHNB2QKIEIP6Cco7tIT+x/fcMN/tZF+qgLflUFCmlhZhx6Ujn
UAInGXM/puGfe5UMI+4IyQsmhXXmCZ8zPxn3ydJg5vApCCJpJdWpVpQRHJSU3nOf9RN3yJe6sLZUtrkG
Bhl1IigLtgXt9VZrvnmBa49D7vdGSX+O/WQ3DL0JhvCEmnDmAgfg8AfeIG1MwOBNalodXAuaKFKxtYCc
9txNJ2OZdlhZZzvo6zuvSB9kRG77WBF+E0Jz9fqBmxlSTAuVjpe4dEQ4gXbEzqMAYdIXYYinXjyB4qUL
cpYTU6Ux/kgI7IrPLQ77q6fO8N4jN5I2SJ/0Refl8H3oQUx91DMZOSt40ncQRA5v2GG4MobHK7rVV+af
/FVTZ87BYjVAPNsfJjBO6ZwyHZUunOeZK6+i+9J7PgGPI+Oum3LxqbydKqWWHn1wAbWle/q7yAsrfZj2
v4rXyl8qXpHXVfq4T7SfVU1gjhQZ/Z+R/438b9L3Lv8D5AaR2+OhkGiw+akLgWXnv9danVn/L511c/7n
MeT/6R6f4wcOH5P7F8ohHBAe+8lwSHJjJGyvjqIVSOlBEjvBmJOQz/0rEQU+StrM4yDk+2qrEIRfiXpC
uP0wlSAd+6bdyTmC0VmQOCNYDHzSQaC6pr3wxFFKXQttf4Ksqs407e9+XDkkyvtz5PxdHtsCV00gMIOw
qELpgHxH+5d0HaWtOk9clIkDcvK8mkcJZ2LAyC4Ll1YgF2qSdVh3eZKeDlTZ1Idz5erYHs4hfkprMvLW
jxkYyZnnz2BINEC0Tr3WP7v4qCYCedab4zv/RSQ4NEcyHmOAQZVpqVZW50pb3cimRv4z8p+R/0x6iPyn
NG6pNPDJ58CX7v+W4v+111Y7Rv57BPlvpsfLRmBKB5tJfyS3CdSnhqjBnREGtaZUSwOZJLNUFeqJsYgX
H7VSaljSfel6Kr0Xam/n6kIXqOb0x7znk2uUalFSjSL4t5hrmYDxvau8DP4b/Df4b/B/Af4rExwn4p8T
Anip/5dS/L/2atvYfz0O/hd7vLwRTLfRaS+/ZvYQNTexjX5ftBnWANATltp0EBpfaUgHcJ3C/jrvAT6X
euvjsY2uAh9g8ZVlLUFv3VoQe0+/SToBFWUQqj5/+20G5RfHzzPrf4P/Bv9N+rfGfx3J9Wut/9faJf//
a8b//6Piv+7xOSbgKqbvLArTOl/EEh6QpTfH1bnaTAL4x0NdqBTwg/gBTt/usfs+zc29aacB8/bsvtPu
rC73ipLTNbsABv8N/hv8N+lh+I+mhl/x/FenvVmK/9NuG/x/RPzHHi+B/wFanKI9tQ7no4yzx6EHSO7O
UQdI4wLG4L/Bf4P/Jn2n+H8tYmf0mdH/HrD+35i1/2xtmPg/j4P/eY+X4H93Alm1BxiVLxUECoHsoiCI
68qxiogFHiLChb8O/IeaAowCXGfOiDvvwwBGn7YRoCNay1QDHxNcT4W5KVZNHSEK6HzbN4ywZ/Df4L/B
f5O+R/yf+M7nH//+eP1/e7OzZuz/HgX/occXBn8ZoImf8J2I06Fpj2HuNDZfDrR1loSA9IT7OvLLiu26
+miGihYTRgl5fnO5Uh4E/vKNATqNW0b+l3hbOaYFsYMEC9x8QD2FOilSPs1gFA8G/w3+G/w36X781zEt
+GfKAEvj/7Zm7f9b7bbZ/38M/E97fGH4353uwUHj1Un3+Df2lO3uHe8d7u4d7rzV/l/O9Ot6ra0s9rL4
LFlQliDCyC4ez3yvSELszOmLkijGiReLxigI83jBDwj4uyBMy9lMeBYgK1m13Vivs9yZygJPKum33Es1
y1S3uJ+MUY0g8EeQ4EaKboP0ihdjzBXOcNL3z7V8+D07cJqFvaPoMA+xXszJGsMHk4z8Z+Q/I/+Z9AD5
D0OnDSZfPf5fZ2NzVv+z0Voz8t8jyH+qx0vS3z/oNrq1S9LIfDRu0PGYDvxDGzgoTKFLPFLt2P7Qw4vc
6x6pfoIoHNk+CDJIIhqg82BjJ2Lw3+C/wX+THhf/o8TjcuWrzP/NzfWF+I9zfhr/V9dam0/YusF/w/8N
/zf836Rvyf+7r/YOz06bY/dLzv/F6z9U/8/Ef2uvbbbWzfrvW6Sf2Csv6KPjbTLff5UIl6Ovc7nFTnBF
2HgpIhmzHYpCDmPF88SQzvc9ZYf2lRjSyKlUfmXn/7H/+vjo5Kx7eHYBP58/f9093O2eHc3xFX98cnR2
tHN08Pz5FuQ8Gwk55SISfobBNSwZXdafACGqx/PnrHppiwYtUy9rdWYr3+Shsk2wsUroO/xpHnyuoYLP
kTdxUaw694fwhU0o+48R9zn6jJ8ECfM5eioIyP9iEPG6djhfVz7dYZj4bl1FhKEdDBXcPiXPuodvGaxa
cX2KT7M4dnWiDc2BrvG7B390356yRHLVuhRLR7KX+yenZ/CFfQ7fwrWH+74thcPkCOrNklh4AlfD0Ajo
O/+yzi4Hwncva1iWdqvPIvtamTw0K5VGo1Gp/PQTazehgXm5L/foI5WTyT+grgMvuK5UXgYe/IX1vEi3
X2TMQxaKkAYF+XdAwwr4aHLyZMv3cqtSuby8HHPoBOFWkI4zsqOYne1WGKQzyHNu7agX8AdbYW8kNPm+
jzsUE+uCNRq/st/PLahqIQjBO796EDjobUL75ISuLQYI0C7xa9YFFfM7Udk9tzopFaXB6HMglJ5adQNH
bZHUmfJ6nm5KFXanUoK7RPDs1jrEYeFyDiONwZcJX77zoRnIzrTP42vO/bSK/8e6U98M77K30FtE4uTc
Wk0rle6vQSOoG9iLZGoK1TyjaEMUsWAkeISDbwJfHQbeZOgFMYuCJM4rSIUcBlQGTD117yT9eW6tpWWK
MYzDGOh3E1fErO+hGU8EQyahNh0MoGlw5HMZZ8SBAlE6Re/759Z6k/0BFYVZ6bITGG1AzBYcqPVw7PXI
R386Esk/K4V5ZL9ss/UWhU7ICBNBIr0Hr59bG02WBpakO0CZ6BKRFZaVEnoYXomctHI3JYYvqGpOfOfc
2sy+OItimbUzmioB7f+BfiyFpxxEXI6QJozjfObAQNrB2XiG4a72faX7QpjEpz+xbpPtCulgxInJFrss
jNxLVn29c1yDojOGxdIHOwf7tcpzYAfIeZDZACsANsiUKzKcIjTF/IkeUnWWuuqHAYre++use7xfx9E/
EMNETWBiR6VwFA4P46Yua4JlKI4DBcr7omegLg8Z6Uwj1bV/NcVn/PfopxYDD5Dvt1KgDMgxTINsyCSK
sNvwzTSOBdTjP4+q7dp/TofIyIKDXsMQLea/psGnw1+ojzrO9IUII4w9Z5eqkbdYKeTCdLAFHQ3h0kLX
tPgRjmKDqioRp+FgIYu19v2rAMbrKY+u4I91WWuqklR9oSgMzjETlEPF5NAZp4JfQP5LGEX8Ejte6LgX
ecCLcqAL7AsVLiINedFUg+9FE8E4CoitQd2zAZgyvXljsPBs0TB8kyEdtv71yI7zXfmqHn+Kc9Zz9W1d
64UBHNERcA3YLJfTI08HVsHvKfBgGLAeGgVyz5UFjgwtE4yp91PWTGrjJB4GymQwNwZgYjwGBgEoAQ2W
R5ZVcDiCQQdCBNkWEA/Ci2m7A2r5heNJRdbo4ZiBrjstjCAYUEUP0NmIOrYnSPs4CtAXcxCpUVQYZ7wH
LR3RQKKO3AEmg/x+h4AF2PFxyuxP4FPgK7KeTZHjEjJdzmDH5Zy+pE9ARFE9GVBrSujOlKGQlKJ6OLtF
eUpFVjMDgm2r2Wxa9dzaYTuzbPiQWTbUlYnFdrvZXK1dqonwUqghVeJSvqKTIWl8HQB1HIIxnS0NfGg5
mZqEZFBIQGBHIg1EU2dv9oGPk5dnEuLSoyZ6TG6V2qwqgyQCSFHfln6YOjUydRM/gfpqF6DKHvAYMfkF
gegJgWhGWwHtvKmXPVk08UA+hEFu0zkVLQUi8GGTEazCPINn+JPfiJQ3wtB9/rw4z16BNAuQgBstbK0B
I9SNsB1V6Wxsw7y7wYEBktbz529CmIfcHtMAVMMeEA37VTnz8FUnp7MQJq+NjBgGa59nUgP2bgep7QbX
vqa3W5igSPQPfJFEyqxD9SSGGekjhVWksIPwQ0IiCiKKG4l4pbBAVSKKqgi/4REFaSLCqr+Q1BqSOnVG
fGyzl0EQh0Aypk+bGhByRbEd6ALy6667eK8gjZCYfKJ5yVOSNeAKmvsI1xAKTGGxBPN6gMOQYljfYDsT
GmITFqBQTUeQe3FQzAhOl9jFl5nodKlkJ1yFCAfkm8tTGJTxAdC6JKqXe76rflXH2iHqequxkcpZar4d
0qoGaaaBtfXCAB92BxibU48vAPbEz0ZwJjhVcY5sW7/g9a8wB1QdM1GqWiMQI0Nvnu//FRYe0KenKg4R
cMOR6AstO9HY2z1ih0e44sGy+1BxjBnqJJGk0F8kC2VrHdaIkItGIiyufFgTr4ChNU5gEQRVoZvpUoHc
0EH3eQH0ZZN1vWt7IosfquWxAS6ImpXOdJ0KraaYhCKmrNevhD3VWehHDtY+Ai3d3GSMVmWXwOixdn31
B0MeAMNnB8hbiLokAYenkgP2ajqxCFVRdMMxuTpVL73glLC6mto9LUAnOq9Nx3m6WcsLMdcTFW99hl/p
VjDKMLP/a/T/Rv9v9P8/tv7/1d7r/cP9x9X/b2wa+y+j/zf6f6P/N/p/o/83+n+j/zf6f6P/N/p/o/83
+n+j/zf6f6P/N/p/o/83yej/jf7/K+j/Nzqtn9cNQ/jR9P/qyO8XPwH2Cee/NtfWzPkvw/8N/zf836Rv
zv+zRQitUpvh5Gvy/85ap9Oe9f+x2jb+X7/N/u9/rCQyWukLf4X7VyycwErRX61YlnUc8QZuT7JRELyn
FTY6WkU/qxO1+IySMGYJavBggWtHE1rH0pK2QctbyKrW0ujKQ42wZqWyn44tCe/q/Zx832jlSvBrtXOj
9ZW4Nh4FHm/QTbUdqtfZsMyuM1xd1xmPnWatWVH6K5nuT1JIGgoOf/Lm9f7h1P4hagCEdkmiVDJ6lwFd
rJEqvomtUKkMomDMer1BgmvpXg+1aUGEOjI/0M7rKpX0nozTS/SZk15HPL2SI4/f5D9wWzf7lfRDpTbO
7kykKhxVMZ7opyUfw89K5fTozcnOXm/vn2d7h6f7R4enbJvd0q6YBRPWqsOfWOo/N/T3X1L/UT+HAf1B
p3T6NYduOGGo/qqfI/Wvvvkv+8pWT/PXoj7dCUcqi7wWg5iu3qs/EnpSveQldvbWlXrrqq9q1bfVX8dT
f+WfHmS9q1QqLh+wHipl1djsBUmMmq4qjgfS5G5Ri9RY41fUeWwp+pa1d4N79bHaNqNuztTcqZpIbbQf
E91nktHGhx252I9N7HukFEcTRRIT7Uf5MTR1VnqTxi7uUlW5r8bdtpXEg8bf4TtgkgSR3Lb0/qVVyyjF
EedABksKbdSxa9J5Dv2dkOn8IruJu4O+tjhAEs1+4Bbqh0kMmIDvwo9xeBUz16mYHWyHXT6oTWfHxH23
p8uChrbjONLvWekTHwcL3muqX7USDTdw9PdgX2W7PERobu4eZMCGtHAf+xZuNGXoCfpmWa2dty7uLPwW
pMs9yZnuj2LSTdS0Q9QmVwfW+QHt89wWanrXuE0/4u5CDwb1HHdy7m51Te6sciWxsWFaYlvTC+W2nt/m
8E6dVbEpXuoRB82uOqGLytPC3VptPkFMQKaHuxl5pxDh6T6BWwu7JCO0qG/g5fvfmu4jubCT5H29tLC3
GEv7K/8K6C793dBbOPXpGXVVtdls1m7lgg7j3ryB/+m98OPMCWrlfEaoZp7TysoAgFnvfMCBQPhVXZTK
wW8Q1Nke/cEt6dJ7lmbm2BYfy8QLnsdSvkiGEqmCHulkLBtaCLC5wKJpzwwasFQnaJ2XKFbgjjW9Qzm3
2G327t0FVBvfIFOqQyU7KMSgrfvqXzxCkzEQanzlXFbtfU1wH5u2MWtpnfL6yGQwEDdND63cqjW2va0w
O69eOOnl7P9e8Mu7B0rIX5sez/pr88fpJ3Wa7AwEN0SShhQxbgvpHD2uWh5d9Q4YoL7wcN8MeaEnQ3YF
wmIl5ZH4gyqDT8+zkq2VUTDmKyg4r7j8agXeW2li3hUQZla0pNlsd1agZN4Ibec9yGupWLGAgH56sVX8
7Lx87EKB+C6bNKimWiG924SO4VFcbdXzN2tlrCfRa05rKCFM38gaNJcQXF4WD/qTGCfwHGyfoVOFgV7o
3DrRq5eGzlSvp8S0lQTNfmQ0YbU2dxjM5Fkye0MAzHS44J4eGVAAqSG/YQMQ0fvQbzQKCFrJGkgzVm24
8Y8XCFKy3MDKJuOTRakp1lgQkBKCzILMBNIekscZCg8i3sRtaNzinG4c63/fyefKyOUDsKkP6WfQxYeB
/yFM+u/k37ILm0zK6IbiTh+yLbkP6K7wgzVDH16DdQW8Ac3xIYzElR1z/WsASxHf1T/w/4XvZrXKCeQV
TalkdxaSUZ+Z01C/MwLq5zhwE4/X4Hf1vNv4f3bjr1bj597F32qFOarGVXP/1eHRyd5O93Qvf1SbklqF
e6ONw2CCoodsslSoUv/VWXtmqI6hpwr91hzbsTOq5mhTGP7jrTmCGwyCGQRkt1ADQLvbcXMYBUlYbdfu
8h+dWhHpkFkjibnTJ8c/ynO+td66ePgkyoHnSE/boTLagCZK/JzPYpPlKKSQ+SJFUB2dvQcDeWxrIK86
Y3cLcRJ4xrW6YogrVoqp7AM7DFJkAKjcoUgpgvyu06paU2PY5ONQrZtxpa3W3+Wt8CLeWr/8YmHHYiVm
QRZLRe7xG5p0gwzEQtwvh0LcSpkpwGIolmqa0vyuRlb13dN3Tz+8g/8+/Bf8A6MPi1ne5IrQOeS9qGRY
hbexovR4Ojdkxj9TfDEFVv1W1Xr6FJenHz7gv/9Fl9bM0MWFnPATXljqzawa9Lb/tlIH6O/EIvIyF3/X
FAFVX3p/qr6IgyrTgrrleDV2ycANiKEAVlVvgaRZozGXyihF4tkr1CCOTav8vvqD2hj8Ow4ijn9j25lt
H5LboiGtYFVh7a2L8hzGgRkNm6idizHa4KhqNayasnSdvv2rtWARVeqJvHXj9HuBWG1GHptXFW20WlK6
zC9YW5GRSFIoBwnlhQrZs3XQQpABSZyv0lOYvmg9RpkiDjmueHX+Mi0XI4oydaH0+a/peVlduFID7nRG
xrA7InISEbMXgMzveXSxxXaBIaDSpCB3s2e3UOLdM2W39Ow2HR9wR0jW9wLnvTpYEcLX8Ci33dcKtyYw
1HuqcsKRMXHSEyo13lNGtWvsQa85AvUxXY/kbxTO5db95ECK/51MasmCa9pyu0r/br+zftEWj0HU05bU
v75LjaCmbboZZKYLyHB5f8kdNNpVBxGULWnJardasDjNa0FF3096tcnU4QFty7eyo2wx7zPl/Bj6a01l
uK+s/7cKBlA0ZLf1CKizzFZt+591pg3Vtt9Cy1WLZwF0O5LSVZ0doPdh0Uvzmv0TrlDz8XZJtdZnDg9A
xbLDA9RZr3eOCwZ3U2cIgPT9xBuwBD89O3mzc/bmpHvAjt6cHewf7rGXRyfstjDFFDRDhRv3k7vV0/Nu
fqYC6/dKPHYEra24qfBmmZ3fS+XodqvEZ0U9ZbW5xJUz3drDuK6/iL+qzLR8hUzKapT9jbXZL8zj/v0F
LQTH2ZR/H4jXBZrnVNTFYv2VhtB/2F7C93DxcH85JKLNpQOc+YvVH9rrvLP1lWtNIwhFgHkAir0079lC
FP1IsFwCmKp83Sa/so3WPU37jVD0M5D0QWj6QES91a1ylx9WeAiw3oueixD0wXj5MZiJpwERMR+IKl8Z
FDNg/JLAdT9mfAXceBB25PhRWHHppeLYhoUqLQHzxd/MgmviBTaqTHDLsonXVdSUydgV/tJFFlrXV+lF
2pOtYtgqR0iKGsEsG0+TWne1WR2yXvcFIIjQacfttBaoJ69a+AAFGIC82zv1rl6iZK+ojCr6FSxmVSZo
XFnOhHc1pVyRm+1CMzULCicXV1i+B417hbmgKzPtcobM2WtYjey9IivFvtZqfayJqhOOvWwxo++lEczz
ZwXtPnHqdD8ifycbtQXlRWHjIs+oR7RVq1Tm8PW0jguZ+4NXQHN4dk58PhuOg9j2MpBszap3ilOmrMdf
CsPEOoMQxZGcEPRVhCrF5UpGZks2uAeipuouk3G1TXJXD5tqUKssQPcFy/qytihNP7EjPzW7IAp6OwFx
wKNjBVXE0lS8tn1lUoGPcyOLQhYcXRQxF4mUNGrFr6J3kF61MASBLCl1gEo1G25C7aDQA3ohe9IoDl8k
WJu34fYJEAzfCKwH3pgPvveA7psp0xOc58/KzPkZq94W2uIubd7SqvYK16Dq+ChZvizE5IetZnPHCluL
CT3GOvYz4ZqmRrYLWjy3kp4BxXGjz5strsPXW+w+ZKGbsrN7hYbZ0+/1OSvejNBHLHsfsOT9+OXuFxRZ
7hVVyvN3ifjgcp8MmdQ0hzvqoihPzJMraFszB/gp5bY+PUhq7SKya2Oy/oxFWSVfTpEAAivdKPFTvXth
wxZF8xLo7qhsBLy5lELZr93pnNe0vm8WslyJwFOnHrcX6vvpgvT9U1CdvTrNZD+2tTMysw1ebOwHS4CV
CtSsR2yh16OW7PVQOO31dDMqSdUYHZtkkkkmmWSSSSaZZJJJJplkkkkmmWSSSSaZZJJJJplkkkkmmWSS
SSaZZJJJJplk0vL0/wFBhZXpAJABAA=="

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
