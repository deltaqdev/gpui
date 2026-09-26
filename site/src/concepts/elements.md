# Elements and rendering

A view is an `Entity<T>` where `T: Render`. `render` returns an element tree
laid out with flexbox, styled with Tailwind-like methods.

```rust,ignore
impl Render for Card {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_2()
            .p_4()
            .border_1()
            .child(self.title.clone())
            .when(self.expanded, |this| this.child(self.body.clone()))
            .when_some(self.footer.as_ref(), |this, footer| this.child(footer.clone()))
    }
}
```

- `SharedString` is the text type. It wraps `&'static str` or `Arc<str>` and
  implements `IntoElement`, so it can be passed to `.child` directly.
- `.when(condition, ...)` and `.when_some(option, ...)` keep conditional
  children readable without `if` blocks that return different types.
- Components that are built only to be turned into elements implement
  `RenderOnce` and derive `IntoElement`, taking `self` by value and `&mut App`
  instead of a `Context<Self>`.
- Long lists use `uniform_list` or `list` so only visible rows are rendered.
  See the `uniform_list` and `list_example` examples in `crates/gpui/examples`.

Rendering is immediate mode over retained state: each frame rebuilds the
element tree from entity state, and GPUI diffs layout and paints on the GPU.
Keep `render` cheap and free of side effects. In particular, never call
`entity.update` on another entity from inside `render`; the fork's dylint
lints under `tooling/lints` flag this.
