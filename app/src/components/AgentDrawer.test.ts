import { describe, it, expect, vi } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import AgentDrawer from "./AgentDrawer.svelte";
import overview from "../lib/fixtures/overview.json";
import type { AgentView, OverviewView } from "../lib/types";
import { invoke } from "../test/setup";

const view = overview as OverviewView;
const agent = (name: string, over: Partial<AgentView> = {}) => ({ ...view.agents.find((a) => a.name === name)!, ...over });
const plan = (text: string) => ({ token: 9, summary: "s", steps: [{ kind: "change", text }] });

function mount(a: AgentView, nudge = 0) {
  const onApplied = vi.fn();
  const onDirty = vi.fn();
  const onClose = vi.fn();
  const r = render(AgentDrawer, { agent: a, palette: view.palette, terminals: [], nudge, onApplied, onDirty, onClose });
  return { onApplied, onDirty, onClose, rerender: r.rerender };
}
function answer(text: string) {
  invoke.mockImplementation(async (cmd: string) => (cmd === "preview" ? plan(text) : cmd === "apply" ? 4 : true));
}
const bar = () => within(screen.getByRole("region", { name: "Change waiting" }));

describe("AgentDrawer", () => {
  it("shows how to start the agent, without a name header or usage cards", async () => {
    const user = userEvent.setup();
    mount(agent("agent1"));
    expect(screen.queryByRole("heading", { name: "agent1" })).not.toBeInTheDocument();
    expect(screen.getByText("you@example.com")).toBeInTheDocument();
    expect(screen.getByText(/in any terminal/)).toHaveTextContent("Or type agent1 in any terminal");
    await user.click(screen.getByRole("button", { name: "Copy" }));
    expect(await navigator.clipboard.readText()).toBe("agent1");
    expect(screen.queryByText("5-hour use")).not.toBeInTheDocument();
    expect(screen.queryByText("Weekly use")).not.toBeInTheDocument();
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
    await userEvent.click(screen.getByRole("radio", { name: "coral" }));
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

  it("sign in again uses the same terminal choice", async () => {
    invoke.mockResolvedValue(true);
    mount(agent("agent1"));
    await userEvent.click(screen.getByRole("button", { name: "Sign in again" }));
    expect(invoke).toHaveBeenCalledWith("sign_in", { name: "agent1", app: null });
  });

  it("an apply that fails says why and stays visible", async () => {
    invoke.mockImplementation(async (cmd: string) => {
      if (cmd === "preview") return plan("Changes the color.");
      if (cmd === "apply") throw "That preview has expired. Look at it again.";
    });
    mount(agent("boss"));
    await userEvent.click(screen.getByRole("radio", { name: "coral" }));
    await userEvent.click(await bar().findByRole("button", { name: "Change color" }));
    expect(await screen.findByText("That preview has expired. Look at it again.")).toBeInTheDocument();
    await new Promise((r) => setTimeout(r, 50));
    expect(screen.getByText("That preview has expired. Look at it again.")).toBeInTheDocument();
  });

  it("after a change is applied, Apply stays off until the page reloads", async () => {
    answer("Changes the color.");
    mount(agent("boss"));
    await userEvent.click(screen.getByRole("radio", { name: "coral" }));
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
