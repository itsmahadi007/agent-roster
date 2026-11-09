import { invoke } from "@tauri-apps/api/core";
import type { ChangeView, FirstRunView, OpRequest, OverviewView, PlanView } from "./types";

export const overview = () => invoke<OverviewView>("overview");
export const firstRun = () => invoke<FirstRunView>("first_run");
export const preview = (req: OpRequest) => invoke<PlanView>("preview", { req });
export const apply = (token: number) => invoke<number | null>("apply", { token });
export const changes = () => invoke<ChangeView[]>("changes");
export const undo = (id: number) => invoke<void>("undo", { id });
export const undoAll = () => invoke<number>("undo_all");
export const signIn = (name: string) => invoke<void>("sign_in", { name });
export const openTerminalAs = (name: string) => invoke<void>("open_terminal_as", { name });

/** Backend errors arrive as plain strings. */
export const message = (e: unknown) => (typeof e === "string" ? e : e instanceof Error ? e.message : String(e));
