<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import type { OpRequest, OverviewView, PlanView, Screen } from "./lib/types";
  import { message, overview, preview } from "./lib/api";
  import { startPoll } from "./lib/poll";
  import Agents from "./screens/Agents.svelte";
  import AddAgent from "./screens/AddAgent.svelte";
  import FirstRun from "./screens/FirstRun.svelte";
  import Changes from "./screens/Changes.svelte";
  import PlanSheet from "./components/PlanSheet.svelte";
  import AgentSheet from "./components/AgentSheet.svelte";
  import TopBar from "./components/TopBar.svelte";
  import Help from "./screens/Help.svelte";
  import { chrome } from "./lib/platform";

  let view = $state<OverviewView | null>(null);
  let screen = $state<Screen>("agents");
  let error = $state<string | null>(null);
  let notice = $state<string | null>(null);
  let sheet = $state<{ plan: PlanView; req: OpRequest; label: string } | null>(null);
  /** The agent whose settings are open, by id so a rename keeps it open. */
  let editing = $state<string | null>(null);
  /** Bumped after each change so the settings sheet starts fresh from the new state. */
  let editRound = $state(0);
  const edited = $derived(view?.agents.find((a) => a.id === editing) ?? null);

  async function refresh() {
    try {
      view = await overview();
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

  async function limits(name: string, on: boolean) {
    notice = null;
    try {
      const req: OpRequest = on ? { op: "enable-limits", name } : { op: "disable-limits", name };
      sheet = { plan: await preview(req), req, label: on ? "Show usage" : "Hide usage" };
    } catch (e) {
      notice = message(e);
    }
  }

  function edit(name: string) {
    notice = null;
    editing = view?.agents.find((x) => x.name === name)?.id ?? null;
  }

  async function changed(note: string, op: OpRequest["op"]) {
    notice = note;
    if (op === "remove") editing = null;
    await refresh();
    editRound += 1;
  }

  function sheetDone() {
    sheet = null;
    refresh();
  }

  onMount(() => {
    document.documentElement.style.setProperty("--bar-h", chrome === "none" ? "0px" : "40px");
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
<div class="content" class:fixed={view && view.agents.length > 0 && screen === "agents"}>
{#if notice}
  <div class="notice" role="status">
    <span>{notice}</span>
    <button class="link" onclick={() => (notice = null)}>Dismiss</button>
  </div>
{/if}

{#if error && !view}
  <div class="page"><p class="error">{error}</p></div>
{:else if !view}
  <div class="page"><p class="lede">Looking at your agents…</p></div>
{:else if view.agents.length === 0 && screen === "agents"}
  <FirstRun palette={view.palette} onDone={() => navigate("agents")} />
{:else if screen === "help"}
  <Help {view} onBack={() => navigate("agents")} />
{:else if screen === "changes"}
  <Changes onBack={() => navigate("agents")} />
{:else if screen === "add"}
  <AddAgent
    palette={view.palette}
    nextColor={view.next_color}
    onDone={(note) => navigate("agents", note)}
    onCancel={() => navigate("agents")}
  />
{:else}
  <Agents {view} onNavigate={(s) => navigate(s)} onLimits={limits} onEdit={edit} onRefresh={refresh} />
{/if}
</div>
</div>

{#if edited && view}
  {#key `${edited.id}:${editRound}`}
    <AgentSheet agent={edited} palette={view.palette} onApplied={changed} onClose={() => (editing = null)} />
  {/key}
{/if}

{#if sheet}
  <PlanSheet plan={sheet.plan} req={sheet.req} confirmLabel={sheet.label} onDone={sheetDone} onCancel={() => (sheet = null)} />
{/if}

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
  /* Your agents scrolls its own list; the rest of that screen stays put. */
  .content.fixed {
    overflow: hidden;
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
  }
</style>
