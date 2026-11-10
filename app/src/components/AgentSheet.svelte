<script lang="ts">
  import type { AgentView, ColorView, OpRequest, PlanView } from "../lib/types";
  import { apply, message, openTerminalAs, preview, signIn } from "../lib/api";

  let {
    agent,
    palette,
    onApplied,
    onClose,
  }: {
    agent: AgentView;
    palette: ColorView[];
    /** A change went through: what to tell the person, and which kind it was. */
    onApplied: (notice: string, op: OpRequest["op"]) => void;
    onClose: () => void;
  } = $props();

  // The sheet is mounted again after every change, so starting from the agent is enough.
  // svelte-ignore state_referenced_locally
  let name = $state(agent.name);
  // svelte-ignore state_referenced_locally
  let role = $state(agent.role ?? "");
  /** A change picked with a click (color, sharing, default, remove). */
  let picked = $state<OpRequest | null>(null);
  let plan = $state<PlanView | null>(null);
  let error = $state<string | null>(null);
  let busy = $state(false);
  let retry = $state(0);
  let actionError = $state<string | null>(null);

  const tool = $derived(agent.tool);

  /** One change at a time: the last thing touched. */
  const req = $derived.by((): OpRequest | null => {
    if (picked) return picked;
    const n = name.trim();
    if (n && n !== agent.name) return { op: "rename", name: agent.name, new_name: n };
    const r = role.trim();
    if (r !== (agent.role ?? "")) return { op: "role", name: agent.name, role: r || null };
    return null;
  });

  function pick(r: OpRequest) {
    name = agent.name;
    role = agent.role ?? "";
    picked = r;
  }

  const shownColor = $derived(picked?.op === "color" ? picked.color : agent.color_label);
  const shownShare = $derived(picked?.op === "share" ? picked.on : agent.share);
  const hex = $derived(palette.find((c) => c.label === shownColor)?.hex ?? agent.color);

  const LABEL: Record<string, string> = {
    rename: "Rename",
    color: "Change color",
    role: "Save role",
    default: "Make default",
    remove: "Remove",
  };
  const applyLabel = $derived(
    !req ? "Apply" : req.op === "share" ? (req.on ? "Turn on sharing" : "Turn off sharing") : (LABEL[req.op] ?? "Apply"),
  );

  function notice(r: OpRequest): string {
    switch (r.op) {
      case "rename":
        return `${r.name} is now ${r.new_name}. Type ${r.new_name} in any terminal to start it.`;
      case "color":
        return `${r.name} is ${r.color} now.`;
      case "role":
        return r.role ? `${r.name}'s role is saved. Other agents see it from their next session.` : `${r.name} has no role now.`;
      case "share":
        return r.on ? `${r.name} now uses the skills of your main ${agent.tool_label} account.` : `${r.name} no longer shares your skills.`;
      case "default":
        return `A plain ${tool} in a new terminal now opens ${r.name}.`;
      case "remove":
        return `${r.name} was removed. Its login folder is still there, and you can undo this under What changed.`;
      default:
        return "Done.";
    }
  }

  let seq = 0;
  $effect(() => {
    const r = req;
    void retry;
    const mine = ++seq;
    plan = null;
    error = null;
    if (!r) return;
    const timer = setTimeout(
      async () => {
        try {
          const p = await preview(r);
          if (mine === seq) plan = p;
        } catch (e) {
          if (mine === seq) error = message(e);
        }
      },
      r.op === "rename" || r.op === "role" ? 250 : 0,
    );
    return () => clearTimeout(timer);
  });

  async function go() {
    if (!plan || !req) return;
    const r = req;
    busy = true;
    try {
      await apply(plan.token);
      onApplied(notice(r), r.op);
    } catch (e) {
      // The preview is used up or stale: say why and look again.
      error = message(e);
      retry += 1;
    } finally {
      busy = false;
    }
  }

  async function act(f: () => Promise<void>) {
    actionError = null;
    try {
      await f();
    } catch (e) {
      actionError = message(e);
    }
  }

  /** Drop the pending change and put every field back. */
  function cancel() {
    name = agent.name;
    role = agent.role ?? "";
    picked = null;
  }
</script>

<svelte:window
  onkeydown={(e) => {
    if (e.key === "Escape") onClose();
  }}
/>

<div class="backdrop">
  <div class="sheet" role="dialog" aria-modal="true" aria-labelledby="edit-title">
    <h4 id="edit-title">Edit <span class="agent-name ink" style:color={agent.color} style:--c={agent.color}>{agent.name}</span></h4>
    <p class="lede">One change at a time. Each shows exactly what it will do before you apply it, and can be undone later.</p>

    <div class="grid">
      <label class="k" for="edit-name">Name</label>
      <div>
        <input
          id="edit-name"
          class="name agent-name ink"
          spellcheck="false"
          autocomplete="off"
          style:color={hex} style:--c={hex}
          style:caret-color={hex}
          bind:value={name}
          oninput={() => {
            picked = null;
            role = agent.role ?? "";
          }}
        />
        <p class="hint">The command you type to start it. The login stays signed in and keeps its history.</p>
      </div>

      <span class="k">Color</span>
      <div>
        <div class="sw" role="radiogroup" aria-label="Color">
          {#each palette as c (c.label)}
            <button
              role="radio"
              aria-checked={c.label === shownColor}
              aria-label={c.label}
              title={c.label}
              class:on={c.label === shownColor}
              style:background={c.hex}
              onclick={() => (c.label === agent.color_label ? (picked = null) : pick({ op: "color", name: agent.name, color: c.label }))}
            ></button>
          {/each}
        </div>
        <p class="hint">Used for its row, its name and its menu bar dot</p>
      </div>

      <label class="k" for="edit-role">Role</label>
      <div>
        <input
          id="edit-role"
          class="field"
          spellcheck="false"
          placeholder="what others should ask it for"
          bind:value={role}
          oninput={() => {
            picked = null;
            name = agent.name;
          }}
        />
        <p class="hint">Other agents read this to decide what to hand it. Clear it to remove the role.</p>
      </div>

      <span class="k">Skills</span>
      <div class="tg">
        <button
          class="sw-k"
          class:on={shownShare}
          role="switch"
          aria-checked={shownShare}
          aria-label="Share my skills"
          onclick={() =>
            shownShare === agent.share ? pick({ op: "share", name: agent.name, on: !agent.share }) : (picked = null)}
        ></button>
        <div>
          Share my skills
          <small>{shownShare ? `On. Uses the skills of your main ${agent.tool_label} account.` : "Off. This account keeps its own skills."}</small>
        </div>
      </div>

      <span class="k">Default</span>
      <div>
        {#if agent.default}
          <p class="plain">A plain {tool} in a new terminal opens {agent.name}</p>
        {:else}
          <button class="btn sm" onclick={() => pick({ op: "default", name: agent.name })}>Make default for {tool}</button>
          <p class="hint">Then typing just <code class="mono">{tool}</code> opens this account instead of the current default</p>
        {/if}
      </div>

      <span class="k">Use</span>
      <div>
        <div class="row">
          <button class="btn sm" onclick={() => act(() => openTerminalAs(agent.name))}>Open a terminal</button>
          <button class="btn sm" onclick={() => act(() => signIn(agent.name))}>Sign in again</button>
        </div>
        <p class="hint">Sign in again opens {agent.tool_label}'s own sign-in, to fix a sign-in or switch to another account</p>
        {#if actionError}<p class="error">{actionError}</p>{/if}
      </div>

      <span class="k">Remove</span>
      <div>
        <button class="btn sm danger" aria-label="Remove {agent.name}" onclick={() => pick({ op: "remove", name: agent.name })}>
          Remove from roster
        </button>
        <p class="hint">Removes its command. Its login folder stays on this computer, and you can undo this under What changed.</p>
      </div>
    </div>

    <div class="will" aria-live="polite">
      {#if error}
        <p class="error">{error}</p>
      {:else if plan}
        {#each plan.steps as s, i (i)}<div>{s.text}</div>{/each}
      {:else if req}
        <div class="dim">Looking at what will change…</div>
      {:else}
        <div class="dim">Change something above to see exactly what will happen</div>
      {/if}
    </div>
    <div class="foot sheet-foot">
      {#if req}
        <button class="btn" onclick={cancel} disabled={busy}>Cancel</button>
        <button class="btn pri" class:danger-pri={req.op === "remove"} onclick={go} disabled={!plan || busy}>{applyLabel}</button>
      {:else}
        <button class="btn" onclick={onClose}>Close</button>
      {/if}
    </div>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: var(--bar-h, 0px) 0 0 0;
    background: var(--scrim);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 10;
  }
  /* The fields scroll; what will happen and the buttons always stay in view. */
  .sheet {
    width: min(680px, calc(100vw - 48px));
    max-height: calc(100% - 32px);
    display: flex;
    flex-direction: column;
    background: var(--base);
    border: 1px solid var(--line-2);
    border-radius: 16px;
    padding: 24px;
    box-shadow: var(--shadow);
  }
  h4 {
    margin: 0 0 4px;
    font-size: 16px;
    line-height: 24px;
    font-weight: 600;
  }
  .grid {
    display: grid;
    grid-template-columns: 72px minmax(0, 1fr);
    gap: 12px 16px;
    margin-top: 16px;
    padding-right: 4px;
    align-items: start;
    flex: 1;
    min-height: 0;
    overflow-y: auto;
  }
  .k {
    font-size: 13px;
    line-height: 20px;
    color: var(--mut);
    padding-top: 6px;
  }
  .name {
    width: 100%;
    background: none;
    border: 0;
    border-bottom: 2px solid var(--line);
    font-family: inherit;
    font-size: 26px;
    line-height: 32px;
    padding: 0 0 4px;
    outline: none;
  }
  .name:focus-visible {
    border-bottom-color: var(--line-2);
  }
  .field {
    width: 100%;
    height: 32px;
    background: var(--raise);
    border: 1px solid var(--line);
    border-radius: 8px;
    padding: 0 12px;
    color: var(--txt);
    font: inherit;
    font-size: 13px;
  }
  .field::placeholder {
    color: var(--dim);
  }
  .hint {
    margin: 4px 0 0;
    font-size: 12px;
    line-height: 16px;
    color: var(--dim);
  }
  .plain {
    margin: 0;
    padding-top: 6px;
    font-size: 13px;
    line-height: 20px;
  }
  .sw {
    display: flex;
    gap: 8px;
    padding-top: 2px;
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
      0 0 0 2px var(--base),
      0 0 0 4px var(--txt);
  }
  .tg {
    display: flex;
    align-items: center;
    gap: 12px;
    font-size: 13px;
    line-height: 20px;
    padding-top: 4px;
  }
  .tg small {
    display: block;
    color: var(--dim);
    font-size: 12px;
    line-height: 16px;
  }
  .sw-k {
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
  .sw-k::after {
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
  .sw-k.on {
    background: var(--jade);
  }
  .sw-k.on::after {
    transform: translateX(16px);
    background: #ffffff;
  }
  .row {
    display: flex;
    gap: 8px;
  }
  .danger {
    color: var(--coral);
  }
  .danger-pri {
    background: var(--coral);
    border-color: var(--coral);
    color: #ffffff;
  }
  .error {
    margin: 4px 0 0;
  }
  code {
    font-size: 11px;
    color: var(--txt);
    padding: 0 4px;
    border-radius: 4px;
    background: var(--raise);
    border: 1px solid var(--line);
  }
  .will {
    flex: none;
    margin-top: 16px;
    background: var(--well);
    border: 1px solid var(--line);
    border-radius: 8px;
    padding: 12px 16px;
    font-size: 13px;
    line-height: 22px;
    color: var(--mut);
    min-height: 48px;
  }
  .will .error {
    margin: 0;
  }
  .dim {
    color: var(--dim);
  }
</style>
