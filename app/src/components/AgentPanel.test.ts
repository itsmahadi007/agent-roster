import { describe, it, expect, vi } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import AgentPanel from "./AgentPanel.svelte";
import overview from "../lib/fixtures/overview.json";
import type { AgentView, OverviewView } from "../lib/types";
import { invoke } from "../test/setup";
import { logins, prefs, resetSignIn } from "../lib/signin.svelte";
import { beforeEach } from "vitest";

const view = overview as OverviewView;
const agent = (name: string, over: Partial<AgentView> = {}) => ({ ...view.agents.find((a) => a.name === name)!, ...over });
const plan = (text: string) => ({ token: 9, summary: "s", steps: [{ kind: "change", text }] });

function mount(a: AgentView, nudge = 0) {
  const onApplied = vi.fn();
  const onDirty = vi.fn();
  const onClose = vi.fn();
  const r = render(AgentPanel, { agent: a, palette: view.palette, terminals: [], nudge, onApplied, onDirty, onClose });
  return { onApplied, onDirty, onClose, rerender: r.rerender };
}
function answer(text: string) {
  invoke.mockImplementation(async (cmd: string) => (cmd === "preview" ? plan(text) : cmd === "apply" ? 4 : true));
}
const bar = () => within(screen.getByRole("region", { name: "Change waiting" }));

describe("AgentPanel", () => {
  beforeEach(() => resetSignIn());

  it("Sign in with: your browser by default, the Agent Roster window one click away", async () => {
    invoke.mockResolvedValue({ sign_in_with: "window" });
    mount(agent("agent1"));
    const group = screen.getByRole("radiogroup", { name: "Sign in with" });
    expect(within(group).getByRole("radio", { name: "Your browser" })).toHaveAttribute("aria-checked", "true");
    await userEvent.click(within(group).getByRole("radio", { name: "Agent Roster window" }));
    expect(invoke).toHaveBeenCalledWith("set_sign_in_with", { with: "window" });
    expect(within(group).getByRole("radio", { name: "Agent Roster window" })).toHaveAttribute("aria-checked", "true");
  });

  it("with the window setting, Sign in opens the Agent Roster window, not a terminal", async () => {
    prefs.signInWith = "window";
    invoke.mockResolvedValue(null);
    mount(agent("agent2"));
    await userEvent.click(screen.getByRole("button", { name: "Sign in" }));
    expect(invoke).toHaveBeenCalledWith("sign_in_in_app", { name: "agent2" });
    expect(invoke).not.toHaveBeenCalledWith("sign_in", expect.anything());
    expect(await screen.findByText("Signing in… (finish in the Agent Roster window)")).toBeInTheDocument();
  });

  it("a failed sign-in says why and offers the browser", async () => {
    const a = agent("agent2");
    logins[a.id] = { state: "failed", reason: "This sign-in needs your browser." };
    invoke.mockResolvedValue(true);
    mount(a);
    expect(screen.getByRole("alert")).toHaveTextContent("This sign-in needs your browser.");
    await userEvent.click(screen.getByRole("button", { name: "Sign in with your browser" }));
    expect(invoke).toHaveBeenCalledWith("sign_in", { name: "agent2", app: null });
    await waitFor(() => expect(screen.queryByRole("alert")).not.toBeInTheDocument());
  });

  it("a sign-in window that can't start says why", async () => {
    prefs.signInWith = "window";
    invoke.mockRejectedValue("Codex isn't installed on this computer.");
    mount(agent("agent2"));
    await userEvent.click(screen.getByRole("button", { name: "Sign in" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Codex isn't installed on this computer.");
    expect(screen.getByRole("button", { name: "Sign in with your browser" })).toBeInTheDocument();
  });

  it("sections run This week, Use it, Settings, then Remove alone at the bottom", () => {
    mount(agent("agent1"));
    const heads = screen.getAllByRole("heading", { level: 4 }).map((h) => h.textContent);
    expect(heads).toEqual(["This week", "Use it", "Settings"]);
    const remove = screen.getByRole("button", { name: "Remove agent1" });
    const settings = screen.getByRole("region", { name: "Settings" });
    expect(settings.compareDocumentPosition(remove) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(settings).not.toContainElement(remove);
  });

  it("this week: a big bar, used and left, the reset and when it was checked", () => {
    const { container } = render(AgentPanel, {
      agent: agent("agent1"),
      palette: view.palette,
      terminals: [],
      updated: "Last update 4m ago.",
      onApplied: vi.fn(),
      onDirty: vi.fn(),
      onClose: vi.fn(),
    });
    const week = within(screen.getByRole("region", { name: "This week" }));
    expect(week.getByText("61% of the week used")).toBeInTheDocument();
    expect(week.getByText("39% left")).toBeInTheDocument();
    expect(week.getByText("Resets in 3d 16h · checked 4m ago")).toBeInTheDocument();
    expect(container.querySelector(".track.big i")).toHaveStyle({ width: "61%" });
  });

  it("shows how to start the agent", async () => {
    const user = userEvent.setup();
    mount(agent("agent1"));
    expect(screen.getByText("you@example.com")).toBeInTheDocument();
    expect(screen.getByText(/in any terminal/)).toHaveTextContent("Or type agent1 in any terminal");
    await user.click(screen.getByRole("button", { name: "Copy command" }));
    expect(await navigator.clipboard.readText()).toBe("agent1");
    expect(screen.queryByText("5-hour use")).not.toBeInTheDocument();
  });

  it("with terminals found: Open in terminal and Copy command side by side", async () => {
    const user = userEvent.setup();
    render(AgentPanel, {
      agent: agent("agent1"),
      palette: view.palette,
      terminals: [{ id: "wt", label: "Windows Terminal", runs: true }],
      onApplied: vi.fn(),
      onDirty: vi.fn(),
      onClose: vi.fn(),
    });
    const use = within(screen.getByRole("region", { name: "Use it" }));
    expect(use.getByRole("button", { name: /Open in/ })).toBeInTheDocument();
    await user.click(use.getByRole("button", { name: "Copy command" }));
    expect(await navigator.clipboard.readText()).toBe("agent1");
    expect(use.getByRole("button", { name: "Copied" })).toBeInTheDocument();
  });

  it("one sign-in place: Sign in when signed out, Switch account when signed in, never Sign in again", () => {
    mount(agent("agent2"));
    expect(screen.getAllByRole("button", { name: "Sign in" })).toHaveLength(1);
    expect(screen.queryByRole("button", { name: "Switch account" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Sign in again" })).not.toBeInTheDocument();
  });

  it("the picker shows the new colors, and an old coral agent shows as pink", () => {
    mount(agent("boss", { color: "#ff7a6b", color_label: "coral" }));
    const group = within(screen.getByRole("radiogroup", { name: "Color" }));
    expect(group.getAllByRole("radio").map((r) => r.getAttribute("aria-label"))).toEqual(["cobalt", "cyan", "jade", "violet", "pink"]);
    expect(group.getByRole("radio", { name: "pink" })).toHaveAttribute("aria-checked", "true");
    expect(group.queryByRole("radio", { name: "coral" })).not.toBeInTheDocument();
    expect(group.queryByRole("radio", { name: "marigold" })).not.toBeInTheDocument();
    expect(screen.getByLabelText("Name")).toHaveStyle({ color: "#ff6bc4" });
  });

  it("an agent without numbers says how to get them", () => {
    mount(agent("boss", { windows: [], reset_text: null, limits_note: "usage shows after you use boss" }));
    expect(
      screen.getByText("Weekly use shows after you use boss. Type boss in a terminal and send one message."),
    ).toBeInTheDocument();
  });

  it("a signed-out agent has a Sign in button that opens its login", async () => {
    invoke.mockResolvedValue(true);
    mount(agent("agent2"));
    expect(screen.getByText(/Weekly use shows after sign in/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Sign in" }));
    expect(invoke).toHaveBeenCalledWith("sign_in", { name: "agent2", app: null });
  });

  it("an expired login the check found has a Sign in button, though the tool says ready", async () => {
    invoke.mockResolvedValue(true);
    mount(agent("boss", { state: "ready", health: "sign-in", limits_note: "sign in" }));
    await userEvent.click(screen.getByRole("button", { name: "Sign in" }));
    expect(invoke).toHaveBeenCalledWith("sign_in", { name: "boss", app: null });
  });

  it("a used-up limit says which one and when it resets", () => {
    mount(
      agent("boss", {
        health: "limit-reached",
        reset_text: "resets in 5d 2h",
        limits_note: "5-hour limit reached, resets in 2h 0m",
        limit_text: "boss has used its 5-hour limit; it resets in 2h 0m.",
      }),
    );
    expect(screen.getByText("boss has used its 5-hour limit; it resets in 2h 0m.")).toBeInTheDocument();
    expect(screen.queryByText(/weekly limit/)).not.toBeInTheDocument();
  });

  it("says where usage comes from, with no Show usage button", () => {
    mount(agent("agent2", { state: "ready", health: "ok", claude_limits: false }));
    expect(screen.getByText(/asks Claude Code itself/)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Show usage" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Hide usage" })).not.toBeInTheDocument();
  });

  it("Hide usage shows only while the status line is on", () => {
    mount(agent("agent1"));
    expect(screen.getByRole("button", { name: "Hide usage" })).toBeInTheDocument();
  });

  it("checking, no subscription and unreachable each say so", async () => {
    const none = { windows: [], reset_text: null, state: "ready" as const };
    const { rerender } = mount(agent("agent2", { ...none, health: "checking", limits_note: "checking" }));
    expect(screen.getByText(/Checking with Claude Code/)).toBeInTheDocument();
    expect(screen.queryByText(/send one message/)).not.toBeInTheDocument();
    await rerender({ agent: agent("agent2", { ...none, health: "no-subscription", limits_note: "no subscription" }) });
    expect(screen.getByText(/no Claude Code subscription/)).toBeInTheDocument();
    await rerender({ agent: agent("agent1", { health: "unreachable", as_of: "as of 3:40 pm" }) });
    expect(screen.getByText(/Couldn't reach Claude Code just now/)).toHaveTextContent("as of 3:40 pm");
  });

  it("Escape cancels a waiting change first, then closes", async () => {
    answer("Renames.");
    const { onClose, onDirty } = mount(agent("agent1"));
    const input = screen.getByLabelText("Name");
    await userEvent.clear(input);
    await userEvent.type(input, "lead");
    await waitFor(() => expect(onDirty).toHaveBeenLastCalledWith(true));
    await userEvent.keyboard("{Escape}");
    expect(screen.getByLabelText("Name")).toHaveValue("agent1");
    expect(onClose).not.toHaveBeenCalled();
    await waitFor(() => expect(onDirty).toHaveBeenLastCalledWith(false));
    await userEvent.keyboard("{Escape}");
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("says to apply or cancel when nudged", async () => {
    answer("Renames.");
    const { rerender } = mount(agent("agent1"));
    const input = screen.getByLabelText("Name");
    await userEvent.clear(input);
    await userEvent.type(input, "lead");
    await bar().findByText("Renames.");
    expect(screen.queryByText("Apply or cancel this change first.")).not.toBeInTheDocument();
    await rerender({ nudge: 1 });
    expect(await bar().findByText("Apply or cancel this change first.")).toBeInTheDocument();
  });

  it("renames after showing what will change", async () => {
    answer("Renames the agent1 command to lead.");
    const { onApplied } = mount(agent("agent1"));
    const input = screen.getByLabelText("Name");
    await userEvent.clear(input);
    await userEvent.type(input, "lead");
    expect(await bar().findByText("Renames the agent1 command to lead.")).toBeInTheDocument();
    await userEvent.click(bar().getByRole("button", { name: "Rename" }));
    await waitFor(() =>
      expect(onApplied).toHaveBeenCalledWith("agent1 is now lead. Type lead in any terminal to start it.", "rename"),
    );
  });

  it("a ready-made role is one click, then Save role", async () => {
    answer("Tells your agents that agent1 does: Leader.");
    mount(agent("agent1"));
    await userEvent.click(screen.getByRole("button", { name: "Leader" }));
    expect(invoke).toHaveBeenLastCalledWith("preview", { req: { op: "role", name: "agent1", role: "Leader" } });
    expect(await bar().findByRole("button", { name: "Save role" })).toBeEnabled();
  });

  it("a typed role matching a preset lights that button", () => {
    mount(agent("agent1", { role: "leader" }));
    expect(screen.getByRole("button", { name: "Leader" })).toHaveAttribute("aria-pressed", "true");
  });

  it("good at: remove a tag, add one, or reset", async () => {
    answer("Tells your agents.");
    mount(agent("agent1", { good_at: ["coding", "planning"], good_at_custom: true }));
    await userEvent.click(screen.getByRole("button", { name: "Remove planning" }));
    expect(invoke).toHaveBeenLastCalledWith("preview", { req: { op: "good-at", name: "agent1", items: ["coding"] } });
    await userEvent.type(screen.getByLabelText("Add what it's good at"), "tests{Enter}");
    expect(invoke).toHaveBeenLastCalledWith("preview", {
      req: { op: "good-at", name: "agent1", items: ["coding", "tests"] },
    });
    await userEvent.click(screen.getByRole("button", { name: "Reset to defaults" }));
    expect(invoke).toHaveBeenLastCalledWith("preview", { req: { op: "good-at", name: "agent1", items: null } });
  });

  it("changes color, sharing, default; remove keeps the login folder", async () => {
    answer("Done.");
    mount(agent("boss"));
    await userEvent.click(screen.getByRole("radio", { name: "pink" }));
    expect(invoke).toHaveBeenLastCalledWith("preview", { req: { op: "color", name: "boss", color: "coral" } });
    await userEvent.click(screen.getByRole("switch", { name: "Share my skills" }));
    expect(invoke).toHaveBeenLastCalledWith("preview", { req: { op: "share", name: "boss", on: true } });
    await userEvent.click(screen.getByRole("button", { name: "Make default for codex" }));
    expect(invoke).toHaveBeenLastCalledWith("preview", { req: { op: "default", name: "boss" } });
    expect(screen.getByText(/Its login folder stays on this computer/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Remove boss" }));
    expect(invoke).toHaveBeenLastCalledWith("preview", { req: { op: "remove", name: "boss" } });
    await userEvent.click(bar().getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("region", { name: "Change waiting" })).not.toBeInTheDocument();
  });

  it("shows why a change can't be made", async () => {
    invoke.mockImplementation(async (cmd: string) => {
      if (cmd === "preview") throw "agent3 is already an agent.";
    });
    mount(agent("agent1"));
    const input = screen.getByLabelText("Name");
    await userEvent.clear(input);
    await userEvent.type(input, "agent3");
    expect(await screen.findByText("agent3 is already an agent.")).toBeInTheDocument();
  });

  it("switch account uses the same terminal choice", async () => {
    invoke.mockResolvedValue(true);
    mount(agent("agent1"));
    await userEvent.click(screen.getByRole("button", { name: "Switch account" }));
    expect(invoke).toHaveBeenCalledWith("sign_in", { name: "agent1", app: null });
  });

  it("an apply that fails says why and stays visible", async () => {
    invoke.mockImplementation(async (cmd: string) => {
      if (cmd === "preview") return plan("Changes the color.");
      if (cmd === "apply") throw "That preview has expired. Look at it again.";
    });
    mount(agent("boss"));
    await userEvent.click(screen.getByRole("radio", { name: "pink" }));
    await userEvent.click(await bar().findByRole("button", { name: "Change color" }));
    expect(await screen.findByText("That preview has expired. Look at it again.")).toBeInTheDocument();
    await new Promise((r) => setTimeout(r, 50));
    expect(screen.getByText("That preview has expired. Look at it again.")).toBeInTheDocument();
  });

  it("after a change is applied, Apply stays off until the page reloads", async () => {
    answer("Changes the color.");
    mount(agent("boss"));
    await userEvent.click(screen.getByRole("radio", { name: "pink" }));
    const btn = await bar().findByRole("button", { name: "Change color" });
    await userEvent.click(btn);
    await waitFor(() => expect(btn).toBeDisabled());
  });

  it("the regular refresh doesn't restart a waiting preview", async () => {
    answer("Renames.");
    const { rerender } = mount(agent("agent1"));
    const input = screen.getByLabelText("Name");
    await userEvent.clear(input);
    await userEvent.type(input, "lead");
    await bar().findByText("Renames.");
    const before = invoke.mock.calls.filter((c) => c[0] === "preview").length;
    await rerender({ agent: { ...agent("agent1") } });
    await new Promise((r) => setTimeout(r, 400));
    expect(invoke.mock.calls.filter((c) => c[0] === "preview").length).toBe(before);
    expect(bar().getByText("Renames.")).toBeInTheDocument();
  });
});
