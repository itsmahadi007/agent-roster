<script lang="ts">
  import { browserOnly, type ColorView, type PlanView, type Tool } from "../lib/types";
  import { apply, message, preview, signIn } from "../lib/api";
  import { prefs, signInInApp } from "../lib/signin.svelte";
  import Overlay from "../components/Overlay.svelte";
  import RolePicker from "../components/RolePicker.svelte";
  import { swatches } from "../lib/colors";
  import { onMount } from "svelte";

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

  const TOOLS: { tool: Tool; label: string; maker: string }[] = [
    { tool: "claude", label: "Claude Code", maker: "Anthropic" },
    { tool: "codex", label: "Codex", maker: "OpenAI" },
    { tool: "grok", label: "Grok", maker: "xAI" },
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

  const colors = $derived(swatches(palette));
  const color = $derived(picked || nextColor);
  const hex = $derived(colors.find((c) => c.value === color)?.hex ?? "var(--txt)");
  /** Role and sharing are folded away: most people never change them. */
  let more = $state(false);
  let nameInput: HTMLInputElement | undefined = $state();
  onMount(() => nameInput?.focus({ preventScroll: true }));
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
      if (prefs.signInWith === "window" && !browserOnly(tool)) await signInInApp(added);
      else await signIn(added);
      onDone();
    } catch (e) {
      onDone(`${added} was added. Sign in didn't open: ${message(e)}`);
    }
  }
</script>

<Overlay kind="dialog" label="Add an agent" title="Add an agent" onClose={() => !busy && onCancel()}>
  <div class="lbl first" id="add-tool">Which app does this account use?</div>
  <div class="tools" role="group" aria-labelledby="add-tool">
    {#each TOOLS as t (t.tool)}
      <button class:on={tool === t.tool} aria-pressed={tool === t.tool} aria-label={t.label} onclick={() => (tool = t.tool)}>
        {t.label}<small>{t.maker}</small>
      </button>
    {/each}
    <button class="off" disabled aria-label="Gemini soon">Gemini<small>soon</small></button>
  </div>

  <div class="lbl">Name: the command you type to start it</div>
  <input
    class="big agent-name ink"
    aria-label="Name"
    placeholder="name"
    spellcheck="false"
    autocomplete="off"
    style:color={hex}
    style:--c={hex}
    style:caret-color={hex}
    bind:this={nameInput}
    bind:value={name}
  />

  <div class="lbl">Color</div>
  <div class="sw" role="radiogroup" aria-label="Color">
    {#each colors as c (c.value)}
      <button
        role="radio"
        aria-checked={c.value === color}
        aria-label={c.label}
        title={c.label}
        class:on={c.value === color}
        style:background={c.hex}
        onclick={() => (picked = c.value)}
      ></button>
    {/each}
  </div>

  <button class="more" aria-expanded={more} onclick={() => (more = !more)}>
    {more ? "▾ More options" : "▸ More options (role, share skills)"}
  </button>
  {#if more}
    <div class="lbl">Role (optional): other agents read this to decide what to hand it</div>
    <RolePicker value={role} onChange={(v) => (role = v)} />
    <div class="tg">
      <button class="k" class:on={share} role="switch" aria-checked={share} aria-label="Share my skills" onclick={() => (share = !share)}></button>
      <div>
        Share my skills
        <small>{share ? `On. Uses the skills of your main ${toolLabel} account.` : "Off. This account starts fully separate."}</small>
      </div>
    </div>
  {/if}

  <div class="sum" aria-live="polite">
    {#if applyError}<p class="error">{applyError}</p>{/if}
    {#if error}
      <p class="error">{error}</p>
    {:else if plan}
      <p>Adds the <b>{name.trim()}</b> command for {toolLabel}, with its own login folder, then opens its sign-in.</p>
      <details>
        <summary>Details</summary>
        <ul>
          {#each plan.steps as s, i (i)}<li data-kind={s.kind}>{s.text}</li>{/each}
        </ul>
      </details>
    {:else}
      <p class="dim">Type a name to see exactly what will happen</p>
    {/if}
  </div>

  {#snippet foot()}
    <button class="btn" onclick={onCancel} disabled={busy}>Cancel</button>
    <button class="btn pri" onclick={add} disabled={!plan || busy}>Add and sign in</button>
  {/snippet}
</Overlay>

<style>
  .lbl {
    font-size: 12px;
    line-height: 16px;
    color: var(--mut);
    margin: 14px 0 6px;
  }
  .lbl.first {
    margin-top: 2px;
  }
  .tools {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
  }
  .tools button {
    height: 52px;
    border-radius: 10px;
    border: 1px solid var(--line);
    background: var(--raise);
    color: var(--txt);
    font-size: 13px;
    font-weight: 600;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    transition: border-color 150ms var(--ease);
  }
  .tools small {
    font-size: 11px;
    line-height: 14px;
    font-weight: 400;
    color: var(--mut);
  }
  .tools button:hover:not(:disabled):not(.on) {
    border-color: var(--line-2);
  }
  .tools button.on {
    border-color: var(--txt);
    box-shadow: inset 0 0 0 1px var(--txt);
  }
  .tools button.off {
    color: var(--dim);
    font-weight: 400;
    cursor: default;
  }
  .tools button:focus-visible,
  .more:focus-visible,
  .sw button:focus-visible {
    outline: 2px solid var(--txt);
    outline-offset: 2px;
  }
  .big {
    width: 100%;
    background: none;
    border: 0;
    border-bottom: 1px solid var(--line);
    font-family: inherit;
    font-size: 26px;
    line-height: 32px;
    letter-spacing: -0.02em;
    padding: 2px 0 6px;
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
  .sw {
    display: flex;
    gap: 8px;
  }
  .sw button {
    width: 24px;
    height: 24px;
    border-radius: 50%;
    border: 0;
    padding: 0;
  }
  .sw button.on {
    box-shadow:
      0 0 0 2px var(--panel),
      0 0 0 4px var(--txt);
  }
  .more {
    display: block;
    margin: 14px 0 0;
    padding: 0;
    border: 0;
    background: none;
    color: var(--mut);
    font-size: 13px;
  }
  .more:hover {
    color: var(--txt);
  }
  .tg {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-top: 14px;
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
  .sum {
    margin: 14px 0 0;
    padding: 10px 12px;
    border-radius: 10px;
    background: var(--well);
    border: 1px solid var(--line);
    font-size: 12px;
    line-height: 18px;
    color: var(--mut);
  }
  .sum p {
    margin: 0;
  }
  .sum p + p {
    margin-top: 6px;
  }
  .sum b {
    color: var(--txt);
    font-weight: 500;
  }
  details {
    margin-top: 4px;
  }
  summary {
    cursor: pointer;
    color: var(--txt);
    text-decoration: underline;
    text-decoration-color: var(--line-2);
    text-underline-offset: 3px;
    width: max-content;
  }
  ul {
    margin: 6px 0 0;
    padding-left: 18px;
  }
  [data-kind="add"],
  [data-kind="change"],
  [data-kind="remove"] {
    color: var(--txt);
  }
  .dim {
    color: var(--dim);
  }
</style>
