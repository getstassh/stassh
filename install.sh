#!/bin/sh

set -eu

REPO="${STASSH_INSTALL_REPO:-getstassh/stassh}"
BINARY_NAME="stassh"
INSTALL_DIR="${STASSH_INSTALL_DIR:-$HOME/.local/bin}"
VERSION=""
tmp_dir=""
archive_file=""
checksums_file=""
tmp_version=""

cleanup() {
    [ -n "$tmp_version" ] && rm -f "$tmp_version"
    [ -n "$archive_file" ] && rm -f "$archive_file"
    [ -n "$checksums_file" ] && rm -f "$checksums_file"
    [ -n "$tmp_dir" ] && rm -rf "$tmp_dir"
}

trap cleanup EXIT INT TERM

usage() {
    cat <<'EOF'
Usage: install.sh [--version <tag>] [--dir <path>]

Options:
  --version <tag>  Install a specific GitHub release tag, for example v0.1.0
  --dir <path>     Install directory (default: ~/.local/bin)
  -h, --help       Show this help text
EOF
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --version)
            [ "$#" -ge 2 ] || {
                printf 'missing value for %s\n' "$1" >&2
                exit 1
            }
            VERSION="$2"
            shift 2
            ;;
        --dir)
            [ "$#" -ge 2 ] || {
                printf 'missing value for %s\n' "$1" >&2
                exit 1
            }
            INSTALL_DIR="$2"
            shift 2
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            printf 'unknown argument: %s\n' "$1" >&2
            usage >&2
            exit 1
            ;;
    esac
done

need_cmd() {
    command -v "$1" >/dev/null 2>&1 || {
        printf 'missing required command: %s\n' "$1" >&2
        exit 1
    }
}

download() {
    url="$1"
    dest="$2"

    if command -v curl >/dev/null 2>&1; then
        curl -fsSL "$url" -o "$dest"
        return
    fi

    if command -v wget >/dev/null 2>&1; then
        wget -qO "$dest" "$url"
        return
    fi

    printf 'missing required command: curl or wget\n' >&2
    exit 1
}

need_cmd uname
need_cmd mktemp
need_cmd tar
need_cmd chmod
need_cmd mkdir

OS=$(uname -s)
ARCH=$(uname -m)

case "$OS" in
    Linux)
        OS_PART="unknown-linux-gnu"
        ;;
    Darwin)
        OS_PART="apple-darwin"
        ;;
    *)
        printf 'unsupported operating system: %s\n' "$OS" >&2
        exit 1
        ;;
esac

case "$ARCH" in
    x86_64|amd64)
        ARCH_PART="x86_64"
        ;;
    arm64|aarch64)
        ARCH_PART="aarch64"
        ;;
    *)
        printf 'unsupported architecture: %s\n' "$ARCH" >&2
        exit 1
        ;;
esac

TARGET="${ARCH_PART}-${OS_PART}"

if [ -z "$VERSION" ]; then
    api_url="https://api.github.com/repos/$REPO/releases/latest"
    tmp_version=$(mktemp)
    download "$api_url" "$tmp_version"
    VERSION=$(sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$tmp_version" | head -n 1)
    rm -f "$tmp_version"
    tmp_version=""
fi

[ -n "$VERSION" ] || {
    printf 'failed to determine the latest release version\n' >&2
    exit 1
}

ASSET_NAME="${BINARY_NAME}-${VERSION#v}-${TARGET}.tar.gz"
BASE_URL="https://github.com/$REPO/releases/download/$VERSION"

tmp_dir=$(mktemp -d)
archive_file="$tmp_dir/$ASSET_NAME"
checksums_file="$tmp_dir/SHA256SUMS"

download "$BASE_URL/$ASSET_NAME" "$archive_file"
download "$BASE_URL/SHA256SUMS" "$checksums_file"

expected_checksum=$(grep "  $ASSET_NAME$" "$checksums_file" | awk '{print $1}')
[ -n "$expected_checksum" ] || {
    printf 'could not find checksum for %s\n' "$ASSET_NAME" >&2
    exit 1
}

if command -v sha256sum >/dev/null 2>&1; then
    actual_checksum=$(sha256sum "$archive_file" | awk '{print $1}')
elif command -v shasum >/dev/null 2>&1; then
    actual_checksum=$(shasum -a 256 "$archive_file" | awk '{print $1}')
else
    printf 'missing required command: sha256sum or shasum\n' >&2
    exit 1
fi

[ "$expected_checksum" = "$actual_checksum" ] || {
    printf 'checksum mismatch for %s\n' "$ASSET_NAME" >&2
    exit 1
}

tar -xzf "$archive_file" -C "$tmp_dir"

mkdir -p "$INSTALL_DIR"

if command -v install >/dev/null 2>&1; then
    install "$tmp_dir/$BINARY_NAME" "$INSTALL_DIR/$BINARY_NAME"
else
    cp "$tmp_dir/$BINARY_NAME" "$INSTALL_DIR/$BINARY_NAME"
    chmod 755 "$INSTALL_DIR/$BINARY_NAME"
fi

printf 'installed %s %s to %s/%s\n' "$BINARY_NAME" "$VERSION" "$INSTALL_DIR" "$BINARY_NAME"

case ":$PATH:" in
    *":$INSTALL_DIR:"*)
        ;;
    *)
        printf 'warning: %s is not on your PATH\n' "$INSTALL_DIR" >&2
        ;;
esac
