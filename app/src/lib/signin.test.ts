import { describe, it, expect, beforeEach } from "vitest";
import { invoke } from "../test/setup";
import { logins, loadPrefs, onSignInUpdate, prefs, resetSignIn, setSignInWith, signInInApp } from "./signin.svelte";

describe("sign-in setting and progress", () => {
  beforeEach(() => resetSignIn());

  it("the setting is the browser until the backend says otherwise", async () => {
    expect(prefs.signInWith).toBe("browser");
    invoke.mockRejectedValue("no backend");
    await loadPrefs();
    expect(prefs.signInWith).toBe("browser");
    invoke.mockResolvedValue({ sign_in_with: "window" });
    await loadPrefs();
    expect(invoke).toHaveBeenLastCalledWith("settings");
    expect(prefs.signInWith).toBe("window");
  });

  it("changing the setting saves it", async () => {
    invoke.mockResolvedValue({ sign_in_with: "window" });
    await setSignInWith("window");
    expect(invoke).toHaveBeenCalledWith("set_sign_in_with", { with: "window" });
    expect(prefs.signInWith).toBe("window");
  });

  it("follows a login from start to done, failed or canceled", async () => {
    invoke.mockResolvedValue(null);
    await signInInApp("boss", "b1");
    expect(invoke).toHaveBeenCalledWith("sign_in_in_app", { name: "boss" });
    expect(logins.b1?.state).toBe("waiting");
    onSignInUpdate({ id: "b1", name: "boss", state: "failed", reason: "This sign-in needs your browser." });
    expect(logins.b1).toEqual({ state: "failed", reason: "This sign-in needs your browser." });
    onSignInUpdate({ id: "b1", name: "boss", state: "waiting", reason: null });
    onSignInUpdate({ id: "b1", name: "boss", state: "done", reason: null });
    expect(logins.b1).toBeUndefined();
    onSignInUpdate({ id: "b1", name: "boss", state: "waiting", reason: null });
    onSignInUpdate({ id: "b1", name: "boss", state: "canceled", reason: null });
    expect(logins.b1).toBeUndefined();
  });

  it("a login that can't start says why", async () => {
    invoke.mockRejectedValue("Codex isn't installed on this computer.");
    await signInInApp("boss", "b1");
    expect(logins.b1).toEqual({ state: "failed", reason: "Codex isn't installed on this computer." });
  });
});
