import { describe, it, expect, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import Updates from "./Updates.svelte";
import { updater } from "../test/setup";

describe("Updates", () => {
  it("shows the version and says when it is up to date", async () => {
    render(Updates);
    expect(await screen.findByText("Version 0.2.0")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Check for updates" }));
    expect(await screen.findByText("You have the latest version")).toBeInTheDocument();
  });

  it("offers a new version, installs it and restarts", async () => {
    const downloadAndInstall = vi.fn(async (cb: (e: unknown) => void) => {
      cb({ event: "Started", data: { contentLength: 100 } });
      cb({ event: "Progress", data: { chunkLength: 50 } });
      cb({ event: "Finished" });
    });
    updater.check.mockResolvedValue({ version: "0.3.0", downloadAndInstall });
    render(Updates);
    expect(await screen.findByText("Agent Roster 0.3.0 is ready")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Update now" }));
    expect(downloadAndInstall).toHaveBeenCalled();
    await waitFor(() => expect(updater.relaunch).toHaveBeenCalled());
  });

  it("Later hides the offer until the next check", async () => {
    updater.check.mockResolvedValue({ version: "0.3.0", downloadAndInstall: vi.fn() });
    render(Updates);
    await userEvent.click(await screen.findByRole("button", { name: "Later" }));
    expect(screen.queryByText("Agent Roster 0.3.0 is ready")).not.toBeInTheDocument();
  });

  it("a failed check stays quiet until asked", async () => {
    updater.check.mockRejectedValue(new Error("offline"));
    render(Updates);
    expect(await screen.findByText("Version 0.2.0")).toBeInTheDocument();
    expect(screen.queryByText(/offline/)).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Check for updates" }));
    expect(await screen.findByText("Couldn't check for updates. Try again later.")).toBeInTheDocument();
  });
});
