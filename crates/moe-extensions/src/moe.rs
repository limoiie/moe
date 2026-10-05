//! Moe 自身的元扩展：设置面入口（ADR-0009——v1 不做设置 UI，用命令打开配置文件）。

use moe_core::contract::{ActionResult, CommandMeta, Extension, InputKind, MoeError};

pub struct Moe;

impl Extension for Moe {
    fn id(&self) -> &str {
        "moe"
    }

    fn title(&self) -> &str {
        "Moe"
    }

    fn commands(&self) -> Vec<CommandMeta> {
        vec![CommandMeta {
            id: "moe.open-config".into(),
            extension_id: "moe".into(),
            title: "Moe: 打开配置文件".into(),
            subtitle: Some("呼出键等设置".into()),
            input: InputKind::None,
        }]
    }

    fn invoke(
        &self,
        command_id: &str,
        _query: Option<&str>,
        _selection: Option<&str>,
    ) -> Result<ActionResult, MoeError> {
        match command_id {
            "moe.open-config" => {
                moe_platform::config::open_in_editor()
                    .map_err(|err| MoeError::Internal(err.to_string()))?;
                Ok(ActionResult::Silent)
            }
            _ => Err(MoeError::NotFound),
        }
    }
}
