import { describe, it, expect } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import App from "./App.svelte";
import overview from "./lib/fixtures/overview.json";
import type { OverviewView } from "./lib/types";
import { invoke, listen } from "./test/setup";

function withAgent2Ready(): OverviewView {
  const v = structuredClone(overview) as OverviewView;
  const a2 = v.agents[1];
  Object.assign(a2, { state: "ready", status_text: "ready", limits_note: "usage not shown", email: "you@example.com" });
  return v;
}

describe("App", () => {
  it("Refresh asks every tool for usage and shows the answer", async () => {
    const fresh = withAgent2Ready();
    Object.assign(fresh.agents[1], { health: "no-subscription", limits_note: "no subscription" });
    invoke.mockImplementation(async (cmd: string) => {
      if (cmd === "overview") return overview;
      if (cmd === "refresh_usage") return fresh;
      throw new Error(cmd);
    });
    render(App);
    await userEvent.click(await screen.findByRole("button", { name: "Refresh" }));
    expect(invoke).toHaveBeenCalledWith("refresh_usage");
    expect(await screen.findByText("no subscription")).toBeInTheDocument();
  });

  it("the 10-second poll only reads the overview, and a finished check updates the list", async () => {
    invoke.mockImplementation(async (cmd: string) => (cmd === "overview" ? overview : null));
    render(App);
    await screen.findByRole("button", { name: /^agent2/ });
    expect(invoke.mock.calls.map((c) => c[0])).not.toContain("refresh_usage");
    expect(listen).toHaveBeenCalledWith("usage-checked", expect.any(Function));
    const onChecked = listen.mock.calls.find((c) => c[0] === "usage-checked")![1] as () => void;
    const before = invoke.mock.calls.length;
    onChecked();
    await waitFor(() => expect(invoke.mock.calls.length).toBeGreaterThan(before));
    expect(invoke.mock.calls.at(-1)![0]).toBe("overview");
  });

  it("hears sign-in progress and refreshes when one is done", async () => {
    invoke.mockImplementation(async (cmd: string) => (cmd === "overview" ? overview : cmd === "settings" ? { sign_in_with: "window" } : null));
    render(App);
    await screen.findByRole("button", { name: /^agent2/ });
    expect(invoke).toHaveBeenCalledWith("settings");
    const { prefs } = await import("./lib/signin.svelte");
    await waitFor(() => expect(prefs.signInWith).toBe("window"));
    const onSignIn = listen.mock.calls.find((c) => c[0] === "sign-in")![1] as (e: { payload: unknown }) => void;
    const before = invoke.mock.calls.length;
    onSignIn({ payload: { id: "a2", name: "agent2", state: "done", reason: null } });
    await waitFor(() => expect(invoke.mock.calls.length).toBeGreaterThan(before));
    expect(invoke.mock.calls.at(-1)![0]).toBe("overview");
    prefs.signInWith = "browser";
  });

  it("Hide usage previews disable-limits", async () => {
    invoke.mockImplementation(async (cmd: string) => {
      if (cmd === "overview") return overview;
      if (cmd === "preview") return { token: 8, summary: "s", steps: [{ kind: "change", text: "Puts back your status line." }] };
      throw new Error(cmd);
    });
    render(App);
    await userEvent.click(await screen.findByRole("button", { name: /^agent1/ }));
    await userEvent.click(await screen.findByRole("button", { name: "Hide usage" }));
    expect(invoke).toHaveBeenCalledWith("preview", { req: { op: "disable-limits", name: "agent1" } });
    expect(within(await screen.findByRole("region", { name: "Change waiting" })).getByRole("button", { name: "Hide usage" })).toBeInTheDocument();
  });

  it("add agent button opens the add screen", async () => {
    invoke.mockImplementation(async (cmd: string) => (cmd === "overview" ? overview : null));
    render(App);
    await userEvent.click(await screen.findByRole("button", { name: "Add agent" }));
    expect(screen.getByLabelText("Name")).toBeInTheDocument();
  });
});

describe("App first run", () => {
  it("empty roster shows first run", async () => {
    const empty = { ...(overview as OverviewView), agents: [], calls: [], warnings: [] };
    invoke.mockImplementation(async (cmd: string) => {
      if (cmd === "overview") return empty;
      if (cmd === "first_run") return { candidates: [], prototype: false };
      throw new Error(cmd);
    });
    render(App);
    expect(await screen.findByText("No Claude Code or Codex login was found")).toBeInTheDocument();
  });
});

describe("App history", () => {
  it("what changed opens the history and back returns", async () => {
    invoke.mockImplementation(async (cmd: string) => (cmd === "overview" ? overview : cmd === "changes" ? [] : null));
    render(App);
    await userEvent.click(await screen.findByRole("button", { name: "What changed" }));
    expect(await screen.findByRole("heading", { name: "Everything Agent Roster did" })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Back to your agents" }));
    expect(await screen.findByRole("heading", { name: "Your agents" })).toBeInTheDocument();
  });
});

describe("App rename and help", () => {
  it("renaming on the agent page says how to start it", async () => {
    invoke.mockImplementation(async (cmd: string) => {
      if (cmd === "overview") return overview;
      if (cmd === "preview") return { token: 2, summary: "Renamed agent1 to lead", steps: [{ kind: "change", text: "Renames the agent1 command to lead." }] };
      if (cmd === "apply") return 7;
      throw new Error(cmd);
    });
    render(App);
    await userEvent.click(await screen.findByRole("button", { name: /^agent1/ }));
    const input = screen.getByLabelText("Name");
    await userEvent.clear(input);
    await userEvent.type(input, "lead");
    const bar = within(await screen.findByRole("region", { name: "Change waiting" }));
    await bar.findByText("Renames the agent1 command to lead.");
    await userEvent.click(bar.getByRole("button", { name: "Rename" }));
    expect(await screen.findByText("agent1 is now lead. Type lead in any terminal to start it.")).toBeInTheDocument();
  });

  it("removing the open agent closes the drawer", async () => {
    invoke.mockImplementation(async (cmd: string) => {
      if (cmd === "overview") return overview;
      if (cmd === "preview") return { token: 3, summary: "Removed boss", steps: [{ kind: "remove", text: "Removes the boss command." }] };
      if (cmd === "apply") return 8;
      throw new Error(cmd);
    });
    render(App);
    await userEvent.click(await screen.findByRole("button", { name: /^boss/ }));
    await userEvent.click(screen.getByRole("button", { name: "Remove boss" }));
    const bar = within(await screen.findByRole("region", { name: "Change waiting" }));
    await userEvent.click(await bar.findByRole("button", { name: "Remove" }));
    expect(await screen.findByText(/boss was removed/)).toBeInTheDocument();
    await waitFor(() => expect(screen.queryAllByRole("button", { expanded: true })).toHaveLength(0));
    expect(screen.queryByLabelText("Name")).not.toBeInTheDocument();
  });

  it("how it works explains the app and goes back", async () => {
    invoke.mockImplementation(async (cmd: string) => (cmd === "overview" ? overview : null));
    render(App);
    await userEvent.click(await screen.findByRole("button", { name: "How it works" }));
    expect(await screen.findByRole("heading", { name: "How Agent Roster works" })).toBeInTheDocument();
    expect(screen.getByText(/Type its name in any terminal/)).toBeInTheDocument();
    expect(screen.getByText(/Click an agent's row to open it in place/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Back to your agents" }));
    expect(await screen.findByRole("heading", { name: "Your agents" })).toBeInTheDocument();
  });
});

describe("App rows", () => {
  const renames = () =>
    invoke.mockImplementation(async (cmd: string) => {
      if (cmd === "overview") return structuredClone(overview);
      if (cmd === "preview") return { token: 2, summary: "s", steps: [{ kind: "change", text: "Renames." }] };
      return null;
    });

  it("nothing is open at first; clicking a row opens it, clicking again closes it", async () => {
    renames();
    render(App);
    const a1 = await screen.findByRole("button", { name: /^agent1/ });
    expect(screen.queryByLabelText("Name")).not.toBeInTheDocument();
    await userEvent.click(a1);
    expect(a1).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByLabelText("Name")).toHaveValue("agent1");
    await userEvent.click(a1);
    expect(a1).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByLabelText("Name")).not.toBeInTheDocument();
  });

  it("another row can't open while a change waits", async () => {
    renames();
    render(App);
    await userEvent.click(await screen.findByRole("button", { name: /^agent1/ }));
    const input = screen.getByLabelText("Name");
    await userEvent.clear(input);
    await userEvent.type(input, "lead");
    await within(await screen.findByRole("region", { name: "Change waiting" })).findByText("Renames.");
    await userEvent.click(screen.getByRole("button", { name: /^agent3/ }));
    expect(screen.getByRole("button", { name: /^agent3/ })).toHaveAttribute("aria-expanded", "false");
    expect(screen.getByLabelText("Name")).toHaveValue("lead");
    expect(await screen.findByText("Apply or cancel this change first.")).toBeInTheDocument();
  });

  it("open row and its edits survive a refresh", async () => {
    renames();
    render(App);
    await userEvent.click(await screen.findByRole("button", { name: /^agent1/ }));
    const input = screen.getByLabelText("Name");
    await userEvent.clear(input);
    await userEvent.type(input, "lead");
    await userEvent.click(screen.getByRole("button", { name: "Refresh" }));
    expect(await screen.findByText("Checked just now")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /^agent1/ })).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByLabelText("Name")).toHaveValue("lead");
  });

  it("leaving for another screen while a change waits is refused", async () => {
    renames();
    render(App);
    await userEvent.click(await screen.findByRole("button", { name: /^agent1/ }));
    const input = screen.getByLabelText("Name");
    await userEvent.clear(input);
    await userEvent.type(input, "lead");
    await within(await screen.findByRole("region", { name: "Change waiting" })).findByText("Renames.");
    await userEvent.click(screen.getByRole("button", { name: "How it works" }));
    expect(screen.queryByRole("heading", { name: "How Agent Roster works" })).not.toBeInTheDocument();
    expect(screen.getByLabelText("Name")).toHaveValue("lead");
    expect(await screen.findByText("Apply or cancel this change first.")).toBeInTheDocument();
  });

  it("an agent added outside the app doesn't take over the open row", async () => {
    const after = structuredClone(overview) as OverviewView;
    after.agents.push({ ...structuredClone(after.agents[3]), id: "a5", name: "helper" });
    let added = false;
    invoke.mockImplementation(async (cmd: string) => {
      if (cmd === "overview" || cmd === "refresh_usage") return structuredClone(added ? after : overview);
      if (cmd === "preview") return { token: 2, summary: "s", steps: [{ kind: "change", text: "Renames." }] };
      return null;
    });
    render(App);
    await userEvent.click(await screen.findByRole("button", { name: /^agent1/ }));
    const input = screen.getByLabelText("Name");
    await userEvent.clear(input);
    await userEvent.type(input, "lead");
    added = true;
    await userEvent.click(screen.getByRole("button", { name: "Refresh" }));
    await screen.findByRole("button", { name: /^helper/ });
    expect(screen.getByRole("button", { name: /^helper/ })).toHaveAttribute("aria-expanded", "false");
    expect(screen.getByRole("button", { name: /^agent1/ })).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByLabelText("Name")).toHaveValue("lead");
  });

  it("a new waiting change clears the last notice so it can't cover the save bar", async () => {
    invoke.mockImplementation(async (cmd: string) => {
      if (cmd === "overview") return structuredClone(overview);
      if (cmd === "preview") return { token: 2, summary: "s", steps: [{ kind: "change", text: "Changes it." }] };
      if (cmd === "apply") return 7;
      return null;
    });
    render(App);
    await userEvent.click(await screen.findByRole("button", { name: /^agent1/ }));
    await userEvent.click(screen.getByRole("radio", { name: "coral" }));
    const bar = within(await screen.findByRole("region", { name: "Change waiting" }));
    await userEvent.click(await bar.findByRole("button", { name: "Change color" }));
    expect(await screen.findByText("agent1 is coral now.")).toBeInTheDocument();
    await userEvent.click(await screen.findByRole("radio", { name: "jade" }));
    await screen.findByRole("region", { name: "Change waiting" });
    await waitFor(() => expect(screen.queryByText("agent1 is coral now.")).not.toBeInTheDocument());
  });

  it("add agent opens the new agent's row", async () => {
    const after = structuredClone(overview) as OverviewView;
    after.agents.push({ ...structuredClone(after.agents[3]), id: "a5", name: "helper" });
    let added = false;
    invoke.mockImplementation(async (cmd: string) => {
      if (cmd === "overview") return added ? after : overview;
      if (cmd === "preview") return { token: 4, summary: "Added helper", steps: [{ kind: "change", text: "Adds helper." }] };
      if (cmd === "apply") {
        added = true;
        return 9;
      }
      return null;
    });
    render(App);
    await userEvent.click(await screen.findByRole("button", { name: "Add agent" }));
    await userEvent.type(screen.getByLabelText("Name"), "helper");
    const go = screen.getByRole("button", { name: "Add and sign in" });
    await waitFor(() => expect(go).toBeEnabled());
    await userEvent.click(go);
    await waitFor(() => expect(screen.getByRole("button", { name: /^helper/ })).toHaveAttribute("aria-expanded", "true"));
  });
});
