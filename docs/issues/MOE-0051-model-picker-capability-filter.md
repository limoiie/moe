# M9w: the model picker narrows by capability

- **ID**: MOE-0051
- **State**: done
- **Labels**: ready-for-agent, feature, ux
- **Created**: 2026-10-12
- **Updated**: 2026-10-12
- **Commit**: 8cbe8a7

The picker's capability vocabulary — tool calls, reasoning, structured output, attachments, vision —
reached the search haystack and the detail card, but never the list: typing "vision" found the models,
and nothing let you *see* which ones have it or narrow to them. That was a named follow-up in two
places, ADR-0043's consumer note and the picker's own comment (`model_item`, `ai_models.rs`):
"capability words are already in the search haystack, and promoting them to tags is a follow-up".

The reason to do it now rather than when it was written: it is the first **second consumer** of the
primitives work. The Filter Control, ⌘P's ownership of its chord, the derived-values-over-tags rule and
the tag→filter loop have all existed on exactly one surface (chat history) since they landed.

## Decision

- **The capability words become `Item.tags`**, and the picker's list result declares one **derived**
  dimension (`Filter::derived("capability", "Capability")`). Derived, not declared: the values come
  from the tags on the returned items, so the picker cannot offer a capability no model carries — the
  same rule history follows, with no new code path on either side.
- **The tags are the card's vocabulary**, not the row subtitle's. The subtitle says "tools",
  `capability_words` (and therefore the card's **Capabilities** line) says "tool calls"; the tag list
  and the filter dimension follow the card, so the two surfaces that name a capability name it the
  same way.
- **A model the catalog knows nothing about carries no tags**, so it stays reachable through the
  dimension's reset rather than being filtered out by a vocabulary it never had.

## Delivery

- `crates/moe-extensions/src/ai_models.rs` — `model_item`'s `tags` and `choose_model_with`'s filter
  declaration. No change to the contract, the mirror, the UI or the styles: everything the picker
  needed already existed, which is the point of the ticket.

## Acceptance

- [x] The picker's result declares one derived dimension (`capability` / "Capability"), with an empty
      declared value list — so the values are the tags, per ADR-0043.
- [x] Catalogued models carry the card's capability words as tags; the test asserts the vocabulary
      matches what the card shows for the same model.
- [x] A custom model with no catalog metadata carries no tags.
- [x] Nothing on the platform side changed: the Filter Control, the derived-values rule and the tag→
      filter loop are used exactly as they shipped.
- [x] 122 Rust tests (was 121), 43 UI tests, `tsc`, `vite build`, clippy green. **`just lint` is red on
      the pre-existing `cargo fmt` diff in `anthropic.rs`** — unchanged by this work.

## Comments

- 2026-10-12 (agent): filed and closed in one pass, from the "what's next" question after the
  primitives work landed. Not a tracer bullet in the usual sense — it adds no new capability to the
  platform — but it is the first thing that makes the platform's newest rules *load-bearing* rather
  than demonstrated: the same four rules now run on a second surface with a different vocabulary, and
  anything that only worked because history is history would have shown up here.
- 2026-10-12 (agent): **not verified on screen.** The picker now shows a Capability chip in its input
  row and ⌘P opens its filter there. Worth a pass, since this is the surface where the filter's
  declared-vs-derived distinction stops being theoretical.
- 2026-10-12 (agent): **verified by the reporter.** The picker shows its Capability chip, ⌘P opens it,
  and the values are the capability words. The premise — that the platform's rules needed no
  surface-specific code to appear on a second surface — held.
