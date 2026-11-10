import { describe, it, expect, vi } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import AgentPage from "./AgentPage.svelte";
import overview from "../lib/fixtures/overview.json";
import type { AgentView, OverviewView } from "../lib/types";
import { invoke } from "../test/setup";

const view = overview as OverviewView;
const agent = (name: string, over: Partial<AgentView> = {}) => ({ ...view.agents.find((a) => a.name === name)!, ...over });
const plan = (text: string) => ({ token: 9, summary: "s", steps: [{ kind: "change", text }] });

function mount(a: AgentView) {
  const onApplied = vi.fn();
  render(AgentPage, { agent: a, palette: view.palette, terminals: [], onApplied });
  return { onApplied };
}
function answer(text: string) {
  invoke.mockImplementation(async (cmd: string) => (cmd === "preview" ? plan(text) : cmd === "apply" ? 4 : true));
}
const bar = () => within(screen.getByRole("region", { name: "Change waiting" }));

describe("AgentPage", () => {
  it("shows the agent, how to start it, and its usage", async () => {
    const user = userEvent.setup();
    mount(agent("agent1"));
    expect(screen.getByRole("heading", { name: "agent1" })).toBeInTheDocument();
    expect(screen.getByText(/in any terminal/)).toHaveTextContent("Or type agent1 in any terminal");
    await user.click(screen.getByRole("button", { name: "Copy" }));
    expect(await navigator.clipboard.readText()).toBe("agent1");
    expect(screen.getByText("61%")).toBeInTheDocument();
    expect(screen.getByText("week resets in 3d 16h")).toBeInTheDocument();
  });

  it("an agent without numbers says how to get them", () => {
    mount(agent("boss", { windows: [], reset_text: null, limits_note: "usage shows after you use boss" }));
    expect(screen.getAllByText("Shows after you use boss").length).toBe(2);
    expect(screen.getByText("Type boss in a terminal and send one message")).toBeInTheDocument();
  });

  it("Show usage waits in the save bar", async () => {
    answer("Changes the status line.");
    const { onApplied } = mount(agent("agent2", { state: "ready", claude_limits: false, limits_note: "usage not shown" }));
    await userEvent.click(screen.getByRole("button", { name: "Show usage" }));
    expect(invoke).toHaveBeenCalledWith("preview", { req: { op: "enable-limits", name: "agent2" } });
    await userEvent.click(await bar().findByRole("button", { name: "Show usage" }));
    await waitFor(() => expect(onApplied).toHaveBeenCalledWith(expect.stringContaining("agent2"), "enable-limits"));
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
});
