// The last used style, shared by every editor window through the WebView's local storage.
// Reading or writing never throws: storage can be unavailable or hold anything.
import { DEFAULT_STYLE, type StyleSettings, validateStyle } from "../model/style";

export const STYLE_PREFS_KEY = "snipt.editor.lastStyle.v1";

/** The stored style with invalid fields replaced by the defaults, or the defaults. */
export function loadStyle(): StyleSettings {
  try {
    const raw = localStorage.getItem(STYLE_PREFS_KEY);
    return raw === null ? DEFAULT_STYLE : validateStyle(JSON.parse(raw));
  } catch {
    return DEFAULT_STYLE;
  }
}

export function saveStyle(style: StyleSettings): void {
  try {
    localStorage.setItem(STYLE_PREFS_KEY, JSON.stringify(style));
  } catch {
    // Not remembering the style is harmless.
  }
}
