#!/bin/sh
set -eu

REPO="harmont-dev/harmont-cli"
INSTALL_DIR="${HM_INSTALL_DIR:-$HOME/.harmont/bin}"

main() {
    need_cmd curl
    need_cmd tar
    need_cmd uname

    local os arch target version url archive

    os=$(detect_os)
    arch=$(detect_arch)
    target="${arch}-${os}"

    if [ -n "${HM_VERSION:-}" ]; then
        version="$HM_VERSION"
    else
        version=$(latest_version)
    fi

    archive="hm-${version}-${target}.tar.gz"
    url="https://github.com/${REPO}/releases/download/v${version}/${archive}"

    echo "Installing hm ${version} (${target})..."

    mkdir -p "$INSTALL_DIR"

    curl -fsSL "$url" | tar xz -C "$INSTALL_DIR"
    chmod +x "${INSTALL_DIR}/hm"

    echo "Installed hm to ${INSTALL_DIR}/hm"

    if ! echo ":$PATH:" | grep -q ":${INSTALL_DIR}:"; then
        echo ""
        echo "Add hm to your PATH by adding this to your shell profile:"
        echo ""
        echo "  export PATH=\"${INSTALL_DIR}:\$PATH\""
    fi

    echo ""
    "${INSTALL_DIR}/hm" --version
}

detect_os() {
    local uname_s
    uname_s=$(uname -s)
    case "$uname_s" in
        Linux)  echo "unknown-linux-gnu" ;;
        Darwin) echo "apple-darwin" ;;
        *)      err "unsupported OS: $uname_s" ;;
    esac
}

detect_arch() {
    local uname_m
    uname_m=$(uname -m)
    case "$uname_m" in
        x86_64|amd64)       echo "x86_64" ;;
        aarch64|arm64)      echo "aarch64" ;;
        *)                  err "unsupported architecture: $uname_m" ;;
    esac
}

latest_version() {
    local tag
    tag=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
        | grep '"tag_name"' \
        | sed 's/.*"tag_name": *"v//;s/".*//')
    if [ -z "$tag" ]; then
        err "could not determine latest version"
    fi
    echo "$tag"
}

need_cmd() {
    if ! command -v "$1" > /dev/null 2>&1; then
        err "need '$1' (command not found)"
    fi
}

err() {
    echo "error: $1" >&2
    exit 1
}

main "$@"
