# Application and contexts

Everything in GPUI starts from an `Application`. Its `run` callback receives
`&mut App`, the root context: global state, entities, windows, and system
services all hang off it.

Contexts are the argument named `cx`. There are four you will meet:

| Type | Where you get it | Use it for |
|---|---|---|
| `App` | `Application::run`, event handlers on elements | Reading and updating entities, globals, opening windows |
| `Context<T>` | Inside `Entity<T>::update` and `Render::render` | Same as `App`, plus `cx.notify()`, `cx.emit()`, `cx.spawn()` bound to this entity |
| `AsyncApp` | `cx.spawn(...)` | Held across `.await` points on the foreground thread |
| `AsyncWindowContext` | `cx.spawn_in(window, ...)` | As above, with a window attached |

`Context<T>` dereferences to `App`, so a function that takes `&App` also
accepts `&Context<T>`.

When a function takes callbacks, they come after `cx`. When a function needs
the window, the argument is `window: &mut Window` and it comes before `cx`.

Inside an `update` closure, always use the inner `cx` the closure gives you.
Reaching for an outer context inside the closure is a double borrow and
panics at runtime.

Zed's own [Contexts in depth](../upstream/contexts.md) chapter describes the
full type hierarchy.
