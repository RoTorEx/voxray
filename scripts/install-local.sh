#!/usr/bin/env sh
set -eu

binary="${1:?binary path is required}"
install_dir="${2:?install directory is required}"
[ -x "$binary" ] || { echo "ERROR: missing binary $binary" >&2; exit 1; }

mkdir -p "$install_dir"
chmod 0700 "$install_dir"
bin_dir="$install_dir/bin"
mkdir -p "$bin_dir"
temporary="$bin_dir/.voxray-install-$$"
trap 'rm -f "$temporary"' EXIT HUP INT TERM
cp "$binary" "$temporary"
chmod 0755 "$temporary"
mv "$temporary" "$bin_dir/voxray"
if [ -f "$install_dir/voxray" ]; then
    rm "$install_dir/voxray"
fi
trap - EXIT HUP INT TERM

profile=""
case "${SHELL:-}" in
    */bash) profile="$HOME/.bashrc" ;;
    *) profile="$HOME/.zshrc" ;;
esac
if [ "$install_dir" = "$HOME/.x-cli-voxray" ]; then
    line='export PATH="$HOME/.x-cli-voxray/bin:$PATH"'
else
    line="export PATH=\"$bin_dir:\$PATH\""
fi
if [ -n "$profile" ]; then
    touch "$profile"
    grep -Fqx "$line" "$profile" || printf '\n# x-cli-voxray\n%s\n' "$line" >> "$profile"
fi

echo "Installed $bin_dir/voxray"
echo "For this shell: export PATH=\"$bin_dir:\$PATH\""
