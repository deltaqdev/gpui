# Shared helpers for deltaq scripts. Source, do not execute.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
UPSTREAM_URL="https://github.com/zed-industries/zed"
UPSTREAM_REPO="zed-industries/zed"
# Remote holding your personal fork of Zed, used to push branches for Zed PRs.
ZED_FORK_REMOTE="${ZED_FORK_REMOTE:-zedfork}"
MAIN_BRANCH="${MAIN_BRANCH:-gpui}"

ensure_upstream_remote() {
    if ! git -C "$ROOT" remote get-url upstream >/dev/null 2>&1; then
        git -C "$ROOT" remote add upstream "$UPSTREAM_URL"
    fi
}

require_clean_tree() {
    if [[ -n "$(git -C "$ROOT" status --porcelain --untracked-files=no)" ]]; then
        echo "error: working tree is not clean" >&2
        exit 1
    fi
}

regenerate_workspace() {
    # $1: path to the Cargo.toml to filter (defaults to the current one)
    local source="${1:-$ROOT/Cargo.toml}"
    local filtered
    filtered="$(mktemp)"
    "$ROOT/deltaq/scripts/filter-workspace" "$source" > "$filtered"
    mv "$filtered" "$ROOT/Cargo.toml"
    # cargo drops lockfile entries nothing depends on any more; the second pass
    # then removes patches and profile overrides for crates that left the lock
    (cd "$ROOT" && cargo metadata --format-version 1 >/dev/null)
    "$ROOT/deltaq/scripts/filter-workspace" "$ROOT/Cargo.toml" > "$filtered"
    mv "$filtered" "$ROOT/Cargo.toml"
    # and a final resolve clears the [[patch.unused]] entries the first pass wrote
    (cd "$ROOT" && cargo metadata --format-version 1 >/dev/null 2>&1)
}
