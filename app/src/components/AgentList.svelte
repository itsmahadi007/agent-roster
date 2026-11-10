<script lang="ts">
  import type { ColorView, OpRequest, OverviewView, Screen, TerminalApp } from "../lib/types";
  import AgentDrawer from "./AgentDrawer.svelte";
  import Updates from "./Updates.svelte";

  let {
    view,
    open,
    palette,
    terminals,
    round,
    nudge,
    onToggle,
    onNavigate,
    onRefresh,
    onSite,
    onApplied,
    onDirty,
    onClose,
  }: {
    view: OverviewView;
    /** The id of the agent whose row is open, or null. */
    open: string | null;
    palette: ColorView[];
    terminals: TerminalApp[];
    /** Bumped after each change so the open drawer starts fresh from the new state. */
    round: number;
    /** Bumped when a close or switch is refused because a change waits. */
    nudge: number;
    onToggle: (id: string) => void;
    onNavigate: (s: Screen) => void;
    onRefresh: () => Promise<void>;
    onSite: () => void;
    onApplied: (notice: string, op: OpRequest["op"]) => void;
    onDirty: (dirty: boolean) => void;
    onClose: () => void;
  } = $props();

  /** The weekly limit; core sends no other window. */
  const week = (a: OverviewView["agents"][number]) => a.windows.find((w) => w.label === "week") ?? null;

  let items: HTMLElement | undefined = $state();
  function move(e: KeyboardEvent, i: number) {
    const next = e.key === "ArrowDown" ? i + 1 : e.key === "ArrowUp" ? i - 1 : -1;
    if (next < 0 || next >= view.agents.length || (e.key !== "ArrowDown" && e.key !== "ArrowUp")) return;
    e.preventDefault();
    items?.querySelectorAll<HTMLButtonElement>("button.item")[next]?.focus();
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

  <div class="items" bind:this={items}>
    {#each view.agents as a, i (a.id)}
      {@const w = week(a)}
      {@const isOpen = a.id === open}
      <button
        class="item"
        class:open={isOpen}
        aria-expanded={isOpen}
        onclick={() => onToggle(a.id)}
        onkeydown={(e) => move(e, i)}
      >
        <span class="sq" style:background={a.color}></span>
        <span class="who">
          <span class="nm agent-name ink" style:color={a.color} style:--c={a.color}>{a.name}</span>
          <span class="meta">{a.role ? `${a.role} · ` : ""}{a.tool_label} · {a.status_text}</span>
        </span>
        <span class="use">
          {#if w && a.health === "limit-reached"}
            <!-- Which limit is used up and its own reset: a 5-hour limit resets long before the week. -->
            <span class="line">
              <span class="pc num hot">{a.limits_note ?? "limit reached"}</span>
            </span>
            <span class="track"><i style:width="{w.used_percent}%" style:background="var(--coral)"></i></span>
          {:else if w}
            <span class="line">
              <span class="pc num" class:hot={w.hot}>{w.used_percent}% week</span>
              {#if a.reset_text}<span class="rs num" class:hot={w.hot}>{a.reset_text}</span>{/if}
            </span>
            <span class="track"><i style:width="{w.used_percent}%" style:background={w.hot ? "var(--coral)" : a.color}></i></span>
            <span class="remaining num" class:hot={w.hot}>{Math.max(0, Math.min(100, 100 - w.used_percent))}% remaining</span>
            {#if a.as_of}<span class="asof">{a.as_of}</span>{/if}
          {:else if a.health === "sign-in"}
            <span class="pc dim">sign in</span>
          {:else if a.health === "limit-reached"}
            <span class="pc hot">{a.limits_note ?? "limit reached"}</span>
          {:else if a.health !== "ok" && a.limits_note}
            <span class="pc dim">{a.limits_note}</span>
          {:else}
            <span class="pc dim">not used yet</span>
          {/if}
        </span>
        <svg class="chev" width="12" height="12" viewBox="0 0 12 12" aria-hidden="true"><path d="M4.5 2.5 8 6l-3.5 3.5" /></svg>
      </button>
      {#if isOpen}
        {#key `${a.id}:${round}`}
          <AgentDrawer agent={a} {palette} {terminals} {nudge} {onApplied} {onDirty} {onClose} />
        {/key}
      {/if}
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
          title="Asks Claude Code and Codex for every agent's use now. Agent Roster also does this every 5 minutes."
          onclick={refresh}>{checking ? "Checking…" : "Refresh"}</button
        >
      {/if}
    </div>
    <div class="row links">
      <button class="link" onclick={() => onNavigate("help")}>How it works</button>
      <button class="link" onclick={() => onNavigate("changes")}>What changed</button>
    </div>
    <Updates />
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
    display: flex;
    flex-direction: column;
    padding: 0 24px;
  }
  .lh {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 24px 0 12px;
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
    display: flex;
    flex-direction: column;
  }
  .item {
    display: grid;
    grid-template-columns: 10px minmax(0, 1fr) minmax(150px, 220px) 12px;
    align-items: center;
    gap: 14px;
    min-height: 64px;
    padding: 12px 14px;
    margin-bottom: 8px;
    border: 1px solid var(--line);
    border-radius: 10px;
    background: var(--raise);
    color: var(--txt);
    text-align: left;
    transition: border-color 150ms var(--ease);
  }
  .item:hover {
    border-color: var(--line-2);
  }
  .item.open {
    border-color: var(--line-2);
    border-bottom-left-radius: 0;
    border-bottom-right-radius: 0;
    margin-bottom: 0;
  }
  .item:focus-visible {
    outline: 2px solid var(--txt);
    outline-offset: 2px;
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
    font-size: 17px;
    line-height: 22px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .meta {
    font-size: 12px;
    line-height: 16px;
    color: var(--mut);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .use {
    display: flex;
    flex-direction: column;
    align-items: stretch;
    gap: 6px;
    min-width: 0;
  }
  .line {
    display: flex;
    justify-content: space-between;
    gap: 8px;
    font-size: 12px;
    white-space: nowrap;
  }
  .pc {
    font-weight: 500;
    color: var(--txt);
  }
  .pc.dim {
    font-weight: 400;
    color: var(--dim);
    text-align: right;
  }
  .rs {
    color: var(--mut);
  }
  .remaining {
    color: var(--mut);
    font-size: 12px;
    line-height: 16px;
  }
  .asof {
    font-size: 11px;
    line-height: 12px;
    color: var(--dim);
    text-align: right;
  }
  .track {
    display: block;
    height: 5px;
    border-radius: 3px;
    background: var(--hole);
    overflow: hidden;
  }
  .track i {
    display: block;
    height: 100%;
  }
  .chev {
    fill: none;
    stroke: var(--mut);
    stroke-width: 1.5;
    stroke-linecap: round;
    stroke-linejoin: round;
    transition: transform 150ms var(--ease);
  }
  .item.open .chev {
    transform: rotate(90deg);
  }
  .foot {
    border-top: 1px solid var(--line);
    margin-top: 16px;
    padding: 16px 0 24px;
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
    margin-top: 0 !important;
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
