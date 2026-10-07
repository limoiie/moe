//! Screen placement geometry (ADR-0032), shared by the macOS-native AppKit path and the
//! tauri-monitor path (Linux/Windows): the panel is horizontally centered on the active screen and
//! anchored near its top, so when a page changes shape the panel grows downward while its top edge
//! and horizontal center stay put.

/// Where a top-centered panel sits on a screen.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PanelAnchor {
    /// The panel's left edge.
    pub left: f64,
    /// Downward distance from the visible area's top edge to the panel's top edge.
    pub top_drop: f64,
}

/// The panel's top drop: a fifth of the visible height, floored at 48 (the Raycast-like "near the
/// top" position; the floor keeps very short screens from pressing the panel against the menu bar).
pub fn panel_top_drop(visible_height: f64) -> f64 {
    (visible_height * 0.2).max(48.0)
}

/// Anchor a panel: centered horizontally in the visible area, `top_drop` below its top edge.
/// x/width/height are the *visible* area's (menu bar and dock excluded); the coordinate space is
/// the caller's — only the horizontal extent and the height participate, so AppKit's
/// bottom-left-origin space and top-left-origin monitor spaces compose identically.
pub fn panel_anchor(
    visible_x: f64,
    visible_width: f64,
    visible_height: f64,
    panel_width: f64,
) -> PanelAnchor {
    PanelAnchor {
        left: visible_x + ((visible_width - panel_width) / 2.0).max(0.0),
        top_drop: panel_top_drop(visible_height),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn top_drop_is_a_fifth_of_the_visible_height() {
        assert_eq!(panel_top_drop(900.0), 180.0);
        assert_eq!(panel_top_drop(982.0), 196.4);
    }

    #[test]
    fn top_drop_floors_on_short_screens() {
        assert_eq!(panel_top_drop(300.0), 60.0);
        assert_eq!(panel_top_drop(240.0), 48.0);
        assert_eq!(panel_top_drop(200.0), 48.0);
    }

    #[test]
    fn anchor_centers_the_panel_horizontally() {
        let anchor = panel_anchor(0.0, 1600.0, 900.0, 768.0);
        assert_eq!(anchor.left, 416.0);
        assert_eq!(anchor.top_drop, 180.0);
    }

    #[test]
    fn anchor_accounts_for_the_screens_origin() {
        // A screen to the right of the main one (origin x > 0)
        let anchor = panel_anchor(1600.0, 1600.0, 900.0, 768.0);
        assert_eq!(anchor.left, 2016.0);
    }

    #[test]
    fn anchor_keeps_a_too_wide_panel_flush_left() {
        let anchor = panel_anchor(0.0, 600.0, 900.0, 768.0);
        assert_eq!(anchor.left, 0.0);
    }

    #[test]
    fn anchor_measures_the_drop_from_the_visible_area() {
        // Built-in display: visible height excludes the menu bar
        let anchor = panel_anchor(0.0, 1512.0, 944.0, 768.0);
        assert_eq!(anchor.left, 372.0);
        assert_eq!(anchor.top_drop, 188.8);
    }
}
