//! moe-core: Extension/Command/Item 契约、统一键位表、Registry（ADR-0003/0006）。

pub mod contract;
pub mod keymap;
pub mod registry;

pub use contract::{Action, ActionResult, CommandMeta, Extension, InputKind, Item, MoeError};
