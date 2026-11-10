<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import type { OpRequest, OverviewView, Screen, TerminalApp } from "./lib/types";
  import { message, openSite, overview, refreshUsage, terminals as findTerminals } from "./lib/api";
  import { startPoll } from "./lib/poll";
  import AddAgent from "./screens/AddAgent.svelte";
  import FirstRun from "./screens/FirstRun.svelte";
  import Changes from "./screens/Changes.svelte";
  import Help from "./screens/Help.svelte";
  import AgentList from "./components/AgentList.svelte";
  import TopBar from "./components/TopBar.svelte";
  import { chrome } from "./lib/platform";
  import { loadPrefs, onSignInUpdate, type SignInUpdate } from "./lib/signin.svelte";

  let view = $state<OverviewView | null>(null);
  let screen = $state<Screen>("agents");
  let error = $state<string | null>(null);
  let notice = $state<string | null>(null);
  let terminals = $state<TerminalApp[]>([]);
  /** The agent whose row is open, by id so a rename keeps it open. Nothing is open at start. */
  let open = $state<string | null>(null);
  /** Whether the open row has a change waiting in its save bar. */
  let dirty = $state(false);
  /** Bumped when a close or switch is refused because a change waits. */
  let nudge = $state(0);
  /** Bumped after each change so the open drawer starts fresh from the new state. */
  let round = $state(0);
  /** Ids seen so far: a new one after this app's Add agent gets opened. */
  let known: Set<string> | null = null;
  /** Set when Add agent finishes here, so an agent added from a terminal doesn't take over. */
  let added = false;

  /** Reads the saved state only: the 10-second poll never asks the tools for usage. */
  async function refresh() {
    try {
      show(await overview());
    } catch (e) {
      error = message(e);
    }
  }

  /** Refresh: asks every tool for usage now, then shows the answers. */
  async function checkUsage() {
    try {
      show(await refreshUsage());
    } catch (e) {
      error = message(e);
    }
  }

  function show(v: OverviewView) {
    const fresh = known && added ? v.agents.find((a) => !known!.has(a.id)) : undefined;
    known = new Set(v.agents.map((a) => a.id));
    view = v;
    if (fresh) {
      added = false;
      if (!dirty) open = fresh.id;
    }
    error = null;
  }

  function navigate(s: Screen, note?: string) {
    // Leaving the list would drop the change waiting in the open row.
    if (dirty && s !== "agents") {
      nudge += 1;
      return;
    }
    screen = s;
    notice = note ?? null;
    if (s === "agents") refresh();
  }

  /** Open a row, or close the open one. A waiting change keeps its row open. */
  function toggle(id: string) {
    if (dirty) {
      nudge += 1;
      return;
    }
    open = open === id ? null : id;
  }

  async function changed(note: string, op: OpRequest["op"]) {
    notice = note;
    if (op === "remove") {
      open = null;
      dirty = false;
    }
    await refresh();
    round += 1;
  }

  onMount(() => {
    document.documentElement.style.setProperty("--bar-h", chrome === "none" ? "0px" : "40px");
    findTerminals()
      .then((t) => (terminals = t ?? []))
      .catch(() => (terminals = []));
    const stop = startPoll(refresh, 10_000);
    const unlisten = listen<Screen>("navigate", (e) => navigate(e.payload));
    // The app asks for usage on start, every 5 minutes and after a sign-in; show each answer.
    const unchecked = listen("usage-checked", () => refresh());
    // Signing in inside an Agent Roster window: the drawer shows progress; a finished one updates the row.
    loadPrefs();
    const unsigned = listen<SignInUpdate>("sign-in", (e) => {
      onSignInUpdate(e.payload);
      if (e.payload.state === "done") refresh();
    });
    return () => {
      stop();
      unlisten.then((f) => f());
      unchecked.then((f) => f());
      unsigned.then((f) => f());
    };
  });
</script>

<div class="shell">
  <TopBar {chrome} />
  {#if notice}
    <div class="notice" role="status">
      <span>{notice}</span>
      <button class="link" onclick={() => (notice = null)}>Dismiss</button>
    </div>
  {/if}

  {#if error && !view}
    <div class="content"><div class="page"><p class="error">{error}</p></div></div>
  {:else if !view}
    <div class="content"><div class="page"><p class="lede">Looking at your agents…</p></div></div>
  {:else if view.agents.length === 0 && screen !== "add"}
    <div class="content"><FirstRun palette={view.palette} onDone={() => navigate("agents")} /></div>
  {:else}
    <div class="content">
      <div class="column">
        {#if screen === "help"}
          <Help {view} onBack={() => navigate("agents")} />
        {:else if screen === "changes"}
          <Changes onBack={() => navigate("agents")} />
        {:else if screen === "add"}
          <AddAgent
            palette={view.palette}
            nextColor={view.next_color}
            onDone={(note) => {
              added = true;
              navigate("agents", note);
            }}
            onCancel={() => navigate("agents")}
          />
        {:else}
          <AgentList
            {view}
            {open}
            palette={view.palette}
            {terminals}
            {round}
            {nudge}
            onToggle={toggle}
            onNavigate={(s) => navigate(s)}
            onRefresh={checkUsage}
            onSite={() => openSite().catch((e) => (notice = message(e)))}
            onApplied={changed}
            onDirty={(d) => {
              dirty = d;
              // The last notice would sit over the new change's save bar.
              if (d) notice = null;
            }}
            onClose={() => (open = null)}
          />
        {/if}
      </div>
    </div>
  {/if}
</div>

<style>
  .shell {
    height: 100%;
    display: flex;
    flex-direction: column;
  }
  .content {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
  }
  /* The content fills the window, like a native app: no centered page with empty sides. */
  .column {
    min-width: 0;
  }
  .notice {
    position: fixed;
    left: 50%;
    bottom: 16px;
    transform: translateX(-50%);
    display: flex;
    gap: 16px;
    align-items: center;
    background: var(--raise);
    border: 1px solid var(--line-2);
    border-radius: 8px;
    padding: 8px 16px;
    font-size: 13px;
    z-index: 40;
    max-width: 640px;
    box-shadow: var(--shadow);
  }
</style>
