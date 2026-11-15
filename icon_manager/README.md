# icon_manager

[Lucide](https://lucide.dev) icons embedded at compile time from `assets/icons/*.svg`, plus small helpers for tinting them and pairing them with text.

## Using an icon

```rust
use icon_manager as icons;

// Sized and coloured icon
icons::tint(icons::trash_2(), 14.0, icons::muted())

// Icon + label, for buttons
icons::label(icons::plus(), "New chat", 14, icons::on_primary())
```

Each icon is a function returning an `iced::widget::Svg`, e.g. `folder_open()`, `send()`, `terminal()`, `play()`. The full list is at the bottom of `src/lib.rs`.

## Helpers

| Function | Description |
| --- | --- |
| `tint(icon, size, color)` | Fixes width/height and sets the stroke colour (Lucide strokes use `currentColor`). |
| `label(icon, caption, size, color)` | Row of an icon (size + 3 px) and text, 6 px apart. |
| `muted()` | Body text colour at 70% opacity. |
| `accent()` | Brand gold. |
| `on_primary()` | Text colour on gold buttons. |
| `on_danger()` | Text colour on danger buttons. |

## Adding an icon

```bash
make add-icon sun moon hard-drive
```

This downloads the SVG into `assets/icons/` (stroke width 1.5) and appends an `icon_fn!` line to the Rust file configured by `ICON_RS` in the Makefile. Check that the line landed in `icon_manager/src/lib.rs`; the Makefile default points at `src/icons.rs`. You can also add a line by hand:

```rust
icon_fn!(sun, "sun.svg");
```

The `icon_fn!` macro uses `include_bytes!` relative to `CARGO_MANIFEST_DIR/../assets/icons/`, so the SVG must exist before building.

## Dependencies

`iced` (with `svg`), [`style`](../style) for the palette.