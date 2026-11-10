import { describe, it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import AgentList from "./AgentList.svelte";
import overview from "../lib/fixtures/overview.json";
import type { OverviewView } from "../lib/types";

const view = overview as OverviewView;
const props = (over = {}) => ({
  view,
  open: null as string | null,
  palette: view.palette,
  terminals: [],
  round: 0,
  nudge: 0,
  onToggle: vi.fn(),
  onNavigate: vi.fn(),
  onRefresh: vi.fn(async () => {}),
  onSite: vi.fn(),
  onApplied: vi.fn(),
  onDirty: vi.fn(),
  onClose: vi.fn(),
  ...over,
});
const row = (name: RegExp) => screen.getByRole("button", { name });

describe("AgentList", () => {
  it("rows show weekly use and when it resets", () => {
    render(AgentList, props());
    expect(screen.getByRole("heading", { name: "Your agents" })).toBeInTheDocument();
    const a1 = row(/^agent1/);
    expect(a1).toHaveTextContent("Claude Code");
    expect(a1).toHaveTextContent("61% week");
    expect(a1).toHaveTextContent("39% remaining");
    expect(a1).toHaveTextContent("resets in 3d 16h");
    const a3 = row(/^agent3/);
    expect(a3).toHaveTextContent("93% week");
    expect(a3).toHaveTextContent("7% remaining");
    expect(a3).toHaveTextContent("resets in 21h 52m");
    expect(a3.querySelector(".pc")).toHaveClass("hot");
    expect(a3.querySelector(".rs")).toHaveClass("hot");
    expect(a1.querySelector(".rs")).not.toHaveClass("hot");
    expect(row(/^agent2/)).toHaveTextContent("sign in");
    for (const b of screen.getAllByRole("button", { expanded: false })) expect(b).not.toHaveTextContent("5 hr");
  });

  it("a checked row with no numbers says it hasn't been used yet", () => {
    const v = structuredClone(view);
    Object.assign(v.agents[3], { windows: [], reset_text: null, health: "ok", limits_note: "usage shows after you use boss" });
    render(AgentList, props({ view: v }));
    expect(row(/^boss/)).toHaveTextContent("not used yet");
  });

  it("rows say what the account needs, never 'not used yet' for it", () => {
    const v = structuredClone(view);
    const none = { windows: [], reset_text: null };
    Object.assign(v.agents[0], { ...none, health: "checking", limits_note: "checking" });
    Object.assign(v.agents[1], { ...none, health: "no-subscription", state: "ready", limits_note: "no subscription" });
    Object.assign(v.agents[2], { ...none, health: "not-installed", state: "not-installed", limits_note: "not installed" });
    render(AgentList, props({ view: v }));
    expect(row(/^agent1/)).toHaveTextContent("checking");
    expect(row(/^agent2/)).toHaveTextContent("no subscription");
    expect(row(/^agent3/)).toHaveTextContent("not installed");
    for (const name of [/^agent1/, /^agent2/, /^agent3/]) expect(row(name)).not.toHaveTextContent("not used yet");
  });

  it("a used-up week says limit reached in coral, with its reset", () => {
    const v = structuredClone(view);
    Object.assign(v.agents[3], {
      health: "limit-reached",
      windows: [{ label: "week", used_percent: 100, hot: true }],
      reset_text: "resets in 2d 3h",
      reset_hot: true,
      limits_note: "limit reached, resets in 2d 3h",
    });
    render(AgentList, props({ view: v }));
    const boss = row(/^boss/);
    expect(boss).toHaveTextContent("limit reached");
    expect(boss).toHaveTextContent("resets in 2d 3h");
    expect(boss.querySelector(".pc")).toHaveClass("hot");
  });

  it("numbers from an earlier check say when they're from", () => {
    const v = structuredClone(view);
    Object.assign(v.agents[0], { health: "unreachable", as_of: "as of 3:40 pm" });
    render(AgentList, props({ view: v }));
    expect(row(/^agent1/)).toHaveTextContent("61% week");
    expect(row(/^agent1/)).toHaveTextContent("as of 3:40 pm");
  });

  it("nothing is open at first; a click asks to open the row", async () => {
    const p = props();
    render(AgentList, p);
    expect(screen.queryAllByRole("button", { expanded: true })).toHaveLength(0);
    expect(screen.queryByLabelText("Name")).not.toBeInTheDocument();
    await userEvent.click(row(/^agent3/));
    expect(p.onToggle).toHaveBeenLastCalledWith("a3");
    row(/^agent1/).focus();
    await userEvent.keyboard("{Enter}");
    expect(p.onToggle).toHaveBeenLastCalledWith("a1");
  });

  it("the open row shows its settings under it", () => {
    render(AgentList, props({ open: "a1" }));
    expect(row(/^agent1/)).toHaveAttribute("aria-expanded", "true");
    expect(row(/^agent3/)).toHaveAttribute("aria-expanded", "false");
    expect(screen.getByLabelText("Name")).toHaveValue("agent1");
    expect(screen.getByRole("radiogroup", { name: "Color" })).toBeInTheDocument();
  });

  it("arrow keys move between rows without opening them", async () => {
    const p = props();
    render(AgentList, p);
    row(/^agent1/).focus();
    await userEvent.keyboard("{ArrowDown}");
    expect(row(/^agent2/)).toHaveFocus();
    await userEvent.keyboard("{ArrowUp}");
    expect(row(/^agent1/)).toHaveFocus();
    expect(p.onToggle).not.toHaveBeenCalled();
  });

  it("footer shows calls, warnings, refresh, links and the credit", async () => {
    const p = props();
    render(AgentList, p);
    expect(screen.getByText("has used 93% of its week. It resets in 21h 52m.")).toBeInTheDocument();
    expect(screen.getByText(/asked/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Refresh" }));
    expect(p.onRefresh).toHaveBeenCalled();
    expect(await screen.findByText("Checked just now")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "How it works" }));
    await userEvent.click(screen.getByRole("button", { name: "What changed" }));
    await userEvent.click(screen.getByRole("button", { name: "Add agent" }));
    expect(p.onNavigate.mock.calls).toEqual([["help"], ["changes"], ["add"]]);
    expect(screen.getByText(/Created by Mahadi Hassan/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("link", { name: "mahadihassan.com" }));
    expect(p.onSite).toHaveBeenCalled();
  });

  it("says nobody is on the line when there are no calls", () => {
    render(AgentList, props({ view: { ...view, calls: [] } }));
    expect(screen.getByText("Nobody is on the line")).toBeInTheDocument();
  });
});
