# GPUI (DeltaQ fork)

A trimmed copy of [zed-industries/zed](https://github.com/zed-industries/zed)
containing only [GPUI](crates/gpui), Zed's GPU-accelerated UI framework, and
the crates it depends on. It is maintained by DeltaQ for
[Rexa](https://github.com/deltaqdev) and Zedra, and syncs with upstream on a
weekly cadence.

This tree is a modified subset of Zed: crates outside GPUI's dependency closure
are removed, the root `Cargo.toml` is regenerated, and changes carried ahead of
upstream are listed in [`deltaq/changes.toml`](deltaq/changes.toml). Kept
crates are otherwise unmodified.

Documentation for DeltaQ developers: <https://gpui.deltaq.dev>

## Layout

| Path | What |
|---|---|
| `crates/gpui*` | GPUI and its platform backends |
| `crates/{collections,util,sum_tree,scheduler,http_client,...}` | Upstream crates GPUI depends on |
| `tooling/` | Upstream `perf` (test runner for `util_macros`) and dylint lints for GPUI |
| `deltaq/` | Fork tooling: keep-list, sync and PR-tracking scripts, [conventions](deltaq/CONVENTIONS.md) |
| `site/` | Source of gpui.deltaq.dev |

## Building

```
cargo build -p gpui
cargo run -p gpui --example hello_world
cargo test --workspace
script/clippy
```

Linux needs the build dependencies listed in `.github/workflows/deltaq-ci.yml`.

## Working with upstream

```
deltaq/scripts/status             # drift from zed main, tracked changes
deltaq/scripts/sync-upstream      # merge zed main
deltaq/scripts/adopt-pr <n>       # take a Zed PR before it lands
deltaq/scripts/new-change <name>  # start work meant for Zed
deltaq/scripts/prepare-upstream-pr <name>
```

Read [`deltaq/CONVENTIONS.md`](deltaq/CONVENTIONS.md) before changing a kept
crate.

## License

Apache-2.0, see [LICENSE-APACHE](LICENSE-APACHE). GPUI and the other crates
here are copyright Zed Industries and contributors. Fonts under `assets/fonts`
are under the SIL Open Font License.
