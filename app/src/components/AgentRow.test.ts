import { describe, it, expect } from "vitest";
import { render, screen, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import AgentRow from "./AgentRow.svelte";
import overview from "../lib/fixtures/overview.json";
import type { OverviewView } from "../lib/types";
import { invoke } from "../test/setup";

const view = overview as OverviewView;
const agent = (name: string) => view.agents.find((a) => a.name === name)!;

describe("AgentRow", () => {
  it("clicking the name asks to rename it", async () => {
    let asked = "";
    render(AgentRow, { agent: agent("agent1"), onRename: (n: string) => (asked = n) });
    await userEvent.click(screen.getByRole("button", { name: "Rename agent1" }));
    expect(asked).toBe("agent1");
  });

  it("explains limits, status and the jack on hover", () => {
    const { container } = render(AgentRow, { agent: agent("agent1") });
    expect(container.querySelector('[data-window="5 hr"]')).toHaveAttribute("title", "22% of the 5-hour limit used");
    expect(container.querySelector('[data-window="week"]')).toHaveAttribute("title", "61% of the weekly limit used");
    expect(container.querySelector(".jack")).toHaveAttribute("title", "On a call with another agent");
    expect(screen.getByText("default for claude")).toHaveAttribute("title", "A plain claude command opens this account in new terminals");
  });

  it("renders_name_in_agent_color", () => {
    render(AgentRow, { agent: agent("agent1") });
    expect(screen.getByText("agent1")).toHaveStyle({ color: "#4d7cff" });
    expect(screen.getByText("Claude Code")).toBeInTheDocument();
    expect(screen.getByText("you@example.com")).toBeInTheDocument();
    expect(screen.getByText("default for claude")).toBeInTheDocument();
    expect(screen.getByText("calling agent3")).toBeInTheDocument();
  });

  it("hot_bar_is_coral", () => {
    const { container } = render(AgentRow, { agent: agent("agent3") });
    expect(screen.getByText("93%")).toHaveClass("hot");
    const fill = container.querySelector('[data-window="week"] i')!;
    expect(fill).toHaveClass("hot");
    expect(fill).toHaveStyle({ width: "93%" });
    expect(screen.getByText("resets in 21h 52m")).toHaveClass("hot");
  });

  it("shows_reset_text", () => {
    const { container } = render(AgentRow, { agent: agent("agent1") });
    expect(screen.getByText("week resets in 3d 16h")).not.toHaveClass("hot");
    expect(container.querySelectorAll("[data-window]")).toHaveLength(2);
    expect(screen.getByText("61%")).not.toHaveClass("hot");
  });

  it("sign_in_button_calls_sign_in", async () => {
    invoke.mockResolvedValue(undefined);
    render(AgentRow, { agent: agent("agent2") });
    expect(screen.getByText("limits appear after sign in")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Sign in" }));
    expect(invoke).toHaveBeenCalledWith("sign_in", { name: "agent2" });
  });

  it("lit jack only when on a live call", () => {
    const { container } = render(AgentRow, { agent: agent("boss") });
    expect(container.querySelector(".jack b")).not.toHaveClass("live");
    const row = within(container as HTMLElement);
    expect(row.getByText("ready")).toBeInTheDocument();
  });
});
