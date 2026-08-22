#!/usr/bin/env sh
set -eu

fail() {
    echo "ERROR: $*" >&2
    exit 1
}

usage() {
    cat <<'EOF'
Prepare one voxray release.

Usage:
  make release

The command checks main, asks for the exact MAJOR.MINOR.PATCH version, runs
make check, updates Cargo.toml, Cargo.lock, and CHANGELOG.md, creates one release
commit, and creates the matching annotated tag.

After review, run:
  make release-push
EOF
}

read_version() {
    sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1
}

validate_version() {
    version="$1"
    printf '%s\n' "$version" \
        | grep -Eq '^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$' \
        || fail "version must be MAJOR.MINOR.PATCH"
    old_ifs="$IFS"
    IFS=.
    set -- $version
    IFS="$old_ifs"

    [ "$#" -eq 3 ] || fail "version must be MAJOR.MINOR.PATCH"
    case "${1:-}" in ''|*[!0-9]*) fail "version must be MAJOR.MINOR.PATCH" ;; esac
    case "${2:-}" in ''|*[!0-9]*) fail "version must be MAJOR.MINOR.PATCH" ;; esac
    case "${3:-}" in ''|*[!0-9]*) fail "version must be MAJOR.MINOR.PATCH" ;; esac
}

version_is_greater() {
    awk -v current="$1" -v target="$2" 'BEGIN {
        split(current, c, "."); split(target, t, ".")
        for (i = 1; i <= 3; i++) {
            if (t[i] > c[i]) exit 0
            if (t[i] < c[i]) exit 1
        }
        exit 1
    }'
}

update_metadata() {
    current="$1"
    target="$2"
    today="$(date +%Y-%m-%d)"
    temp_dir="${TMPDIR:-/tmp}/voxray-release-$$"
    mkdir -p "$temp_dir"

    grep -qx '## \[Unreleased\]' CHANGELOG.md \
        || fail "CHANGELOG.md must contain an exact ## [Unreleased] heading"
    ! grep -Eq "^## \[$target\] - " CHANGELOG.md \
        || fail "CHANGELOG.md already contains release $target"

    awk -v current="$current" -v target="$target" '
        !done && $0 == "version = \"" current "\"" {
            print "version = \"" target "\""
            done = 1
            next
        }
        { print }
        END { if (!done) exit 1 }
    ' Cargo.toml > "$temp_dir/Cargo.toml" \
        || fail "could not update Cargo.toml"

    awk -v target="$target" '
        $0 == "name = \"voxray\"" { package = 1 }
        package && !done && /^version = / {
            print "version = \"" target "\""
            done = 1
            package = 0
            next
        }
        { print }
        END { if (!done) exit 1 }
    ' Cargo.lock > "$temp_dir/Cargo.lock" \
        || fail "could not update Cargo.lock"

    awk -v version="$target" -v release_date="$today" '
        !done && $0 == "## [Unreleased]" {
            print
            print ""
            print "## [" version "] - " release_date
            done = 1
            next
        }
        { print }
        END { if (!done) exit 1 }
    ' CHANGELOG.md > "$temp_dir/CHANGELOG.md" \
        || fail "could not update CHANGELOG.md"

    cp "$temp_dir/Cargo.toml" Cargo.toml
    cp "$temp_dir/Cargo.lock" Cargo.lock
    cp "$temp_dir/CHANGELOG.md" CHANGELOG.md
}

case "${1:-}" in
    "") ;;
    -h|--help)
        usage
        exit 0
        ;;
    *) fail "make release does not accept arguments" ;;
esac

trap 'rm -rf "${TMPDIR:-/tmp}/voxray-release-$$"' EXIT HUP INT TERM

branch="$(git branch --show-current)"
[ "$branch" = "main" ] || fail "release must run from main, not $branch"
[ -z "$(git status --porcelain)" ] || fail "commit or remove local changes before releasing"

git fetch origin main --tags
git merge-base --is-ancestor origin/main HEAD \
    || fail "local main is behind or diverged from origin/main"

current="$(read_version)"
[ -n "$current" ] || fail "could not read version from Cargo.toml"
printf "Current version: %s\n" "$current"
printf "Release version (MAJOR.MINOR.PATCH): "
read -r target
[ -n "$target" ] || fail "release version is required"
validate_version "$target"
version_is_greater "$current" "$target" \
    || fail "release version must be greater than current version $current"

tag="v$target"
! git rev-parse --verify "refs/tags/$tag" >/dev/null 2>&1 \
    || fail "tag $tag already exists"

make check
update_metadata "$current" "$target"
make check

git add Cargo.toml Cargo.lock CHANGELOG.md
git diff --cached --quiet && fail "release produced no metadata changes"
git commit -m "build: bump version to \`$target\`"
git tag -a "$tag" -m "Release $target"

echo "Prepared $tag"
echo "Run: make release-push"
