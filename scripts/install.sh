#!/usr/bin/env sh
set -eu

repo="${VOXRAY_INSTALL_REPO:-RoTorEx/voxray}"
version="${VOXRAY_VERSION:-latest}"
install_dir="${VOXRAY_INSTALL_DIR:-$HOME/.x-cli-voxray}"
token_file="${VOXRAY_GITHUB_TOKEN_FILE:-$HOME/.x-cli-voxray/gh-token}"
archive_path=""
update_path=1

usage() {
    cat <<'EOF'
Usage: voxray-install.sh [--version VERSION|latest] [--install-dir PATH] [--archive PATH] [--no-path-update]

Installs a macOS GitHub Release, or a local release archive. For the private
repository, set GH_INSTALLER_TOKEN. The token is saved with mode 0600 so
`voxray update` can use it later.
EOF
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --version) version="${2:?missing version}"; shift 2 ;;
        --install-dir) install_dir="${2:?missing install directory}"; shift 2 ;;
        --archive) archive_path="${2:?missing archive path}"; shift 2 ;;
        --no-path-update) update_path=0; shift ;;
        -h|--help) usage; exit 0 ;;
        *) echo "ERROR: unknown option: $1" >&2; usage >&2; exit 1 ;;
    esac
done

[ "$(uname -s)" = "Darwin" ] || { echo "ERROR: voxray releases currently support macOS only" >&2; exit 1; }
case "$(uname -m)" in
    arm64|aarch64) arch=aarch64 ;;
    x86_64|amd64) arch=x86_64 ;;
    *) echo "ERROR: unsupported architecture: $(uname -m)" >&2; exit 1 ;;
esac

archive="voxray-macos-$arch.tar.gz"
temp_dir="$(mktemp -d "${TMPDIR:-/tmp}/voxray-install.XXXXXX")"
trap 'rm -rf "$temp_dir"' EXIT HUP INT TERM

token="${GH_INSTALLER_TOKEN:-${GH_TOKEN:-${GITHUB_TOKEN:-}}}"
if [ -z "$token" ] && [ -r "$token_file" ]; then
    token="$(sed -n '1{s/^[[:space:]]*//;s/[[:space:]]*$//;p;q;}' "$token_file")"
fi

download() {
    url=$1
    output=$2
    if [ -n "$token" ]; then
        printf 'header = "Authorization: Bearer %s"\n' "$token" |
            curl --config - -fsSL --output "$output" "$url"
    else
        curl -fsSL --output "$output" "$url"
    fi
}

if [ -n "$archive_path" ]; then
    cp "$archive_path" "$temp_dir/$archive"
else
    [ -n "$token" ] || {
        echo "ERROR: private release download requires GH_INSTALLER_TOKEN" >&2
        exit 1
    }
    if [ "$version" = latest ]; then
        base="https://github.com/$repo/releases/latest/download"
    else
        base="https://github.com/$repo/releases/download/v${version#v}"
    fi
    download "$base/$archive" "$temp_dir/$archive"
    download "$base/$archive.sha256" "$temp_dir/$archive.sha256"
    expected="$(awk 'NR == 1 { print $1 }' "$temp_dir/$archive.sha256")"
    actual="$(shasum -a 256 "$temp_dir/$archive" | awk '{ print $1 }')"
    [ "$actual" = "$expected" ] || { echo "ERROR: archive checksum mismatch" >&2; exit 1; }
fi

tar -xzf "$temp_dir/$archive" -C "$temp_dir"
[ -x "$temp_dir/voxray" ] || { echo "ERROR: archive does not contain voxray" >&2; exit 1; }

mkdir -p "$install_dir"
chmod 0700 "$install_dir"
cp "$temp_dir/voxray" "$install_dir/.voxray-install-$$"
chmod 0755 "$install_dir/.voxray-install-$$"
mv "$install_dir/.voxray-install-$$" "$install_dir/voxray"
if [ ! -f "$install_dir/config.toml" ] && [ -f "$temp_dir/config.example.toml" ]; then
    cp "$temp_dir/config.example.toml" "$install_dir/config.toml"
    chmod 0600 "$install_dir/config.toml"
fi

if [ -n "${GH_INSTALLER_TOKEN:-}" ]; then
    mkdir -p "$(dirname "$token_file")"
    umask 077
    printf '%s\n' "$GH_INSTALLER_TOKEN" > "$token_file.tmp.$$"
    mv "$token_file.tmp.$$" "$token_file"
    chmod 0600 "$token_file"
fi

if [ "$update_path" -eq 1 ]; then
    case "${SHELL:-}" in
        */bash) shell_rc="$HOME/.bashrc" ;;
        *) shell_rc="$HOME/.zshrc" ;;
    esac
    if [ "$install_dir" = "$HOME/.x-cli-voxray" ]; then
        path_line='export PATH="$HOME/.x-cli-voxray:$PATH"'
    else
        path_line="export PATH=\"$install_dir:\$PATH\""
    fi
    touch "$shell_rc"
    grep -Fqx "$path_line" "$shell_rc" || printf '\n# x-cli-voxray\n%s\n' "$path_line" >> "$shell_rc"
fi

echo "Installed $install_dir/voxray"
echo "Run: export PATH=\"$install_dir:\$PATH\""
