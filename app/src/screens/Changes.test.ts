import { describe, it, expect } from "vitest";
import { render, screen, waitFor } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import Changes from "./Changes.svelte";
import changes from "../lib/fixtures/changes.json";
import { invoke } from "../test/setup";

function backend(extra: Record<string, (a: any) => unknown> = {}) {
  invoke.mockImplementation(async (cmd: string, args: any) => {
    if (cmd === "changes") return changes;
    const h = extra[cmd];
    if (!h) throw new Error(`unexpected ${cmd}`);
    return h(args);
  });
}

describe("Changes", () => {
  it("lists_changes_newest_first", async () => {
    backend();
    const { container } = render(Changes, { onBack: () => {} });
    await screen.findByText("Turned on Claude limits for agent1");
    const rows = [...container.querySelectorAll("[data-change]")].map((r) => r.getAttribute("data-change"));
    expect(rows).toEqual(["3", "2", "1"]);
    expect(screen.getAllByRole("button", { name: "Undo" })).toHaveLength(2);
    expect(screen.getByText("undone")).toBeInTheDocument();
    expect(container.querySelector('[data-change="2"] .dot')).toHaveStyle({ background: "#c06bf0" });
  });

  it("undo_one_confirms_then_calls_undo", async () => {
    backend({ undo: () => null });
    render(Changes, { onBack: () => {} });
    await userEvent.click((await screen.findAllByRole("button", { name: "Undo" }))[0]);
    expect(invoke).not.toHaveBeenCalledWith("undo", expect.anything());
    expect(screen.getByText("Undo change 3: Turned on Claude limits for agent1?")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Yes, undo" }));
    expect(invoke).toHaveBeenCalledWith("undo", { id: 3 });
    await waitFor(() => expect(invoke.mock.calls.filter((c) => c[0] === "changes")).toHaveLength(2));
  });

  it("undo_error_is_shown_verbatim", async () => {
    const msg = "Change 3 also changed /h/.config/agent-roster/roster.toml. Undo change 3 first, or undo everything with `roster undo --all`.";
    backend({
      undo: () => {
        throw msg;
      },
    });
    render(Changes, { onBack: () => {} });
    await userEvent.click((await screen.findAllByRole("button", { name: "Undo" }))[1]);
    await userEvent.click(screen.getByRole("button", { name: "Yes, undo" }));
    expect(await screen.findByText(msg)).toBeInTheDocument();
  });

  it("undo_everything_confirms_then_calls_undo_all", async () => {
    backend({ undo_all: () => 2 });
    render(Changes, { onBack: () => {} });
    await userEvent.click(await screen.findByRole("button", { name: "Undo everything" }));
    expect(screen.getByText("Undo every change Agent Roster made?")).toBeInTheDocument();
    expect(invoke).not.toHaveBeenCalledWith("undo_all");
    await userEvent.click(screen.getByRole("button", { name: "Yes, undo" }));
    expect(invoke).toHaveBeenCalledWith("undo_all");
  });

  it("cancel leaves everything alone", async () => {
    backend();
    render(Changes, { onBack: () => {} });
    await userEvent.click(await screen.findByRole("button", { name: "Undo everything" }));
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.queryByText("Undo every change Agent Roster made?")).not.toBeInTheDocument();
  });

  it("hooks_row_says_none_ever", async () => {
    backend();
    render(Changes, { onBack: () => {} });
    expect(await screen.findByText("Hooks added to your tools")).toBeInTheDocument();
    expect(screen.getByText("none, ever")).toBeInTheDocument();
    expect(screen.getByText(/To remove Agent Roster and put all of this back, run/)).toBeInTheDocument();
  });

  it("empty history", async () => {
    invoke.mockResolvedValue([]);
    render(Changes, { onBack: () => {} });
    expect(await screen.findByText("Agent Roster hasn't changed anything yet")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Undo everything" })).not.toBeInTheDocument();
  });
});
