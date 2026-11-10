<script lang="ts">
  import type { OverviewView, Screen } from "../lib/types";

  let {
    view,
    selected,
    onSelect,
    onNavigate,
    onRefresh,
    onSite,
  }: {
    view: OverviewView;
    /** The id of the agent whose page is open, or null on other screens. */
    selected: string | null;
    onSelect: (id: string) => void;
    onNavigate: (s: Screen) => void;
    onRefresh: () => Promise<void>;
    onSite: () => void;
  } = $props();

  const week = (a: OverviewView["agents"][number]) => a.windows.find((w) => w.label === "week") ?? a.windows.at(-1) ?? null;

  function move(e: KeyboardEvent, i: number) {
    const next = e.key === "ArrowDown" ? i + 1 : e.key === "ArrowUp" ? i - 1 : -1;
    if (next < 0 || next >= view.agents.length || (e.key !== "ArrowDown" && e.key !== "ArrowUp")) return;
    e.preventDefault();
    onSelect(view.agents[next].id);
    const items = (e.currentTarget as HTMLElement).parentElement?.querySelectorAll<HTMLButtonElement>("button.item");
    items?.[next]?.focus();
  }

  let checking = $state(false);
  let checked = $state(false);
  let timer: ReturnType<typeof setTimeout> | undefined;
  async function refresh() {
    checking = true;
    checked = false;
    clearTimeout(timer);
    try {
      await onRefresh();
    } finally {
      checking = false;
      checked = true;
      timer = setTimeout(() => (checked = false), 4000);
    }
  }
</script>

<nav class="list" aria-label="Agents">
  <div class="lh">
    <h3>Your agents<span class="count num" aria-hidden="true">{view.agents.length}</span></h3>
    <button class="btn sm pri" onclick={() => onNavigate("add")}>Add agent</button>
  </div>

  <div class="items">
    {#each view.agents as a, i (a.id)}
      {@const w = week(a)}
      <button
        class="item"
        aria-current={a.id === selected ? "true" : undefined}
        class:sel={a.id === selected}
        onclick={() => onSelect(a.id)}
        onkeydown={(e) => move(e, i)}
      >
        <span class="sq" style:background={a.color}></span>
        <span class="who">
          <span class="nm agent-name ink" style:color={a.color} style:--c={a.color}>{a.name}</span>
          <span class="meta">{a.role ? `${a.role} · ` : ""}{a.tool_label}</span>
        </span>
        <span class="use">
          {#if w}
            <span class="pc num" class:hot={w.hot}>{w.used_percent}% week</span>
            <span class="track"><i style:width="{w.used_percent}%" style:background={w.hot ? "var(--coral)" : a.color}></i></span>
          {:else if a.state === "sign-in"}
            <span class="pc dim">sign in</span>
          {:else}
            <span class="pc dim">not used yet</span>
          {/if}
        </span>
      </button>
    {/each}
  </div>

  <div class="foot">
    {#each view.warnings as w, i (i)}
      <p class="warn"><b class="agent-name hot">{w.name}</b> {w.text}</p>
    {/each}
    {#each view.calls as c, i (i)}
      <p class="call" style:border-color={c.from_color}>
        <b class="agent-name ink" style:color={c.from_color} style:--c={c.from_color}>{c.from_name}</b> asked
        <b class="agent-name ink" style:color={c.to_color} style:--c={c.to_color}>{c.to_name}</b>
        <span>{c.detail}</span>
      </p>
    {:else}
      <p class="quiet">Nobody is on the line</p>
    {/each}
    <div class="row">
      <span class="quiet">{view.limits_updated ?? ""}</span>
      {#if checked}
        <span class="ok" role="status">Checked just now</span>
      {:else}
        <button
          class="link"
          disabled={checking}
          title="Reads every agent's latest numbers again. The window also does this every 10 seconds."
          onclick={refresh}>{checking ? "Checking…" : "Refresh"}</button
        >
      {/if}
    </div>
    <div class="row links">
      <button class="link" onclick={() => onNavigate("help")}>How it works</button>
      <button class="link" onclick={() => onNavigate("changes")}>What changed</button>
    </div>
    <p class="credit">
      Created by Mahadi Hassan ·
      <a
        href="https://mahadihassan.com"
        onclick={(e) => {
          e.preventDefault();
          onSite();
        }}>mahadihassan.com</a
      >
    </p>
  </div>
</nav>

<style>
  .list {
    height: 100%;
    min-height: 0;
    display: flex;
    flex-direction: column;
    background: var(--well);
    border-right: 1px solid var(--line);
  }
  .lh {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 20px 20px 12px;
  }
  h3 {
    margin: 0;
    font-size: 18px;
    line-height: 24px;
    font-weight: 600;
  }
  .count {
    margin-left: 8px;
    font-size: 13px;
    font-weight: 400;
    color: var(--mut);
  }
  .items {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 0 12px 8px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .item {
    display: grid;
    grid-template-columns: 10px minmax(0, 1fr) auto;
    align-items: center;
    gap: 12px;
    min-height: 58px;
    padding: 8px 12px;
    border: 0;
    border-radius: 8px;
    background: none;
    color: var(--txt);
    text-align: left;
    transition: background-color 150ms var(--ease);
  }
  .item:hover {
    background: var(--raise);
  }
  .item.sel {
    background: var(--raise);
    box-shadow: inset 0 0 0 1px var(--line-2);
  }
  .item:focus-visible {
    outline: 2px solid var(--txt);
    outline-offset: -2px;
  }
  .sq {
    width: 10px;
    height: 10px;
    border-radius: 3px;
  }
  .who {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .nm {
    font-size: 16px;
    line-height: 20px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .meta {
    font-size: 11px;
    line-height: 16px;
    color: var(--mut);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .use {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 4px;
    min-width: 72px;
  }
  .pc {
    font-size: 12px;
    color: var(--mut);
    white-space: nowrap;
  }
  .pc.dim {
    color: var(--dim);
  }
  .track {
    display: block;
    width: 64px;
    height: 3px;
    border-radius: 2px;
    background: var(--hole);
    overflow: hidden;
  }
  .track i {
    display: block;
    height: 100%;
  }
  .foot {
    border-top: 1px solid var(--line);
    padding: 12px 20px 16px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-size: 12px;
    line-height: 16px;
  }
  .foot p {
    margin: 0;
  }
  .warn {
    color: var(--mut);
  }
  .call {
    border-left: 2px solid;
    padding-left: 8px;
    color: var(--mut);
  }
  .call span {
    display: block;
    color: var(--dim);
  }
  .quiet {
    color: var(--dim);
  }
  .row {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: 8px;
  }
  .row.links {
    justify-content: flex-start;
    gap: 16px;
    margin-top: 2px;
  }
  .ok {
    color: var(--jade);
  }
  .credit {
    margin-top: 6px !important;
    color: var(--dim);
    font-size: 11px;
  }
  .credit a {
    color: var(--mut);
    text-decoration: underline;
    text-decoration-color: var(--line-2);
    text-underline-offset: 3px;
  }
  .credit a:hover {
    color: var(--txt);
  }
</style>
