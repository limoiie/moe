//! moe-core: contracts for Extension/Command/Item, the unified keymap, and the Registry (ADR-0003/0006).

pub mod contract;
pub mod conversation;
pub mod favorites;
pub mod frecency;
pub mod keymap;
pub mod registry;

pub use contract::{Action, ActionResult, CommandMeta, Extension, InputKind, Item, MoeError};
