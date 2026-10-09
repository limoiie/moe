//! Built-in Extensions (ADR-0003: in v1 everything is compiled in, organized by Extension/Namespace).

use moe_core::registry::Registry;

pub mod ai;
pub mod ai_client;
pub mod ai_commands;
pub mod attachment;
pub mod echo;
pub mod moe;
pub mod recency;

pub fn install(registry: &mut Registry) {
    // Registration order decides fallback capture precedence (e.g. `key …` must reach Moe's save
    // item before AI's generic ask); the root page's display order is declared separately
    // (ADR-0020 amendment).
    registry.register(Box::new(moe::Moe));
    registry.register(Box::new(echo::Echo));
    registry.register(Box::new(ai::AiShell));
    registry.register(Box::new(ai_commands::AiCommands));
    registry.set_source_order(&["ai-commands", "ai", "moe", "echo"]);
}
