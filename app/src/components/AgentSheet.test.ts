import { describe, it, expect, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import AgentSheet from "./AgentSheet.svelte";
import overview from "../lib/fixtures/overview.json";
import type { OverviewView } from "../lib/types";
import { invoke } from "../test/setup";

const view = overview as OverviewView;
const agent = (name: string) => view.agents.find((a) => a.name === name)!;
const step = (text: string) => ({ token: 9, summary: "s", steps: [{ kind: "change", text }] });

function mount(name: string, over = {}) {
  const onApplied = vi.fn();
  const onClose = vi.fn();
  render(AgentSheet, { agent: agent(name), palette: view.palette, onApplied, onClose, ...over });
  return { onApplied, onClose };
}

function answer(text: string) {
  invoke.mockImplementation(async (cmd: string) => (cmd === "preview" ? step(text) : cmd === "apply" ? 4 : undefined));
}

describe("AgentSheet", () => {
  it("renames after showing what will change", async () => {
    answer("Renames the agent1 command to lead.");
    const { onApplied } = mount("agent1");
    expect(screen.getByRole("heading", { name: "Edit agent1" })).toBeInTheDocument();
    const input = screen.getByLabelText("Name");
    expect(input).toHaveValue("agent1");
    await userEvent.clear(input);
    await userEvent.type(input, "lead");
    expect(await screen.findByText("Renames the agent1 command to lead.")).toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith("preview", { req: { op: "rename", name: "agent1", new_name: "lead" } });
    await userEvent.click(screen.getByRole("button", { name: "Rename" }));
    expect(invoke).toHaveBeenCalledWith("apply", { token: 9 });
    await waitFor(() =>
      expect(onApplied).toHaveBeenCalledWith("agent1 is now lead. Type lead in any terminal to start it.", "rename"),
    );
  });

  it("shows why a change can't be made", async () => {
    invoke.mockImplementation(async (cmd: string) => {
      if (cmd === "preview") throw "agent3 is already an agent.";
    });
    mount("agent1");
    const input = screen.getByLabelText("Name");
    await userEvent.clear(input);
    await userEvent.type(input, "agent3");
    expect(await screen.findByText("agent3 is already an agent.")).toBeInTheDocument();
  });

  it("changes the color", async () => {
    answer("Changes agent1's color to orchid.");
    const { onApplied } = mount("agent1");
    expect(screen.getByRole("radio", { name: "cobalt" })).toBeChecked();
    await userEvent.click(screen.getByRole("radio", { name: "orchid" }));
    expect(await screen.findByText("Changes agent1's color to orchid.")).toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith("preview", { req: { op: "color", name: "agent1", color: "orchid" } });
    await userEvent.click(screen.getByRole("button", { name: "Change color" }));
    await waitFor(() => expect(onApplied).toHaveBeenCalledWith("agent1 is orchid now.", "color"));
  });

  it("saves a role, and an empty role clears it", async () => {
    answer("Sets the role.");
    mount("agent1");
    const role = screen.getByLabelText("Role");
    await userEvent.type(role, "reviews pull requests");
    await waitFor(() =>
      expect(invoke).toHaveBeenLastCalledWith("preview", {
        req: { op: "role", name: "agent1", role: "reviews pull requests" },
      }),
    );
    expect(await screen.findByRole("button", { name: "Save role" })).toBeEnabled();
  });

  it("turns skill sharing on", async () => {
    answer("Links your skills.");
    mount("agent1");
    await userEvent.click(screen.getByRole("switch", { name: "Share my skills" }));
    await waitFor(() =>
      expect(invoke).toHaveBeenLastCalledWith("preview", { req: { op: "share", name: "agent1", on: true } }),
    );
    expect(await screen.findByRole("button", { name: "Turn on sharing" })).toBeEnabled();
  });

  it("makes an agent the default, or says it already is", async () => {
    answer("Points claude at agent2.");
    mount("agent2");
    await userEvent.click(screen.getByRole("button", { name: "Make default for claude" }));
    await waitFor(() => expect(invoke).toHaveBeenLastCalledWith("preview", { req: { op: "default", name: "agent2" } }));
    expect(await screen.findByRole("button", { name: "Make default" })).toBeEnabled();
  });

  it("says when it is already the default", () => {
    mount("agent1");
    expect(screen.getByText("A plain claude in a new terminal opens agent1")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Make default for claude" })).not.toBeInTheDocument();
  });

  it("removes but keeps the login folder", async () => {
    answer("Removes the boss command.");
    const { onApplied } = mount("boss");
    expect(screen.getByText(/Its login folder stays on this computer/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Remove boss" }));
    expect(invoke).toHaveBeenLastCalledWith("preview", { req: { op: "remove", name: "boss" } });
    await userEvent.click(await screen.findByRole("button", { name: "Remove" }));
    await waitFor(() =>
      expect(onApplied).toHaveBeenCalledWith(
        "boss was removed. Its login folder is still there, and you can undo this under What changed.",
        "remove",
      ),
    );
  });

  it("opens a terminal and signs in again", async () => {
    invoke.mockResolvedValue(undefined);
    mount("agent1");
    await userEvent.click(screen.getByRole("button", { name: "Open a terminal" }));
    expect(invoke).toHaveBeenCalledWith("open_terminal_as", { name: "agent1" });
    await userEvent.click(screen.getByRole("button", { name: "Sign in again" }));
    expect(invoke).toHaveBeenCalledWith("sign_in", { name: "agent1" });
  });

  it("Escape and Close close it", async () => {
    const { onClose } = mount("agent1");
    await userEvent.keyboard("{Escape}");
    await userEvent.click(screen.getByRole("button", { name: "Close" }));
    expect(onClose).toHaveBeenCalledTimes(2);
  });
});
