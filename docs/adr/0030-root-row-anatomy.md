# Root row anatomy: prefix-free names, owner label, shortcut, kind badge

Root command rows used to read like `AI: Quick Ask` with a details blurb next to it. Three things
were wrong with that: the title repeated the extension name that the section header already
shows (and that mixed sections like Favorites/Suggestions cannot show), the blurb crowded the
row while the more useful fact (which extension? what kind of thing is this?) was missing, and
there was nowhere to surface a command's own shortcut.

## Decision

- **Titles carry no extension prefix.** Extensions name their commands plainly
  ("Quick Ask", "Open Config File", "Shout"); the `AI: ` / `Moe: ` / `Echo: ` prefixes are gone
  from titles, fallbacks ("Ask \"…\""), and guidance copy that referenced them.
- **The extension name is the row's secondary text.** The Registry fills
  `CommandMeta.extension_title` when it builds any row (`commands()` and the fallback path), so
  every row labels its owner even inside mixed sections — the details `subtitle` is **not
  rendered on the root page** (it stays available for the search haystack and for result rows in
  the items layer).
- **A declared shortcut renders as a Kbd after the extension name.** `CommandMeta.keybinding`
  holds the *display string* an extension declares. To keep it from drifting, extensions read
  it from the platform table (`moe_core::keymap::display_of(SystemKey::…)`) — today only
  **Open Config File** declares ⌘,. Commands without a shortcut show no Kbd.
- **A kind badge ends the row.** `Extension::command_kind()` declares the vocabulary
  (`CommandMeta.kind`); `None` renders as the platform default **"Command"**. Today the two AI
  extensions declare **"AI Command"**; a future file source would declare "File". The badge reads
  as plain text (see the amendment; it is not a Kbd).
- **Every key panel is the shared `kbdEl` component** (ADR-0015): the row shortcut, the action
  bar, the cards' rows and the row hints all render through it, one block per key, so the whole
  surface stays visually identical.

## Cost

- The details subtitle is now unreachable on the root page (only Favorites/Suggestions rows and
  result rows show metadata) — a command's blurb survives in the search index, so typing still
  matches it.
- `command_kind` per extension is coarse: an extension that ever needs two kinds (e.g. commands
  and results) must set `CommandMeta.kind` per command; the Registry honors a per-command value
  over the extension's.
- The badge default is a bare string in the view layer ("Command"); if it ever needs to be
  configurable it moves into the contract.

## Amendment: the kind label reads as plain text (MOE-0015)

User feedback: the trailing kind rendered as a bordered chip, which reads as a control; it is a
descriptor, not a key. It now renders as plain text at the row's end — same secondary-text token
(13 px) and `--moe-fg-subtle` color as before, and hover still brightens it to `--moe-fg-muted` —
so the shortcut Kbd is the only boxed element at the trailing end. Position, content, and the
`badge` vocabulary in the view layer are unchanged; the class name stays `.moe-kind-badge`.