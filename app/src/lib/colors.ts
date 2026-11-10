import type { ColorView } from "./types";

/**
 * Agent colors say who, usage colors say how much. The agent palette has no warning hues
 * (no red, no yellow) so an agent's color never reads as "almost out".
 *
 * roster-core's colors are these since 0.4.0 (cobalt, cyan, jade, orchid shown as violet,
 * pink). Rosters saved before still say "marigold" / "coral"; the core reads them as cyan and
 * pink. The old names and hex values below are only a safety net for stale data.
 */
export const AGENT_HEX: Record<string, string> = {
  cobalt: "#4f7cff",
  cyan: "#3cc7e6",
  jade: "#2ec4a0",
  violet: "#b86bff",
  pink: "#ff6bc4",
};

/** Stored label → shown label. Labels already new pass through. */
const LABEL: Record<string, string> = {
  cobalt: "cobalt",
  marigold: "cyan",
  jade: "jade",
  orchid: "violet",
  coral: "pink",
};

/** Stored hex (roster-core's AgentColor::hex) → shown hex. */
const HEX: Record<string, string> = {
  "#4d7cff": AGENT_HEX.cobalt,
  "#f0b429": AGENT_HEX.cyan,
  "#2fbf8f": AGENT_HEX.jade,
  "#c06bf0": AGENT_HEX.violet,
  "#ff7a6b": AGENT_HEX.pink,
};

/** The name a stored color label is shown with. */
export function colorName(label: string): string {
  const l = label.toLowerCase();
  return LABEL[l] ?? l;
}

/** The hex a stored agent color is drawn with. Unknown colors are drawn as they are. */
export function agentHex(hex: string | null | undefined): string {
  if (!hex) return "var(--mut)";
  return HEX[hex.toLowerCase()] ?? hex;
}

export interface Swatch {
  /** What is sent to the backend (the stored label). */
  value: string;
  /** What the person sees. */
  label: string;
  hex: string;
}

/** The color picker: the backend's palette, shown with the new names and hues. */
export function swatches(palette: ColorView[]): Swatch[] {
  const seen = new Set<string>();
  const out: Swatch[] = [];
  for (const c of palette) {
    const label = colorName(c.label);
    if (seen.has(label)) continue;
    seen.add(label);
    out.push({ value: c.label, label, hex: AGENT_HEX[label] ?? agentHex(c.hex) });
  }
  return out;
}

export type UsageLevel = "ok" | "warn" | "hot";

/** How full a limit is, from its use alone: never from the agent's color. */
export function usageLevel(usedPercent: number, limitReached = false): UsageLevel {
  if (limitReached || usedPercent >= 90) return "hot";
  if (usedPercent >= 70) return "warn";
  return "ok";
}

/** Bar fill for a level (CSS custom properties from app.css). */
export const BAR: Record<UsageLevel, string> = {
  ok: "var(--u-ok)",
  warn: "var(--u-warn)",
  hot: "var(--u-hot)",
};
