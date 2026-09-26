# Getting started

## Add GPUI to a DeltaQ project

The fork is consumed as a git submodule with path dependencies. Nothing is
published to crates.io.

```sh
git submodule add --depth 1 https://github.com/deltaqdev/gpui vendor/gpui
```

```toml
# Cargo.toml of the consuming workspace
[workspace.dependencies]
gpui = { path = "vendor/gpui/crates/gpui" }
gpui_platform = { path = "vendor/gpui/crates/gpui_platform", features = ["font-kit", "wayland", "x11"] }

[patch.crates-io]
# Cargo only honours [patch] in the root workspace, so repeat the entries
# GPUI needs. Copy the current revisions from vendor/gpui/Cargo.toml.
async-task = { git = "https://github.com/smol-rs/async-task.git", rev = "<rev>" }
calloop = { git = "https://github.com/zed-industries/calloop" }
# Crates such as gpui-component depend on `gpui` from crates.io; this points
# them at the vendored copy so there is one GPUI in the build.
gpui = { path = "vendor/gpui/crates/gpui" }
```

Set `shallow = true` for the submodule in `.gitmodules`. The full Zed history
behind this repository is about 400 MB.

## A first window

```rust,no_run
use gpui::*;

struct Hello;

impl Render for Hello {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().flex().items_center().justify_center().child("Hello from GPUI")
    }
}

fn main() {
    gpui_platform::application().run(|cx: &mut App| {
        cx.open_window(WindowOptions::default(), |_window, cx| cx.new(|_| Hello))
            .expect("failed to open window");
    });
}
```

Run the upstream examples to see more:

```sh
cd vendor/gpui
cargo run -p gpui --example hello_world
cargo run -p gpui --example uniform_list
```

## Building the fork itself

```sh
cargo build -p gpui
cargo test --workspace
script/clippy
```

Linux needs the system packages listed in `.github/workflows/deltaq-ci.yml`.
