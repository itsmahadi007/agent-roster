import { invoke } from "@tauri-apps/api/core";
import type { ChangeView, FirstRunView, OpRequest, OverviewView, PlanView, TerminalApp } from "./types";

export const overview = () => invoke<OverviewView>("overview");
export const firstRun = () => invoke<FirstRunView>("first_run");
export const preview = (req: OpRequest) => invoke<PlanView>("preview", { req });
export const apply = (token: number) => invoke<number | null>("apply", { token });
export const changes = () => invoke<ChangeView[]>("changes");
export const undo = (id: number) => invoke<void>("undo", { id });
export const undoAll = () => invoke<number>("undo_all");
/** Both return true when the terminal runs the command, false when it was only opened (paste it). */
export const signIn = (name: string, app: string | null = null) => invoke<boolean>("sign_in", { name, app });
export const openTerminalAs = (name: string, app: string | null = null) =>
  invoke<boolean>("open_terminal_as", { name, app });
export const terminals = () => invoke<TerminalApp[]>("terminals");
export const openSite = () => invoke<void>("open_site");

/** Copies text; false when the system refused (the text is then shown to copy by hand). */
export async function copyText(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    return false;
  }
}

/** Backend errors arrive as plain strings. */
export const message = (e: unknown) => (typeof e === "string" ? e : e instanceof Error ? e.message : String(e));
