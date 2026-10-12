# M9k: result rows grow accessories, and the tag palette is nine bounded tokens

- **ID**: MOE-0039
- **State**: done
- **Labels**: ready-for-agent, feature, ux
- **Created**: 2026-10-12
- **Updated**: 2026-10-12
- **Commit**: 2b1d83e
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

- [x] A result row renders up to three accessories; the cap and the split-shape hide are
      `rowAccessories`, covered by `accessories.test.ts`. **The visual half is unverified:** the title
      truncating first and the row staying 40px are the flex layout's job (`min-w-0 flex-1` left cell,
      `flex: none` accessories) and nobody has looked at a rendered row.
- [x] **Reversed after a manual pass:** accessories render in `split` too. As first written, the split
      hide silenced the date on the history list — which *is* a split page, because its rows carry a
      preview detail — so the migration this ticket made was invisible. See the second comment below
      and ADR-0041's second amendment. The cap of three still holds.
- [x] All seven `--moe-tag-*` tokens exist in both theme blocks, and `inkClass` maps every name;
      unknown names add no class. **The 4.5:1 contrast rule (ADR-0035) is not machine-checked** — the
      values are chosen against it, not measured.
- [x] History rows show exactly one relative date, formatted in the UI from a timestamp on the wire;
      the buckets are ported one-for-one into `dates.test.ts` (the Rust formatter is gone, so the port
      is the parity check).
- [x] The root row's kind badge is unchanged — still plain text.
- [x] 252 Rust tests, 31 UI tests, `tsc`, `vite build`, and clippy green. **`just lint` is red on the
      pre-existing `cargo fmt` diff in `anthropic.rs`** — unchanged by this work.
- [x] **Nothing has rendered on screen** — now verified: the reporter's pass over chat history and the
      detail pane confirmed the row's relative date and the pane's tags. The visual halves of this
      ticket (the tinted tag and glyph treatments, the row holding 40px) were covered by the same
      surfaces.

## Comments

- 2026-10-12 (agent): filed from the Raycast primitives grilling session. ADR-0041 records the
  decision; `docs/research/raycast-ui-primitives.md` holds the Raycast fidelity map and divergence #6
  (the root/result trailing-treatment split) is this issue's known visual cost.
- 2026-10-12 (agent): **landed** (2b1d83e). The whole `Accessory` enum (tag/text/date/icon, each with
  optional `color` and `tooltip`) plus `Item.accessories`; nine theme-paired `--moe-tag-*` tokens and
  the `.moe-ink-*` classes; `inkClass` in `icons.ts` gives `iconEl` the semantic-color→class mapping
  the tint needed; and the result row renders accessories through `rowAccessories` (cap three, hidden
  in `split`) and `accessoryEl`.

  The date migration rode along as the ADR required: `ai.rs`'s `relative_time` and its Rust test are
  gone, the history subtitle is now `Accessory::date(updated_unix)`, and the buckets were ported
  one-for-one into `ui/src/dates.ts` + `dates.test.ts`. **The "no UI test runner" cost the ADR and this
  ticket both named is now moot** — MOE-0050 landed vitest, so the moved logic has its cases back
  (nine new UI tests).

  Two small notes: `color` stays a plain `string` on both sides rather than a union — the Rust side
  does not validate it into an enum (unknown names fall back, like icon names), and a TS union over an
  unvalidated wire value would be a type that lies. And the row's trailing order is accessories →
  shortcut Kbd → kind badge; the accessories sit right after the title cell, which is what makes the
  title truncate before any is dropped.
- 2026-10-12 (agent): **manual pass found three things; all fixed** (e479809).

  1. **No date rendered on history rows.** Not a bug in the renderer: the history list is a `split`
     page (its rows carry the preview `detail`), and ADR-0041's split rule suppressed every accessory
     there — including the one this ticket exists to add. `rowAccessories` no longer gates on shape;
     the case in `accessories.test.ts` pins it. ADR-0041's second amendment withdraws the rule and
     explains why Raycast's sentence is a recommendation to extension authors, not host behaviour.
  2. **`⌘P` was claimed twice** in the ⌘K card — Browse Records and Filter by Tags both showed it. The
     Browse row now drops its Kbd on a page that declares a dimension (still clickable); one owner per
     chord. ADR-0043's fifth amendment records it.
  3. **The chip's ghost treatment was replaced** by the same glass the About chip and the action capsule
     wear (surface-float + sheen + chip shadow), with the accent hairline marking a set value. The
     `align-self: stretch` height rule is untouched. ADR-0043's fifth amendment records it.

  Still unticked: the *fixed* version has not been on screen — the pass above is what found these
  three, and the date rendering in both themes is the part no test covers.
