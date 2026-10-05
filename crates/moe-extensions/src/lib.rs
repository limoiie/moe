//! 内置 Extensions（ADR-0003：v1 全部编译内置，但按 Extension/Namespace 组织）。

use moe_core::registry::Registry;

pub mod ai;
pub mod echo;

pub fn install(registry: &mut Registry) {
    registry.register(Box::new(echo::Echo));
    registry.register(Box::new(ai::AiShell));
}
