# M7l: the auto write-back crashes Moe when an AI Command finishes

- **ID**: MOE-0004
- **State**: done
- **Labels**: bug
- **Created**: 2026-10-09
- **Updated**: 2026-10-09
- **Assignee**: agent
- **Commit**: 8cc07b3

Opening any AI Command (Improve Writing / Make Shorter / …) and letting the generation finish naturally
crashed Moe the moment the stream completed.

## Root cause

On natural completion the stream's `on_done` emits `CommandEvent::WriteBack`
(`crates/moe-extensions/src/ai_commands.rs`), which reaches `TauriEventEmitter::emit` **on the stream's
worker thread** (`std::thread::spawn`, ai_commands.rs L343). `TauriEventEmitter` handles WriteBack by
calling `deliver_writeback` (`crates/moe-app/src/main.rs`), and `deliver_writeback` called
`hide_panel_blocking` synchronously — `NSPanel`/AppKit window operations are main-thread-only. Hiding the
panel from the stream thread aborted the process. Every other entry point into the panel helpers already
hops to the main thread (`open_side_view` wraps in `run_on_main_thread` with an explicit comment;
`toggle_panel` likewise), and no other surface emits WriteBack (panel chat and Side View pass
`noop_sink`), which matches the observed "only AI Commands, only on natural completion".

## Delivery

- `crates/moe-app/src/main.rs`: `deliver_writeback` posts the hide (and the untrusted-permission
  prompt) to the main thread via `run_on_main_thread` and does not block on it — it must stay safe when
  called from the main thread itself (the sync-command path), so no ack is awaited. The writer thread
  keeps its 120 ms focus-return delay and the `Err` contract.

## Acceptance

- Run an AI Command to natural completion: no crash; the panel hides, the transform is written back to
  the host app.
- Esc-stopped generations (no WriteBack) and explicit write-back actions (main thread) unchanged.
- `cargo fmt --check`, `cargo clippy --workspace -- -D warnings`, `cargo test --workspace` green.

## Comments

- 2026-10-09 (agent): filed from user report; root cause traced to the stream thread's WriteBack → hide
  path (AppKit called off the main thread).
- 2026-10-09 (agent): shipped in `8cc07b3`; `cargo fmt --check`, `cargo clippy --workspace --all-targets
  -- -D warnings`, `cargo test --workspace` (146) green. GUI repro pending on the user's side.