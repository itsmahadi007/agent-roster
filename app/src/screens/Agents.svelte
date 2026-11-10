<script lang="ts">
  import type { OverviewView, Screen } from "../lib/types";
  import AgentRow from "../components/AgentRow.svelte";
  import Cable from "../components/Cable.svelte";
  import SidePanel from "../components/SidePanel.svelte";

  let {
    view,
    onNavigate,
    onLimits,
    onEdit = () => {},
  }: {
    view: OverviewView;
    onNavigate: (s: Screen) => void;
    onLimits: (name: string, on: boolean) => void;
    onEdit?: (name: string) => void;
  } = $props();

  const order = $derived(view.agents.map((a) => a.id));
  const n = $derived(view.agents.length);

  // Only the list scrolls. The cable lives outside it (it loops into the side panel),
  // so it follows the list by the scroll offset.
  let list = $state<HTMLDivElement | null>(null);
  let top = $state(0);
  let more = $state(false);
  function onScroll() {
    if (!list) return;
    top = list.scrollTop;
    more = list.scrollHeight - list.scrollTop - list.clientHeight > 4;
  }
  $effect(() => {
    void n;
    if (!list) return;
    onScroll();
    if (typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver(onScroll);
    ro.observe(list);
    return () => ro.disconnect();
  });
</script>

<div class="main">
  <div class="bay">
    <div class="head">
      <div>
        <h3>Your agents{#if n > 1}<span class="count num" aria-hidden="true" title="{n} agents">{n}</span>{/if}</h3>
        <p>{view.headline}</p>
      </div>
      <div class="actions">
        <button class="link" onclick={() => onNavigate("help")}>How it works</button>
        <button class="btn" onclick={() => onNavigate("changes")}>What changed</button>
        <button class="btn pri" onclick={() => onNavigate("add")}>Add agent</button>
      </div>
    </div>
    <div class="listwrap">
      <div class="list" class:more bind:this={list} onscroll={onScroll}>
        {#each view.agents as agent (agent.id)}
          <AgentRow {agent} {onLimits} {onEdit} />
        {/each}
        {#if n === 1}
          <div class="invite">
            <div>
              <b>Add a second account</b>
              <span>
                When {view.agents[0].name} runs low on its limits, carry on with the other one. Each agent keeps its own login,
                and both can run at the same time.
              </span>
            </div>
            <button class="btn" aria-label="Add a second agent" onclick={() => onNavigate("add")}>Add agent</button>
          </div>
        {/if}
      </div>
      <div class="lane" aria-hidden="true">
        <div class="track" style:transform="translateY({-top}px)">
          <Cable calls={view.calls} {order} />
        </div>
      </div>
    </div>
    <p class="tip">
      Click a name to rename it, change its color or role, make it the default or remove it. Hover a bar, a status or a jack to
      see what it means.
    </p>
  </div>
  <SidePanel {view} />
</div>

<style>
  .main {
    display: grid;
    grid-template-columns: 1fr 330px;
    height: 100%;
    min-height: 0;
  }
  .bay {
    padding: 24px 0 0 24px;
    position: relative;
    min-width: 0;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  .head {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: 16px;
    padding-right: 24px;
    margin-bottom: 16px;
  }
  .head h3 {
    margin: 0;
    font-size: 22px;
    line-height: 28px;
    font-weight: 600;
  }
  .count {
    margin-left: 8px;
    font-size: 14px;
    font-weight: 400;
    color: var(--mut);
  }
  .head p {
    margin: 4px 0 0;
    color: var(--mut);
    font-size: 13px;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 8px;
    flex: none;
  }
  .actions .link {
    margin-right: 8px;
  }
  .listwrap {
    position: relative;
    flex: 1;
    min-height: 0;
  }
  .list {
    height: 100%;
    overflow-y: auto;
    /* No scrollbar: it would sit between the jacks and the cable. The fade says there's more. */
    scrollbar-width: none;
    padding-bottom: 8px;
  }
  .list::-webkit-scrollbar {
    display: none;
  }
  .list.more {
    mask-image: linear-gradient(to bottom, #000 calc(100% - 40px), transparent);
  }
  .lane {
    position: absolute;
    inset: 0 -60px 0 0;
    overflow: hidden;
    pointer-events: none;
  }
  .track {
    position: relative;
    margin-right: 60px;
  }
  .invite {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    align-items: center;
    gap: 16px;
    min-height: 76px;
    padding: 12px 20px;
    margin-right: 24px;
    border: 1px dashed var(--line-2);
    border-radius: 12px;
    font-size: 13px;
    line-height: 20px;
    color: var(--mut);
  }
  .invite b {
    display: block;
    color: var(--txt);
    font-weight: 500;
    font-size: 14px;
  }
  .tip {
    margin: 0;
    padding: 12px 24px 16px 0;
    border-top: 1px solid var(--line);
    font-size: 12px;
    line-height: 16px;
    color: var(--dim);
  }
</style>
