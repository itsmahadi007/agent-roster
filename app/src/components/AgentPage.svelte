<script lang="ts">
  import type { AgentView, ColorView, OpRequest, PlanView, TerminalApp } from "../lib/types";
  import { apply, copyText, message, preview, signIn } from "../lib/api";
  import { rememberedTerminal } from "../lib/terminal";
  import TerminalButton from "./TerminalButton.svelte";
  import RolePicker from "./RolePicker.svelte";

  let {
    agent,
    palette,
    terminals,
    onApplied,
  }: {
    agent: AgentView;
    palette: ColorView[];
    terminals: TerminalApp[];
    /** A change went through: what to tell the person, and which kind it was. */
    onApplied: (notice: string, op: OpRequest["op"]) => void;
  } = $props();

  // The page is mounted again after every change, so starting from the agent is enough.
  // svelte-ignore state_referenced_locally
  let name = $state(agent.name);
  // svelte-ignore state_referenced_locally
  let role = $state(agent.role ?? "");
  // svelte-ignore state_referenced_locally
  let good = $state<string[]>([...agent.good_at]);
  let newTag = $state("");
  /** A change picked with a click (color, sharing, default, usage, remove, reset). */
  let picked = $state<OpRequest | null>(null);
  /** Typing waits a moment before previewing; clicks preview at once. */
  let typing = $state(false);
  let plan = $state<PlanView | null>(null);
  let error = $state<string | null>(null);
  let busy = $state(false);
  let retry = $state(0);
  let actionError = $state<string | null>(null);
  let copied = $state(false);

  const same = (a: string[], b: string[]) => a.length === b.length && a.every((x, i) => x === b[i]);

  /** One change at a time: the last thing touched. */
  const req = $derived.by((): OpRequest | null => {
    if (picked) return picked;
    const n = name.trim();
    if (n && n !== agent.name) return { op: "rename", name: agent.name, new_name: n };
    const r = role.trim();
    if (r !== (agent.role ?? "")) return { op: "role", name: agent.name, role: r || null };
    if (!same(good, agent.good_at)) return { op: "good-at", name: agent.name, items: [...good] };
    return null;
  });

  /** Put every field back, then queue `r` (or nothing). */
  function reset(r: OpRequest | null = null) {
    name = agent.name;
    role = agent.role ?? "";
    good = [...agent.good_at];
    newTag = "";
    typing = false;
    picked = r;
  }

  const shownColor = $derived(picked?.op === "color" ? picked.color : agent.color_label);
  const shownShare = $derived(picked?.op === "share" ? picked.on : agent.share);
  const hex = $derived(palette.find((c) => c.label === shownColor)?.hex ?? agent.color);
  const tool = $derived(agent.tool);

  const LABEL: Record<string, string> = {
    rename: "Rename",
    color: "Change color",
    role: "Save role",
    default: "Make default",
    remove: "Remove",
    "enable-limits": "Show usage",
    "disable-limits": "Hide usage",
  };
  const applyLabel = $derived.by(() => {
    if (!req) return "Apply";
    if (req.op === "share") return req.on ? "Turn on sharing" : "Turn off sharing";
    if (req.op === "good-at") return req.items ? "Save" : "Reset";
    return LABEL[req.op] ?? "Apply";
  });

  function notice(r: OpRequest): string {
    switch (r.op) {
      case "rename":
        return `${r.name} is now ${r.new_name}. Type ${r.new_name} in any terminal to start it.`;
      case "color":
        return `${r.name} is ${r.color} now.`;
      case "role":
        return r.role ? `${r.name}'s role is saved. Other agents see it from their next session.` : `${r.name} has no role now.`;
      case "good-at":
        return r.items
          ? `Saved what ${r.name} is good at. Other agents see it from their next session.`
          : `${r.name} is back to what ${agent.tool_label} is usually good at.`;
      case "share":
        return r.on ? `${r.name} now uses the skills of your main ${agent.tool_label} account.` : `${r.name} no longer shares your skills.`;
      case "default":
        return `A plain ${tool} in a new terminal now opens ${r.name}.`;
      case "enable-limits":
        return `Showing ${r.name}'s usage. The numbers appear after its next message.`;
      case "disable-limits":
        return `${r.name}'s usage is hidden, and your old status line is back.`;
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
    const wait = typing ? 250 : 0;
    const mine = ++seq;
    plan = null;
    error = null;
    if (!r) return;
    const run = async () => {
      try {
        const p = await preview(r);
        if (mine === seq) plan = p;
      } catch (e) {
        if (mine === seq) error = message(e);
      }
    };
    if (wait === 0) {
      run();
      return;
    }
    const timer = setTimeout(run, wait);
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

  async function signInAgain() {
    actionError = null;
    const app = terminals.find((t) => t.id === rememberedTerminal()) ?? null;
    if (app && !app.runs) await copyText(`roster login ${agent.name}`);
    try {
      const ran = await signIn(agent.name, app?.id ?? null);
      if (!ran && app) actionError = `roster login ${agent.name} is copied. Paste it in ${app.label} and press Return.`;
    } catch (e) {
      actionError = message(e);
    }
  }

  async function copyName() {
    copied = await copyText(agent.name);
    setTimeout(() => (copied = false), 2000);
  }

  function addTag() {
    const t = newTag.trim();
    if (!t || good.includes(t)) return;
    picked = null;
    typing = false;
    good = [...good, t];
    newTag = "";
  }

  const win = (label: string) => agent.windows.find((w) => w.label === label) ?? null;
  const signedOut = $derived(agent.state === "sign-in");
  const usageOff = $derived(agent.claude_limits === false && !signedOut && agent.state !== "not-installed");
  const noNumbers = $derived(agent.windows.length === 0);
  const live = $derived(agent.state === "working" || agent.live);
</script>

<svelte:window
  onkeydown={(e) => {
    if (e.key === "Escape" && req) reset();
  }}
/>

<div class="page-wrap">
  <div class="scroll">
    <header class="top">
      <div class="who">
        <h2 class="agent-name ink" style:color={agent.color} style:--c={agent.color}>{agent.name}</h2>
        <div class="meta">
          <b>{agent.tool_label}</b>
          {#if agent.email}<span>{agent.email}</span>{:else if signedOut}<span>not signed in yet</span>{/if}
          {#if agent.plan}<span>{agent.plan}</span>{/if}
          {#if agent.default_label}<span>{agent.default_label}</span>{/if}
          <span class="st" class:ink={live} style:--c={agent.color} style:color={live ? agent.color : "var(--mut)"}>
            <i style:background={live ? agent.color : "var(--line-2)"}></i>{agent.status_text}
          </span>
        </div>
      </div>
      <TerminalButton name={agent.name} {terminals} />
    </header>

    <p class="callout">
      Or type <code class="mono">{agent.name}</code> in any terminal
      <button class="link" onclick={copyName}>{copied ? "Copied" : "Copy"}</button>
    </p>

    <div class="cards">
      {#each [["5 hr", "5-hour use"], ["week", "Weekly use"]] as [label, title], i (label)}
        {@const w = win(label)}
        <div class="card">
          <div class="lab">{title}</div>
          {#if w}
            <div class="val num" class:hot={w.hot}>{w.used_percent}%</div>
            <span class="track"><i style:width="{w.used_percent}%" style:background={w.hot ? "var(--coral)" : agent.color}></i></span>
            {#if agent.reset_text && agent.windows[agent.windows.length - 1]?.label === label}
              <div class="sub num" class:hot={agent.reset_hot}>{agent.reset_text}</div>
            {/if}
          {:else if signedOut}
            <div class="val dim">Shows after sign in</div>
          {:else if usageOff}
            <div class="val dim">Not shown yet</div>
          {:else if noNumbers}
            <div class="val dim">Shows after you use {agent.name}</div>
            <div class="sub">
              {i === 0 ? `Type ${agent.name} in a terminal and send one message` : "Refresh, in the list, checks again right away"}
            </div>
          {:else}
            <div class="val dim">Not reported</div>
            <div class="sub">{agent.tool_label} didn't report this limit</div>
          {/if}
        </div>
      {/each}
    </div>
    {#if usageOff}
      <p class="usage-line">
        Claude Code shows its use in its status line. Agent Roster can read it there; it never limits the agent.
        <button class="btn sm" onclick={() => reset({ op: "enable-limits", name: agent.name })}>Show usage</button>
      </p>
    {:else if agent.claude_limits === true}
      <p class="usage-line quiet">
        Agent Roster reads this account's use from Claude Code's status line.
        <button class="link" onclick={() => reset({ op: "disable-limits", name: agent.name })}>Hide usage</button>
      </p>
    {/if}

    <div class="grid">
      <label class="k" for="page-name">Name</label>
      <div>
        <input
          id="page-name"
          class="name agent-name ink"
          spellcheck="false"
          autocomplete="off"
          style:color={hex}
          style:--c={hex}
          style:caret-color={hex}
          bind:value={name}
          oninput={() => {
            picked = null;
            typing = true;
            role = agent.role ?? "";
            good = [...agent.good_at];
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
              onclick={() => reset(c.label === agent.color_label ? null : { op: "color", name: agent.name, color: c.label })}
            ></button>
          {/each}
        </div>
        <p class="hint">Used for its name, its place in the list and its menu bar dot</p>
      </div>

      <span class="k">Role</span>
      <div>
        <RolePicker
          value={role}
          onChange={(v) => {
            const keepTyping = v !== "" && !v.match(/^(Leader|Worker \d|Reviewer|Tester|Designer)$/);
            name = agent.name;
            good = [...agent.good_at];
            picked = null;
            typing = keepTyping;
            role = v;
          }}
        />
        <p class="hint">Other agents read this to decide what to hand it. A leader splits big tasks and hands parts to the workers.</p>
      </div>

      <span class="k">Good at</span>
      <div>
        <div class="tags">
          {#each good as t (t)}
            <span class="tag">
              {t}
              <button
                aria-label="Remove {t}"
                title="Remove"
                onclick={() => {
                  picked = null;
                  typing = false;
                  name = agent.name;
                  role = agent.role ?? "";
                  good = good.filter((x) => x !== t);
                }}>×</button
              >
            </span>
          {/each}
          <input
            class="add"
            aria-label="Add what it's good at"
            placeholder="+ add"
            spellcheck="false"
            bind:value={newTag}
            onkeydown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                addTag();
              }
            }}
          />
        </div>
        <p class="hint">
          {agent.good_at_custom ? "Set by you." : `Filled in from ${agent.tool_label}.`} Other agents read it to pick who to ask.
          {#if agent.good_at_custom}
            <button class="link" onclick={() => reset({ op: "good-at", name: agent.name, items: null })}>Reset to defaults</button>
          {/if}
        </p>
      </div>

      <span class="k">Skills</span>
      <div class="tg">
        <button
          class="sw-k"
          class:on={shownShare}
          role="switch"
          aria-checked={shownShare}
          aria-label="Share my skills"
          onclick={() => reset(shownShare === agent.share ? { op: "share", name: agent.name, on: !agent.share } : null)}
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
          <button class="btn sm" onclick={() => reset({ op: "default", name: agent.name })}>Make default for {tool}</button>
          <p class="hint">Then typing just <code class="mono">{tool}</code> opens this account instead of the current default</p>
        {/if}
      </div>

      <span class="k">Sign in</span>
      <div>
        <button class="btn sm" onclick={signInAgain}>Sign in again</button>
        <p class="hint">Opens {agent.tool_label}'s own sign-in, to fix a sign-in or switch to another account</p>
        {#if actionError}<p class="note">{actionError}</p>{/if}
      </div>

      <span class="k">Remove</span>
      <div>
        <button class="btn sm danger" aria-label="Remove {agent.name}" onclick={() => reset({ op: "remove", name: agent.name })}>
          Remove from roster
        </button>
        <p class="hint">Removes its command. Its login folder stays on this computer, and you can undo this under What changed.</p>
      </div>
    </div>
  </div>

  {#if req}
    <section class="save" aria-label="Change waiting">
      <div class="what" aria-live="polite">
        {#if error}
          <p class="error">{error}</p>
        {:else if plan}
          <b>{plan.summary}</b>
          {#each plan.steps as s, i (i)}<span>{s.text}</span>{/each}
        {:else}
          <span class="dim">Looking at what will change…</span>
        {/if}
      </div>
      <div class="acts">
        <button class="btn" onclick={() => reset()} disabled={busy}>Cancel</button>
        <button class="btn pri" class:danger-pri={req.op === "remove"} onclick={go} disabled={!plan || busy}>{applyLabel}</button>
      </div>
    </section>
  {/if}
</div>

<style>
  .page-wrap {
    height: 100%;
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 24px 32px 32px;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .top {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 16px;
  }
  .who {
    min-width: 0;
  }
  h2 {
    margin: 0;
    font-size: 34px;
    line-height: 40px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .meta {
    margin-top: 6px;
    display: flex;
    flex-wrap: wrap;
    gap: 4px 12px;
    font-size: 12px;
    line-height: 16px;
    color: var(--mut);
  }
  .meta b {
    color: var(--txt);
    font-weight: 500;
  }
  .st {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-weight: 500;
  }
  .st i {
    width: 6px;
    height: 6px;
    border-radius: 50%;
  }
  .callout {
    margin: -4px 0 0;
    font-size: 13px;
    color: var(--mut);
    display: flex;
    align-items: center;
    gap: 6px;
  }
  code {
    font-size: 12px;
    color: var(--txt);
    padding: 0 6px;
    border-radius: 4px;
    background: var(--raise);
    border: 1px solid var(--line);
  }
  .cards {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }
  .card {
    background: var(--raise);
    border: 1px solid var(--line);
    border-radius: 12px;
    padding: 12px 16px;
    min-height: 92px;
  }
  .lab {
    font-size: 12px;
    color: var(--mut);
  }
  .val {
    font-size: 24px;
    line-height: 32px;
    font-weight: 500;
    margin: 2px 0 6px;
  }
  .val.dim {
    font-size: 14px;
    line-height: 20px;
    color: var(--mut);
    font-weight: 400;
    margin: 8px 0 4px;
  }
  .track {
    display: block;
    height: 6px;
    border-radius: 3px;
    background: var(--hole);
    overflow: hidden;
  }
  .track i {
    display: block;
    height: 100%;
  }
  .sub {
    font-size: 12px;
    line-height: 16px;
    color: var(--dim);
    margin-top: 6px;
  }
  .usage-line {
    margin: -4px 0 0;
    font-size: 12px;
    line-height: 16px;
    color: var(--mut);
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .usage-line.quiet {
    color: var(--dim);
  }
  .grid {
    display: grid;
    grid-template-columns: 84px minmax(0, 1fr);
    gap: 16px 20px;
    align-items: start;
    border-top: 1px solid var(--line);
    padding-top: 20px;
    margin-top: 4px;
  }
  .k {
    font-size: 13px;
    line-height: 20px;
    color: var(--mut);
    padding-top: 6px;
  }
  .name {
    width: 100%;
    max-width: 420px;
    height: 36px;
    background: var(--raise);
    border: 1px solid var(--line);
    border-radius: 8px;
    padding: 0 12px;
    font-family: inherit;
    font-size: 18px;
    outline: none;
  }
  .name:focus-visible {
    border-color: var(--line-2);
  }
  .hint {
    margin: 6px 0 0;
    font-size: 12px;
    line-height: 16px;
    color: var(--dim);
  }
  .hint .link {
    margin-left: 6px;
  }
  .plain {
    margin: 0;
    padding-top: 6px;
    font-size: 13px;
  }
  .sw {
    display: flex;
    gap: 8px;
    padding-top: 4px;
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
  .tags {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    align-items: center;
  }
  .tag {
    height: 28px;
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 0 4px 0 10px;
    border-radius: 6px;
    background: var(--raise);
    border: 1px solid var(--line);
    font-size: 12px;
  }
  .tag button {
    width: 20px;
    height: 20px;
    border: 0;
    border-radius: 4px;
    background: none;
    color: var(--dim);
    font-size: 14px;
    line-height: 1;
  }
  .tag button:hover {
    color: var(--txt);
    background: var(--well);
  }
  .add {
    height: 28px;
    width: 120px;
    border: 1px dashed var(--line-2);
    border-radius: 6px;
    background: none;
    color: var(--txt);
    padding: 0 10px;
    font: inherit;
    font-size: 12px;
  }
  .add::placeholder {
    color: var(--mut);
  }
  .tg {
    display: flex;
    align-items: center;
    gap: 12px;
    font-size: 13px;
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
  .danger {
    color: var(--coral);
  }
  .note {
    margin: 6px 0 0;
    font-size: 12px;
    color: var(--jade);
  }
  .save {
    flex: none;
    border-top: 1px solid var(--line-2);
    background: var(--well);
    padding: 12px 32px;
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 16px;
    align-items: center;
  }
  .what {
    display: flex;
    flex-direction: column;
    font-size: 12px;
    line-height: 18px;
    color: var(--mut);
    min-width: 0;
  }
  .what b {
    font-size: 13px;
    color: var(--txt);
    font-weight: 500;
  }
  .what .error {
    margin: 0;
  }
  .dim {
    color: var(--dim);
  }
  .acts {
    display: flex;
    gap: 8px;
  }
  .danger-pri {
    background: var(--coral);
    border-color: var(--coral);
    color: #ffffff;
  }
</style>
