<script lang="ts">
  import { onMount } from "svelte";
  import type { ChangeView } from "../lib/types";
  import { changes, message, undo, undoAll } from "../lib/api";
  import PageHead from "../components/PageHead.svelte";

  let { onBack }: { onBack: () => void } = $props();

  let list = $state<ChangeView[] | null>(null);
  let error = $state<string | null>(null);
  let asking = $state<{ text: string; run: () => Promise<unknown> } | null>(null);
  let busy = $state(false);

  const day = new Intl.DateTimeFormat(undefined, { dateStyle: "medium" });
  const time = new Intl.DateTimeFormat(undefined, { timeStyle: "short" });
  function when(at: number) {
    const d = new Date(at * 1000);
    const today = new Date().toDateString() === d.toDateString();
    return `${today ? "today" : day.format(d)} at ${time.format(d)}`;
  }

  async function load() {
    try {
      list = await changes();
    } catch (e) {
      error = message(e);
    }
  }

  function ask(text: string, run: () => Promise<unknown>) {
    error = null;
    asking = { text, run };
  }

  async function confirm() {
    if (!asking) return;
    busy = true;
    try {
      await asking.run();
    } catch (e) {
      error = message(e);
    } finally {
      busy = false;
      asking = null;
      await load();
    }
  }

  onMount(load);
</script>

<div class="page">
  <PageHead title="Everything Agent Roster did" sub="Each change has a backup. Undo one, or undo all of them." {onBack} />

  {#if list}
    {#each list as c (c.id)}
      <div class="found" data-change={c.id}>
        <span class="dot" style:background={c.color ?? "var(--mut)"}></span>
        <span class="meta">
          <span class="what" class:gone={c.undone}>{c.summary}</span><br />
          <span class="dim num">{when(c.at)}</span>
        </span>
        {#if c.undone}
          <span class="dim">undone</span>
        {:else}
          <button class="btn sm" onclick={() => ask(`Undo change ${c.id}: ${c.summary}?`, () => undo(c.id))} disabled={busy}>Undo</button>
        {/if}
      </div>
    {:else}
      <p class="lede">Agent Roster hasn't changed anything yet</p>
    {/each}
    <div class="found">
      <span class="dot" style:background="var(--jade)"></span>
      <span class="meta"><span class="what">Hooks added to your tools</span><br /><span class="jade">none, ever</span></span>
    </div>
  {/if}

  {#if asking}
    <div class="ask" role="alertdialog" aria-label={asking.text}>
      <span>{asking.text}</span>
      <div class="foot">
        <button class="btn sm" onclick={() => (asking = null)} disabled={busy}>Cancel</button>
        <button class="btn sm pri" onclick={confirm} disabled={busy}>Yes, undo</button>
      </div>
    </div>
  {/if}
  {#if error}<p class="error">{error}</p>{/if}

  <div class="foot bottom">
    <span class="dim">To remove Agent Roster and put all of this back, run <code class="mono">roster uninstall</code> in a terminal.</span>
    {#if list?.some((c) => !c.undone)}
      <button class="btn" onclick={() => ask("Undo every change Agent Roster made?", () => undoAll())} disabled={busy}>Undo everything</button>
    {/if}
  </div>
</div>

<style>
  .found {
    display: flex;
    align-items: center;
    gap: 16px;
    min-height: 68px;
    padding: 12px 0;
    border-bottom: 1px solid var(--line);
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    flex: none;
  }
  .meta {
    flex: 1;
    font-size: 13px;
    line-height: 20px;
  }
  .what {
    color: var(--txt);
    user-select: text;
  }
  .gone {
    color: var(--mut);
    text-decoration: line-through;
  }
  .dim {
    color: var(--dim);
    font-size: 12px;
  }
  .jade {
    color: var(--jade);
  }
  .ask {
    margin-top: 16px;
    background: var(--well);
    border: 1px solid var(--line);
    border-radius: 8px;
    padding: 12px 16px;
    font-size: 13px;
  }
  .ask .foot {
    margin-top: 12px;
  }
  .bottom {
    justify-content: space-between;
    align-items: center;
  }
</style>
