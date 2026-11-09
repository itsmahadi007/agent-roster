<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import type { OverviewView, PlanView, Screen } from "./lib/types";
  import { message, overview, preview } from "./lib/api";
  import { startPoll } from "./lib/poll";
  import Agents from "./screens/Agents.svelte";
  import AddAgent from "./screens/AddAgent.svelte";
  import PlanSheet from "./components/PlanSheet.svelte";

  let view = $state<OverviewView | null>(null);
  let screen = $state<Screen>("agents");
  let error = $state<string | null>(null);
  let notice = $state<string | null>(null);
  let sheet = $state<{ plan: PlanView; label: string } | null>(null);

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
      const plan = await preview(on ? { op: "enable-limits", name } : { op: "disable-limits", name });
      sheet = { plan, label: on ? "Turn on" : "Turn off" };
    } catch (e) {
      notice = message(e);
    }
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
  <div class="pad"><p class="error">{error}</p></div>
{:else if !view}
  <div class="pad"><p class="hello-s">Looking at your agents…</p></div>
{:else if screen === "add"}
  <AddAgent
    palette={view.palette}
    nextColor={view.next_color}
    onDone={(note) => navigate("agents", note)}
    onCancel={() => navigate("agents")}
  />
{:else}
  <Agents {view} onNavigate={(s) => navigate(s)} onLimits={limits} />
{/if}

{#if sheet}
  <PlanSheet plan={sheet.plan} confirmLabel={sheet.label} onDone={sheetDone} onCancel={() => (sheet = null)} />
{/if}

<style>
  .notice {
    position: fixed;
    left: 50%;
    bottom: 18px;
    transform: translateX(-50%);
    display: flex;
    gap: 14px;
    align-items: center;
    background: var(--raise);
    border: 1px solid #33414f;
    border-radius: 10px;
    padding: 10px 14px;
    font-size: 13px;
    z-index: 20;
    max-width: 640px;
  }
</style>
