<script lang="ts">
  import type { PlanView } from "../lib/types";
  import { apply, message, preview } from "../lib/api";

  let {
    name,
    color,
    onDone,
    onCancel,
  }: { name: string; color: string; onDone: (newName: string) => void; onCancel: () => void } = $props();

  // svelte-ignore state_referenced_locally
  let next = $state(name);
  let plan = $state<PlanView | null>(null);
  let error = $state<string | null>(null);
  let busy = $state(false);
  let retry = $state(0);

  let seq = 0;
  $effect(() => {
    const want = next.trim();
    void retry;
    const mine = ++seq;
    plan = null;
    error = null;
    if (!want || want === name) return;
    const timer = setTimeout(async () => {
      try {
        const p = await preview({ op: "rename", name, new_name: want });
        if (mine === seq) plan = p;
      } catch (e) {
        if (mine === seq) error = message(e);
      }
    }, 250);
    return () => clearTimeout(timer);
  });

  async function rename() {
    if (!plan) return;
    busy = true;
    try {
      await apply(plan.token);
      onDone(next.trim());
    } catch (e) {
      // The preview is used up or stale: say why and look again.
      error = message(e);
      retry += 1;
    } finally {
      busy = false;
    }
  }
</script>

<div class="backdrop">
  <div class="sheet" role="dialog" aria-modal="true" aria-labelledby="rename-title">
    <form
      onsubmit={(e) => {
        e.preventDefault();
        rename();
      }}
    >
      <h4 id="rename-title">Rename {name}</h4>
      <p class="lede">
        The new name becomes the command you type in a terminal. The login stays signed in, and its history and settings stay
        as they are.
      </p>
      <!-- svelte-ignore a11y_autofocus -->
      <input
        class="big wide"
        aria-label="New name"
        spellcheck="false"
        autocomplete="off"
        autofocus
        style:color
        style:caret-color={color}
        bind:value={next}
      />
      <div class="will" aria-live="polite">
        {#if error}
          <p class="error">{error}</p>
        {:else if plan}
          {#each plan.steps as s, i (i)}<div>{s.text}</div>{/each}
        {:else}
          <div class="dim">Type a new name to see exactly what will change</div>
        {/if}
      </div>
      <div class="foot">
        <button type="button" class="btn" onclick={onCancel} disabled={busy}>Cancel</button>
        <button type="submit" class="btn pri" disabled={!plan || busy}>Rename</button>
      </div>
    </form>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(5, 9, 14, 0.62);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 10;
  }
  .sheet {
    width: min(520px, calc(100vw - 48px));
    background: var(--base);
    border: 1px solid var(--line-2);
    border-radius: 16px;
    padding: 24px;
    box-shadow: 0 24px 64px rgba(0, 0, 0, 0.5);
  }
  h4 {
    margin: 0 0 4px;
    font-size: 16px;
    line-height: 24px;
    font-weight: 600;
  }
  .big {
    width: 100%;
    margin-top: 16px;
    background: none;
    border: 0;
    border-bottom: 2px solid var(--line);
    font-family: inherit;
    font-size: 30px;
    line-height: 36px;
    letter-spacing: -0.01em;
    padding: 0 0 8px;
    outline: none;
  }
  .big:focus-visible {
    outline: none;
    border-bottom-color: var(--line-2);
  }
  .will {
    margin-top: 16px;
    background: var(--well);
    border: 1px solid var(--line);
    border-radius: 8px;
    padding: 12px 16px;
    font-size: 13px;
    line-height: 22px;
    color: var(--mut);
    min-height: 68px;
  }
  .will .error {
    margin: 0;
  }
  .dim {
    color: var(--dim);
  }
</style>
