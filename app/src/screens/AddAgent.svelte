<script lang="ts">
  import type { ColorView, PlanView, Tool } from "../lib/types";
  import { apply, message, preview, signIn } from "../lib/api";
  import PageHead from "../components/PageHead.svelte";

  let {
    palette,
    nextColor,
    onDone,
    onCancel,
  }: {
    palette: ColorView[];
    nextColor: string;
    onDone: (notice?: string) => void;
    onCancel: () => void;
  } = $props();

  const TOOLS: { tool: Tool; label: string }[] = [
    { tool: "claude", label: "Claude Code" },
    { tool: "codex", label: "Codex" },
  ];

  let name = $state("");
  let tool = $state<Tool>("claude");
  let picked = $state("");
  let share = $state(false);
  let role = $state("");
  let plan = $state<PlanView | null>(null);
  let error = $state<string | null>(null);
  let busy = $state(false);
  let applyError = $state<string | null>(null);
  let retry = $state(0);

  const color = $derived(picked || nextColor);
  const hex = $derived(palette.find((c) => c.label === color)?.hex ?? "var(--txt)");
  const toolLabel = $derived(TOOLS.find((t) => t.tool === tool)!.label);

  let seq = 0;
  $effect(() => {
    const req = {
      op: "add" as const,
      name: name.trim(),
      tool,
      color,
      role: role.trim() || null,
      share,
    };
    void retry;
    const mine = ++seq;
    plan = null;
    error = null;
    if (!req.name) return;
    const timer = setTimeout(async () => {
      try {
        const p = await preview(req);
        if (mine === seq) plan = p;
      } catch (e) {
        if (mine === seq) error = message(e);
      }
    }, 250);
    return () => clearTimeout(timer);
  });

  async function add() {
    if (!plan) return;
    busy = true;
    applyError = null;
    const added = name.trim();
    try {
      await apply(plan.token);
    } catch (e) {
      // The preview is used up or stale: show why and look again.
      applyError = message(e);
      busy = false;
      retry += 1;
      return;
    }
    try {
      await signIn(added);
      onDone();
    } catch (e) {
      onDone(`${added} was added. Sign in didn't open: ${message(e)}`);
    }
  }
</script>

<div class="page add">
  <PageHead title="Add an agent" sub="Its name is the command you type to start it" onBack={onCancel} />
  <div class="cols">
    <div class="form">
      <input
        class="big wide"
        aria-label="Name"
        placeholder="name"
        spellcheck="false"
        autocomplete="off"
        style:color={hex}
        style:caret-color={hex}
        bind:value={name}
      />

      <div class="lbl">Color</div>
      <p class="hint">Its row, its name and its menu bar dot use this color</p>
      <div class="sw" role="radiogroup" aria-label="Color">
        {#each palette as c (c.label)}
          <button
            role="radio"
            aria-checked={c.label === color}
            aria-label={c.label}
            class:on={c.label === color}
            style:background={c.hex}
            onclick={() => (picked = c.label)}
          ></button>
        {/each}
      </div>

      <div class="lbl">Tool</div>
      <p class="hint">The app this account signs in to. You sign in right after adding it.</p>
      <div class="seg">
        {#each TOOLS as t (t.tool)}
          <button class:on={tool === t.tool} aria-pressed={tool === t.tool} onclick={() => (tool = t.tool)}>{t.label}</button>
        {/each}
        <button class="off" disabled>Gemini soon</button>
      </div>

      <div class="tg">
        <button class="k" class:on={share} role="switch" aria-checked={share} aria-label="Share my skills" onclick={() => (share = !share)}></button>
        <div>
          Share my skills
          <small>{share ? `On. Uses the skills of your main ${toolLabel} account.` : "Off. This account starts fully separate."}</small>
        </div>
      </div>

      <label class="lbl role">
        Role (optional)
        <span class="hint">Other agents read this to decide what to hand it</span>
        <input bind:value={role} placeholder="what others should ask it for" spellcheck="false" />
      </label>
    </div>
    <div class="out">
      <div class="lbl first">What will happen</div>
      {#if applyError}<p class="error">{applyError}</p>{/if}
      <div class="will" aria-live="polite">
        {#if error}
          <p class="error">{error}</p>
        {:else if plan}
          {#each plan.steps as s, i (i)}<div>{s.text}</div>{/each}
        {:else}
          <div class="dim">Type a name to see exactly what will happen</div>
        {/if}
      </div>

      <div class="foot">
        <button class="btn" onclick={onCancel} disabled={busy}>Cancel</button>
        <button class="btn pri" onclick={add} disabled={!plan || busy}>Add and sign in</button>
      </div>
    </div>
  </div>
</div>

<style>
  .add {
    max-width: 1040px;
  }
  .cols {
    display: grid;
    grid-template-columns: minmax(0, 440px) minmax(0, 1fr);
    gap: 48px;
    align-items: start;
  }
  .lbl.first {
    margin-top: 0;
  }
  .big {
    width: 100%;
    background: none;
    border: 0;
    border-bottom: 2px solid var(--line);
    font-family: inherit;
    font-size: 44px;
    line-height: 48px;
    letter-spacing: -0.02em;
    padding: 0 0 12px;
    outline: none;
    transition: border-color 150ms var(--ease);
  }
  .big::placeholder {
    color: var(--dim);
  }
  .big:focus-visible {
    outline: none;
    border-bottom-color: var(--line-2);
  }
  .lbl {
    font-size: 13px;
    line-height: 20px;
    color: var(--mut);
    margin: 20px 0 8px;
  }
  .hint {
    display: block;
    margin: -4px 0 8px;
    font-size: 12px;
    line-height: 16px;
    color: var(--dim);
  }
  .role .hint {
    margin: -4px 0 0;
  }
  .sw {
    display: flex;
    gap: 8px;
  }
  .sw button {
    width: 28px;
    height: 28px;
    border-radius: 50%;
    border: 0;
    padding: 0;
  }
  .sw button.on {
    box-shadow:
      0 0 0 2px var(--base),
      0 0 0 4px var(--txt);
  }
  .seg {
    display: grid;
    grid-template-columns: 1fr 1fr 1fr;
    gap: 8px;
  }
  .seg button {
    height: 40px;
    border-radius: 8px;
    border: 1px solid var(--line);
    background: var(--raise);
    color: var(--txt);
    font-size: 13px;
    font-weight: 500;
    transition: border-color 150ms var(--ease);
  }
  .seg button:hover:not(:disabled):not(.on) {
    border-color: var(--line-2);
  }
  .seg button.on {
    border-color: var(--txt);
    box-shadow: inset 0 0 0 1px var(--txt);
  }
  .seg button.off {
    color: var(--dim);
    cursor: default;
  }
  .tg {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-top: 20px;
    font-size: 13px;
    line-height: 20px;
  }
  .tg .k {
    width: 36px;
    height: 20px;
    border-radius: 10px;
    background: var(--line-2);
    position: relative;
    flex: none;
    border: 0;
    padding: 0;
    transition: background-color 150ms var(--ease);
  }
  .tg .k::after {
    content: "";
    position: absolute;
    left: 3px;
    top: 3px;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: var(--mut);
    transition:
      transform 150ms var(--ease),
      background-color 150ms var(--ease);
  }
  .tg .k.on {
    background: var(--jade);
  }
  .tg .k.on::after {
    transform: translateX(16px);
    background: var(--txt);
  }
  .tg small {
    display: block;
    color: var(--dim);
    font-size: 12px;
    line-height: 16px;
  }
  .role {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .role input {
    height: 36px;
    background: var(--raise);
    border: 1px solid var(--line);
    border-radius: 8px;
    padding: 0 12px;
    color: var(--txt);
    font: inherit;
    font-size: 13px;
  }
  .role input::placeholder {
    color: var(--dim);
  }
  .will {
    background: var(--well);
    border: 1px solid var(--line);
    border-radius: 8px;
    padding: 12px 16px;
    font-size: 13px;
    line-height: 22px;
    color: var(--mut);
    min-height: 112px;
  }
  .will .error {
    margin: 0;
  }
  .dim {
    color: var(--dim);
  }
  .out > .error {
    margin: 0 0 8px;
  }
</style>
