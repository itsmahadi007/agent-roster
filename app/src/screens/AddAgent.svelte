<script lang="ts">
  import type { ColorView, PlanView, Tool } from "../lib/types";
  import { apply, message, preview, signIn } from "../lib/api";

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
    const added = name.trim();
    try {
      await apply(plan.token);
    } catch (e) {
      error = message(e);
      busy = false;
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

<div class="add">
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
    <input bind:value={role} placeholder="what others should ask it for" spellcheck="false" />
  </label>

  <div class="will" aria-live="polite">
    {#if error}
      <p class="error">{error}</p>
    {:else if plan}
      {#each plan.steps as s, i (i)}<div>{s.text}</div>{/each}
    {:else}
      <div class="dim">Type a name to see exactly what will happen.</div>
    {/if}
  </div>

  <div class="foot">
    <button class="btn" onclick={onCancel} disabled={busy}>Cancel</button>
    <button class="btn pri" onclick={add} disabled={!plan || busy}>Add and sign in</button>
  </div>
</div>

<style>
  .add {
    width: 420px;
    margin: 0 auto;
    padding: 26px 0 30px;
  }
  .big {
    width: 100%;
    background: none;
    border: 0;
    border-bottom: 2px solid var(--line);
    font-family: inherit;
    font-size: 46px;
    line-height: 1;
    letter-spacing: -0.015em;
    padding: 18px 0 14px;
    outline: none;
  }
  .big::placeholder {
    color: var(--dim);
  }
  .big:focus-visible {
    outline: none;
    border-bottom-color: #3a4758;
  }
  .lbl {
    font-size: 12.5px;
    color: var(--mut);
    margin: 16px 0 8px;
  }
  .sw {
    display: flex;
    gap: 9px;
  }
  .sw button {
    width: 26px;
    height: 26px;
    border-radius: 50%;
    border: 2px solid transparent;
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
    gap: 6px;
  }
  .seg button {
    text-align: center;
    padding: 10px 6px;
    border-radius: 9px;
    border: 1px solid var(--line);
    background: var(--raise);
    color: var(--txt);
    font-size: 13px;
    font-weight: 600;
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
    margin-top: 14px;
    font-size: 13px;
  }
  .tg .k {
    width: 34px;
    height: 20px;
    border-radius: 999px;
    background: #2a3646;
    position: relative;
    flex: none;
    border: 0;
    padding: 0;
  }
  .tg .k::after {
    content: "";
    position: absolute;
    left: 3px;
    top: 3px;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: #8592a5;
    transition: left 0.15s;
  }
  .tg .k.on {
    background: var(--jade);
  }
  .tg .k.on::after {
    left: 17px;
    background: var(--txt);
  }
  .tg small {
    display: block;
    color: var(--dim);
    font-size: 12px;
  }
  .role {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .role input {
    background: var(--raise);
    border: 1px solid var(--line);
    border-radius: 9px;
    padding: 8px 10px;
    color: var(--txt);
    font: inherit;
    font-size: 13px;
  }
  .will {
    margin-top: 16px;
    background: var(--well);
    border: 1px solid var(--line);
    border-radius: 10px;
    padding: 12px 14px;
    font-size: 12.5px;
    line-height: 1.75;
    color: var(--mut);
    min-height: 92px;
  }
  .will .error {
    margin: 0;
  }
  .dim {
    color: var(--dim);
  }
</style>
