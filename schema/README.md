# schema

JSON Schemas shared by the agent, the renderer and the web app.

| File | What it defines |
|---|---|
| [`template.v1.json`](template.v1.json) | Screen templates (SPEC §8), JSON Schema draft 2020-12 |
| [`examples/`](examples/) | The four built-in starter templates (TPL-012) |
| [`tests/invalid/`](tests/invalid/) | Broken templates with the error each must produce |

Validate templates with the `cb-template` tool:

```bash
cargo run -q --manifest-path agent/Cargo.toml -p chalkboard-template --bin cb-template -- validate schema/examples/*.json
```

## Template format v1

A template is a JSON document with a `canvas` (logical size, which also
fixes the orientation), named `styles`, optional custom `fonts`, a `soldOut`
presentation and one `root` node.

```json
{
  "schemaVersion": 1,
  "id": "my-menu",
  "name": "My menu",
  "canvas": { "width": 1920, "height": 1080, "background": "#14110f" },
  "styles": { "name": { "font": "Inter", "size": 40, "color": "#ffffff" } },
  "root": { "type": "repeater", "source": "items", "capacity": 8, "template": { "...": "..." } }
}
```

All sizes are logical pixels of the canvas. The renderer scales the whole
canvas uniformly to the real output (1080p, rotated, or 4K on the Pi 5).

### Nodes

| Type | Purpose | Required |
|---|---|---|
| `container` | Groups `children` with a `layout` | — |
| `text` | Text with bindings | `text`, `fit` |
| `price` | A price; shrinks rather than being cut off | `value`, `minSize` |
| `image` | A blob or `{{item.image}}` / `{{category.image}}` | `src`, `width`, `height` |
| `shape` | `rect`, `ellipse` or `line` | `shape`, `width`, `height` |
| `repeater` | Repeats `template` per entry; paginates beyond `capacity` | `source`, `capacity`, `template` |
| `conditional` | Shows `then` or `else` depending on `when` | `when`, `then` |

Every node may have an `id`, a `style` (a name or a list of names applied
in order), `width`/`height` (pixels, `"content"` or `"fill"`), and — depending
on the parent's layout — `grow` (flex), `cell` (grid) or `x`/`y` (absolute).

### Layouts

The same models LVGL implements, so the renderer and the preview match:

- `{"type": "flex", "direction": "row" | "column", "wrap", "gap", "justify", "align"}`
- `{"type": "grid", "columns": [...], "rows": [...], "columnGap", "rowGap"}` with tracks in pixels, `"content"` or fractions such as `"1fr"`
- `{"type": "absolute"}` for decorations positioned with `x`/`y`

### Bindings

Text may contain `{{path | filter:arg}}`. There is no logic in strings;
use `conditional` nodes instead.

| Path | Available |
|---|---|
| `page.title`, `page.language`, `screen.name` | everywhere |
| `category.title`, `category.description` | inside a repeater over `categories` |
| `item.name`, `item.description`, `item.price`, `item.prices.<variant>` | inside a repeater over `items` or `category.items` |

Filters: `price` and `secondary` (prices only, in the primary or the
secondary display currency), `upper`, `lower`, `truncate:N`,
`default:"text"`.

### Text that does not fit

Every `text` node declares `fit`:

- `{"mode": "wrap", "maxLines": N}` — wrap, ellipsis on the last line
- `{"mode": "shrink", "minSize": N}` — shrink the font down to `minSize`, then ellipsis
- `{"mode": "clip"}` — one line with ellipsis

`price` nodes always shrink (to `minSize`) and are never cut off.

### Conditions

`when` holds exactly one of `soldOut`, `hasImage`, `hasDescription`,
`hasTag`, `hasBadge`, `hasVariant`, `language`, or the combinators `not`,
`all` and `any`.

### Validation

The JSON Schema checks structure. The `chalkboard-template` crate also
checks what a schema cannot: binding paths in their scope (with
"did you mean" suggestions), filters, style and font references, unique ids,
placement properties against the parent layout, value ranges, and size
limits (1 MiB, 2000 nodes, 32 levels). Errors name the exact location, for
example:

```text
root.template.children[0].text: unknown binding 'item.nmae' (did you mean 'item.name'?)
```
