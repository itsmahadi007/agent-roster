/**
 * Which window frame the app draws itself. On the Mac the system's own buttons sit on top of
 * the app's bar; on Windows the app draws them; elsewhere the system frame is kept.
 * Must match tauri.macos.conf.json and tauri.windows.conf.json.
 */
export type Chrome = "mac" | "windows" | "none";

export function chromeFor(ua: string): Chrome {
  if (/Macintosh|Mac OS X/.test(ua)) return "mac";
  if (/Windows/.test(ua)) return "windows";
  return "none";
}

export const chrome: Chrome = typeof navigator === "undefined" ? "none" : chromeFor(navigator.userAgent);
