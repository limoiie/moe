use crate::contract::{
    Action, ActionResult, CommandMeta, Emitter, Extension, Item, MoeError, NoopEmitter,
};
use crate::frecency::FrecencyLookup;
use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};
use std::sync::Arc;

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
                let haystack = format!(
                    "{} {} {}",
                    cmd.title,
                    cmd.subtitle.as_deref().unwrap_or(""),
                    ext.title()
                );
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
        if scored.is_empty() {
            // 无匹配：给扩展一个「捕获输入」的机会（如 AI 问答）。
            // 保持注册顺序（先注册的扩展优先，例如 `key …` 命中 Moe 的保存项）。
            return self
                .extensions
                .iter()
                .filter_map(|ext| ext.fallback_command(q))
                .collect();
        }
        scored.into_iter().map(|(_, _, cmd)| cmd).collect()
    }

    fn find(&self, command_id: &str) -> Option<&dyn Extension> {
        self.extensions
            .iter()
            .map(Box::as_ref)
            .find(|e| e.commands().iter().any(|c| c.id == command_id))
    }

    pub fn find_extension(&self, extension_id: &str) -> Option<&dyn Extension> {
        self.extensions
            .iter()
            .map(Box::as_ref)
            .find(|e| e.id() == extension_id)
    }

    /// Side View 续聊：按扩展 id 路由（ADR-0004）。
    pub fn side_continue(
        &self,
        extension_id: &str,
        conversation_id: &str,
        message: &str,
        emitter: Arc<dyn Emitter>,
    ) -> Result<String, MoeError> {
        self.find_extension(extension_id)
            .ok_or(MoeError::NotFound)?
            .side_continue(conversation_id, message, emitter)
    }

    pub fn invoke(
        &self,
        command_id: &str,
        query: Option<&str>,
        selection: Option<&str>,
    ) -> Result<ActionResult, MoeError> {
        self.invoke_streaming(command_id, query, selection, Arc::new(NoopEmitter))
    }

    pub fn invoke_streaming(
        &self,
        command_id: &str,
        query: Option<&str>,
        selection: Option<&str>,
        emitter: Arc<dyn Emitter>,
    ) -> Result<ActionResult, MoeError> {
        self.find(command_id)
            .ok_or(MoeError::NotFound)?
            .invoke_streaming(command_id, query, selection, emitter)
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
    use crate::contract::{ActionKind, CommandEvent, InputKind};
    use crate::frecency::{FrecencyLookup, NoFrecency};
    use std::sync::Mutex;

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
                    live: false,
                },
                CommandMeta {
                    id: "toy.hello".into(),
                    extension_id: "toy".into(),
                    title: "Hello Toy".into(),
                    subtitle: Some("backspace demo".into()),
                    input: InputKind::None,
                    live: false,
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
                        detail: None,
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

    struct StreamingToy;

    impl Extension for StreamingToy {
        fn id(&self) -> &str {
            "stream"
        }
        fn title(&self) -> &str {
            "Stream"
        }
        fn commands(&self) -> Vec<CommandMeta> {
            vec![CommandMeta {
                id: "stream.ask".into(),
                extension_id: "stream".into(),
                title: "Stream: Ask".into(),
                subtitle: None,
                input: InputKind::Query,
                live: false,
            }]
        }
        fn invoke(
            &self,
            _command_id: &str,
            _query: Option<&str>,
            _selection: Option<&str>,
        ) -> Result<ActionResult, MoeError> {
            // 覆盖了 invoke_streaming，默认路径不应被走到
            Err(MoeError::Internal(
                "default invoke should not be used".into(),
            ))
        }
        fn invoke_streaming(
            &self,
            command_id: &str,
            _query: Option<&str>,
            _selection: Option<&str>,
            emitter: Arc<dyn Emitter>,
        ) -> Result<ActionResult, MoeError> {
            let item = Item {
                id: "stream.item".into(),
                title: "partial…".into(),
                subtitle: None,
                actions: vec![action("write-back", ActionKind::Primary)],
                payload: serde_json::Value::Null,
                detail: None,
            };
            emitter.emit(CommandEvent::ItemUpdated {
                command_id: command_id.to_string(),
                item: item.clone(),
            });
            Ok(ActionResult::List { items: vec![item] })
        }
    }

    #[derive(Default)]
    struct RecordingEmitter(Arc<Mutex<Vec<CommandEvent>>>);

    impl Emitter for RecordingEmitter {
        fn emit(&self, event: CommandEvent) {
            self.0.lock().unwrap().push(event);
        }
    }

    #[test]
    fn streaming_events_route_through_registry() {
        let mut r = Registry::new();
        r.register(Box::new(StreamingToy));
        let recorder = RecordingEmitter::default();
        let events = Arc::clone(&recorder.0);

        let result = r
            .invoke_streaming("stream.ask", Some("q"), None, Arc::new(recorder))
            .unwrap();
        assert!(matches!(result, ActionResult::List { .. }));

        let events = events.lock().unwrap();
        assert_eq!(events.len(), 1);
        match &events[0] {
            CommandEvent::ItemUpdated { command_id, item } => {
                assert_eq!(command_id, "stream.ask");
                assert_eq!(item.id, "stream.item");
            }
        }
    }

    #[test]
    fn non_streaming_extensions_fall_back_to_invoke() {
        let recorder = RecordingEmitter::default();
        let result = registry()
            .invoke_streaming("toy.hello", None, None, Arc::new(recorder))
            .unwrap();
        assert_eq!(result, ActionResult::WriteBack { text: "hi".into() });
    }

    /// Side View 续聊契约（IIE4AD-360）：按扩展路由；未实现/未知扩展为 NotFound。
    /// 返回实际会话 id（空 id 的“新建”语义由扩展实现）。
    #[test]
    fn side_continue_routes_to_extension_or_not_found() {
        struct SideToy;
        impl Extension for SideToy {
            fn id(&self) -> &str {
                "side"
            }
            fn title(&self) -> &str {
                "Side"
            }
            fn commands(&self) -> Vec<CommandMeta> {
                vec![]
            }
            fn invoke(
                &self,
                _command_id: &str,
                _query: Option<&str>,
                _selection: Option<&str>,
            ) -> Result<ActionResult, MoeError> {
                Err(MoeError::NotFound)
            }
            fn side_continue(
                &self,
                conversation_id: &str,
                message: &str,
                emitter: Arc<dyn Emitter>,
            ) -> Result<String, MoeError> {
                emitter.emit(CommandEvent::ItemUpdated {
                    command_id: "ai.side".into(),
                    item: Item {
                        id: "side.item".into(),
                        title: message.into(),
                        subtitle: None,
                        actions: vec![],
                        payload: serde_json::Value::Null,
                        detail: None,
                    },
                });
                Ok(if conversation_id.is_empty() {
                    "42".into()
                } else {
                    conversation_id.into()
                })
            }
        }

        let mut r = Registry::new();
        r.register(Box::new(SideToy));
        r.register(Box::new(Toy));
        let recorder = RecordingEmitter::default();
        let events = Arc::clone(&recorder.0);

        let id = r
            .side_continue("side", "", "你好", Arc::new(recorder))
            .unwrap();
        assert_eq!(id, "42");
        assert_eq!(events.lock().unwrap().len(), 1);
        assert_eq!(
            r.side_continue("side", "7", "x", Arc::new(NoopEmitter))
                .unwrap(),
            "7"
        );
        assert!(matches!(
            r.side_continue("toy", "1", "x", Arc::new(NoopEmitter)),
            Err(MoeError::NotFound)
        ));
        assert!(matches!(
            r.side_continue("nope", "1", "x", Arc::new(NoopEmitter)),
            Err(MoeError::NotFound)
        ));
    }

    #[test]
    fn unmatched_query_offers_fallback_command() {
        struct FallbackToy;
        impl Extension for FallbackToy {
            fn id(&self) -> &str {
                "fb"
            }
            fn title(&self) -> &str {
                "Fallback"
            }
            fn commands(&self) -> Vec<CommandMeta> {
                vec![]
            }
            fn invoke(
                &self,
                _command_id: &str,
                _query: Option<&str>,
                _selection: Option<&str>,
            ) -> Result<ActionResult, MoeError> {
                Err(MoeError::NotFound)
            }
            fn fallback_command(&self, query: &str) -> Option<CommandMeta> {
                Some(CommandMeta {
                    id: "fb.ask".into(),
                    extension_id: "fb".into(),
                    title: format!("Ask「{query}」"),
                    subtitle: None,
                    input: InputKind::Query,
                    live: false,
                })
            }
        }

        let mut r = Registry::new();
        r.register(Box::new(FallbackToy));
        let hits = r.search("hello", &NoFrecency);
        assert_eq!(hits.len(), 1);
        assert!(hits[0].title.contains("hello"));

        // 有正常匹配时不出现 fallback
        let mut r = Registry::new();
        r.register(Box::new(FallbackToy));
        r.register(Box::new(Toy));
        let hits = r.search("toy", &NoFrecency);
        assert!(hits.iter().all(|c| c.id != "fb.ask"));
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
        // 副标题也进索引（"backspace demo" 只存在于 toy.hello 的 subtitle）
        let hits = registry().search("pace", &NoFrecency);
        assert_eq!(
            hits.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
            ["toy.hello"]
        );
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
                    live: false,
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
