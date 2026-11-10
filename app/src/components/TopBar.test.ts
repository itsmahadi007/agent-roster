import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import TopBar from "./TopBar.svelte";
import { win } from "../test/setup";

describe("TopBar", () => {
  it("draws minimize, maximize and close on Windows", async () => {
    const { container } = render(TopBar, { chrome: "windows" });
    expect(container.querySelector("[data-tauri-drag-region]")).toBeInTheDocument();
    expect(screen.getByText("Agent Roster")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Minimize" }));
    await userEvent.click(screen.getByRole("button", { name: "Maximize" }));
    await userEvent.click(screen.getByRole("button", { name: "Close" }));
    expect(win.minimize).toHaveBeenCalled();
    expect(win.toggleMaximize).toHaveBeenCalled();
    expect(win.close).toHaveBeenCalled();
  });

  it("leaves room for the Mac's own buttons", () => {
    const { container } = render(TopBar, { chrome: "mac" });
    expect(container.querySelector(".bar.mac")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Close" })).not.toBeInTheDocument();
  });

  it("draws nothing where the system frame is kept", () => {
    const { container } = render(TopBar, { chrome: "none" });
    expect(container.querySelector(".bar")).not.toBeInTheDocument();
  });
});
