<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import type { OpRequest, OverviewView, Screen, TerminalApp } from "./lib/types";
  import { message, openSite, overview, terminals as findTerminals } from "./lib/api";
  import { startPoll } from "./lib/poll";
  import AddAgent from "./screens/AddAgent.svelte";
  import FirstRun from "./screens/FirstRun.svelte";
  import Changes from "./screens/Changes.svelte";
  import Help from "./screens/Help.svelte";
  import AgentList from "./components/AgentList.svelte";
  import AgentPage from "./components/AgentPage.svelte";
  import TopBar from "./components/TopBar.svelte";
  import { chrome } from "./lib/platform";

  let view = $state<OverviewView | null>(null);
  let screen = $state<Screen>("agents");
  let error = $state<string | null>(null);
  let notice = $state<string | null>(null);
  let terminals = $state<TerminalApp[]>([]);
  /** The agent whose page is open, by id so a rename keeps it open. */
  let selected = $state<string | null>(null);
  /** Bumped after each change so the page starts fresh from the new state. */
  let round = $state(0);
  /** Ids seen so far: a new one after Add agent gets selected. */
  let known: Set<string> | null = null;

  const current = $derived(view?.agents.find((a) => a.id === selected) ?? view?.agents[0] ?? null);

  async function refresh() {
    try {
      const v = await overview();
      const fresh = known ? v.agents.find((a) => !known!.has(a.id)) : undefined;
      known = new Set(v.agents.map((a) => a.id));
      view = v;
      if (fresh) selected = fresh.id;
      error = null;
    } catch (e) {
      error = message(e);
    }
  }

  function navigate(s: Screen, note?: string) {
    screen = s;
    notice = note ?? null;
    if (s === "agents") refresh();
  }

  function select(id: string) {
    selected = id;
    screen = "agents";
  }

  async function changed(note: string, op: OpRequest["op"]) {
    notice = note;
    if (op === "remove" && view && current) {
      // Open the agent above the removed one (or below, if it was first).
      const gone = current.id;
      const i = view.agents.findIndex((a) => a.id === gone);
      const next = view.agents[i - 1] ?? view.agents[i + 1] ?? null;
      selected = next?.id ?? null;
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
    return () => {
      stop();
      unlisten.then((f) => f());
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
    <div class="body">
      <AgentList
        {view}
        selected={screen === "agents" ? (current?.id ?? null) : null}
        onSelect={select}
        onNavigate={(s) => navigate(s)}
        onRefresh={refresh}
        onSite={() => openSite().catch((e) => (notice = message(e)))}
      />
      <main class="right">
        {#if screen === "help"}
          <div class="pane"><Help {view} onBack={() => navigate("agents")} /></div>
        {:else if screen === "changes"}
          <div class="pane"><Changes onBack={() => navigate("agents")} /></div>
        {:else if screen === "add"}
          <div class="pane">
            <AddAgent
              palette={view.palette}
              nextColor={view.next_color}
              onDone={(note) => navigate("agents", note)}
              onCancel={() => navigate("agents")}
            />
          </div>
        {:else if current}
          {#key `${current.id}:${round}`}
            <AgentPage agent={current} palette={view.palette} {terminals} onApplied={changed} />
          {/key}
        {/if}
      </main>
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
  .body {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: 320px minmax(0, 1fr);
  }
  .right {
    min-width: 0;
    min-height: 0;
  }
  .pane {
    height: 100%;
    overflow-y: auto;
  }
  .notice {
    position: fixed;
    left: calc(50% + 160px);
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
