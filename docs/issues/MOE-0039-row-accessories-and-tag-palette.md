# M9k: result rows grow accessories, and the tag palette is nine bounded tokens

- **ID**: MOE-0039
- **State**: todo
- **Labels**: ready-for-agent, feature, ux
- **Created**: 2026-10-12
- **Updated**: 2026-10-12
- **Blocks**: MOE-0044

Result rows carry a title and a subtitle and nothing else, so an Extension holding a status, a date
or a tag has nowhere to put it: AI history folds a relative timestamp into the subtitle, and the
model picker's only real tag vocabulary (`capabilities: Vec<String>`, `ai_models.rs`) rides a
wire type that never reaches the panel. ADR-0041 adopts Raycast's `List.Item.Accessory` and its
nine-name `Color` enum, mapped onto ADR-0035's semantic token layer.

## Decision

ADR-0041 holds the reasoning. The load-bearing points for implementation:

- `Item.accessories` — one tagged enum, five kinds (`tag`, `text`, `date`, `icon`), each with an
  optional `tooltip`. Shipping four of the five costs nothing over shipping one.
- **The platform formats dates; the contract carries a timestamp, not a rendered string.** `ai.rs`'s
  `relative_time` subtitle migrates onto a `date` accessory *in this same change* — doing one without
  the other renders two dates on every history row. The reason is the wire format (a timestamp can be
  sorted, localized, or rendered absolutely later), **not** liveness: history rows are fetched once
  and never re-render as time passes, so a backend-formatted string is exactly as fresh.
  `recency.rs`'s group labels stay backend-side (section names, not live values).
- **Known coverage loss, accepted deliberately.** `relative_time` is covered today by a Rust unit test
  (`relative_time_buckets`, `crates/moe-extensions/src/ai.rs`) and the UI has no test runner — only
  `typecheck` and `build`. This ticket moves tested logic into an untested layer. Either port the cases
  when a UI runner lands, or keep the Rust formatter and send both timestamp and string.
- Nine theme-paired tokens `--moe-tag-{blue,green,magenta,orange,purple,red,yellow}`; PrimaryText
  and SecondaryText are the existing `--moe-fg` / `--moe-fg-muted`, so an uncolored accessory needs
  no token. `Color.Raw` is rejected — ADR-0035's contrast guarantee is measured against the worst
  *composed* translucent backdrop, which an unseen hex cannot satisfy.
- A tag is **tinted text on a same-color transparent background, with no border**. The root row's
  kind badge stays plain text (ADR-0030's amendment: a descriptor is not a control).
- Accessories hide in the `split` shape; at most three render; the title truncates first.
- `iconEl` needs a semantic-color→class mapping — it takes only a `className` today
  (`iconEl`, `ui/src/icons.ts`).

## Delivery

- `crates/moe-core/src/contract.rs` — the `Accessory` enum and the `Item` field, serde camelCase
  like the rest of the contract.
- `ui/src/types.ts` — the contract mirror (one file shared by both windows, ADR-0018).
- `ui/src/styles.css` — the seven tag tokens, light and dark.
- `ui/src/icons.ts` — the tint mapping.
- `ui/src/main.ts` — the result-row renderer's trailing end.
- `crates/moe-extensions/src/ai.rs` — `history_item`'s subtitle → a `date` accessory.
- A relative-date formatter in `ui/src`.

## Acceptance

- A result row renders up to three accessories after the subtitle; the title truncates before any
  accessory is dropped, and the row stays 40px.
- No accessory renders in the `split` shape's 280px column.
- All seven tokens resolve in both themes, and every tag text step is ≥4.5:1 against the composed
  surface (ADR-0035's measurement rule, not a raw-contrast check).
- History rows show exactly one relative date, formatted in the UI from a timestamp on the wire, and
  the rendered string matches the Rust formatter's output for the same age.
- The root row's kind badge is unchanged — still plain text.
- `just lint`, `just test`, `just ui-build` green.

## Comments

- 2026-10-12 (agent): filed from the Raycast primitives grilling session. ADR-0041 records the
  decision; `docs/research/raycast-ui-primitives.md` holds the Raycast fidelity map and divergence #6
  (the root/result trailing-treatment split) is this issue's known visual cost.
