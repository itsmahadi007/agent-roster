import { describe, it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import AgentList from "./AgentList.svelte";
import overview from "../lib/fixtures/overview.json";
import type { OverviewView } from "../lib/types";

const view = overview as OverviewView;
const props = (over = {}) => ({
  view,
  selected: "a1",
  onSelect: vi.fn(),
  onNavigate: vi.fn(),
  onRefresh: vi.fn(async () => {}),
  onSite: vi.fn(),
  ...over,
});

describe("AgentList", () => {
  it("lists every agent with its tool and weekly use", () => {
    render(AgentList, props());
    expect(screen.getByRole("heading", { name: "Your agents" })).toBeInTheDocument();
    const a1 = screen.getByRole("button", { name: /agent1/ });
    expect(a1).toHaveAttribute("aria-current", "true");
    expect(a1).toHaveTextContent("Claude Code");
    expect(a1).toHaveTextContent("61% week");
    expect(screen.getByRole("button", { name: /agent3/ })).toHaveTextContent("93% week");
    expect(screen.getByRole("button", { name: /agent2/ })).toHaveTextContent("sign in");
  });

  it("click or arrow keys pick an agent", async () => {
    const p = props();
    render(AgentList, p);
    await userEvent.click(screen.getByRole("button", { name: /agent3/ }));
    expect(p.onSelect).toHaveBeenLastCalledWith("a3");
    screen.getByRole("button", { name: /agent1/ }).focus();
    await userEvent.keyboard("{ArrowDown}");
    expect(p.onSelect).toHaveBeenLastCalledWith("a2");
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
