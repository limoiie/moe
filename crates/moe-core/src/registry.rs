use crate::contract::{
    Action, ActionResult, CommandMeta, CommandSection, Emitter, EntryKind, Extension,
    ExtensionMeta, Item, MoeError, NoopEmitter, Selection,
};
use crate::favorites::Favorites;
use crate::frecency::FrecencyLookup;
use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};
use std::sync::Arc;

/// "Suggestions" section (IIE4AD-395): on an empty query, lists recently used commands — up to 5, most recent first.
const SUGGESTION_LIMIT: usize = 5;
/// Suggestions section header (Raycast-style semantics; other section headers are extension names).
const SUGGESTIONS_TITLE: &str = "Suggestions";
/// "Favorites" section header (ADR-0027): user-curated commands, pinned above Suggestions.
const FAVORITES_TITLE: &str = "Favorites";
/// "Results" section header (ADR-0033): a searching page is ONE scored section, not per-source groups.
const RESULTS_TITLE: &str = "Results";

/// Group an ordered command list by "source" (Extension); group order = order of each group's best item
/// (matching score order on a query; frecency order on an empty query).
/// The grouping key is extension_id (unique); the display name is the extension's title — two same-named extensions still form two groups.
fn group_by_extension(
    extensions: &[Box<dyn Extension>],
    commands: impl Iterator<Item = CommandMeta>,
) -> Vec<CommandSection> {
    let mut sections: Vec<CommandSection> = Vec::new();
    for cmd in commands {
        let title = extensions
            .iter()
            .map(Box::as_ref)
            .find(|ext| ext.id() == cmd.extension_id)
            .map(|ext| ext.title())
            .unwrap_or(&cmd.extension_id)
            .to_string();
        match sections
            .iter_mut()
            .find(|section| section.items[0].extension_id == cmd.extension_id)
        {
            Some(section) => section.items.push(cmd),
            None => sections.push(CommandSection {
                title,
                items: vec![cmd],
            }),
        }
    }
    sections
}

/// Registry of all built-in Extensions (ADR-0003: built in at compile time, isolated by Namespace).
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

    /// All commands, each annotated with its owner's display name and kind (ADR-0030) — rows
    /// label themselves from these, so mixed sections (Favorites / Suggestions) stay correct.
    pub fn commands(&self) -> Vec<CommandMeta> {
        self.extensions
            .iter()
            .flat_map(|extension| {
                let title = extension.title().to_string();
                let kind = extension.command_kind();
                extension
                    .commands()
                    .into_iter()
                    .map(move |cmd| CommandMeta {
                        extension_title: cmd.extension_title.or(Some(title.clone())),
                        kind: cmd.kind.or_else(|| kind.clone()),
                        ..cmd
                    })
            })
            .collect()
    }

    /// Command palette search: nucleo fuzzy matching scores, frecency breaks ties.
    /// Empty query: a "Favorites" section is pinned on top (user-curated, ADR-0027), then a "Suggestions"
    /// section (recently used commands, IIE4AD-395), then grouped by source; a non-empty query is ONE
    /// "Results" section in score order (ADR-0033 — supersedes ADR-0020's per-source grouping while
    /// searching; each row still labels its owner, ADR-0030); `selection` matters only in the
    /// "no match → fallback" step.
    pub fn search(
        &self,
        query: &str,
        selection: Option<&Selection>,
        frecency: &dyn FrecencyLookup,
        favorites: &Favorites,
    ) -> Vec<CommandSection> {
        let q = query.trim();
        if q.is_empty() {
            let mut commands = self.commands();
            commands.sort_by(|a, b| {
                frecency
                    .frecency(&b.id)
                    .partial_cmp(&frecency.frecency(&a.id))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            let mut sections = Vec::new();
            // Favorites (ADR-0027): pinned first, in user order; ids of commands that no longer
            // exist (extension removed or renamed) are skipped silently.
            let mut pinned: std::collections::HashSet<String> = std::collections::HashSet::new();
            let favorite_items: Vec<CommandMeta> = favorites
                .ids()
                .iter()
                .filter_map(|id| commands.iter().find(|cmd| &cmd.id == id).cloned())
                .collect();
            if !favorite_items.is_empty() {
                pinned.extend(favorite_items.iter().map(|cmd| cmd.id.clone()));
                sections.push(CommandSection {
                    title: FAVORITES_TITLE.into(),
                    items: favorite_items,
                });
            }
            // Suggestions (IIE4AD-395): recently used commands pinned as their own section;
            // remaining commands group by extension as usual, without repeating favorites/suggestions.
            let mut used: Vec<(u64, CommandMeta)> = commands
                .iter()
                .filter(|cmd| !pinned.contains(&cmd.id))
                .filter_map(|cmd| frecency.last_used(&cmd.id).map(|t| (t, cmd.clone())))
                .collect();
            used.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.id.cmp(&b.1.id)));
            if !used.is_empty() {
                let suggested: std::collections::HashSet<String> = used
                    .iter()
                    .take(SUGGESTION_LIMIT)
                    .map(|(_, cmd)| cmd.id.clone())
                    .collect();
                pinned.extend(suggested);
                sections.push(CommandSection {
                    title: SUGGESTIONS_TITLE.into(),
                    items: used
                        .into_iter()
                        .take(SUGGESTION_LIMIT)
                        .map(|(_, cmd)| cmd)
                        .collect(),
                });
            }
            let rest = commands.into_iter().filter(|cmd| !pinned.contains(&cmd.id));
            sections.extend(group_by_extension(&self.extensions, rest));
            return sections;
        }
        let ordered: Vec<CommandMeta> = {
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
                        // Ordering contract (IIE4AD-346): exact prefix > position > frecency.
                        // The matcher's word-boundary heuristic cannot express "prefix first", so add an explicit bonus.
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
                // No match: give extensions a chance to "capture" the input (e.g. AI quick ask).
                // Registration order is kept (earlier extensions win, e.g. `key …` hits Moe's save item),
                // and captures land in the same single Results section (ADR-0033).
                let fallback: Vec<CommandMeta> = self
                    .extensions
                    .iter()
                    .flat_map(|extension| {
                        let title = extension.title().to_string();
                        let kind = extension.command_kind();
                        extension
                            .fallback_command(q, selection)
                            .into_iter()
                            .map(move |cmd| CommandMeta {
                                extension_title: cmd.extension_title.or(Some(title.clone())),
                                kind: cmd.kind.or_else(|| kind.clone()),
                                ..cmd
                            })
                    })
                    .collect();
                return if fallback.is_empty() {
                    Vec::new()
                } else {
                    vec![CommandSection {
                        title: RESULTS_TITLE.into(),
                        items: fallback,
                    }]
                };
            }
            scored.into_iter().map(|(_, _, cmd)| cmd).collect()
        };
        // ADR-0033: one flat scored list — the searching page no longer splits into per-source groups.
        vec![CommandSection {
            title: RESULTS_TITLE.into(),
            items: ordered,
        }]
    }

    fn find(&self, command_id: &str) -> Option<&dyn Extension> {
        self.extensions
            .iter()
            .map(Box::as_ref)
            .find(|e| e.commands().iter().any(|c| c.id == command_id))
    }

    /// General entries (ADR-0014): resolve, per the Extension owning the current command, its declared
    /// Browse (⌘P) / New (⌘N) entry command. None = the Extension has no such records.
    pub fn entry_command(&self, from_command: &str, kind: EntryKind) -> Option<CommandMeta> {
        let extension = self.find(from_command)?;
        match kind {
            EntryKind::Browse => extension.browse_command(),
            EntryKind::New => extension.new_command(),
        }
    }

    /// Delete the current record (general action Delete, ⌃X, ADR-0022): routed by the command's owning Extension.
    pub fn delete_item(&self, command_id: &str, item: &Item) -> Result<usize, MoeError> {
        self.find(command_id)
            .ok_or(MoeError::NotFound)?
            .delete_item(command_id, item)
    }

    /// Delete all records (general action DeleteAll, ⌃⇧X, ADR-0022): routed the same way.
    pub fn delete_all(&self, command_id: &str) -> Result<usize, MoeError> {
        self.find(command_id)
            .ok_or(MoeError::NotFound)?
            .delete_all(command_id)
    }

    pub fn find_extension(&self, extension_id: &str) -> Option<&dyn Extension> {
        self.extensions
            .iter()
            .map(Box::as_ref)
            .find(|e| e.id() == extension_id)
    }

    /// Extension identity for the avatar chip (ADR-0026): the Extension owning `command_id`,
    /// with a representative icon (its first command's icon, when any).
    pub fn extension_meta(&self, command_id: &str) -> Option<ExtensionMeta> {
        let extension = self.find(command_id)?;
        Some(ExtensionMeta {
            id: extension.id().to_string(),
            title: extension.title().to_string(),
            icon: extension.commands().first().and_then(|c| c.icon.clone()),
        })
    }

    /// Side View continuation: routed by extension id (ADR-0004).
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

    /// Stop generation in progress across all extensions; returns the number aborted (IIE4AD-365).
    pub fn stop_generation(&self) -> usize {
        self.extensions
            .iter()
            .map(|extension| extension.stop_generation())
            .sum()
    }

    pub fn invoke(
        &self,
        command_id: &str,
        query: Option<&str>,
        selection: Option<&Selection>,
    ) -> Result<ActionResult, MoeError> {
        self.invoke_streaming(command_id, query, selection, Arc::new(NoopEmitter))
    }

    pub fn invoke_streaming(
        &self,
        command_id: &str,
        query: Option<&str>,
        selection: Option<&Selection>,
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
    use crate::frecency::{Frecency, FrecencyLookup, NoFrecency};
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
                    icon: None,
                    input: InputKind::Query,
                    live: false,
                    keybinding: None,
                    extension_title: None,
                    kind: None,
                },
                CommandMeta {
                    id: "toy.hello".into(),
                    extension_id: "toy".into(),
                    title: "Hello Toy".into(),
                    subtitle: Some("backspace demo".into()),
                    icon: None,
                    input: InputKind::None,
                    live: false,
                    keybinding: None,
                    extension_title: None,
                    kind: None,
                },
            ]
        }
        fn invoke(
            &self,
            command_id: &str,
            _query: Option<&str>,
            _selection: Option<&Selection>,
        ) -> Result<ActionResult, MoeError> {
            match command_id {
                "toy.list" => Ok(ActionResult::list(vec![Item {
                    id: "item-1".into(),
                    title: "hello world".into(),
                    subtitle: None,
                    actions: vec![
                        action("write-back", ActionKind::Primary),
                        action("copy", ActionKind::Secondary),
                    ],
                    payload: serde_json::Value::Null,
                    detail: None,
                    pending: false,
                    icon: None,
                }])),
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
                icon: None,
                input: InputKind::Query,
                live: false,
                keybinding: None,
                extension_title: None,
                kind: None,
            }]
        }
        fn invoke(
            &self,
            _command_id: &str,
            _query: Option<&str>,
            _selection: Option<&Selection>,
        ) -> Result<ActionResult, MoeError> {
            // invoke_streaming is overridden, so the default path should not be reached
            Err(MoeError::Internal(
                "default invoke should not be used".into(),
            ))
        }
        fn invoke_streaming(
            &self,
            command_id: &str,
            _query: Option<&str>,
            _selection: Option<&Selection>,
            emitter: Arc<dyn Emitter>,
        ) -> Result<ActionResult, MoeError> {
            let item = Item {
                id: "stream.item".into(),
                title: "partial…".into(),
                subtitle: None,
                actions: vec![action("write-back", ActionKind::Primary)],
                payload: serde_json::Value::Null,
                detail: None,
                pending: false,
                icon: None,
            };
            emitter.emit(CommandEvent::ItemUpdated {
                command_id: command_id.to_string(),
                item: item.clone(),
            });
            Ok(ActionResult::list(vec![item]))
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
            CommandEvent::WriteBack { .. } => panic!("StreamingToy emits no WriteBack"),
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

    /// Side View continuation contract (IIE4AD-360): routed by extension; unimplemented/unknown extensions are NotFound.
    /// Returns the actual conversation id (the "new" semantic of an empty id is implemented by the extension).
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
                _selection: Option<&Selection>,
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
                        pending: false,
                        icon: None,
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
            .side_continue("side", "", "hello", Arc::new(recorder))
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

    /// Stop generation aggregates across extensions (IIE4AD-365): the default implementation returns 0 without erroring.
    #[test]
    fn stop_generation_sums_extension_counts() {
        struct StopToy;
        impl Extension for StopToy {
            fn id(&self) -> &str {
                "stop"
            }
            fn title(&self) -> &str {
                "Stop"
            }
            fn commands(&self) -> Vec<CommandMeta> {
                vec![]
            }
            fn invoke(
                &self,
                _command_id: &str,
                _query: Option<&str>,
                _selection: Option<&Selection>,
            ) -> Result<ActionResult, MoeError> {
                Err(MoeError::NotFound)
            }
            fn stop_generation(&self) -> usize {
                2
            }
        }

        let mut r = Registry::new();
        r.register(Box::new(StopToy));
        r.register(Box::new(Toy));
        assert_eq!(r.stop_generation(), 2);
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
                _selection: Option<&Selection>,
            ) -> Result<ActionResult, MoeError> {
                Err(MoeError::NotFound)
            }
            fn fallback_command(
                &self,
                query: &str,
                _selection: Option<&Selection>,
            ) -> Option<CommandMeta> {
                Some(CommandMeta {
                    id: "fb.ask".into(),
                    extension_id: "fb".into(),
                    title: format!("Ask \"{query}\""),
                    subtitle: None,
                    icon: None,
                    input: InputKind::Query,
                    live: false,
                    keybinding: None,
                    extension_title: None,
                    kind: None,
                })
            }
        }

        let mut r = Registry::new();
        r.register(Box::new(FallbackToy));
        let hits = r.search("hello", None, &NoFrecency, &Favorites::default());
        assert_eq!(hits.len(), 1);
        assert_eq!(
            hits[0].title, "Results",
            "captures land in the same single section (ADR-0033)"
        );
        assert!(hits[0].items[0].title.contains("hello"));

        // No fallback when there is a normal match
        let mut r = Registry::new();
        r.register(Box::new(FallbackToy));
        r.register(Box::new(Toy));
        let hits = r.search("toy", None, &NoFrecency, &Favorites::default());
        assert!(flat(&hits).iter().all(|c| c.id != "fb.ask"));
    }

    /// General entries (ADR-0014): entries resolve by "the Extension owning the current command";
    /// a declared id must be routable (otherwise the panel's invoke hits NotFound), undeclared is None.
    #[test]
    fn entry_command_resolves_per_extension_and_must_be_routable() {
        struct EntryToy;
        impl Extension for EntryToy {
            fn id(&self) -> &str {
                "entry"
            }
            fn title(&self) -> &str {
                "Entry"
            }
            fn commands(&self) -> Vec<CommandMeta> {
                vec![entry_meta("entry.run"), entry_meta("entry.browse")]
            }
            fn invoke(
                &self,
                _command_id: &str,
                _query: Option<&str>,
                _selection: Option<&Selection>,
            ) -> Result<ActionResult, MoeError> {
                Err(MoeError::NotFound)
            }
            fn browse_command(&self) -> Option<CommandMeta> {
                Some(entry_meta("entry.browse"))
            }
        }

        fn entry_meta(id: &str) -> CommandMeta {
            CommandMeta {
                id: id.into(),
                extension_id: "entry".into(),
                title: id.into(),
                subtitle: None,
                icon: None,
                input: InputKind::None,
                live: false,
                keybinding: None,
                extension_title: None,
                kind: None,
            }
        }

        let mut r = Registry::new();
        r.register(Box::new(EntryToy));
        r.register(Box::new(Toy));

        // Starting from this extension's command: get the declared entry, whose id is in this extension's commands()
        // (Registry::find only knows commands(); ids outside it can't be invoked — the same guard as fallback)
        let browse = r
            .entry_command("entry.run", EntryKind::Browse)
            .expect("browse entry");
        assert_eq!(browse.id, "entry.browse");
        assert!(
            r.commands().iter().any(|c| c.id == browse.id),
            "entry id must be routable in commands(): {}",
            browse.id
        );

        // New undeclared: None (the platform gives an inline hint, not silently)
        assert!(r.entry_command("entry.run", EntryKind::New).is_none());
        // Extension with no entries declared: both keybindings are None
        assert!(r.entry_command("toy.hello", EntryKind::Browse).is_none());
        // Nonexistent command: None (no panic)
        assert!(r.entry_command("nope.nope", EntryKind::Browse).is_none());
    }

    /// Avatar chip identity (ADR-0026): the owning Extension's id/title plus a representative icon.
    #[test]
    fn extension_meta_resolves_the_owner_and_its_avatar() {
        struct Branded;
        impl Extension for Branded {
            fn id(&self) -> &str {
                "branded"
            }
            fn title(&self) -> &str {
                "Branded"
            }
            fn commands(&self) -> Vec<CommandMeta> {
                vec![CommandMeta {
                    id: "branded.run".into(),
                    extension_id: "branded".into(),
                    title: "Run".into(),
                    subtitle: None,
                    icon: Some("wand-2".into()),
                    input: InputKind::None,
                    live: false,
                    keybinding: None,
                    extension_title: None,
                    kind: None,
                }]
            }
            fn invoke(
                &self,
                _command_id: &str,
                _query: Option<&str>,
                _selection: Option<&Selection>,
            ) -> Result<ActionResult, MoeError> {
                Err(MoeError::NotFound)
            }
        }

        let mut r = Registry::new();
        r.register(Box::new(Branded));
        let meta = r.extension_meta("branded.run").expect("owner resolves");
        assert_eq!(meta.id, "branded");
        assert_eq!(meta.title, "Branded");
        assert_eq!(
            meta.icon.as_deref(),
            Some("wand-2"),
            "avatar = the first command's icon"
        );
        // Toy commands carry no icon: the avatar falls back to None (UI renders its own fallback)
        let toy = registry()
            .extension_meta("toy.hello")
            .expect("owner resolves");
        assert_eq!(toy.title, "Toy");
        assert_eq!(toy.icon, None);
        // Unknown command: None (no panic)
        assert!(registry().extension_meta("nope.nope").is_none());
    }

    /// Delete slots (ADR-0022): routed by the command's owning Extension; unimplemented extensions are NotFound, no panic.
    #[test]
    fn delete_routes_to_extension_or_not_found() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        struct DeleteToy {
            deleted: AtomicUsize,
        }
        impl Extension for DeleteToy {
            fn id(&self) -> &str {
                "del"
            }
            fn title(&self) -> &str {
                "Delete"
            }
            fn commands(&self) -> Vec<CommandMeta> {
                vec![CommandMeta {
                    id: "del.list".into(),
                    extension_id: "del".into(),
                    title: "Del List".into(),
                    subtitle: None,
                    icon: None,
                    input: InputKind::None,
                    live: false,
                    keybinding: None,
                    extension_title: None,
                    kind: None,
                }]
            }
            fn invoke(
                &self,
                _command_id: &str,
                _query: Option<&str>,
                _selection: Option<&Selection>,
            ) -> Result<ActionResult, MoeError> {
                Err(MoeError::NotFound)
            }
            fn delete_item(&self, _command_id: &str, item: &Item) -> Result<usize, MoeError> {
                if item.payload.is_null() {
                    return Err(MoeError::NotFound);
                }
                Ok(self.deleted.fetch_add(1, Ordering::SeqCst) + 1)
            }
            fn delete_all(&self, _command_id: &str) -> Result<usize, MoeError> {
                Ok(7)
            }
        }

        let mut r = Registry::new();
        r.register(Box::new(DeleteToy {
            deleted: AtomicUsize::new(0),
        }));
        let item = Item {
            id: "del.1".into(),
            title: "Entry".into(),
            subtitle: None,
            icon: None,
            actions: vec![],
            payload: serde_json::json!({ "conversationId": "1" }),
            detail: None,
            pending: false,
        };
        assert_eq!(r.delete_item("del.list", &item).unwrap(), 1);
        assert_eq!(r.delete_all("del.list").unwrap(), 7);
        // Extension without delete / nonexistent command: NotFound
        let mut plain = Registry::new();
        plain.register(Box::new(Toy));
        assert!(matches!(
            plain.delete_all("toy.hello"),
            Err(MoeError::NotFound)
        ));
        assert!(matches!(
            plain.delete_item("nope.nope", &item),
            Err(MoeError::NotFound)
        ));
    }

    /// Flatten a section list into a command list (for assertions).
    fn flat(hits: &[CommandSection]) -> Vec<&CommandMeta> {
        hits.iter().flat_map(|s| &s.items).collect()
    }

    #[test]
    fn empty_query_lists_everything() {
        let hits = registry().search("", None, &NoFrecency, &Favorites::default());
        assert_eq!(hits.len(), 1, "single extension => single section");
        assert_eq!(hits[0].title, "Toy");
        assert_eq!(hits[0].items.len(), 2);
    }

    /// ADR-0033: a searching page is ONE "Results" section in score order, even when the matches
    /// span multiple extensions (each row still labels its owner, ADR-0030).
    #[test]
    fn search_results_are_one_scored_section_across_extensions() {
        struct OtherToy;
        impl Extension for OtherToy {
            fn id(&self) -> &str {
                "other"
            }
            fn title(&self) -> &str {
                "Other"
            }
            fn commands(&self) -> Vec<CommandMeta> {
                vec![CommandMeta {
                    id: "other.toy".into(),
                    extension_id: "other".into(),
                    title: "Another Toy".into(),
                    subtitle: None,
                    icon: None,
                    input: InputKind::None,
                    live: false,
                    keybinding: None,
                    extension_title: None,
                    kind: None,
                }]
            }
            fn invoke(
                &self,
                _command_id: &str,
                _query: Option<&str>,
                _selection: Option<&Selection>,
            ) -> Result<ActionResult, MoeError> {
                Err(MoeError::NotFound)
            }
        }

        let mut r = Registry::new();
        r.register(Box::new(Toy));
        r.register(Box::new(OtherToy));
        let hits = r.search("toy", None, &NoFrecency, &Favorites::default());
        assert_eq!(
            hits.len(),
            1,
            "matches from both extensions merge into one section"
        );
        assert_eq!(hits[0].title, "Results");
        let ids: Vec<&str> = hits[0].items.iter().map(|c| c.id.as_str()).collect();
        assert!(ids.contains(&"toy.list"));
        assert!(ids.contains(&"toy.hello"));
        assert!(ids.contains(&"other.toy"));
        // Score order (not source order): "Toy: List" earns the prefix bonus for "toy"
        assert_eq!(ids[0], "toy.list");
    }

    /// Fake frecency with last_used (for suggestions tests).
    struct RecentFrecency(std::collections::HashMap<String, (f64, u64)>);

    impl FrecencyLookup for RecentFrecency {
        fn frecency(&self, command_id: &str) -> f64 {
            self.0
                .get(command_id)
                .map(|(score, _)| *score)
                .unwrap_or(0.0)
        }

        fn last_used(&self, command_id: &str) -> Option<u64> {
            self.0.get(command_id).map(|(_, used)| *used)
        }
    }

    fn recent(pairs: &[(&str, u64)]) -> RecentFrecency {
        RecentFrecency(
            pairs
                .iter()
                .map(|(id, used)| ((*id).to_string(), (0.0, *used)))
                .collect(),
        )
    }

    /// Suggestions (IIE4AD-395): empty query pins recently used commands on top; the rest group as usual without repeats.
    #[test]
    fn empty_query_prepends_recent_suggestions() {
        // Toy has two commands; only toy.list has a use record
        let hits = registry().search(
            "",
            None,
            &recent(&[("toy.list", 100)]),
            &Favorites::default(),
        );
        assert_eq!(
            hits.len(),
            2,
            "suggestions section + the rest grouped by extension"
        );
        assert_eq!(hits[0].title, "Suggestions");
        assert_eq!(
            hits[0]
                .items
                .iter()
                .map(|c| c.id.as_str())
                .collect::<Vec<_>>(),
            ["toy.list"]
        );
        // Those already suggested don't repeat in the remaining groups
        assert_eq!(hits[1].title, "Toy");
        assert_eq!(
            hits[1]
                .items
                .iter()
                .map(|c| c.id.as_str())
                .collect::<Vec<_>>(),
            ["toy.hello"]
        );
        // No suggestions section on a non-empty query (Raycast-style: suggestions appear only on empty input)
        assert!(
            registry()
                .search(
                    "toy",
                    None,
                    &recent(&[("toy.list", 100)]),
                    &Favorites::default()
                )
                .iter()
                .all(|s| s.title != "Suggestions")
        );
    }

    /// Suggestions capped at 5, most recent first (IIE4AD-395).
    #[test]
    fn suggestions_are_capped_at_five_and_most_recent_first() {
        struct OneCommand(&'static str);
        impl Extension for OneCommand {
            fn id(&self) -> &str {
                self.0
            }
            fn title(&self) -> &str {
                "One"
            }
            fn commands(&self) -> Vec<CommandMeta> {
                vec![CommandMeta {
                    id: format!("{}.run", self.0),
                    extension_id: self.0.into(),
                    title: format!("Run {}", self.0),
                    subtitle: None,
                    icon: None,
                    input: InputKind::None,
                    live: false,
                    keybinding: None,
                    extension_title: None,
                    kind: None,
                }]
            }
            fn invoke(
                &self,
                _command_id: &str,
                _query: Option<&str>,
                _selection: Option<&Selection>,
            ) -> Result<ActionResult, MoeError> {
                Err(MoeError::NotFound)
            }
        }

        let mut r = Registry::new();
        for name in ["a", "b", "c", "d", "e", "f"] {
            r.register(Box::new(OneCommand(name)));
        }
        // Use times deliberately shuffled: f most recent, a oldest
        let used: Vec<(String, u64)> = vec![
            ("a.run", 10),
            ("f.run", 60),
            ("c.run", 30),
            ("b.run", 20),
            ("e.run", 50),
            ("d.run", 40),
        ]
        .into_iter()
        .map(|(id, t)| (id.to_string(), t))
        .collect();
        let hits = r.search(
            "",
            None,
            &RecentFrecency(used.into_iter().map(|(id, t)| (id, (0.0, t))).collect()),
            &Favorites::default(),
        );
        assert_eq!(hits[0].title, "Suggestions");
        assert_eq!(
            hits[0]
                .items
                .iter()
                .map(|c| c.id.as_str())
                .collect::<Vec<_>>(),
            ["f.run", "e.run", "d.run", "c.run", "b.run"],
            "most recent first, at most 5"
        );
        // The oldest one (a.run) stays in the remaining groups
        assert_eq!(
            flat(&hits[1..])
                .iter()
                .map(|c| c.id.as_str())
                .collect::<Vec<_>>(),
            ["a.run"]
        );
    }

    /// Forgetting usage (ADR-0025): the command leaves the Suggestions section but
    /// remains listed in its source section; clearing all removes the section entirely.
    #[test]
    fn forgetting_usage_removes_suggestions_but_keeps_commands_listed() {
        let mut r = Registry::new();
        r.register(Box::new(Toy));
        let mut frecency = Frecency::default();
        frecency.record("toy.hello", std::time::SystemTime::now());

        let hits = r.search("", None, &frecency, &Favorites::default());
        assert_eq!(hits[0].title, "Suggestions");
        assert_eq!(hits[0].items[0].id, "toy.hello");

        frecency.remove("toy.hello");
        let hits = r.search("", None, &frecency, &Favorites::default());
        assert!(
            hits.iter().all(|s| s.title != "Suggestions"),
            "no usage left => no Suggestions section"
        );
        assert!(
            flat(&hits).iter().any(|c| c.id == "toy.hello"),
            "the command stays listed in its source section"
        );

        // Clear-all path behaves the same once usage exists again
        frecency.record("toy.list", std::time::SystemTime::now());
        frecency.record("toy.hello", std::time::SystemTime::now());
        frecency.clear();
        let hits = r.search("", None, &frecency, &Favorites::default());
        assert!(hits.iter().all(|s| s.title != "Suggestions"));
        assert_eq!(flat(&hits).len(), 2, "both commands still listed");
    }

    /// Favorites (ADR-0027): pinned above Suggestions in user order; not repeated below;
    /// unknown ids (extension gone) are skipped silently.
    #[test]
    fn favorites_are_pinned_above_suggestions_without_repeats() {
        let mut favorites = Favorites::default();
        favorites.toggle("toy.hello");
        favorites.toggle("missing.command");
        let hits = registry().search("", None, &recent(&[("toy.list", 100)]), &favorites);
        assert_eq!(
            hits.iter().map(|s| s.title.as_str()).collect::<Vec<_>>(),
            ["Favorites", "Suggestions"],
            "favorites sit above suggestions; both toy commands are pinned so no Toy group remains"
        );
        assert_eq!(hits[0].items[0].id, "toy.hello");
        // The favorite is not repeated in Suggestions (toy.list) nor in its own group
        let below: Vec<&str> = flat(&hits[1..]).iter().map(|c| c.id.as_str()).collect();
        assert_eq!(below, ["toy.list"], "favorites are not repeated below");
        // Unknown ids are skipped silently (no panic, no empty section for them)
        assert!(hits[0].items.iter().all(|c| c.id != "missing.command"));
    }

    /// Without favorites the top section stays Suggestions (existing behavior, IIE4AD-395).
    #[test]
    fn no_favorites_means_no_favorites_section() {
        let hits = registry().search(
            "",
            None,
            &recent(&[("toy.list", 100)]),
            &Favorites::default(),
        );
        assert_eq!(hits[0].title, "Suggestions");
    }

    /// The empty-query top order is Favorites → Suggestions → sources even when only favorites exist.
    #[test]
    fn favorites_alone_still_show_before_sources() {
        let mut favorites = Favorites::default();
        favorites.toggle("toy.list");
        let hits = registry().search("", None, &NoFrecency, &favorites);
        assert_eq!(hits[0].title, "Favorites");
        assert_eq!(hits[0].items.len(), 1);
        assert_eq!(hits[1].title, "Toy");
        assert!(hits[1].items.iter().all(|c| c.id != "toy.list"));
    }

    /// Rows are annotated with their owner's name and kind (ADR-0030): the UI labels rows from the
    /// payload, so mixed sections (Favorites/Suggestions) and fallbacks stay correct.
    #[test]
    fn commands_are_annotated_with_owner_and_kind() {
        struct Kinded;
        impl Extension for Kinded {
            fn id(&self) -> &str {
                "kinded"
            }
            fn title(&self) -> &str {
                "Kinded"
            }
            fn command_kind(&self) -> Option<String> {
                Some("AI Command".into())
            }
            fn commands(&self) -> Vec<CommandMeta> {
                vec![CommandMeta {
                    id: "kinded.run".into(),
                    extension_id: "kinded".into(),
                    title: "Run".into(),
                    subtitle: None,
                    icon: None,
                    input: InputKind::None,
                    live: false,
                    keybinding: Some("⌘,".into()),
                    extension_title: None,
                    kind: None,
                }]
            }
            fn invoke(
                &self,
                _command_id: &str,
                _query: Option<&str>,
                _selection: Option<&Selection>,
            ) -> Result<ActionResult, MoeError> {
                Err(MoeError::NotFound)
            }
        }

        let mut r = Registry::new();
        r.register(Box::new(Kinded));
        let cmd = &r.commands()[0];
        assert_eq!(cmd.extension_title.as_deref(), Some("Kinded"));
        assert_eq!(cmd.kind.as_deref(), Some("AI Command"));
        assert_eq!(
            cmd.keybinding.as_deref(),
            Some("⌘,"),
            "extension-declared shortcut kept"
        );
        // Plain extensions (Toy) get the owner name but no kind → the UI renders the "Command" default
        let toy = &registry().commands()[0];
        assert_eq!(toy.extension_title.as_deref(), Some("Toy"));
        assert_eq!(toy.kind, None);
    }

    #[test]
    fn empty_query_orders_by_frecency() {
        let hits = registry().search(
            "",
            None,
            &fixed(&[("toy.hello", 9.0)]),
            &Favorites::default(),
        );
        assert_eq!(
            flat(&hits)
                .iter()
                .map(|c| c.id.as_str())
                .collect::<Vec<_>>(),
            ["toy.hello", "toy.list"]
        );
    }

    #[test]
    fn search_matches_title_and_extension() {
        let hits = registry().search("list", None, &NoFrecency, &Favorites::default());
        assert_eq!(
            flat(&hits)
                .iter()
                .map(|c| c.id.as_str())
                .collect::<Vec<_>>(),
            ["toy.list"]
        );
        assert_eq!(
            flat(&registry().search("toy", None, &NoFrecency, &Favorites::default())).len(),
            2
        );
        // Subtitles are indexed too ("backspace demo" exists only in toy.hello's subtitle)
        let hits = registry().search("pace", None, &NoFrecency, &Favorites::default());
        assert_eq!(
            flat(&hits)
                .iter()
                .map(|c| c.id.as_str())
                .collect::<Vec<_>>(),
            ["toy.hello"]
        );
    }

    #[test]
    fn fuzzy_subsequence_matches_and_prefix_wins() {
        // "tl": a subsequence of "Toy: List"; in "Hello Toy" l precedes t, so no match
        let hits = registry().search("tl", None, &NoFrecency, &Favorites::default());
        assert_eq!(
            flat(&hits)
                .iter()
                .map(|c| c.id.as_str())
                .collect::<Vec<_>>(),
            ["toy.list"]
        );
        assert!(
            registry()
                .search("zzz", None, &NoFrecency, &Favorites::default())
                .is_empty()
        );
        // Prefix match beats subsequence match
        let hits = registry().search("toy", None, &NoFrecency, &Favorites::default());
        assert_eq!(flat(&hits).first().map(|c| c.id.as_str()), Some("toy.list"));
    }

    #[test]
    fn frecency_breaks_score_ties() {
        // Two twin commands (same title, same extension name): equal nucleo scores, frecency decides the order
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
                    icon: None,
                    input: InputKind::None,
                    live: false,
                    keybinding: None,
                    extension_title: None,
                    kind: None,
                }]
            }
            fn invoke(
                &self,
                _command_id: &str,
                _query: Option<&str>,
                _selection: Option<&Selection>,
            ) -> Result<ActionResult, MoeError> {
                Err(MoeError::NotFound)
            }
        }

        let mut r = Registry::new();
        r.register(Box::new(Twin("a")));
        r.register(Box::new(Twin("b")));
        let hits = r.search(
            "deploy",
            None,
            &fixed(&[("b.deploy", 9.0)]),
            &Favorites::default(),
        );
        assert_eq!(
            flat(&hits)
                .iter()
                .map(|c| c.id.as_str())
                .collect::<Vec<_>>(),
            ["b.deploy", "a.deploy"]
        );
        // ADR-0033: both twins land in the single "Results" section; frecency still breaks the score tie
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].title, "Results");
        assert_eq!(hits[0].items[0].id, "b.deploy");
        assert_eq!(hits[0].items[1].id, "a.deploy");
    }

    #[test]
    fn item_stream_composes_to_write_back() {
        let r = registry();
        let ActionResult::List { items, .. } = r.invoke("toy.list", None, None).unwrap() else {
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
