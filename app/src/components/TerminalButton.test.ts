import { describe, it, expect, beforeEach } from "vitest";
import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import TerminalButton from "./TerminalButton.svelte";
import { invoke } from "../test/setup";

const found = [
  { id: "Terminal", label: "Terminal", runs: true },
  { id: "Warp", label: "Warp", runs: false },
];

describe("TerminalButton", () => {
  beforeEach(() => localStorage.clear());

  it("opens the default terminal running the agent", async () => {
    invoke.mockResolvedValue(true);
    render(TerminalButton, { name: "agent2", terminals: found });
    await userEvent.click(screen.getByRole("button", { name: "Open in terminal" }));
    expect(invoke).toHaveBeenCalledWith("open_terminal_as", { name: "agent2", app: null });
  });

  it("a terminal that can't run commands gets the command copied", async () => {
    const user = userEvent.setup();
    invoke.mockResolvedValue(false);
    render(TerminalButton, { name: "agent2", terminals: found });
    await user.click(screen.getByRole("button", { name: "Choose a terminal" }));
    await user.click(screen.getByRole("menuitem", { name: /Warp/ }));
    expect(invoke).toHaveBeenCalledWith("open_terminal_as", { name: "agent2", app: "Warp" });
    expect(await navigator.clipboard.readText()).toBe("agent2");
    expect(await screen.findByText("agent2 is copied. Paste it in Warp and press Return.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Open in Warp" })).toBeInTheDocument();
  });

  it("with no terminal found, it offers the command to copy", async () => {
    const user = userEvent.setup();
    render(TerminalButton, { name: "agent2", terminals: [] });
    await user.click(screen.getByRole("button", { name: "Copy command" }));
    expect(await navigator.clipboard.readText()).toBe("agent2");
    expect(screen.getByText("Open any terminal, type agent2 and press Return.")).toBeInTheDocument();
  });
});
