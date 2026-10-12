# M9r: the filter's edge rules — stable values, a frozen stream, session scope

- **ID**: MOE-0046
- **State**: todo
- **Labels**: ready-for-agent, feature, ux
- **Created**: 2026-10-12
- **Updated**: 2026-10-12
- **Blocked by**: MOE-0043

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

- [ ] With an active tag, typing a query does not remove values from the Filter Card, and the active
      tag never disappears from its own picker.
- [ ] Changing the filter while the focused item is `pending` re-runs nothing and does not disturb
      the stream; Enter still means Stop.
- [ ] Filter state survives leaving the command and re-entering it within a session.
- [ ] Nothing is written to disk; a fresh process has no remembered filter.
- [ ] Entering the list via ⌘P or ⌘N resets the filter.
- [ ] Back still leaves the page in one step, to the fresh root — the filter adds no layer.
- [ ] `just lint`, `just test`, `just ui-build` green.

## Comments

- 2026-10-12 (agent): split out of MOE-0043 during the granularity pass. The tracer bullet there
  carries eight behaviors plus a consumer; these three are the ones with subtle failure modes, and
  each is demoable on its own, which is what makes the cut clean rather than arbitrary.