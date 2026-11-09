import { describe, it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import PlanSheet from "./PlanSheet.svelte";
import type { PlanView } from "../lib/types";
import { invoke } from "../test/setup";

const plan: PlanView = {
  token: 7,
  summary: "Turned on Claude limits for agent1",
  steps: [
    { kind: "change", text: "Routes agent1's status line through Agent Roster." },
    { kind: "note", text: "Nothing else on your computer changes." },
  ],
};

describe("PlanSheet", () => {
  it("shows the summary and every step with its mark", () => {
    const { container } = render(PlanSheet, { plan, confirmLabel: "Turn on", onDone: () => {}, onCancel: () => {} });
    expect(screen.getByText(plan.summary)).toBeInTheDocument();
    expect(screen.getByText(plan.steps[0].text)).toBeInTheDocument();
    expect(container.querySelector('[data-kind="change"] .mark')?.textContent).toBe("~");
  });

  it("applies the previewed token then reports done", async () => {
    invoke.mockResolvedValue(4);
    const onDone = vi.fn();
    render(PlanSheet, { plan, confirmLabel: "Turn on", onDone, onCancel: () => {} });
    await userEvent.click(screen.getByRole("button", { name: "Turn on" }));
    expect(invoke).toHaveBeenCalledWith("apply", { token: 7 });
    expect(onDone).toHaveBeenCalledWith(4);
  });

  it("plan_sheet_apply_error_is_shown", async () => {
    const msg = "Your roster changed since this preview. Look at the new preview before applying.";
    invoke.mockRejectedValue(msg);
    const onDone = vi.fn();
    render(PlanSheet, { plan, confirmLabel: "Turn on", onDone, onCancel: () => {} });
    await userEvent.click(screen.getByRole("button", { name: "Turn on" }));
    expect(await screen.findByText(msg)).toBeInTheDocument();
    expect(onDone).not.toHaveBeenCalled();
  });

  it("cancel changes nothing", async () => {
    const onCancel = vi.fn();
    render(PlanSheet, { plan, confirmLabel: "Turn on", onDone: () => {}, onCancel });
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onCancel).toHaveBeenCalled();
    expect(invoke).not.toHaveBeenCalled();
  });
});
