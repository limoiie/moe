# M9r: the filter's edge rules — stable values, a frozen stream, session scope

- **ID**: MOE-0046
- **State**: done
- **Labels**: ready-for-agent, feature, ux
- **Created**: 2026-10-12
- **Updated**: 2026-10-12
- **Commit**: 72eb9ff
- **Blocked by**: MOE-0043 (done)

The correctness rules that surround MOE-0043's visible path. Separately demoable — each one is
observable on its own — but not independently useful, which is why it is a sibling rather than
another three behaviors inside the tracer bullet.

## Decision

ADR-0043 names all three; this ticket is where they get built and verified.

- **Values are derived *before* the tag filter is applied.** The Filter Card's value list comes from
  the tag-unfiltered set, so typing a query cannot shrink the picker and the active tag can never
  vanish from its own list. Two views of one list, deliberately: unfiltered for the picker, filtered
  for the rows.
- **A filter change re-runs nothing while an item is `pending`.** ADR-0038's "while a result streams,
  the page owns the input" guard covers the filter exactly as it covers the Input Bar, or changing a
  tag mid-stream orphans the generation.
- **State is session-scoped per (Extension, command), never written to disk, and adds no Back layer.**
  The faithful reading of both precedents: Raycast's `storeValue` is a cache, not a preference; and
  ADR-0038 governs the Input Bar text and Back's destination, not filter memory. Entering the list
  fresh via ⌘P / ⌘N resets it, consistent with ADR-0038's "every entry clears it".

## Delivery

- `ui/src/main.ts` — the derive-before-filter bookkeeping, the pending guard on the filter's change
  path, and the session-scoped store keyed by (Extension, command).

## Acceptance

- [x] With an active tag, typing a query does not remove values from the Filter Card, and the active
      tag never disappears from its own picker — **landed in MOE-0043** (derive-before-filter), and
      covered by `deriveFilterValues`' tests rather than only by this text.
- [x] Changing the filter while an item is `pending` re-runs nothing and does not disturb the stream;
      Enter still means Stop. The change is refused outright (`filterChangeAllowed`, checked against
      `allItems`), which also gates the card's open and the control's affordance — so the control goes
      visibly disabled with a reason in its tooltip. Enter-when-pending was already Stop. **Verified:**
      the reporter's pass saw the control dim and refuse to open mid-stream.
- [x] Filter state survives leaving the command and re-entering it within a session — a session-only
      `Map` keyed by command id.
- [x] Nothing is written to disk; a fresh process has no remembered filter — the memory is a module
      `Map`, so this is structural rather than a promise.
- [x] Entering the list via ⌘P or ⌘N resets the filter — `openEntry` clears the key once the entry
      command resolves, which is also the only path that does; a palette re-invocation keeps it, which
      is Raycast's `storeValue` reading.
- [x] Back still leaves the page in one step, to the fresh root — the filter was never a layer, and
      this change adds none.
- [x] 43 UI tests (was 40), 253 Rust tests, `tsc`, `vite build` green. **`just lint` is red on the
      pre-existing `cargo fmt` diff in `anthropic.rs`** — unchanged by this work.

## Comments

- 2026-10-12 (agent): split out of MOE-0043 during the granularity pass. The tracer bullet there
  carries eight behaviors plus a consumer; these three are the ones with subtle failure modes, and
  each is demoable on its own, which is what makes the cut clean rather than arbitrary.
- 2026-10-12 (agent): **scope is now two rules, not three.** "Values are derived before the tag filter
  is applied" landed inside MOE-0043 (7a500d1), because shipping the vanishing-value bug and fixing it
  a ticket later would have been silly — and it is now covered by `deriveFilterValues`' tests in
  `ui/src/filter.ts` rather than only by the acceptance text. What remains here is session scope
  (the remembered value surviving a re-entry) and the pending-freeze.
- 2026-10-12 (agent): **landed** (72eb9ff). The freeze became a named predicate,
  `filterChangeAllowed`, rather than an inline `some(pending)` in three places — the Filter Control's
  affordance, the card's open and the change path all read it, so they cannot disagree, which is the
  same discipline `filterableDimension` follows for "does this page have a dimension at all".

  Two decisions worth flagging. **The freeze checks `allItems`**, not the filtered list: a stream runs
  whether or not the active filter happens to show the item, and gating on the filtered list would let
  a change slip through and orphan it. And **a value the new set no longer carries is forgotten, not
  merely ignored** — the settle step writes the outcome back over the remembered key, so the derived
  list stays the authority rather than a stale value resurrecting later.

  ADR-0043's sixth amendment records both. Unverified on screen: no pass has yet changed a filter
  mid-stream, which is the one behavior here that needs hands.