#!/usr/bin/env sh
set -eu

binary="${1:?binary path is required}"
install_dir="${2:?install directory is required}"
[ -x "$binary" ] || { echo "ERROR: missing binary $binary" >&2; exit 1; }

mkdir -p "$install_dir"
chmod 0700 "$install_dir"
temporary="$install_dir/.voxray-install-$$"
trap 'rm -f "$temporary"' EXIT HUP INT TERM
cp "$binary" "$temporary"
chmod 0755 "$temporary"
mv "$temporary" "$install_dir/voxray"
trap - EXIT HUP INT TERM

if [ ! -f "$install_dir/config.toml" ]; then
    cp config.example.toml "$install_dir/config.toml"
    chmod 0600 "$install_dir/config.toml"
fi

profile=""
case "${SHELL:-}" in
    */zsh) profile="$HOME/.zshrc" ;;
    */bash) profile="$HOME/.bashrc" ;;
esac
if [ "$install_dir" = "$HOME/.x-cli-voxray" ]; then
    line='export PATH="$HOME/.x-cli-voxray:$PATH"'
else
    line="export PATH=\"$install_dir:\$PATH\""
fi
if [ -n "$profile" ]; then
    touch "$profile"
    grep -Fqx "$line" "$profile" || printf '\n# x-cli-voxray\n%s\n' "$line" >> "$profile"
fi

echo "Installed $install_dir/voxray"
echo "For this shell: export PATH=\"$install_dir:\$PATH\""
