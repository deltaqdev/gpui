# Fork conventions

This repository is `zed-industries/zed` trimmed to GPUI. It exists so DeltaQ
products (Rexa, Zedra) can use GPUI changes before Zed merges them, ship their
own GPUI changes, and send those changes back to Zed, all without waiting on
Zed's release cycle.

Everything specific to this fork lives under `deltaq/`, `site/`, and
`.github/workflows/deltaq-*.yml`. Those paths never exist upstream, so upstream
merges never touch them.

## The one rule

**Edits to kept upstream files happen only inside a tracked change.**

Two kinds of commits touch this repository:

1. *Trim and sync commits* (the initial trim, every `sync-upstream`). These
   never edit a kept file. They only delete paths not in `deltaq/keep.txt` and
   regenerate the root `Cargo.toml`. Every kept file is byte-identical to
   upstream after such a commit. This is what keeps `git merge upstream/main`
   mechanical.
2. *Change commits*. Editing `crates/gpui/src/window.rs` for a feature or fix
   is normal and expected. It is done on a `change/<name>` branch, recorded in
   `deltaq/changes.toml`, and merged into `gpui` by pull request.

What is forbidden is a loose commit on `gpui` that edits a kept file without
a `changes.toml` entry. Such a commit belongs to no change, so it cannot be
cherry-picked onto `upstream/main` for a Zed PR, cannot be dropped once Zed
merges an equivalent, and will conflict on every future sync.

## Branches

| Branch | Purpose |
|---|---|
| `gpui` | Default. Trimmed upstream plus merged changes. Consumers pin this. |
| `sync/upstream-<sha>` | Output of `sync-upstream`, merged into `gpui` by PR. |
| `change/<name>` | One change, based on `gpui`. Own work or an adopted Zed PR head. |
| `adopt/pr-<n>`, `update/<name>-<sha>` | Merge branches produced by the scripts, merged into `gpui` by PR. |
| `to-zed/<name>` | Cherry-picks of a change onto `upstream/main`, pushed to your Zed fork for the Zed PR. |

`upstream` is a remote (`https://github.com/zed-industries/zed`), never a work
branch. Feature work happens on the trimmed tree, which builds in a fraction
of the time.

## Workflows

Sync with upstream (weekly by CI, or by hand):

```
deltaq/scripts/sync-upstream      # merge, strip, regenerate Cargo.toml, resolve
                                  # exit 2 = real conflict in a kept crate
```

Adopt a Zed PR before Zed merges it:

```
deltaq/scripts/adopt-pr 12345     # branch, merge into adopt/pr-12345, record in changes.toml
```

Start an own change and send it to Zed:

```
deltaq/scripts/new-change fast-tables        # change/fast-tables off gpui
# commit work, open a PR against gpui
deltaq/scripts/prepare-upstream-pr fast-tables   # cherry-pick onto upstream/main, open Zed PR
```

See where things stand:

```
deltaq/scripts/status
deltaq/scripts/track-changes      # queries GitHub for each tracked Zed PR
```

## Change lifecycle

Every non-upstream diff on `gpui` is one entry in `deltaq/changes.toml`.

1. `tracking`: the Zed PR is open. The daily `Track changes` workflow watches
   its head. When it moves, a fork PR titled `Update Zed #n` follows it.
2. `merged-upstream`: Zed merged it. The next `sync-upstream` takes upstream's
   version of the change's files (Zed squash-merges, so the content matches)
   and flips the entry to `done`.
3. `closed-upstream`: Zed closed it unmerged. Decide: keep it as an own change
   (edit the entry to `kind = "own"`, `zed_pr = 0`, `status = "tracking"`) or
   revert it from `gpui`.

Keep changes small and single-purpose. A change that Zed will not take (for
example a DeltaQ-only platform target) is still tracked, with `zed_pr = 0`, so
that syncs know which files to expect conflicts in.

## Consuming this repository

Vendor it as a git submodule and use path dependencies:

```toml
gpui = { path = "vendor/gpui/crates/gpui" }
gpui_platform = { path = "vendor/gpui/crates/gpui_platform" }

[patch.crates-io]
# Cargo applies [patch] only from the root workspace, so the consumer must
# repeat the entries gpui needs. Copy them from this repository's Cargo.toml.
async-task = { git = "https://github.com/smol-rs/async-task.git", rev = "..." }
calloop = { git = "https://github.com/zed-industries/calloop" }
# If the consumer also depends on crates that pull `gpui` from crates.io
# (for example gpui-component), point that name at the vendored copy:
gpui = { path = "vendor/gpui/crates/gpui" }
```

Set `shallow = true` for the submodule in `.gitmodules`; the full history is
about 400 MB.

## Licensing

Every crate kept here is Apache-2.0 (`deltaq/scripts/check-licenses` enforces
it). `LICENSE-APACHE` at the root is the licence for the whole tree.

- Apache-2.0 requires modified files to carry a notice that they changed. This
  fork satisfies that at repository level: `README.md` states that the tree is
  a modified subset of Zed, and every change is listed in `deltaq/changes.toml`.
- "Zed" and "GPUI" are Zed Industries names. Do not publish these crates to
  crates.io under their upstream names.
- Fonts under `assets/fonts` are SIL OFL 1.1; keep their licence files.
- Third-party crate licences are checked with `cargo-about` using upstream's
  accepted list (`script/licenses/zed-licenses.toml`).
- Contributing to Zed requires signing Zed's CLA on your first Zed PR.
