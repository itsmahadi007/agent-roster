import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import TopBar from "./TopBar.svelte";
import { win } from "../test/setup";

describe("TopBar", () => {
  it("draws minimize, maximize and close on Windows", async () => {
    const { container } = render(TopBar, { chrome: "windows" });
    expect(container.querySelector(".drag[data-tauri-drag-region]")).toBeInTheDocument();
    expect(screen.getByText("Agent Roster")).toBeInTheDocument();
    // The app icon, then the name, on the left; the three buttons on the right.
    const bar = container.querySelector(".bar.windows")!;
    expect(bar.firstElementChild).toHaveClass("icon");
    expect(bar.lastElementChild).toHaveClass("ctl");
    expect(bar.querySelectorAll(".ctl button")).toHaveLength(3);
    await userEvent.click(screen.getByRole("button", { name: "Minimize" }));
    await userEvent.click(screen.getByRole("button", { name: "Maximize" }));
    await userEvent.click(screen.getByRole("button", { name: "Close" }));
    expect(win.minimize).toHaveBeenCalled();
    expect(win.toggleMaximize).toHaveBeenCalled();
    expect(win.close).toHaveBeenCalled();
  });

  it("leaves room for the Mac's own buttons and draws none of its own", () => {
    const { container } = render(TopBar, { chrome: "mac" });
    const bar = container.querySelector(".bar.mac") as HTMLElement;
    expect(bar).toBeInTheDocument();
    expect(bar.firstElementChild).toHaveClass("lights");
    expect(bar.firstElementChild).toHaveStyle({ width: "76px" });
    expect(screen.getByText("Agent Roster")).toBeInTheDocument();
    expect(container.querySelector(".icon")).not.toBeInTheDocument();
    expect(container.querySelector(".ctl")).not.toBeInTheDocument();
    expect(screen.queryByRole("button")).not.toBeInTheDocument();
    expect(container.querySelector(".drag[data-tauri-drag-region]")).toBeInTheDocument();
  });

  it("draws nothing where the system frame is kept", () => {
    const { container } = render(TopBar, { chrome: "none" });
    expect(container.querySelector(".bar")).not.toBeInTheDocument();
  });
});
