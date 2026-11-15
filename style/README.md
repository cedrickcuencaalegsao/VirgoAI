# style

Virgo AI's look and feel: deep navy surfaces with a single gold accent. All colours are defined here and nowhere else.

**Palette:** `#14213D` navy surface, `#FCA311` gold, `#E5E5E5` text, with darker `bg` and `sidebar_bg` steps and a lighter `surface_2`.

## What's included

**Scales**

- Spacing: `SP_1`…`SP_6` (4, 8, 12, 16, 24)
- Radii: `R_CONTROL` (12), `R_PANEL` (20)
- `READING_WIDTH` (780) for the centred chat column

**Theme** – `palette()` and `theme()` build the custom iced `Theme` named "Virgo". `bold()` returns Inter Bold, so `Inter-Bold.ttf` must be loaded in `main.rs`.

**Text styles** – `dim`, `success`, `danger_text`, `accent`.

**Containers** – `sidebar`, `card`, `inset`, `user_bubble`, `plan_card`, `error_card`, `thinking_box`, `code_block`, `code_area`, `row_added`, `row_removed`, `row_selected`, `composer`, `indicator(active)`, `dot(color)`, `map_area`, `stat_pill`.

**Buttons** – `nav_item(active)`, `primary_button` (gold gradient), `send_button` and `stop_button` (round), `danger_button`, `ghost_button`, `chip`, `note_card(active)`, `filter_chip(active)`.

**Inputs** – `composer_input`, `composer_editor`, `plan_editor`, `field`, `pick` (pick list), and `scroll` (thin translucent scrollbar).

**Memory map** – `group_color(i)` gives a stable colour per note group from a six-colour palette.

## Usage

```rust
button(text("Save")).style(style::primary_button);
container(content).style(style::card);
text("secondary").style(style::dim);
```

Helpers such as `style::alpha(color, a)` build translucent variants of a colour.

## Dependencies

`iced`.