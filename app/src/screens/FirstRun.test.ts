import { describe, it, expect, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import FirstRun from "./FirstRun.svelte";
import firstRun from "../lib/fixtures/first-run.json";
import overview from "../lib/fixtures/overview.json";
import type { FirstRunView, OverviewView } from "../lib/types";
import { invoke } from "../test/setup";

const palette = (overview as OverviewView).palette;
const plan = { token: 2, summary: "Added claude1 and agent3 (existing logins)", steps: [{ kind: "add", text: "Adds a claude1 command for every terminal." }] };

function backend(fr: FirstRunView, extra: Record<string, (a: any) => unknown> = {}) {
  invoke.mockImplementation(async (cmd: string, args: any) => {
    if (cmd === "first_run") return fr;
    const h = extra[cmd];
    if (!h) throw new Error(`unexpected ${cmd}`);
    return h(args);
  });
}

describe("FirstRun", () => {
  it("lists_candidates_with_versions", async () => {
    backend(firstRun as FirstRunView);
    render(FirstRun, { palette, onDone: () => {} });
    expect(await screen.findByRole("heading", { name: "You already have two agents" })).toBeInTheDocument();
    expect(screen.getByText("claude1")).toBeInTheDocument();
    expect(screen.getByText("Claude Code 2.1.0")).toBeInTheDocument();
    expect(screen.getByText("you@example.com")).toBeInTheDocument();
    expect(screen.getByText("Codex 0.167.0")).toBeInTheDocument();
    expect(screen.getByText("Gemini")).toBeInTheDocument();
    expect(screen.getByText("soon")).toBeInTheDocument();
    expect(
      screen.getByText(/Your logins stay exactly where they are\. Agent Roster gives each one a name and a command\./),
    ).toBeInTheDocument();
  });

  it("rename_changes_the_pick", async () => {
    backend(firstRun as FirstRunView, { preview: () => plan, apply: () => 1 });
    const onDone = vi.fn();
    render(FirstRun, { palette, onDone });
    await screen.findByText("codex1");
    await userEvent.click(screen.getAllByRole("button", { name: "Rename" })[1]);
    const input = screen.getByLabelText("New name for codex1");
    await userEvent.clear(input);
    await userEvent.type(input, "agent3");
    await userEvent.click(screen.getByRole("button", { name: "Continue" }));
    expect(invoke).toHaveBeenCalledWith("preview", {
      req: { op: "adopt-all", picks: [["claude", "claude1"], ["codex", "agent3"]] },
    });
    expect(await screen.findByText(plan.steps[0].text)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Put them on the roster" }));
    await waitFor(() => expect(onDone).toHaveBeenCalled());
  });

  it("prototype_offers_import", async () => {
    backend({ ...(firstRun as FirstRunView), prototype: true }, { preview: () => plan });
    render(FirstRun, { palette, onDone: () => {} });
    await userEvent.click(await screen.findByRole("button", { name: "Bring them in" }));
    expect(invoke).toHaveBeenCalledWith("preview", { req: { op: "import" } });
    expect(await screen.findByRole("button", { name: "Import" })).toBeInTheDocument();
  });

  it("first_run_without_tools_disables_continue", async () => {
    const none: FirstRunView = {
      prototype: false,
      candidates: (firstRun as FirstRunView).candidates.map((c) => ({ ...c, installed: false, version: null, email: null, signed_in: false })),
    };
    backend(none);
    render(FirstRun, { palette, onDone: () => {} });
    expect(await screen.findByText("No Claude Code, Codex or Grok login was found")).toBeInTheDocument();
    expect(screen.getAllByText("not installed")).toHaveLength(2);
    expect(screen.getByRole("button", { name: "Continue" })).toBeDisabled();
    expect(screen.queryByRole("button", { name: "Rename" })).not.toBeInTheDocument();
  });

  it("one installed tool says one agent and skips the missing one", async () => {
    const fr = structuredClone(firstRun) as FirstRunView;
    fr.candidates[1] = { ...fr.candidates[1], installed: false, version: null };
    backend(fr, { preview: () => plan });
    render(FirstRun, { palette, onDone: () => {} });
    expect(await screen.findByText("You already have one agent")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Continue" }));
    expect(invoke).toHaveBeenCalledWith("preview", { req: { op: "adopt-all", picks: [["claude", "claude1"]] } });
  });

  it("preview error is shown", async () => {
    backend(firstRun as FirstRunView, {
      preview: () => {
        throw "claude1 is already a command on this computer.";
      },
    });
    render(FirstRun, { palette, onDone: () => {} });
    await userEvent.click(await screen.findByRole("button", { name: "Continue" }));
    expect(await screen.findByText("claude1 is already a command on this computer.")).toBeInTheDocument();
  });
});
