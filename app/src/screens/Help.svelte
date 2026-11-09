<script lang="ts">
  import type { OverviewView } from "../lib/types";
  import PageHead from "../components/PageHead.svelte";

  let { view, onBack }: { view: OverviewView; onBack: () => void } = $props();

  const first = $derived(view.agents[0]);
  const ex = $derived(first?.name ?? "agent1");
  const other = $derived(view.agents[1]?.name ?? "agent2");
  const tool = $derived(first?.tool_label ?? "Claude Code");
</script>

<div class="page help">
  <PageHead title="How Agent Roster works" sub="A short tour of this window, in the order you'll need it" {onBack} />
  <div class="grid">
    <section>
      <h4>What an agent is</h4>
      <p>
        One Claude Code or Codex login with a name you chose. Each agent has its own sign-in and its own limits, so when
        one runs low you can carry on with another.
      </p>
    </section>
    <section>
      <h4>Start one</h4>
      <p>
        Type its name in any terminal, like <code class="mono">{ex}</code>. It opens {tool} signed in as that account, in
        the folder you're in. If a new name isn't found, open a new terminal window.
      </p>
    </section>
    <section>
      <h4>Hand work to another agent</h4>
      <p>
        Inside a session, ask for another agent by name, like "ask {other} to write the tests". While it works, a cable
        joins the two jacks on the right and the call shows under On the line now.
      </p>
    </section>
    <section>
      <h4>Limits</h4>
      <p>
        The bars show how much of each limit an account has used: 5 hr is the 5-hour limit, week the weekly one. They turn
        red at 90%, and update each time the agent is used. Claude Code limits stay off until you click Turn on limits on
        its row.
      </p>
    </section>
    <section>
      <h4>What a row says</h4>
      <dl>
        <dt>ready</dt><dd>signed in and idle</dd>
        <dt>working</dt><dd>a session is open</dd>
        <dt>calling …</dt><dd>it handed work to that agent</dd>
        <dt>Sign in</dt><dd>it needs a sign-in first</dd>
        <dt>default for claude</dt><dd>a plain <code class="mono">claude</code> opens this account</dd>
      </dl>
    </section>
    <section>
      <h4>Sign in</h4>
      <p>
        A new agent signs in once. Sign in opens a terminal with the tool's own sign-in for that account. Agent Roster
        never sees your password and never reads your login files.
      </p>
    </section>
    <section>
      <h4>Rename and more</h4>
      <p>
        Click an agent's name to rename it. Its login stays signed in. Color, role, default and remove are in the terminal
        for now: run <code class="mono">roster --help</code>.
      </p>
    </section>
    <section>
      <h4>Nothing changes without asking</h4>
      <p>
        Every change shows a preview first. Each one is kept under What changed with a backup, and can be undone one at a
        time or all at once. No hooks are ever added to your tools.
      </p>
    </section>
    <section>
      <h4>Menu bar</h4>
      <p>
        The Agent Roster icon in the menu bar (the tray on Windows and Linux) lists every agent with its weekly use, and
        opens a terminal as any of them without this window.
      </p>
    </section>
  </div>
</div>

<style>
  .help {
    max-width: 1040px;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(296px, 1fr));
    gap: 24px 32px;
  }
  h4 {
    margin: 0 0 4px;
    font-size: 14px;
    line-height: 20px;
    font-weight: 600;
  }
  p {
    margin: 0;
    font-size: 13px;
    line-height: 20px;
    color: var(--mut);
    user-select: text;
  }
  dl {
    margin: 0;
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 0 12px;
    font-size: 13px;
    line-height: 20px;
  }
  dt {
    color: var(--txt);
    font-weight: 500;
  }
  dd {
    margin: 0;
    color: var(--mut);
  }
  code {
    font-size: 12px;
    color: var(--txt);
    padding: 0 4px;
    border-radius: 4px;
    background: var(--raise);
    border: 1px solid var(--line);
  }
</style>
