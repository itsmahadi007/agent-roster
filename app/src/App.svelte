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
  import RenameSheet from "./components/RenameSheet.svelte";
  import Help from "./screens/Help.svelte";

  let view = $state<OverviewView | null>(null);
  let screen = $state<Screen>("agents");
  let error = $state<string | null>(null);
  let notice = $state<string | null>(null);
  let sheet = $state<{ plan: PlanView; req: OpRequest; label: string } | null>(null);
  let renaming = $state<{ name: string; color: string } | null>(null);

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
      sheet = { plan: await preview(req), req, label: on ? "Turn on" : "Turn off" };
    } catch (e) {
      notice = message(e);
    }
  }

  function rename(name: string) {
    notice = null;
    const a = view?.agents.find((x) => x.name === name);
    renaming = { name, color: a?.color ?? "var(--txt)" };
  }

  function renamed(newName: string) {
    const old = renaming?.name;
    renaming = null;
    notice = `${old} is now ${newName}. Type ${newName} in any terminal to start it.`;
    refresh();
  }

  function sheetDone() {
    sheet = null;
    refresh();
  }

  onMount(() => {
    const stop = startPoll(refresh, 10_000);
    const unlisten = listen<Screen>("navigate", (e) => navigate(e.payload));
    return () => {
      stop();
      unlisten.then((f) => f());
    };
  });
</script>

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
  <Agents {view} onNavigate={(s) => navigate(s)} onLimits={limits} onRename={rename} />
{/if}

{#if renaming}
  <RenameSheet name={renaming.name} color={renaming.color} onDone={renamed} onCancel={() => (renaming = null)} />
{/if}

{#if sheet}
  <PlanSheet plan={sheet.plan} req={sheet.req} confirmLabel={sheet.label} onDone={sheetDone} onCancel={() => (sheet = null)} />
{/if}

<style>
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
    z-index: 20;
    max-width: 640px;
  }
</style>
