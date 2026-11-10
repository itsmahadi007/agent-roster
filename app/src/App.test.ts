import { describe, it, expect } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import App from "./App.svelte";
import overview from "./lib/fixtures/overview.json";
import type { OverviewView } from "./lib/types";
import { invoke } from "./test/setup";

function withAgent2Ready(): OverviewView {
  const v = structuredClone(overview) as OverviewView;
  const a2 = v.agents[1];
  Object.assign(a2, { state: "ready", status_text: "ready", limits_note: "usage not shown", email: "you@example.com" });
  return v;
}

describe("App", () => {
  it("turn_on_limits_opens_plan_sheet", async () => {
    const v = withAgent2Ready();
    invoke.mockImplementation(async (cmd: string) => {
      if (cmd === "overview") return v;
      if (cmd === "preview")
        return { token: 5, summary: "Turned on Claude limits for agent2", steps: [{ kind: "change", text: "Routes the status line." }] };
      if (cmd === "apply") return 6;
      throw new Error(cmd);
    });
    render(App);
    await userEvent.click(await screen.findByRole("button", { name: "Show usage" }));
    expect(invoke).toHaveBeenCalledWith("preview", { req: { op: "enable-limits", name: "agent2" } });
    expect(await screen.findByText("Routes the status line.")).toBeInTheDocument();
    await userEvent.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Show usage" }));
    expect(invoke).toHaveBeenCalledWith("apply", { token: 5 });
    await waitFor(() => expect(screen.queryByText("Routes the status line.")).not.toBeInTheDocument());
  });

  it("turn off limits previews disable-limits", async () => {
    invoke.mockImplementation(async (cmd: string) => {
      if (cmd === "overview") return overview;
      if (cmd === "preview") return { token: 8, summary: "s", steps: [{ kind: "change", text: "Puts back your status line." }] };
      throw new Error(cmd);
    });
    render(App);
    await userEvent.click(await screen.findByRole("button", { name: "Hide usage" }));
    expect(invoke).toHaveBeenCalledWith("preview", { req: { op: "disable-limits", name: "agent1" } });
    expect(within(await screen.findByRole("dialog")).getByRole("button", { name: "Hide usage" })).toBeInTheDocument();
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
  it("renaming from a row refreshes and says how to start it", async () => {
    invoke.mockImplementation(async (cmd: string) => {
      if (cmd === "overview") return overview;
      if (cmd === "preview") return { token: 2, summary: "Renamed agent1 to lead", steps: [{ kind: "change", text: "Renames the agent1 command to lead." }] };
      if (cmd === "apply") return 7;
      throw new Error(cmd);
    });
    render(App);
    await userEvent.click(await screen.findByRole("button", { name: "Edit agent1" }));
    const input = screen.getByLabelText("Name");
    await userEvent.clear(input);
    await userEvent.type(input, "lead");
    await screen.findByText("Renames the agent1 command to lead.");
    await userEvent.click(screen.getByRole("button", { name: "Rename" }));
    expect(await screen.findByText("agent1 is now lead. Type lead in any terminal to start it.")).toBeInTheDocument();
  });

  it("removing an agent closes its settings", async () => {
    invoke.mockImplementation(async (cmd: string) => {
      if (cmd === "overview") return overview;
      if (cmd === "preview") return { token: 3, summary: "Removed boss", steps: [{ kind: "remove", text: "Removes the boss command." }] };
      if (cmd === "apply") return 8;
      throw new Error(cmd);
    });
    render(App);
    await userEvent.click(await screen.findByRole("button", { name: "Edit boss" }));
    await userEvent.click(screen.getByRole("button", { name: "Remove boss" }));
    await userEvent.click(await screen.findByRole("button", { name: "Remove" }));
    expect(await screen.findByText(/boss was removed/)).toBeInTheDocument();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("how it works explains the app and goes back", async () => {
    invoke.mockImplementation(async (cmd: string) => (cmd === "overview" ? overview : null));
    render(App);
    await userEvent.click(await screen.findByRole("button", { name: "How it works" }));
    expect(await screen.findByRole("heading", { name: "How Agent Roster works" })).toBeInTheDocument();
    expect(screen.getByText(/Type its name in any terminal/)).toBeInTheDocument();
    expect(screen.getByText(/Click an agent's name to open its settings/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Back to your agents" }));
    expect(await screen.findByRole("heading", { name: "Your agents" })).toBeInTheDocument();
  });
});
