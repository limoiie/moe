// Appearance (ADR-0035): two themes — light and dark — behind one semantic token layer (styles.css).
//
// Resolution order:
//   1. the inline head script seeds `data-theme` from the OS appearance before the first paint;
//   2. this module applies the config preference (`[ui] theme` in config.toml) once the backend answers;
//   3. in "system" the OS appearance keeps winning: `prefers-color-scheme` changes re-resolve live;
//   4. a runtime switch anywhere (`set_theme`, the About card's theme rows) arrives as the
//      `theme-changed` broadcast, so both windows re-theme together without a restart.
//
// A failed invoke/listen (plain-browser dev, no backend) leaves the OS appearance in charge.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export type ThemePreference = "system" | "light" | "dark";

/** The live preference: config.toml seeds it at startup, a runtime switch replaces it. */
let preference: ThemePreference = "system";

function isPreference(value: string): value is ThemePreference {
  return value === "system" || value === "light" || value === "dark";
}

function systemTheme(): "light" | "dark" {
  return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

function applyTheme(): void {
  document.documentElement.dataset.theme = preference === "system" ? systemTheme() : preference;
}

/** The current preference (the About card's theme rows check the active one). */
export function themePreference(): ThemePreference {
  return preference;
}

/** Apply the appearance; both windows call this once at startup. */
export function initTheme(): void {
  window.matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => {
    if (preference === "system") applyTheme();
  });
  applyTheme();

  void listen<string>("theme-changed", (event) => {
    if (isPreference(event.payload)) {
      preference = event.payload;
      applyTheme();
    }
  }).catch(() => {
    // No backend (browser dev): the OS appearance stays in charge
  });

  void invoke<string>("ui_theme")
    .then((value) => {
      if (isPreference(value)) {
        preference = value;
        applyTheme();
      }
    })
    .catch(() => {
      // No backend (browser dev): the OS appearance stays in charge
    });
}