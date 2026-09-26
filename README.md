# GPUI (DeltaQ fork)

A trimmed copy of [zed-industries/zed](https://github.com/zed-industries/zed) containing only [GPUI](crates/gpui), Zed's GPU-accelerated UI framework, and the crates it depends on. It is maintained by DeltaQ for [Rexa](https://github.com/deltaqdev/rexa-releases) and [Zedra](https://github.com/deltaqdev/zedra), and syncs with upstream on a weekly cadence.

Documentation: <https://gpui.deltaq.dev>

## Notice

The motivation of this fork is maintaining a GPUI version that powers our projects at DeltaQ: Rexa and Zedra. Currently we have no crates.io release, consumers vendor it as a git submodule.

We aim to contribute our changes back to Zed. GPUI is excellent, and it would be even better as an independent project moving at its own pace. Thanks to the Zed team for building it.

## Building

```sh
cargo build -p gpui
cargo run -p gpui --example hello_world
cargo test --workspace
script/clippy
```

## Working with upstream

```sh
deltaq/scripts/status                      # drift from zed main, tracked changes
deltaq/scripts/sync-upstream               # merge zed main
deltaq/scripts/adopt-pr <n>                # take a Zed PR before it lands
deltaq/scripts/new-change <name>           # start work meant for Zed
deltaq/scripts/prepare-upstream-pr <name>  # open the Zed PR for a change
```

Read [`deltaq/CONVENTIONS.md`](deltaq/CONVENTIONS.md) before changing a kept crate.

## License

Apache-2.0, see [LICENSE-APACHE](LICENSE-APACHE). GPUI and the other crates here are copyright Zed Industries and contributors. Fonts under `assets/fonts` are under the SIL Open Font License.
