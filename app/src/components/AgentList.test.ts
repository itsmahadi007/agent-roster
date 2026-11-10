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
    expect(a1).toHaveTextContent("resets in 3d 16h");
    const a3 = row(/^agent3/);
    expect(a3).toHaveTextContent("93% week");
    expect(a3).toHaveTextContent("resets in 21h 52m");
    expect(a3.querySelector(".pc")).toHaveClass("hot");
    expect(a3.querySelector(".rs")).toHaveClass("hot");
    expect(a1.querySelector(".rs")).not.toHaveClass("hot");
    expect(row(/^agent2/)).toHaveTextContent("sign in");
    for (const b of screen.getAllByRole("button", { expanded: false })) expect(b).not.toHaveTextContent("5 hr");
  });

  it("a row with no numbers says it hasn't been used yet", () => {
    const v = structuredClone(view);
    Object.assign(v.agents[3], { windows: [], reset_text: null, limits_note: "usage shows after you use boss" });
    render(AgentList, props({ view: v }));
    expect(row(/^boss/)).toHaveTextContent("not used yet");
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
