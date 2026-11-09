<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import type { OverviewView, Screen } from "./lib/types";
  import { message, overview } from "./lib/api";
  import { startPoll } from "./lib/poll";
  import Agents from "./screens/Agents.svelte";

  let view = $state<OverviewView | null>(null);
  let screen = $state<Screen>("agents");
  let error = $state<string | null>(null);

  async function refresh() {
    try {
      view = await overview();
      error = null;
    } catch (e) {
      error = message(e);
    }
  }

  function navigate(s: Screen) {
    screen = s;
    if (s === "agents") refresh();
  }

  function limits(_name: string, _on: boolean) {}

  onMount(() => {
    const stop = startPoll(refresh, 10_000);
    const unlisten = listen<Screen>("navigate", (e) => navigate(e.payload));
    return () => {
      stop();
      unlisten.then((f) => f());
    };
  });
</script>

{#if error && !view}
  <div class="pad"><p class="error">{error}</p></div>
{:else if !view}
  <div class="pad"><p class="hello-s">Looking at your agents…</p></div>
{:else}
  <Agents {view} onNavigate={navigate} onLimits={limits} />
{/if}
