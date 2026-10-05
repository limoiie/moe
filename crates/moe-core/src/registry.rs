use crate::contract::{Action, ActionResult, CommandMeta, Extension, Item, MoeError};
use crate::frecency::FrecencyLookup;
use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

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

    /// 命令盘搜索：nucleo 模糊匹配打分，frecency 平分决胜；空查询按 frecency 排序。
    pub fn search(&self, query: &str, frecency: &dyn FrecencyLookup) -> Vec<CommandMeta> {
        let q = query.trim();
        if q.is_empty() {
            let mut commands = self.commands();
            commands.sort_by(|a, b| {
                frecency
                    .frecency(&b.id)
                    .partial_cmp(&frecency.frecency(&a.id))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            return commands;
        }

        let pattern = Pattern::parse(q, CaseMatching::Smart, Normalization::Smart);
        let mut matcher = Matcher::new(Config::DEFAULT);
        let mut buf = Vec::new();
        let q_lower = q.to_lowercase();
        let mut scored: Vec<(u32, f64, CommandMeta)> = Vec::new();
        for ext in &self.extensions {
            for cmd in ext.commands() {
                let haystack = format!("{} {}", cmd.title, ext.title());
                let hay = Utf32Str::new(&haystack, &mut buf);
                if let Some(score) = pattern.score(hay, &mut matcher) {
                    // 排序契约（IIE4AD-346）：精确前缀 > 位置 > frecency。
                    // matcher 的词边界启发式不足以表达「前缀优先」，显式加成。
                    let title_lower = cmd.title.to_lowercase();
                    let bonus = if title_lower == q_lower {
                        1_000_000
                    } else if title_lower.starts_with(&q_lower) {
                        500_000
                    } else {
                        0
                    };
                    scored.push((score + bonus, frecency.frecency(&cmd.id), cmd));
                }
            }
        }
        scored.sort_by(|a, b| {
            b.0.cmp(&a.0)
                .then(b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal))
                .then_with(|| a.2.id.cmp(&b.2.id))
        });
        scored.into_iter().map(|(_, _, cmd)| cmd).collect()
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
    use crate::frecency::{FrecencyLookup, NoFrecency};

    struct FixedFrecency(std::collections::HashMap<String, f64>);

    impl FrecencyLookup for FixedFrecency {
        fn frecency(&self, command_id: &str) -> f64 {
            self.0.get(command_id).copied().unwrap_or(0.0)
        }
    }

    fn fixed(pairs: &[(&str, f64)]) -> FixedFrecency {
        FixedFrecency(
            pairs
                .iter()
                .map(|(id, score)| ((*id).to_string(), *score))
                .collect(),
        )
    }

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
        assert_eq!(registry().search("", &NoFrecency).len(), 2);
    }

    #[test]
    fn empty_query_orders_by_frecency() {
        let hits = registry().search("", &fixed(&[("toy.hello", 9.0)]));
        assert_eq!(
            hits.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
            ["toy.hello", "toy.list"]
        );
    }

    #[test]
    fn search_matches_title_and_extension() {
        let hits = registry().search("list", &NoFrecency);
        assert_eq!(
            hits.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
            ["toy.list"]
        );
        assert_eq!(registry().search("toy", &NoFrecency).len(), 2);
    }

    #[test]
    fn fuzzy_subsequence_matches_and_prefix_wins() {
        // "tl"："Toy: List" 的子序列；"Hello Toy" 里 l 在 t 之前，不匹配
        let hits = registry().search("tl", &NoFrecency);
        assert_eq!(
            hits.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
            ["toy.list"]
        );
        assert!(registry().search("zzz", &NoFrecency).is_empty());
        // 前缀匹配优先于子串匹配
        let hits = registry().search("toy", &NoFrecency);
        assert_eq!(hits.first().map(|c| c.id.as_str()), Some("toy.list"));
    }

    #[test]
    fn frecency_breaks_score_ties() {
        // 两个孪生命令（同标题、同扩展名）：nucleo 分数相同，由 frecency 决定顺序
        struct Twin(&'static str);
        impl Extension for Twin {
            fn id(&self) -> &str {
                self.0
            }
            fn title(&self) -> &str {
                "Twin"
            }
            fn commands(&self) -> Vec<CommandMeta> {
                vec![CommandMeta {
                    id: format!("{}.deploy", self.0),
                    extension_id: self.0.into(),
                    title: "Deploy".into(),
                    subtitle: None,
                    input: InputKind::None,
                }]
            }
            fn invoke(
                &self,
                _command_id: &str,
                _query: Option<&str>,
                _selection: Option<&str>,
            ) -> Result<ActionResult, MoeError> {
                Err(MoeError::NotFound)
            }
        }

        let mut r = Registry::new();
        r.register(Box::new(Twin("a")));
        r.register(Box::new(Twin("b")));
        let hits = r.search("deploy", &fixed(&[("b.deploy", 9.0)]));
        assert_eq!(
            hits.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
            ["b.deploy", "a.deploy"]
        );
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
