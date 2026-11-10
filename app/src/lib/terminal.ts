/** The terminal app picked last time. A per-computer preference, not a roster change. */
const KEY = "agent-roster.terminal";

export function rememberedTerminal(): string | null {
  try {
    return localStorage.getItem(KEY);
  } catch {
    return null;
  }
}

export function rememberTerminal(id: string | null) {
  try {
    if (id) localStorage.setItem(KEY, id);
    else localStorage.removeItem(KEY);
  } catch {
    // Only a preference; the default terminal still works.
  }
}
