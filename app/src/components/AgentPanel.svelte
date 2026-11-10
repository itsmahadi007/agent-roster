<script lang="ts">
  import type { AgentView, ColorView, OpRequest, PlanView, TerminalApp } from "../lib/types";
  import { apply, copyText, message, preview, signIn } from "../lib/api";
  import { rememberedTerminal } from "../lib/terminal";
  import { clearLogin, logins, prefs, setSignInWith, signInInApp, type SignInWith } from "../lib/signin.svelte";
  import TerminalButton from "./TerminalButton.svelte";
  import { agentHex, BAR, colorName, swatches, usageLevel } from "../lib/colors";
  import RolePicker from "./RolePicker.svelte";
  import { onDestroy } from "svelte";

  let {
    agent,
    palette,
    terminals,
    updated = null,
    nudge = 0,
    onApplied,
    onDirty,
    onClose,
  }: {
    agent: AgentView;
    palette: ColorView[];
    terminals: TerminalApp[];
    /** When the numbers were last checked ("Last update 4m ago."). */
    updated?: string | null;
    /** Goes up each time the person tries to close or switch rows while a change waits. */
    nudge?: number;
    /** A change went through: what to tell the person, and which kind it was. */
    onApplied: (notice: string, op: OpRequest["op"]) => void;
    /** Whether a change is waiting in the save bar. */
    onDirty: (dirty: boolean) => void;
    /** Escape with nothing waiting. */
    onClose: () => void;
  } = $props();

  // The panel is mounted again after every change, so starting from the agent is enough.
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
  /** Why the last Apply failed; kept while the same change is looked at again. */
  let applyError = $state<string | null>(null);
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
  const colors = $derived(swatches(palette));
  const hex = $derived(colors.find((c) => c.value === shownColor)?.hex ?? agentHex(agent.color));
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
        return `${r.name} is ${colorName(r.color)} now.`;
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
        return `Claude Code's status line now reports ${r.name}'s use too.`;
      case "disable-limits":
        return `Your old status line is back for ${r.name}. Agent Roster still asks Claude Code itself for its use.`;
      case "remove":
        return `${r.name} was removed. Its login folder is still there, and you can undo this under What changed.`;
      default:
        return "Done.";
    }
  }

  // The list refreshes every 10 seconds with fresh objects; compare by value so a waiting
  // preview isn't redone each time.
  const reqKey = $derived(req ? JSON.stringify(req) : "");

  // svelte-ignore state_referenced_locally
  const nudgeAtStart = nudge;
  /** The nudge count when the current change began; a higher one means "apply or cancel first". */
  let nudgeSeen = $state(nudgeAtStart);
  const nudged = $derived(req !== null && nudge > nudgeSeen);
  $effect(() => {
    const dirty = reqKey !== "";
    onDirty(dirty);
    if (!dirty) nudgeSeen = nudge;
  });
  onDestroy(() => onDirty(false));
  $effect(() => {
    void reqKey;
    applyError = null;
  });

  let seq = 0;
  $effect(() => {
    const r: OpRequest | null = reqKey ? JSON.parse(reqKey) : null;
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
    applyError = null;
    try {
      await apply(plan.token);
    } catch (e) {
      // The preview is used up or stale: say why and look again.
      applyError = message(e);
      busy = false;
      retry += 1;
      return;
    }
    // Stays busy: this panel is replaced once the list has the new state.
    onApplied(notice(r), r.op);
  }

  /** Sign in, the way the setting says: the browser (a terminal running `roster login`) or an Agent Roster window. */
  async function signInAgain() {
    actionError = null;
    if (prefs.signInWith === "window") {
      await signInInApp(agent.name, agent.id);
      return;
    }
    await signInBrowser();
  }

  /** Today's sign-in: a terminal running `roster login`, which opens the browser. */
  async function signInBrowser() {
    actionError = null;
    clearLogin(agent.id);
    const app = terminals.find((t) => t.id === rememberedTerminal()) ?? null;
    if (app && !app.runs) await copyText(`roster login ${agent.name}`);
    try {
      const ran = await signIn(agent.name, app?.id ?? null);
      if (!ran && app) actionError = `roster login ${agent.name} is copied. Paste it in ${app.label} and press Return.`;
    } catch (e) {
      actionError = message(e);
    }
  }

  let settingError = $state<string | null>(null);
  async function chooseSignIn(w: SignInWith) {
    settingError = null;
    try {
      await setSignInWith(w);
    } catch (e) {
      settingError = message(e);
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

  // Also when the tool says signed in but the last check found the login expired.
  const signedOut = $derived(agent.health === "sign-in");
  const noNumbers = $derived(agent.windows.length === 0);
  const login = $derived(logins[agent.id]);
  /** The weekly limit; core sends no other window. */
  const week = $derived(agent.windows.find((w) => w.label === "week") ?? null);
  const level = $derived(week ? usageLevel(week.used_percent, agent.health === "limit-reached") : "ok");
  const used = $derived(week ? Math.max(0, Math.min(100, week.used_percent)) : 0);
  const resetLine = $derived(agent.reset_text ? agent.reset_text.charAt(0).toUpperCase() + agent.reset_text.slice(1) : null);
  const checkedLine = $derived(agent.as_of ?? (updated ? updated.replace(/^Last update/, "checked").replace(/\.$/, "") : null));
  const opensIn = $derived(prefs.signInWith === "window" ? "Opens in an Agent Roster window" : "Opens in your browser");
</script>

<svelte:window
  onkeydown={(e) => {
    // Escape closes an open menu first, then drops a waiting change, and only then closes the panel.
    if (e.key !== "Escape" || document.querySelector('[role="menu"]')) return;
    if (req) reset();
    else onClose();
  }}
/>

<div class="agent">
  <section class="sec" aria-labelledby="sec-week">
    <h4 id="sec-week">This week</h4>
    <div class="box">
      {#if login?.state === "failed"}
        <div class="sign-fail" role="alert">
          <span>{login.reason}</span>
          <button class="btn sm" onclick={signInBrowser}>Sign in with your browser</button>
        </div>
      {:else if login}
        <p class="line" role="status">Signing in… (finish in the Agent Roster window)</p>
      {/if}

      {#if signedOut}
        <p class="big-line">Not signed in yet</p>
        <p class="line">Weekly use shows after sign in.</p>
        <div class="acts-row">
          <button class="btn sm pri" onclick={signInAgain}>Sign in</button>
        </div>
        <p class="hint">{opensIn} · change it under Settings</p>
        {#if actionError}<p class="note">{actionError}</p>{/if}
      {:else if agent.state === "not-installed"}
        <p class="line">{agent.status_text}</p>
      {:else}
        {#if week}
          <div class="week-top">
            <b class="num" class:hot={level === "hot"} class:warn-use={level === "warn"} data-level={level}>
              {week.used_percent}% of the week used
            </b>
            <span class="num left">{Math.max(0, 100 - used)}% left</span>
          </div>
          <span class="track big" data-level={level}><i style:width="{used}%" style:background={BAR[level]}></i></span>
          {#if resetLine || checkedLine}
            <p class="hint num">{[resetLine, checkedLine].filter(Boolean).join(" · ")}</p>
          {/if}
        {/if}
        {#if agent.health === "checking"}
          <p class="line">Checking with {agent.tool_label}…</p>
        {:else if agent.health === "no-subscription"}
          <p class="line">
            This account is signed in with no {agent.tool_label} subscription (none, or it ended), so it has no weekly limit to show.
          </p>
        {:else if agent.health === "unreachable"}
          <p class="line">
            Couldn't reach {agent.tool_label} just now.{agent.as_of ? ` These numbers are ${agent.as_of}.` : " It tries again soon."}
          </p>
        {:else if agent.health === "limit-reached"}
          <p class="line hot">{agent.limit_text ?? `${agent.name} has reached a usage limit.`}</p>
        {:else if noNumbers}
          <p class="line">Weekly use shows after you use {agent.name}. Type {agent.name} in a terminal and send one message.</p>
        {/if}
        <p class="line quiet">
          Agent Roster asks {agent.tool_label} itself for this account's use, every 5 minutes. It never limits the agent.
        </p>
      {/if}
    </div>
  </section>

  <section class="sec" aria-labelledby="sec-use">
    <h4 id="sec-use">Use it</h4>
    <div class="acts-row">
      <TerminalButton name={agent.name} {terminals} />
      {#if terminals.length > 0}
        <button class="btn" onclick={copyName}>{copied ? "Copied" : "Copy command"}</button>
      {/if}
    </div>
    <p class="hint">Or type <code class="mono">{agent.name}</code> in any terminal.</p>
  </section>

  <section class="sec" aria-labelledby="sec-settings">
    <h4 id="sec-settings">Settings</h4>
    <div class="grid box">
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
          {#each colors as c (c.value)}
            <button
              role="radio"
              aria-checked={c.value === shownColor}
              aria-label={c.label}
              title={c.label}
              class:on={c.value === shownColor}
              style:background={c.hex}
              onclick={() => reset(c.value === agent.color_label ? null : { op: "color", name: agent.name, color: c.value })}
            ></button>
          {/each}
        </div>
        <p class="hint">Its dot and its name use this color</p>
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
          {#each good as t, i (i)}
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

      {#if agent.claude_limits === true}
        <span class="k">Status line</span>
        <div>
          <p class="plain">Claude Code's status line reports its use too.</p>
          <button class="btn sm" onclick={() => reset({ op: "disable-limits", name: agent.name })}>Hide usage</button>
        </div>
      {/if}

      <span class="k">Account</span>
      <div>
        <p class="plain acct">
          {#if agent.email}<span>{agent.email}</span>{:else if signedOut}<span class="dim">Not signed in yet</span>{/if}
          {#if agent.plan}<span class="dim">{agent.plan}</span>{/if}
        </p>
        {#if !signedOut}
          <button class="btn sm" onclick={signInAgain}>Switch account</button>
          <p class="hint">Opens {agent.tool_label}'s own sign-in, to fix a sign-in or switch to another account</p>
          {#if actionError}<p class="note">{actionError}</p>{/if}
        {/if}
        <div class="with">
          <span class="dim">Sign in with</span>
          <div class="seg" role="radiogroup" aria-label="Sign in with">
            <button role="radio" aria-checked={prefs.signInWith === "browser"} onclick={() => chooseSignIn("browser")}>Your browser</button>
            <button role="radio" aria-checked={prefs.signInWith === "window"} onclick={() => chooseSignIn("window")}
              >Agent Roster window</button
            >
          </div>
        </div>
        <p class="hint">
          For every agent. {prefs.signInWith === "window"
            ? "The sign-in page opens in a private Agent Roster window. Some sign-ins (Microsoft, Apple, company) need your browser."
            : "Your browser opens the sign-in page."}
        </p>
        {#if settingError}<p class="hint">{settingError}</p>{/if}
      </div>
    </div>
  </section>

  <section class="sec danger-zone" aria-label="Remove">
    <button class="btn sm danger" onclick={() => reset({ op: "remove", name: agent.name })}>Remove {agent.name}</button>
    <p class="hint">Removes its command. Its login folder stays on this computer, and you can undo this under What changed.</p>
  </section>

  {#if req}
    <section class="save" aria-label="Change waiting">
      <div class="what" aria-live="polite">
        {#if nudged}<p class="nudge">Apply or cancel this change first.</p>{/if}
        {#if applyError}<p class="error">{applyError}</p>{/if}
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
  .agent {
    display: flex;
    flex-direction: column;
    min-height: 100%;
  }
  .sec {
    margin: 0 0 16px;
  }
  h4 {
    margin: 0 0 8px;
    font-size: 11px;
    line-height: 16px;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    font-weight: 600;
    color: var(--dim);
  }
  .box {
    background: var(--raise);
    border: 1px solid var(--line);
    border-radius: 10px;
    padding: 12px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .week-top {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: 8px;
    font-size: 13px;
  }
  .week-top b {
    font-weight: 600;
    color: var(--txt);
  }
  .left {
    color: var(--mut);
  }
  .big-line {
    margin: 0;
    font-weight: 600;
    font-size: 14px;
  }
  .track {
    display: block;
    height: 8px;
    border-radius: 4px;
    background: var(--hole);
    overflow: hidden;
  }
  .track i {
    display: block;
    height: 100%;
  }
  .acts-row {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    align-items: center;
  }
  code {
    font-size: 12px;
    color: var(--txt);
    padding: 0 6px;
    border-radius: 4px;
    background: var(--raise);
    border: 1px solid var(--line);
  }
  .line {
    margin: 0;
    font-size: 12px;
    line-height: 16px;
    color: var(--mut);
  }
  .line.quiet {
    color: var(--dim);
  }
  .grid {
    display: grid;
    grid-template-columns: 72px minmax(0, 1fr);
    gap: 16px 14px;
    align-items: start;
  }
  .k {
    font-size: 13px;
    line-height: 20px;
    color: var(--mut);
    padding-top: 6px;
  }
  .name {
    width: 100%;
    height: 34px;
    background: var(--well);
    border: 1px solid var(--line);
    border-radius: 8px;
    padding: 0 10px;
    font-family: inherit;
    font-size: 16px;
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
  .box > .hint {
    margin: 0;
  }
  .hint .link {
    margin-left: 6px;
  }
  .plain {
    margin: 0;
    padding-top: 6px;
    font-size: 13px;
  }
  .acct {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 10px;
    padding: 6px 0 8px;
  }
  .acct:empty {
    display: none;
  }
  .sw {
    display: flex;
    gap: 8px;
    padding-top: 4px;
  }
  .sw button {
    width: 22px;
    height: 22px;
    border-radius: 50%;
    border: 0;
    padding: 0;
  }
  .sw button.on {
    box-shadow:
      0 0 0 2px var(--raise),
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
    background: var(--well);
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
    background: var(--raise);
  }
  .add {
    height: 28px;
    width: 100px;
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
  .danger-zone {
    margin-top: 4px;
    padding-top: 14px;
    border-top: 1px dashed var(--line-2);
    margin-bottom: 18px;
  }
  .danger {
    color: var(--u-hot);
  }
  .sign-fail {
    display: flex;
    align-items: center;
    gap: 12px;
    font-size: 12px;
    line-height: 16px;
    color: var(--u-hot);
  }
  .with {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 10px;
    margin-top: 10px;
    font-size: 12px;
  }
  .seg {
    display: inline-flex;
    border: 1px solid var(--line);
    border-radius: 6px;
    overflow: hidden;
  }
  .seg button {
    height: 26px;
    padding: 0 10px;
    border: 0;
    background: none;
    color: var(--mut);
    font: inherit;
    font-size: 12px;
  }
  .seg button + button {
    border-left: 1px solid var(--line);
  }
  .seg button[aria-checked="true"] {
    background: var(--well);
    color: var(--txt);
  }
  .note {
    margin: 6px 0 0;
    font-size: 12px;
    color: var(--jade);
  }
  .save {
    position: sticky;
    bottom: 0;
    z-index: 5;
    margin: auto -18px 0;
    border-top: 1px solid var(--line-2);
    background: var(--raise);
    padding: 12px 18px;
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 12px;
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
  .what .error,
  .what .nudge {
    margin: 0;
  }
  .what .nudge {
    color: var(--u-hot);
    font-weight: 500;
  }
  .dim {
    color: var(--dim);
  }
  .acts {
    display: flex;
    gap: 8px;
  }
  .danger-pri {
    background: var(--u-hot);
    border-color: var(--u-hot);
    color: #ffffff;
  }
</style>
