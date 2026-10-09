//! Platform-level unified keymap (Keymap). Extensions must not override it; they can only attach shortcuts to their own actions.
//!
//! General actions share one set of keybinding semantics across all Commands / sub-apps (ADR-0014/0022/0029):
//! Browse (⌘P, record list), Actions (⌘K, action list), New (⌘N, new record),
//! Delete (⌃X, delete current record), DeleteAll (⌃⇧X, delete all records),
//! Favorite (⌘⇧F, add/remove the current command from Favorites).
//! The AI surfaces' launch keys are platform-fixed the same way (ADR-0036 amendment):
//! Quick Ask (⌘/, the panel's conversation page) and Open Side Chat (⌘⇧/, the Side View directly).
//! The platform only fixes keybindings and routing; concrete entries are declared by Extensions.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SystemKey {
    NavDown,
    NavUp,
    /// Apply: the Focused Item's primary action.
    Apply,
    /// First secondary action (default semantic: copy plain text).
    SecondaryCopy,
    /// Show All Actions: expand the current context's primary/secondary action list (⌘K on every surface, ADR-0014 amendment).
    ShowAllActions,
    /// Browse: open the current Extension's record list (AI = conversation history). General action, default ⌘P.
    Browse,
    /// New: create a new record (AI = new conversation). General action, default ⌘N.
    New,
    /// Delete: delete the current record (AI = current conversation). General action, default ⌃X (ADR-0022).
    Delete,
    /// DeleteAll: delete all records (AI = all conversations). General action, default ⌃⇧X (ADR-0022).
    DeleteAll,
    /// About: open the app's About card (the avatar chip's meta actions: Open Config File / Save AI Key / Send Feedback). Default ⌘⇧K.
    About,
    /// Favorite: add/remove the current command from Favorites (ADR-0029). General action, default ⌘⇧F.
    Favorite,
    /// OpenConfig: open the config file (`moe.open-config`'s semantic). Default ⌘, — the macOS Preferences convention.
    OpenConfig,
    /// Esc: layered back (with input → clear; on a result layer → back up one layer; otherwise close the panel).
    Back,
    /// Materialize: turn into the Extension's Side View. Default ⌘J (ADR-0014 amendment).
    Materialize,
    /// Attach: add attachments to the current question (AI semantic: `@path` mention, ADR-0010).
    Attach,
    /// Quick Ask: open the AI extension's conversation page in the panel from anywhere (ADR-0036). Default ⌘/.
    QuickAsk,
    /// Open Side Chat: open the AI chat in the Side View directly, without entering a panel page. Default ⌘⇧/.
    OpenSideChat,
}

/// (display string, semantic). The UI and keyboard events bind by semantic; the display string goes into the Hints Bar.
pub fn default_keymap() -> Vec<(&'static str, SystemKey)> {
    use SystemKey as K;
    vec![
        ("↓ / ⌃N", K::NavDown),
        ("↑ / ⌃P", K::NavUp),
        ("⏎", K::Apply),
        ("⌥⏎", K::SecondaryCopy),
        // One key on every surface: ⌘K (ADR-0014 amendment: the sub-app's ⌘⇧P alias is gone)
        ("⌘K", K::ShowAllActions),
        ("⌘P", K::Browse),
        ("⌘N", K::New),
        ("⌃X", K::Delete),
        ("⌃⇧X", K::DeleteAll),
        ("⌘⇧K", K::About),
        ("⌘⇧F", K::Favorite),
        ("⌘,", K::OpenConfig),
        ("Esc", K::Back),
        ("⌘J", K::Materialize),
        ("⌘⇧A", K::Attach),
        ("⌘/", K::QuickAsk),
        ("⌘⇧/", K::OpenSideChat),
    ]
}

/// The display string for one semantic (the first row that carries it), e.g. `OpenConfig` → "⌘,".
/// Extensions declaring a command's invocation shortcut read it from here, so the displayed Kbd
/// can never drift from the platform table (ADR-0030).
pub fn display_of(key: SystemKey) -> Option<&'static str> {
    default_keymap()
        .into_iter()
        .find(|(_, semantic)| *semantic == key)
        .map(|(display, _)| display)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each semantic appears exactly once in the keymap (a display string may hold multiple keybindings, e.g. "↓ / ⌃N").
    #[test]
    fn every_semantic_has_exactly_one_row() {
        let table = default_keymap();
        let mut seen: Vec<SystemKey> = Vec::new();
        for (display, key) in &table {
            assert!(!display.trim().is_empty(), "{key:?} missing display string");
            assert!(
                !seen.contains(key),
                "{key:?} appears multiple times: semantics must be unique, keybindings may be many",
            );
            seen.push(*key);
        }
        assert_eq!(
            seen.len(),
            17,
            "keep the Hints Bar grouping in sync when adding semantics"
        );
    }

    /// General actions' default keybindings are a contract across Commands/sub-apps (ADR-0014/0022).
    #[test]
    fn general_actions_keep_their_default_bindings() {
        let table = default_keymap();
        let display = |key: SystemKey| {
            table
                .iter()
                .find(|(_, k)| *k == key)
                .map(|(d, _)| *d)
                .unwrap_or_else(|| panic!("{key:?} not registered"))
        };
        assert_eq!(display(SystemKey::Browse), "⌘P");
        assert_eq!(display(SystemKey::New), "⌘N");
        assert_eq!(display(SystemKey::ShowAllActions), "⌘K");
        assert_eq!(display(SystemKey::Materialize), "⌘J");
        assert_eq!(display(SystemKey::Delete), "⌃X");
        assert_eq!(display(SystemKey::DeleteAll), "⌃⇧X");
    }

    /// About is a platform semantic too: ⌘⇧K opens the app's About card (ADR-0027).
    #[test]
    fn about_keeps_its_default_binding() {
        let table = default_keymap();
        let display = table
            .iter()
            .find(|(_, k)| *k == SystemKey::About)
            .map(|(d, _)| *d);
        assert_eq!(display, Some("⌘⇧K"));
    }

    /// Favorite is a platform semantic too: ⌘⇧F toggles the current command's favorite state (ADR-0029).
    #[test]
    fn favorite_keeps_its_default_binding() {
        let table = default_keymap();
        let display = table
            .iter()
            .find(|(_, k)| *k == SystemKey::Favorite)
            .map(|(d, _)| *d);
        assert_eq!(display, Some("⌘⇧F"));
    }

    /// OpenConfig follows the macOS Preferences convention: ⌘, (ADR-0027 amendment).
    #[test]
    fn open_config_keeps_the_preferences_convention() {
        let table = default_keymap();
        let display = table
            .iter()
            .find(|(_, k)| *k == SystemKey::OpenConfig)
            .map(|(d, _)| *d);
        assert_eq!(display, Some("⌘,"));
    }

    /// The AI surfaces' launch keys are a platform contract too (ADR-0036 amendment):
    /// ⌘/ opens the Quick Ask page, ⌘⇧/ opens the side chat directly.
    #[test]
    fn ai_surfaces_keep_their_launch_bindings() {
        let table = default_keymap();
        let display = |key: SystemKey| table.iter().find(|(_, k)| *k == key).map(|(d, _)| *d);
        assert_eq!(display(SystemKey::QuickAsk), Some("⌘/"));
        assert_eq!(display(SystemKey::OpenSideChat), Some("⌘⇧/"));
    }

    /// `display_of` is the single read path extensions use for declared invocation shortcuts (ADR-0030).
    #[test]
    fn display_of_reads_the_table() {
        assert_eq!(display_of(SystemKey::OpenConfig), Some("⌘,"));
        assert_eq!(display_of(SystemKey::Favorite), Some("⌘⇧F"));
    }
}
