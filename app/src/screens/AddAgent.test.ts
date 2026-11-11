import { describe, it, expect, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import AddAgent from "./AddAgent.svelte";
import overview from "../lib/fixtures/overview.json";
import type { OverviewView, PlanView } from "../lib/types";
import { invoke } from "../test/setup";
import { prefs, resetSignIn } from "../lib/signin.svelte";

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
  it("add_and_sign_in_uses_the_window_when_set", async () => {
    resetSignIn();
    prefs.signInWith = "window";
    backend({ preview: () => plan, apply: () => 9, sign_in_in_app: () => null });
    const onDone = vi.fn();
    render(AddAgent, props({ onDone }));
    await userEvent.type(screen.getByLabelText("Name"), "boss");
    await screen.findByText(plan.steps[0].text);
    await userEvent.click(screen.getByRole("button", { name: "Add and sign in" }));
    try {
      await waitFor(() => expect(onDone).toHaveBeenCalled());
      expect(invoke).toHaveBeenCalledWith("sign_in_in_app", { name: "boss" });
      expect(invoke).not.toHaveBeenCalledWith("sign_in", expect.anything());
    } finally {
      resetSignIn();
    }
  });

  it("a grok agent signs in with the browser even when the window is set", async () => {
    resetSignIn();
    prefs.signInWith = "window";
    backend({ preview: () => plan, apply: () => 9, sign_in: () => true });
    const onDone = vi.fn();
    render(AddAgent, props({ onDone }));
    await userEvent.click(screen.getByRole("button", { name: "Grok" }));
    await userEvent.type(screen.getByLabelText("Name"), "grok2");
    await waitFor(() =>
      expect(invoke).toHaveBeenLastCalledWith("preview", {
        req: { op: "add", name: "grok2", tool: "grok", color: "pink", role: null, share: false },
      }),
    );
    await screen.findByText(plan.steps[0].text);
    await userEvent.click(screen.getByRole("button", { name: "Add and sign in" }));
    try {
      await waitFor(() => expect(onDone).toHaveBeenCalled());
      expect(invoke).not.toHaveBeenCalledWith("sign_in_in_app", expect.anything());
    } finally {
      resetSignIn();
    }
  });

  it("add_preview_shows_backend_steps", async () => {
    backend({ preview: () => plan });
    render(AddAgent, props());
    await userEvent.click(screen.getByRole("button", { name: "Codex" }));
    await userEvent.type(screen.getByLabelText("Name"), "boss");
    await waitFor(() =>
      expect(invoke).toHaveBeenLastCalledWith("preview", {
        req: { op: "add", name: "boss", tool: "codex", color: "pink", role: null, share: false },
      }),
    );
    for (const s of plan.steps) expect(await screen.findByText(s.text)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Add and sign in" })).toBeEnabled();
  });

  it("name_shows_in_the_chosen_color", async () => {
    backend({ preview: () => plan });
    render(AddAgent, props());
    await userEvent.type(screen.getByLabelText("Name"), "boss");
    await userEvent.click(screen.getByRole("radio", { name: "violet" }));
    expect(screen.getByLabelText("Name")).toHaveStyle({ color: "#b86bff" });
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
    expect(invoke).toHaveBeenCalledWith("sign_in", { name: "boss", app: null });
  });

  it("is a dialog with a title that closes with ✕, Escape, the dimmed list or Cancel, and no Back link", async () => {
    const onCancel = vi.fn();
    render(AddAgent, props({ onCancel }));
    expect(screen.getByRole("dialog", { name: "Add an agent" })).toHaveAttribute("aria-modal", "true");
    expect(screen.getByRole("heading", { name: "Add an agent" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Back/ })).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Close" }));
    await userEvent.keyboard("{Escape}");
    await userEvent.click(screen.getByTestId("scrim"));
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onCancel).toHaveBeenCalledTimes(4);
  });

  it("asks Tool, then Name, then Color; role and sharing are folded away", async () => {
    render(AddAgent, props());
    const tool = screen.getByRole("button", { name: "Claude Code" });
    const name = screen.getByLabelText("Name");
    const color = screen.getByRole("radiogroup", { name: "Color" });
    expect(tool.compareDocumentPosition(name) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(name.compareDocumentPosition(color) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(name).toHaveFocus();
    expect(screen.getByText("Name: the command you type to start it")).toBeInTheDocument();
    expect(screen.queryByRole("switch", { name: "Share my skills" })).not.toBeInTheDocument();
    const more = screen.getByRole("button", { name: "▸ More options (role, share skills)" });
    expect(more).toHaveAttribute("aria-expanded", "false");
    await userEvent.click(more);
    expect(screen.getByText("Role (optional): other agents read this to decide what to hand it")).toBeInTheDocument();
    expect(screen.getByRole("switch", { name: "Share my skills" })).toBeInTheDocument();
  });

  it("the picker has only the new colors, and the next color shows as its new name", () => {
    render(AddAgent, props());
    const radios = screen.getAllByRole("radio").map((r) => r.getAttribute("aria-label"));
    expect(radios).toEqual(["cobalt", "cyan", "jade", "violet", "pink"]);
    // next_color is pink in the fixture.
    expect(screen.getByRole("radio", { name: "pink" })).toHaveAttribute("aria-checked", "true");
  });

  it("one line says what will happen, with the full preview under Details", async () => {
    backend({ preview: () => plan });
    render(AddAgent, props());
    await userEvent.type(screen.getByLabelText("Name"), "boss");
    expect(await screen.findByText(/command for Claude Code, with its own login folder/)).toBeInTheDocument();
    const details = screen.getByText("Details").closest("details")!;
    expect(details).not.toHaveAttribute("open");
    for (const s of plan.steps) expect(details).toHaveTextContent(s.text);
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
    await userEvent.click(screen.getByRole("button", { name: /More options/ }));
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
