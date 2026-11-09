<script lang="ts">
  import type { OverviewView, Screen } from "../lib/types";
  import AgentRow from "../components/AgentRow.svelte";
  import Cable from "../components/Cable.svelte";
  import SidePanel from "../components/SidePanel.svelte";

  let {
    view,
    onNavigate,
    onLimits,
  }: {
    view: OverviewView;
    onNavigate: (s: Screen) => void;
    onLimits: (name: string, on: boolean) => void;
  } = $props();

  const order = $derived(view.agents.map((a) => a.id));
</script>

<div class="main">
  <div class="bay">
    <div class="head">
      <div>
        <h3>Your agents</h3>
        <p>{view.headline}</p>
      </div>
      <div class="actions">
        <button class="btn" onclick={() => onNavigate("changes")}>What changed</button>
        <button class="btn pri" onclick={() => onNavigate("add")}>Add agent</button>
      </div>
    </div>
    <div class="rows">
      {#each view.agents as agent (agent.id)}
        <AgentRow {agent} {onLimits} />
      {/each}
      <Cable calls={view.calls} {order} />
    </div>
  </div>
  <SidePanel {view} />
</div>

<style>
  .main {
    display: grid;
    grid-template-columns: 1fr 330px;
    min-height: 100%;
  }
  .bay {
    padding: 22px 0 18px 26px;
    position: relative;
    min-width: 0;
  }
  .head {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: 16px;
    padding-right: 26px;
    margin-bottom: 18px;
  }
  .head h3 {
    margin: 0;
    font-size: 22px;
    font-weight: 700;
    letter-spacing: -0.01em;
  }
  .head p {
    margin: 3px 0 0;
    color: var(--mut);
    font-size: 13px;
  }
  .actions {
    display: flex;
    gap: 8px;
    flex: none;
  }
  .rows {
    position: relative;
  }
</style>
