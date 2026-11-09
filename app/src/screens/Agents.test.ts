import { describe, it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import Agents from "./Agents.svelte";
import overview from "../lib/fixtures/overview.json";
import type { OverviewView } from "../lib/types";

const view = overview as OverviewView;

describe("Agents", () => {
  it("shows_four_rows_and_side_panel", () => {
    render(Agents, { view, onNavigate: () => {}, onLimits: () => {} });
    expect(screen.getByRole("heading", { name: "Your agents" })).toBeInTheDocument();
    expect(screen.getByText(view.headline)).toBeInTheDocument();
    for (const n of ["agent1", "agent2", "agent3", "boss"]) {
      expect(screen.getAllByText(n).length).toBeGreaterThan(0);
    }
    expect(screen.getByText("On the line now")).toBeInTheDocument();
    expect(screen.getByText("Write tests for the login page and run them.")).toBeInTheDocument();
    expect(screen.getByText("started 2m ago in ~/projects/portfolio")).toBeInTheDocument();
    expect(screen.getByText("has used 93% of its week. It resets in 21h 52m.")).toBeInTheDocument();
    expect(screen.getByText("Updated each time an agent is used. Last update 4m ago.")).toBeInTheDocument();
    expect(screen.getByText("No hooks added.")).toBeInTheDocument();
  });

  it("cable_drawn_for_each_call", () => {
    const { container } = render(Agents, { view, onNavigate: () => {}, onLimits: () => {} });
    expect(container.querySelectorAll("svg.cable path.pulse")).toHaveLength(1);
    const d = container.querySelector("svg.cable path.pulse")!.getAttribute("d");
    expect(d).toBe("M0 38 C 66 38, 66 206, 0 206");
  });

  it("nobody on the line when there are no calls", () => {
    render(Agents, { view: { ...view, calls: [] }, onNavigate: () => {}, onLimits: () => {} });
    expect(screen.getByText("Nobody is on the line")).toBeInTheDocument();
    expect(screen.getByText("When one agent asks another for help, the call shows here")).toBeInTheDocument();
  });

  it("tells a new user how to rename and where help is", () => {
    render(Agents, { view, onNavigate: () => {}, onLimits: () => {} });
    expect(screen.getByText("Click a name to rename it. Hover a bar, a status or a jack to see what it means.")).toBeInTheDocument();
  });

  it("header buttons navigate", async () => {
    const onNavigate = vi.fn();
    render(Agents, { view, onNavigate, onLimits: () => {} });
    await userEvent.click(screen.getByRole("button", { name: "Add agent" }));
    await userEvent.click(screen.getByRole("button", { name: "What changed" }));
    expect(onNavigate.mock.calls).toEqual([["add"], ["changes"]]);
  });
});

describe("Agents with repeated calls", () => {
  it("two identical calls both show", () => {
    const twice = { ...view, calls: [view.calls[0], view.calls[0]], warnings: [view.warnings[0], view.warnings[0]] };
    render(Agents, { view: twice, onNavigate: () => {}, onLimits: () => {} });
    expect(screen.getAllByText("Write tests for the login page and run them.")).toHaveLength(2);
    expect(screen.getAllByText("has used 93% of its week. It resets in 21h 52m.")).toHaveLength(2);
  });
});
