# M9v: the UI gets a unit test runner, and the layer stack's ordering is pinned

- **ID**: MOE-0050
- **State**: done
- **Labels**: ready-for-agent, refactor, ux
- **Created**: 2026-10-12
- **Updated**: 2026-10-12
- **Commit**: ced4f91

The UI had no test runner — `ui-check` and `ui-build` only, so `tsc` and `vite` were the whole safety
net. Three tickets then landed on behavior nothing could check: MOE-0040's Empty View and loading
hairline, MOE-0041's card migration, MOE-0042's key routing. Each closed with a hand-verification
note, and the outstanding work grew with every ticket.

MOE-0042's `ui/src/layers.ts` earned the runner: 61 lines of pure logic with no DOM, where the failure
mode is a *missing branch* rather than a type error. That is exactly the shape a unit test catches and
a compiler cannot.

## Decision

- **vitest**, because the UI is already a Vite project: no second bundler, no config file needed (the
  defaults pick up `src/**/*.test.ts` in a node environment), and `vite.config.ts` is untouched — so
  the three-entry build cannot be affected by test setup.
- **`pnpm -C ui test` / `just ui-test`**, and `ui-test` joins the `ci` recipe. CI is now
  `lint` (Rust) → `test` (Rust) → `ui-test` → `ui-build`.
- **Tests live beside their module** (`src/layers.test.ts`), so the pair moves together.
- **Explicit `vitest` imports** rather than globals, so no `types` entry is needed in `tsconfig.json`
  and the test file is typechecked by the ordinary `tsc` pass.

## What is pinned

Six tests over `createLayers`, covering the properties that drift silently:

- **Which layer is front-most**: the last registered one, regardless of the order they were opened in.
  This is the invariant the whole stack rests on, and getting it backwards is a one-character change.
- **A pop takes exactly one layer**, front to back, and `closeTop()` reports false when the surface is
  empty so a back handler can fall through. Layers behind the front-most must be untouched, or Back
  stops being layered.
- Closed layers are skipped when finding the front-most; the empty stack reports nothing open.
- **`isOpen` is re-read, never cached** — because a card's own Escape handling closes it without the
  stack seeing it, and the stack has to agree afterwards.

## Acceptance

- [x] `just ui-test` passes: 6 tests, one file.
- [x] A *failing* test was demonstrated rather than assumed: inverting `top()`'s scan so the FIRST
      registered layer became front-most broke exactly the two ordering tests and nothing else. Reverted
      afterwards. A green suite that cannot go red would have been worse than no suite.
- [x] `tsc --noEmit` still covers the test file (it is under `src/`), and `tsc + vite build` is
      unaffected.
- [x] The test file is **not** in the bundle — checked by grepping `ui/dist` for an identifier that
      exists only in the test.
- [x] `ui-test` is in the `ci` recipe.

## Comments

- 2026-10-12 (agent): filed while implementing MOE-0042, from the recommendation that a 61-line pure
  module deserved a runner. Not a feature ticket: it exists so the next four tickets
  (MOE-0039, 0043, 0044, 0049) can be checked by machine instead of by hand.
- 2026-10-12 (agent): **done** (ced4f91). Deliberately no coverage goal — the runner is for the pure
  logic that drifts silently, not for the DOM rendering, which still needs a human. What it does not
  fix is recorded on MOE-0041 and MOE-0042: their acceptance boxes for "no behavior change observable
  to a user" remain unticked, and this changes nothing about that.