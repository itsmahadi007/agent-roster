<script lang="ts">
  import type { TerminalApp } from "../lib/types";
  import { copyText, message, openTerminalAs } from "../lib/api";
  import { rememberTerminal, rememberedTerminal } from "../lib/terminal";

  let { name, terminals }: { name: string; terminals: TerminalApp[] } = $props();

  let pick = $state<string | null>(rememberedTerminal());
  let open = $state(false);
  let box = $state<HTMLDivElement | null>(null);
  let note = $state<string | null>(null);
  let error = $state<string | null>(null);

  /** The remembered app, if it is still installed. */
  const chosen = $derived(terminals.find((t) => t.id === pick) ?? null);

  async function go(app: TerminalApp | null) {
    open = false;
    note = null;
    error = null;
    pick = app?.id ?? null;
    rememberTerminal(pick);
    // Copy first, inside the click, so the system allows it.
    if (app && !app.runs) await copyText(name);
    try {
      const ran = await openTerminalAs(name, app?.id ?? null);
      if (!ran && app) note = `${name} is copied. Paste it in ${app.label} and press Return.`;
    } catch (e) {
      error = message(e);
    }
  }

  async function copyOnly() {
    open = false;
    error = null;
    await copyText(name);
    note = `Open any terminal, type ${name} and press Return.`;
  }

  const mainLabel = $derived(chosen ? `Open in ${chosen.label}` : "Open in terminal");
</script>

<svelte:window
  onkeydown={(e) => {
    if (e.key === "Escape") open = false;
  }}
  onclick={(e) => {
    if (open && box && !box.contains(e.target as Node)) open = false;
  }}
/>

<div class="tb" bind:this={box}>
  {#if terminals.length === 0}
    <button class="btn" onclick={copyOnly}>Copy command</button>
  {:else}
    <div class="split">
      <button
        class="btn main"
        title={chosen
          ? chosen.runs
            ? `Starts ${name} in ${chosen.label}`
            : `Opens ${chosen.label} and copies ${name} for you to paste`
          : `Starts ${name} in your default terminal`}
        onclick={() => go(chosen)}>{mainLabel}</button
      >
      <button class="btn arrow" aria-label="Choose a terminal" aria-haspopup="menu" aria-expanded={open} onclick={() => (open = !open)}>
        <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><path d="M2 3.5l3 3 3-3" /></svg>
      </button>
    </div>
    {#if open}
      <div class="menu" role="menu">
        <button role="menuitem" onclick={() => go(null)}>
          Default terminal<span>{!chosen ? "chosen" : ""}</span>
        </button>
        {#each terminals as t (t.id)}
          <button role="menuitem" onclick={() => go(t)}>
            {t.label}<span>{t.runs ? (chosen?.id === t.id ? "chosen" : "") : "opens it, copies the command"}</span>
          </button>
        {/each}
        <hr />
        <button role="menuitem" onclick={copyOnly}>Just copy the command</button>
      </div>
    {/if}
  {/if}
  {#if note}<p class="note" role="status">{note}</p>{/if}
  {#if error}<p class="error">{error}</p>{/if}
</div>

<style>
  .tb {
    position: relative;
    display: inline-flex;
    flex-direction: column;
    align-items: flex-end;
  }
  .split {
    display: inline-flex;
  }
  .main {
    border-top-right-radius: 0;
    border-bottom-right-radius: 0;
  }
  .arrow {
    padding: 0 10px;
    border-left: 0;
    border-top-left-radius: 0;
    border-bottom-left-radius: 0;
  }
  .arrow svg {
    fill: none;
    stroke: currentColor;
    stroke-width: 1.5;
  }
  .menu {
    position: absolute;
    top: 40px;
    right: 0;
    z-index: 30;
    min-width: 280px;
    background: var(--raise);
    border: 1px solid var(--line-2);
    border-radius: 10px;
    padding: 4px;
    box-shadow: var(--shadow);
    display: flex;
    flex-direction: column;
  }
  .menu button {
    height: 32px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 0 10px;
    border: 0;
    border-radius: 6px;
    background: none;
    color: var(--txt);
    font-size: 13px;
    text-align: left;
  }
  .menu button:hover,
  .menu button:focus-visible {
    background: var(--well);
    outline: none;
  }
  .menu span {
    color: var(--dim);
    font-size: 12px;
  }
  .menu hr {
    border: 0;
    border-top: 1px solid var(--line);
    margin: 4px 0;
    width: 100%;
  }
  .note {
    margin: 8px 0 0;
    font-size: 12px;
    line-height: 16px;
    color: var(--jade);
    max-width: 320px;
    text-align: right;
  }
  .error {
    margin: 8px 0 0;
    font-size: 12px;
    max-width: 320px;
    text-align: right;
  }
</style>
