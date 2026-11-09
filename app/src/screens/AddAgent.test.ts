import { describe, it, expect, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import AddAgent from "./AddAgent.svelte";
import overview from "../lib/fixtures/overview.json";
import type { OverviewView, PlanView } from "../lib/types";
import { invoke } from "../test/setup";

const view = overview as OverviewView;
const plan: PlanView = {
  token: 3,
  summary: "Added boss (Codex)",
  steps: [
    { kind: "add", text: "Adds a boss command for every terminal." },
    { kind: "add", text: "Creates a private folder for this account." },
    { kind: "note", text: "Nothing else on your computer changes." },
  ],
};

function backend(handlers: Record<string, (args: any) => unknown>) {
  invoke.mockImplementation(async (cmd: string, args: any) => {
    const h = handlers[cmd];
    if (!h) throw new Error(`unexpected ${cmd}`);
    return h(args);
  });
}

const props = (over = {}) => ({ palette: view.palette, nextColor: view.next_color, onDone: () => {}, onCancel: () => {}, ...over });

describe("AddAgent", () => {
  it("add_preview_shows_backend_steps", async () => {
    backend({ preview: () => plan });
    render(AddAgent, props());
    await userEvent.click(screen.getByRole("button", { name: "Codex" }));
    await userEvent.type(screen.getByLabelText("Name"), "boss");
    await waitFor(() =>
      expect(invoke).toHaveBeenLastCalledWith("preview", {
        req: { op: "add", name: "boss", tool: "codex", color: "coral", role: null, share: false },
      }),
    );
    for (const s of plan.steps) expect(await screen.findByText(s.text)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Add and sign in" })).toBeEnabled();
  });

  it("name_shows_in_the_chosen_color", async () => {
    backend({ preview: () => plan });
    render(AddAgent, props());
    await userEvent.type(screen.getByLabelText("Name"), "boss");
    await userEvent.click(screen.getByRole("radio", { name: "orchid" }));
    expect(screen.getByLabelText("Name")).toHaveStyle({ color: "#c06bf0" });
    await waitFor(() =>
      expect(invoke).toHaveBeenLastCalledWith("preview", expect.objectContaining({ req: expect.objectContaining({ color: "orchid" }) })),
    );
  });

  it("add_shows_backend_error", async () => {
    const msg = "Names can't start with a digit. Try a1 instead.";
    backend({
      preview: () => {
        throw msg;
      },
    });
    render(AddAgent, props());
    await userEvent.type(screen.getByLabelText("Name"), "1");
    expect(await screen.findByText(msg)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Add and sign in" })).toBeDisabled();
  });

  it("add_and_sign_in_applies_then_signs_in", async () => {
    backend({ preview: () => plan, apply: () => 9, sign_in: () => null });
    const onDone = vi.fn();
    render(AddAgent, props({ onDone }));
    await userEvent.type(screen.getByLabelText("Name"), "boss");
    await screen.findByText(plan.steps[0].text);
    await userEvent.click(screen.getByRole("button", { name: "Add and sign in" }));
    await waitFor(() => expect(onDone).toHaveBeenCalled());
    const cmds = invoke.mock.calls.map((c) => c[0]);
    expect(cmds.slice(-2)).toEqual(["apply", "sign_in"]);
    expect(invoke).toHaveBeenCalledWith("apply", { token: 3 });
    expect(invoke).toHaveBeenCalledWith("sign_in", { name: "boss" });
  });

  it("has a title and a way back", async () => {
    const onCancel = vi.fn();
    render(AddAgent, props({ onCancel }));
    expect(screen.getByRole("heading", { name: "Add an agent" })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Back to your agents" }));
    expect(onCancel).toHaveBeenCalled();
  });

  it("gemini_is_disabled", () => {
    render(AddAgent, props());
    expect(screen.getByRole("button", { name: "Gemini soon" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Add and sign in" })).toBeDisabled();
    expect(invoke).not.toHaveBeenCalled();
  });

  it("share toggle says what it does", async () => {
    backend({ preview: () => plan });
    render(AddAgent, props());
    expect(screen.getByText("Off. This account starts fully separate.")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("switch", { name: "Share my skills" }));
    expect(screen.getByText("On. Uses the skills of your main Claude Code account.")).toBeInTheDocument();
  });
});

describe("AddAgent stale preview", () => {
  it("looks again after a stale apply and the next click uses the new preview", async () => {
    const stale = "Your roster changed since this preview. Look at the new preview before applying.";
    let previews = 0;
    let applies = 0;
    backend({
      preview: () => ({ ...plan, token: 100 + ++previews }),
      apply: () => {
        applies += 1;
        if (applies === 1) throw stale;
        return 5;
      },
      sign_in: () => null,
    });
    const onDone = vi.fn();
    render(AddAgent, props({ onDone }));
    await userEvent.type(screen.getByLabelText("Name"), "boss");
    await screen.findByText(plan.steps[0].text);
    const first = previews;
    await userEvent.click(screen.getByRole("button", { name: "Add and sign in" }));
    expect(await screen.findByText(stale)).toBeInTheDocument();
    await waitFor(() => expect(previews).toBe(first + 1));
    await waitFor(() => expect(screen.getByRole("button", { name: "Add and sign in" })).toBeEnabled());
    await userEvent.click(screen.getByRole("button", { name: "Add and sign in" }));
    await waitFor(() => expect(onDone).toHaveBeenCalled());
    expect(invoke).toHaveBeenCalledWith("apply", { token: 100 + first + 1 });
  });
});
