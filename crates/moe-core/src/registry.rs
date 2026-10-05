use crate::contract::{Action, ActionResult, CommandMeta, Extension, Item, MoeError};

/// 所有内置 Extension 的注册表（ADR-0003：编译内置，Namespace 隔离）。
#[derive(Default)]
pub struct Registry {
    extensions: Vec<Box<dyn Extension>>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, extension: Box<dyn Extension>) {
        self.extensions.push(extension);
    }

    pub fn extensions(&self) -> &[Box<dyn Extension>] {
        &self.extensions
    }

    pub fn commands(&self) -> Vec<CommandMeta> {
        self.extensions.iter().flat_map(|e| e.commands()).collect()
    }

    /// 命令盘搜索。朴素子串打分；fuzzy + frecency 是后续 ticket（README 已注明）。
    pub fn search(&self, query: &str) -> Vec<CommandMeta> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return self.commands();
        }
        let mut scored: Vec<(i32, CommandMeta)> = Vec::new();
        for ext in &self.extensions {
            for cmd in ext.commands() {
                let haystack = format!("{} {} {}", cmd.title, ext.title(), cmd.id).to_lowercase();
                let Some(pos) = haystack.find(&q) else {
                    continue;
                };
                let mut score: i32 = 100 - pos.min(99) as i32;
                if cmd.title.to_lowercase().starts_with(&q) {
                    score += 100;
                }
                scored.push((score, cmd));
            }
        }
        scored.sort_by_key(|(s, c)| (-*s, c.id.clone()));
        scored.into_iter().map(|(_, c)| c).collect()
    }

    fn find(&self, command_id: &str) -> Option<&dyn Extension> {
        self.extensions
            .iter()
            .map(Box::as_ref)
            .find(|e| e.commands().iter().any(|c| c.id == command_id))
    }

    pub fn invoke(
        &self,
        command_id: &str,
        query: Option<&str>,
        selection: Option<&str>,
    ) -> Result<ActionResult, MoeError> {
        self.find(command_id)
            .ok_or(MoeError::NotFound)?
            .invoke(command_id, query, selection)
    }

    pub fn run_item_action(
        &self,
        command_id: &str,
        item: &Item,
        action: &Action,
    ) -> Result<ActionResult, MoeError> {
        self.find(command_id)
            .ok_or(MoeError::NotFound)?
            .run_item_action(command_id, item, action)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::{ActionKind, InputKind};

    struct Toy;

    fn action(id: &str, kind: ActionKind) -> Action {
        Action {
            id: id.into(),
            title: id.into(),
            kind,
            keybinding: None,
        }
    }

    impl Extension for Toy {
        fn id(&self) -> &str {
            "toy"
        }
        fn title(&self) -> &str {
            "Toy"
        }
        fn commands(&self) -> Vec<CommandMeta> {
            vec![
                CommandMeta {
                    id: "toy.list".into(),
                    extension_id: "toy".into(),
                    title: "Toy: List".into(),
                    subtitle: None,
                    input: InputKind::Query,
                },
                CommandMeta {
                    id: "toy.hello".into(),
                    extension_id: "toy".into(),
                    title: "Hello Toy".into(),
                    subtitle: None,
                    input: InputKind::None,
                },
            ]
        }
        fn invoke(
            &self,
            command_id: &str,
            _query: Option<&str>,
            _selection: Option<&str>,
        ) -> Result<ActionResult, MoeError> {
            match command_id {
                "toy.list" => Ok(ActionResult::List {
                    items: vec![Item {
                        id: "item-1".into(),
                        title: "hello world".into(),
                        subtitle: None,
                        actions: vec![
                            action("write-back", ActionKind::Primary),
                            action("copy", ActionKind::Secondary),
                        ],
                        payload: serde_json::Value::Null,
                    }],
                }),
                "toy.hello" => Ok(ActionResult::WriteBack { text: "hi".into() }),
                _ => Err(MoeError::NotFound),
            }
        }
        fn run_item_action(
            &self,
            _command_id: &str,
            item: &Item,
            action: &Action,
        ) -> Result<ActionResult, MoeError> {
            match action.id.as_str() {
                "write-back" => Ok(ActionResult::WriteBack {
                    text: item.title.clone(),
                }),
                "copy" => Ok(ActionResult::Silent),
                _ => Err(MoeError::NotFound),
            }
        }
    }

    fn registry() -> Registry {
        let mut r = Registry::new();
        r.register(Box::new(Toy));
        r
    }

    #[test]
    fn empty_query_lists_everything() {
        assert_eq!(registry().search("").len(), 2);
    }

    #[test]
    fn search_matches_title_and_extension() {
        let hits = registry().search("list");
        assert_eq!(
            hits.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
            ["toy.list"]
        );
        assert_eq!(registry().search("toy").len(), 2);
    }

    #[test]
    fn item_stream_composes_to_write_back() {
        let r = registry();
        let ActionResult::List { items } = r.invoke("toy.list", None, None).unwrap() else {
            panic!("expected list");
        };
        let item = &items[0];
        let primary = &item.actions[0];
        assert_eq!(primary.kind, ActionKind::Primary);
        let result = r.run_item_action("toy.list", item, primary).unwrap();
        assert_eq!(
            result,
            ActionResult::WriteBack {
                text: "hello world".into()
            }
        );
    }

    #[test]
    fn unknown_command_is_not_found() {
        assert!(matches!(
            registry().invoke("toy.nope", None, None),
            Err(MoeError::NotFound)
        ));
    }
}
