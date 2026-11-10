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
  import AgentPanel from "./components/AgentPanel.svelte";
  import Overlay from "./components/Overlay.svelte";
  import TopBar from "./components/TopBar.svelte";
  import { agentHex } from "./lib/colors";
  import { chrome } from "./lib/platform";
  import { loadPrefs, onSignInUpdate, type SignInUpdate } from "./lib/signin.svelte";

  let view = $state<OverviewView | null>(null);
  let screen = $state<Screen>("agents");
  let error = $state<string | null>(null);
  let notice = $state<string | null>(null);
  let terminals = $state<TerminalApp[]>([]);
  /** The list is the only page: `screen` is what opens over it (Add agent, Help, What changed). */
  /** The agent whose panel is open, by id so a rename keeps it open. Nothing is open at start. */
  let open = $state<string | null>(null);
  /** Whether the open row has a change waiting in its save bar. */
  let dirty = $state(false);
  /** Bumped when a close or switch is refused because a change waits. */
  let nudge = $state(0);
  /** Bumped after each change so the open panel starts fresh from the new state. */
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
    // Opening something else would drop the change waiting in the open panel.
    if (dirty && s !== "agents") {
      nudge += 1;
      return;
    }
    // One thing over the list at a time.
    if (s !== "agents") open = null;
    screen = s;
    notice = note ?? null;
    if (s === "agents") refresh();
  }

  /** Open an agent's panel, or close it. A waiting change keeps its panel open. */
  function toggle(id: string) {
    if (dirty) {
      nudge += 1;
      return;
    }
    screen = "agents";
    open = open === id ? null : id;
  }

  /** ✕, Escape or a click on the list. A waiting change keeps the panel open. */
  function closePanel() {
    if (dirty) {
      nudge += 1;
      return;
    }
    open = null;
  }

  const openAgent = $derived(view && open ? (view.agents.find((a) => a.id === open) ?? null) : null);

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
    // Signing in inside an Agent Roster window: the agent panel shows progress; a finished one updates the row.
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
  {:else}
    <div class="content">
      {#if view.agents.length === 0}
        <FirstRun palette={view.palette} onDone={() => navigate("agents")} />
      {:else}
        <div class="column">
          <AgentList {view} {open} onToggle={toggle} onNavigate={(s) => navigate(s)} onRefresh={checkUsage} onSite={() => openSite().catch((e) => (notice = message(e)))} />
        </div>
      {/if}
    </div>

    <!-- Everything else opens over the list and closes with ✕, Escape or a click on the list. -->
    {#if screen === "add"}
      <AddAgent
        palette={view.palette}
        nextColor={view.next_color}
        onDone={(note) => {
          added = true;
          navigate("agents", note);
        }}
        onCancel={() => navigate("agents")}
      />
    {:else if screen === "help"}
      <Overlay kind="panel" label="How Agent Roster works" sub="A short tour of this window, in the order you'll need it" onClose={() => navigate("agents")}>
        <Help {view} />
      </Overlay>
    {:else if screen === "changes"}
      <Overlay
        kind="panel"
        label="Everything Agent Roster did"
        sub="Each change has a backup. Undo one, or undo all of them."
        onClose={() => navigate("agents")}
      >
        <Changes />
      </Overlay>
    {:else if openAgent}
      {@const a = openAgent}
      {@const c = agentHex(a.color)}
      {@const live = a.state === "working" || a.live}
      <Overlay kind="panel" label={a.name} onClose={closePanel} onEscape={null}>
        {#snippet head()}
          <div class="ah">
            <span class="sq" style:background={c}></span>
            <div class="ahw">
              <h2 class="agent-name ink" style:color={c} style:--c={c}>{a.name}</h2>
              <p class="st">
                {a.tool_label} ·
                <span class:ink={live} style:--c={c} style:color={live ? c : null}>{a.status_text}</span>
              </p>
            </div>
          </div>
        {/snippet}
        {#key `${a.id}:${round}`}
          <AgentPanel
            agent={a}
            palette={view.palette}
            {terminals}
            updated={view.limits_updated}
            {nudge}
            onApplied={changed}
            onDirty={(d) => {
              dirty = d;
              // The last notice would sit over the new change's save bar.
              if (d) notice = null;
            }}
            onClose={closePanel}
          />
        {/key}
      </Overlay>
    {/if}
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
  .ah {
    display: flex;
    gap: 10px;
    align-items: flex-start;
  }
  .sq {
    flex: none;
    width: 10px;
    height: 10px;
    border-radius: 3px;
    margin-top: 8px;
  }
  .ahw {
    min-width: 0;
  }
  .ahw h2 {
    margin: 0;
    font-size: 19px;
    line-height: 26px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .st {
    margin: 0;
    font-size: 12px;
    line-height: 16px;
    color: var(--mut);
  }
</style>
