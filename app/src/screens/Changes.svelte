<script lang="ts">
  import { onMount } from "svelte";
  import type { ChangeView } from "../lib/types";
  import { changes, message, undo, undoAll } from "../lib/api";

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

<div class="pad changes">
  <button class="link back" onclick={onBack}>Back to your agents</button>
  <div class="hello" style:font-size="22px">Everything Agent Roster did</div>
  <p class="hello-s">Each change has a backup. Undo one, or undo all of them.</p>

  {#if list}
    {#each list as c (c.id)}
      <div class="found" data-change={c.id}>
        <span class="dot" style:background={c.color ?? "var(--mut)"}></span>
        <span class="meta">
          <span class="what" class:gone={c.undone}>{c.summary}</span><br />
          <span class="dim">{when(c.at)}</span>
        </span>
        {#if c.undone}
          <span class="dim">undone</span>
        {:else}
          <button class="btn sm" onclick={() => ask(`Undo change ${c.id}: ${c.summary}?`, () => undo(c.id))} disabled={busy}>Undo</button>
        {/if}
      </div>
    {:else}
      <p class="hello-s">Agent Roster hasn't changed anything yet.</p>
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
    <span class="dim">Uninstalling restores all of this.</span>
    {#if list?.some((c) => !c.undone)}
      <button class="btn" onclick={() => ask("Undo every change Agent Roster made?", () => undoAll())} disabled={busy}>Undo everything</button>
    {/if}
  </div>
</div>

<style>
  .changes {
    margin: 0 auto;
  }
  .found {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 14px 0;
    border-bottom: 1px solid var(--line);
  }
  .dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    flex: none;
  }
  .meta {
    flex: 1;
    font-size: 12.5px;
    line-height: 1.5;
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
    font-size: 12.5px;
  }
  .jade {
    color: var(--jade);
  }
  .ask {
    margin-top: 14px;
    background: var(--well);
    border: 1px solid var(--line);
    border-radius: 10px;
    padding: 12px 14px;
    font-size: 13px;
  }
  .ask .foot {
    margin-top: 10px;
  }
  .bottom {
    justify-content: space-between;
    align-items: center;
  }
</style>
