import { invoke } from "@tauri-apps/api/core";
import { message } from "./api";

/** Where Sign in sends the person: the browser (a terminal running `roster login`), or an Agent Roster window. */
export type SignInWith = "browser" | "window";

/** What the backend says about an in-app sign-in (event `sign-in`). */
export interface SignInUpdate {
  id: string;
  name: string;
  state: "waiting" | "done" | "failed" | "canceled";
  reason: string | null;
}

export interface LoginState {
  state: "waiting" | "failed";
  reason: string | null;
}

/** The app's setting; the browser until the backend says otherwise (the owner's default). */
export const prefs = $state<{ signInWith: SignInWith }>({ signInWith: "browser" });

/** In-app sign-ins by agent id: kept here so they survive the panel closing and opening. */
export const logins = $state<Record<string, LoginState>>({});

export async function loadPrefs() {
  try {
    const s = await invoke<{ sign_in_with?: SignInWith } | null>("settings");
    if (s?.sign_in_with) prefs.signInWith = s.sign_in_with;
  } catch {
    // Keep the default: the browser.
  }
}

export async function setSignInWith(w: SignInWith) {
  const s = await invoke<{ sign_in_with?: SignInWith } | null>("set_sign_in_with", { with: w });
  prefs.signInWith = s?.sign_in_with ?? w;
}

export function onSignInUpdate(u: SignInUpdate) {
  if (u.state === "waiting") logins[u.id] = { state: "waiting", reason: null };
  else if (u.state === "failed") logins[u.id] = { state: "failed", reason: u.reason ?? "The sign-in didn't finish." };
  else delete logins[u.id];
}

/** Starts signing `name` in inside an Agent Roster window. `id`, when known, shows the progress at once. */
export async function signInInApp(name: string, id?: string) {
  if (id) logins[id] = { state: "waiting", reason: null };
  try {
    await invoke("sign_in_in_app", { name });
  } catch (e) {
    if (id) logins[id] = { state: "failed", reason: message(e) };
    else throw e;
  }
}

export function clearLogin(id: string) {
  delete logins[id];
}

/** For tests: back to the default and no sign-ins. */
export function resetSignIn() {
  prefs.signInWith = "browser";
  for (const k of Object.keys(logins)) delete logins[k];
}
