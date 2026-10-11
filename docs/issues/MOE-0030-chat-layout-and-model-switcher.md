# M9b: the chat surfaces get a normal chat layout — bottom composer, top-right options, model switcher

- **ID**: MOE-0030
- **State**: done
- **Labels**: ready-for-agent, feature, ux
- **Created**: 2026-10-11
- **Updated**: 2026-10-11
- **Assignee**: agent
- **Commit**: 8b06425
- **Blocked by**: MOE-0029 (the switcher's rows and apply path)

The owner's ask: Raycast moved its quick chat to a normal chat layout — the composer at the
bottom, the actions button and options in the top-right corner — and Moe's Quick Ask page still
hosted its conversation under the panel's top Input Bar (ADR-0036), with the model only
switchable through the palette picker (MOE-0028). Switching models "amongst several providers at
the same time" needed a switcher on the chat surfaces themselves, and the two surfaces' composers
had drifted in shape (the Side View's field carried its buttons on an inner bottom row).

## Decisions

- **The Quick Ask page is a normal chat page** (ADR-0036 amendment): on the page the Input Bar
  row yields to a chat header — back arrow + conversation title on the left, the model chip and
  the actions button in the top-right corner — and the composer pill sits at the bottom. The
  command layer and the result pages keep the Input Bar untouched (ADR-0034); only the chat page
  swaps chrome. The bottom bar (avatar chip + action pill, ADR-0026) hides on the page: the
  composer carries send/stop, and About stays reachable from the command layer.
- **One shared composer component** (`ui/src/composer.ts`): the pill row — round attach button,
  rounded field with the hidden path-input row, the auto-growing draft and the round send/stop
  button inside the field's right edge. The panel page and the Side View mount the same
  component, so the input affordances cannot drift (the message stream already shares
  `messages.ts`). Enter sends and stops-while-generating on both surfaces; the attach row's
  keys (Enter attaches, Esc / empty ⌫ cancels) ride with it.
- **The model switcher** (`ui/src/models.ts`): a chip button in each chat header showing the
  active pair (`Provider · Model`), opening a floating card with the ranked model rows (the
  picker's order, MOE-0028: active provider first, then by provider name, newest first) grouped
  by provider, filterable, Enter/click applying `ai_set_pair`. The card is the actions card's
  component (`card.ts`, extended with an optional row subtitle and check mark). Both surfaces
  share it; the settings window (MOE-0029) manages visibility, the chip switches.
- **The Side View's header gains the chip** in its action capsule (before More Actions), plus a
  gear opening the settings window (MOE-0029); its centered title and traffic lights stay.
- **Keyboard dialect unchanged**: Enter sends / stops, Esc backs (confirming while generating),
  ⌃[/⌃] step, ⌘N blanks, ⌃X removes — only the focused element moves from the Input Bar to the
  composer on the page. ⌘⇧A on the page opens the composer's path row instead of the Input Bar's
  attachment mode (which stays a command-layer mode).

## Delivery

- `ui/src/composer.ts` (new): the shared composer pill.
- `ui/src/models.ts` (new): the active-pair fetch, the chip and the switcher card.
- `ui/src/card.ts`: optional `subtitle` / `checked` on card rows (the switcher's facts + mark).
- `ui/index.html` + `ui/src/main.ts`: the chat header + composer footer, the chrome toggles in
  `render()`, the draft element abstraction (`draftEl`), the header buttons, the "Open Settings"
  actions-card row, and the `openSettings` result branch.
- `ui/chat.html` + `ui/src/chat.ts`: the footer becomes the mounted pill, the header capsule
  gains chip + gear, the old in-field button row and its state code are gone.
- `ui/src/styles.css`: `.moe-chip`, `.moe-icon-button` (the header controls).
- Docs at delivery: ADR-0036 amendment (the page layout), ADR-0034 amendment note (the Input Bar
  is the command layer's; the chat page owns its composer), CONTEXT (Quick Ask term updated),
  README (the layout bullet).

## Acceptance

- On the Quick Ask page the Input Bar row is gone: header (back, title, chip, actions) on top,
  the composer pill at the bottom; leaving the page restores the command layer's chrome.
- The chip shows the active pair on both chat surfaces; opening it lists the ranked models
  grouped by provider with the current one checked; Enter/click switches and the config's
  `[ai] provider` / `model` follow (comment-preserving), both chips refresh.
- The Side View composer is the same pill as the panel's: attach button left, growing draft,
  send/stop inside right; attachments and keys behave as before.
- `pnpm -C ui typecheck` and `pnpm -C ui build` green; no Rust change beyond MOE-0029's IPC.

## Comments

- 2026-10-11 (agent): filed and delivered together with MOE-0029 (one owner ask, one pass).
  Remaining manual: a visual pass of both surfaces in the running app — the panel page's chrome
  swap at the narrowest panel width, the chip truncation with long model names, and the
  switcher card over a 730-model catalog (the rows are capped at 50 like the picker).
- 2026-10-11 (agent): **revised after the owner's design review**, four points: (1) "Set AI Model"
  — and "Stop Generation" while a stream runs — are rows inside the actions card on both chat
  surfaces, not a header chip; (2) the add and actions buttons wear the about chip's glass style;
  (3) the composer is add button + one capsule-like one-row input + actions button, the actions
  button moved from the top-right to the bottom-right end of the row, and the whole row wears the
  root page's bottom-bar glass (about chip + action pill); (4) the page's top bar is the Input Bar
  itself — read-only conversation title in the command pages' own row design — so chat pages and
  command pages share one top bar. `composer.ts` v2 (row, capsule, attach mode inside the capsule)
  and `models.ts` (card-only switcher) carry the revision; the header chip, the inner send button
  and the multi-row draft are gone.
- 2026-10-11 (agent): **revision 2 (owner review)** — (a) bug: the switcher's unfiltered head was
  capped globally at 50 rows, so a catalog-rich active provider (e.g. Alibaba) filled the whole
  card and a configured MiniMax never appeared (and the card filtered client-side, so search
  could not reach the unloaded rows). Fix: the empty-query head is capped per provider
  (`HEAD_PER_PROVIDER = 10`, active provider included) and the card filters server-side through
  `createCard`'s new `onQuery`, so search reaches the whole ranked catalog. (b) The composer row
  moved down: its side/bottom margins are now even (the panel root's own bottom padding is the
  row's bottom space). (c) Card layout consistency: on each surface the model card anchors exactly
  where the actions card does (panel bottom-right above the composer row; Side View centered near
  the top), and every menu card keeps its filter input at the bottom (ADR-0028 amendment) — the
  one searchable-card component (`card.ts`) now serves client-filtered and server-filtered cards
  alike.
- 2026-10-11 (agent): **revision 3 (owner review)** — (a) the switcher only lists the models of
  providers with a resolvable key (config-file/managed keys extra): an unkeyed catalog provider
  never appears, search included; the palette picker stays the catalog browser for discovery, and
  the settings window remains where keys are saved. (b) The switcher rows are bare model names;
  the focused row's facts — context, max output, modalities, capabilities, pricing, dates,
  protocol, docs link — moved into a companion detail card floating to the side of the list,
  following ↑↓ and hover (`focusOnHover` in the card component). The Rust side serves the fields
  pre-formatted (`ModelOption.detail: ModelDetail`), the table-fed facts factored out of the
  picker's markdown formatter; the companion card hides itself when the surface is too narrow for
  the pair (the default Side View width; the expanded width shows it).
- 2026-10-11 (agent): **revision 4** — the switcher pair bottom-aligns: the wrapper stretches its
  two cards to one shared height (no `items-start`), so the detail card and the list card share
  both edges; the list card is a column with its search pinned to the bottom (`mt-auto`), and its
  scroll area keeps its own cap. One visual pass left: the running app.
- 2026-10-11 (agent): **revision 5** — three fixes from the owner's run: (a) the switcher's check
  now means the **active pair** (provider *and* model), not the active provider — the provider-level
  "Current" is the picker's group-header convention and must never mark every row; (b) the detail
  card is **absolutely positioned** against the list card (`inset-y-0 right-full mr-2`), so its
  content can never resize the list: the list card's height is the only one in flow, which makes
  the hover-feedback loop (list shifts → hover targets move → detail changes → list shifts…)
  structurally impossible, and the detail card's own body scrolls when it overflows; its content
  is now compact icon chips (cpu / ↑ out / image / capability icons / $ / clock / plug) so it
  rarely needs to; (c) both surfaces now raise the generating indicator **before** the send invoke
  resolves (the worker starts answering as soon as it spawns), roll the empty answer bubble back
  (`discardAnswer`) and toast the error on failure, and a failed `ai_set_pair` surfaces as a toast
  instead of a silent console line.
- 2026-10-11 (agent): **revision 6** — the list card gets a fixed height (`h-80`, so it reads as a
  panel instead of growing/shrinking with the hit count, and the inset-anchored detail card
  inherits the same stable height), and the detail card is designed as a hierarchy rather than a
  uniform chip cloud: the provider/version id headline, the context and max-output counts as hero
  stats with small captions (the backend sends bare counts — `1M`, `393k` — and the card owns the
  labels and units), capabilities as icon chips (short tags only), Modalities / Pricing / Dates /
  Protocol as labelled definition rows, and the docs link pinned in the footer.
- 2026-10-11 (agent): **revision 7** — the detail card's information architecture was rebuilt from
  the owner's screenshot (pricing wrapped to four lines, "Dates" clipped mid-line): every value is
  now a card-sized single unit — a compact `$0.3 in · $1.2 out /M` pricing line with the cache
  prices on a quieter sub-line underneath, `released`/`knowledge` joined as one timeline row, the
  lifecycle status as a chip (warning-toned only for deprecated/retired), and `vision` as a
  capability chip sourced from the image modality (the coarse Modalities and provider-level
  Protocol rows are gone — the former's signal is the chip, the latter lives in the Settings
  provider header). Long values still fade out at the body's bottom edge
  (`moe-scroll-fade-bottom`) instead of being cut by the pinned footer. The DTO follows the card:
  `ModelDetail` carries `status`/`cache`/`released`/`knowledge` instead of the pre-joined
  `modalities`/`dates`/`protocol` strings.
- 2026-10-11 (agent): **revision 8** — provider brand marks and the model-card information design:
  (a) **Lobe Icons** (lobehub/lobe-icons, MIT, the `@lobehub/icons-static-svg` 1.95.1 assets) are
  vendored as `ui/src/logos/*.svg` (18 marks — color variants where upstream ships one, mono
  `fill="currentColor"` ones the tile tints per theme) with `ui/src/logos/README.md` recording the
  source/version/refresh path; `ui/src/logos.ts` maps catalog provider ids (`minimax-cn` → minimax)
  and falls back to a monogram tile, so runtime-refreshed providers without an asset still render.
  They appear in the model groups' headers, the model detail header and the settings provider
  groups. No React dependency — the SVGs are inlined at build time (the app stays offline).
  (b) Model rows now carry the model's abilities as **rounded tiles** (tool calls / vision /
  reasoning / attachments, strongest first, capped at four; the rest stay in the detail card).
  (c) The detail card follows the LobeHub model-card layout: brand mark + name (15px) + status
  chip, the **modalities as `in → out` rounded tiles** (text = blue "T", image = green, pdf =
  amber), then a hairline-separated **label / value fact list** (Context, Max output, Input,
  Output, Cache read, Cache write, Released, Knowledge — 13px, right-aligned values, tabular
  numerals) and a mono `provider/model-id` footer with the docs link. The backend DTO follows one
  field per line: `modalities_in`/`modalities_out` and split `price_*` fields replace the joined
  prose strings.
- 2026-10-11 (agent): **revision 9** — the detail card fits its height and the phantom-stream bug:
  (a) the companion card is **content-fit** (`bottom-0 right-full mr-2 max-h-full`, no `inset-y-0`):
  it hugs its rows, keeps the shared bottom edge, scrolls its own body past the cap, and — being
  absolutely positioned and bottom-anchored — can never resize the list, so the hover-feedback
  loop stays structurally impossible while a metadata-poor model no longer shows an oversized
  card. (b) **The owner's "no error, no response, no indicator" bug**: the page treated "the last
  stored message is a question" as "a reply is coming", so a request that failed after persisting
  the question (or an app restart) left the page generating forever — Enter became a Stop that
  stopped nothing and the question was swallowed silently. The generating flag now yields to
  **backend truth**: `stop_generation` returns the number of streams actually aborted, and Enter
  only treats a generating flag as live when that count is non-zero (both surfaces); the page
  re-enters waiting mode only when the load sees an unanswered question, then `verifyPendingAnswer`
  asks the new `conversation_streaming` IPC (the live-stream registry) after a 1.5 s grace period
  and — when nothing is live — settles the row, drops the empty waiting bubble and shows an inline
  "the previous question never received an answer" line instead of trapping the next Enter. The
  send itself is deadline-bound (`ui/src/ipc.ts`: `invokeWithTimeout`, 20 s) so a hung backend
  surfaces as an error + toast rather than a dead page, `panel_send` falls back to `ai.quick-ask`
  when the page lost its command id, and the Rust worker logs turn start/finish
  (`moe: chat turn started (conversation …, model …)`) so the app log alone tells "never reached
  the request" from "endpoint cut the stream". The frame-age timing heuristic was tried and
  removed — truth beats timing.
- 2026-10-11 (agent): **revision 10 — the actual root cause of "no response"**, found from the
  owner's second run (`started`/`finished` logged, the answer in the DB, nothing on screen):
  `messages.ts`'s `ensureStreamingBubble` built the answer bubble's root, filled it with the
  Thinking row / activity stack / body / indicator — and never appended it to the log. Every
  streamed frame (and the generating indicator with it) rendered into a **detached node**: the
  answer persisted correctly and appeared on reopen (history goes through `append`), but the live
  surface stayed blank, which is exactly "no error, no response, no generating indicator".
  `git log -S "container.append(root)"` dates the deletion to 1d9eef3 (the MOE-0017 Tool Call
  activity-stack refactor rewrote `root.append(details, body, indicator)` →
  `root.append(details, activity, body, indicator)` and dropped the attach line with it). One line
  restores it (`container.append(root)`); the revision-9 stop-count/`verifyPendingAnswer` work
  stays as the separate repair for genuinely dead streams (the detached indicator made that state
  invisible, which is why the phantom trap looked identical from the outside). The UI still has
  no DOM test runner — a jsdom/vitest smoke test of one streamed frame would have caught this
  class of regression; filed as an option, not done.
