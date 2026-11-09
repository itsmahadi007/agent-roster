import { describe, it, expect, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import RenameSheet from "./RenameSheet.svelte";
import { invoke } from "../test/setup";

const plan = {
  token: 9,
  summary: "Renamed agent1 to lead",
  steps: [
    { kind: "change", text: "Renames the agent1 command to lead." },
    { kind: "note", text: "Updates the roster note for your agents. The login stays as it is." },
  ],
};

describe("RenameSheet", () => {
  it("previews the new name, then renames", async () => {
    invoke.mockImplementation(async (cmd: string) => (cmd === "preview" ? plan : cmd === "apply" ? 4 : null));
    const onDone = vi.fn();
    render(RenameSheet, { name: "agent1", color: "#4d7cff", onDone, onCancel: () => {} });
    expect(screen.getByRole("heading", { name: "Rename agent1" })).toBeInTheDocument();
    const input = screen.getByLabelText("New name");
    expect(input).toHaveValue("agent1");
    expect(screen.getByRole("button", { name: "Rename" })).toBeDisabled();
    await userEvent.clear(input);
    await userEvent.type(input, "lead");
    expect(await screen.findByText("Renames the agent1 command to lead.")).toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith("preview", { req: { op: "rename", name: "agent1", new_name: "lead" } });
    await userEvent.click(screen.getByRole("button", { name: "Rename" }));
    expect(invoke).toHaveBeenCalledWith("apply", { token: 9 });
    await waitFor(() => expect(onDone).toHaveBeenCalledWith("lead"));
  });

  it("shows why a name can't be used", async () => {
    invoke.mockImplementation(async (cmd: string) => {
      if (cmd === "preview") throw "agent3 is already an agent.";
      return null;
    });
    render(RenameSheet, { name: "agent1", color: "#4d7cff", onDone: () => {}, onCancel: () => {} });
    const input = screen.getByLabelText("New name");
    await userEvent.clear(input);
    await userEvent.type(input, "agent3");
    expect(await screen.findByText("agent3 is already an agent.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Rename" })).toBeDisabled();
  });

  it("says the login is kept", () => {
    render(RenameSheet, { name: "agent1", color: "#4d7cff", onDone: () => {}, onCancel: () => {} });
    expect(screen.getByText(/The login stays signed in/)).toBeInTheDocument();
  });
});
