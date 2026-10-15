# Close and delivered results forget the page; Back keeps it

The panel has always kept its page across summons: `ui/src/main.ts` never resets the view on hide, so
hiding mid-page and summoning again returned to that page — with the input, the filter, the detail pane
and even an in-progress chat conversation exactly as they were. That default is right for the *escape*
path (Esc / empty `⌫` / blur-to-dismiss: the user was interrupted, and the work should survive), and it
was the only exit that existed. There was no way to say **get me out of here and forget the page** —
⌘W did nothing on the panel at all, and the two actions that actually finish an errand (write back,
copy) either left the panel up (copy: `ActionResult::Silent` plus a "Copied" toast) or hid it with the
page still lived-in (write back), so the next summon reopened a result the user had already taken.

## Decision

- **Two exits, two postures.** `back()` (Esc / empty `⌫` / blur) is the **page-keeping** exit: the panel
  hides and the page survives the summon. `⌘W` (`SystemKey::Close`, a platform semantic like the rest of
  the keymap) is the **forgetting** exit: the window goes away *and* the page is dropped, so the next
  summon lands on a fresh root with an empty Input Bar, no filter, no detail pane, no conversation. The
  forgetting routine (`resetToRoot`) closes the modal layers, attachment mode, the cards and the model
  switcher, clears the chat page's state, and re-searches the empty query so Suggestions/Favorites are
  current. Session memories that are *not* the page — argument values (ADR-0045) and the per-command
  filter memory (MOE-0046) — survive; they are preferences, not state on screen.
- **An action that delivers the outcome ends the errand, exactly like ⌘W.** Two result kinds mean "the
  result left the panel": `WriteBack` (the text went into the host app; the backend hides the panel) and
  the new **`Delivered`** (the outcome went to the clipboard and the action is done). Both reset the page
  through `applyResult`. `Delivered` exists because `Silent` cannot say this: it means "no UI work" and
  covers things that change nothing for the user (a theme toggle, `moe.open-config`), which must keep the
  page. The Copy actions of AI Chat, AI Commands and Echo return `Delivered`; the `⌥⏎` toast is gone
  (dismissing *is* the feedback, and a toast that outlives the page would greet the next summon).
- **⌘W is a webview binding, not an AppKit one — on macOS that needs the menu fixed.**
  `NSWindowAnimationBehavior`-style inference is not the issue here; the *menu* is: Tauri's default
  macOS menu installs **two** predefined Close items (File ▸ Close and Window ▸ Close, both ⌘W) whose
  action is AppKit's `performClose:`. On the panel — borderless and non-closable — that dead-ends (no
  close button to perform), and while the app is active the menu consumes the chord before the key
  window's webview sees it. `install_app_menu` (`crates/moe-app/src/main.rs`) installs the same menu
  minus those two items, so ⌘W reaches the key window's webview: the panel runs `closePanelToRoot`, the
  Settings window keeps its close-button behavior (`hide`, MOE-0029). The rest of the default menu —
  App / Edit / View / Window / Help, i.e. ⌘C/⌘V/⌘Z/⌘Q/⌘H/⌘M and Services — is unchanged.
- **A live generation is not stopped by ⌘W.** Stop is explicit (⏎ on the page, the pill's Stop row, or
  the Esc confirmation), and stopping is platform-wide (ADR-0036), so a close that silently stopped
  every stream would reach into the Side View. The consequence is stated rather than hidden: a chat
  answer still lands in history (the backend keeps generating), while a result page's stream dies with
  the page — that is what "forget the page" means. The Esc/Back protection against accidental stops
  (ADR-0038) is untouched, because ⌘W is not accidental.

## Cost

- A screen the user wanted to come back to is gone for good after ⌘W — that is the point, but it is a
  one-way door; Esc/blur remain the "come back to it" exits.
- Copy no longer shows a confirmation. The clipboard is the feedback, and the panel's disappearance is
  the second signal; an action that fails still surfaces its error on the page it left behind.
- On macOS the ⌘W menu item is gone from the *app menu*, so the Settings window's native close shortcut
  is a webview binding (`ui/src/settings.ts`) rather than a menu item — same behavior, one more place to
  keep in sync when a fourth window appears.
