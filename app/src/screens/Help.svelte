<script lang="ts">
  import type { OverviewView } from "../lib/types";
  import PageHead from "../components/PageHead.svelte";
  import { openSite } from "../lib/api";

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
      <h4>One account or many</h4>
      <p>
        Add an agent for every account you have, as many as you like. They all stay signed in and can run at the same
        time, each in its own terminal. One agent is always one login.
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
        Inside a session, ask for another agent by name, like "ask {other} to write the tests". While it works, the call
        shows under the list, and both rows say what they're doing.
      </p>
    </section>
    <section>
      <h4>Usage and limits</h4>
      <p>
        Each row's bar shows how much of its weekly limit an account has used, and when that limit resets. It turns coral
        at 90%. Agent Roster asks Claude Code and Codex themselves for these numbers when it starts and every 5 minutes,
        with each agent's own login and no model call, so nothing has to be open first. Refresh, under the list, asks
        again right away. A row also says when an account needs a sign-in, has no subscription, has reached its limit, or
        couldn't be reached (then it keeps the last numbers and says when they're from).</p>
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
        A new agent signs in once. Sign in opens a terminal with the tool's own sign-in for that account, which opens
        your browser. Or pick Sign in with: Agent Roster window (in an agent's row) to sign in inside a private Agent
        Roster window that shows the real sign-in page and its address in the title. Some sign-ins still need your
        browser; the window then says so. Agent Roster never collects your password and never reads your login files.
      </p>
    </section>
    <section>
      <h4>Change an agent</h4>
      <p>
        Click an agent's row to open it in place. There you can rename it, change its color, pick a ready-made role
        like Leader or Worker 1, edit what it's good at, share your skills with it, make it the default, sign in again or
        remove it. Each change waits in a bar at the bottom until you save it. Removing keeps its login folder.</p>
    </section>
    <section>
      <h4>Nothing changes without asking</h4>
      <p>
        Every change shows a preview first. Each one is kept under What changed with a backup, and can be undone one at a
        time or all at once. No hooks are ever added to your tools.
      </p>
    </section>
    <section>
      <h4>Menu bar and closing</h4>
      <p>
        The Agent Roster icon in the menu bar (the tray on Windows) lists every agent with its weekly use and opens a
        terminal as any of them. Closing this window keeps it running there. To stop it, choose Quit Agent Roster from that
        menu.
      </p>
    </section>
    <section>
      <h4>Updates</h4>
      <p>
        Agent Roster checks for a new version when it starts and every few hours. When one is out, the bottom of the list
        offers it: click Update now and the app installs it and restarts. Your agents and settings stay as they are. The
        version you have is shown there too, with Check for updates.
      </p>
    </section>
    <section>
      <h4>Light and dark</h4>
      <p>The window follows your system's light or dark setting. Change it there and the window follows.</p>
    </section>
    <section>
      <h4>From the terminal</h4>
      <p>
        Everything here also works as a command. Run <code class="mono">roster</code> to see your agents, or
        <code class="mono">roster --help</code> for the full list.
      </p>
    </section>
  </div>
  <p class="credit">
    Created by Mahadi Hassan ·
    <a
      href="https://mahadihassan.com"
      onclick={(e) => {
        e.preventDefault();
        openSite();
      }}>mahadihassan.com</a
    >
  </p>
</div>

<style>
  .credit {
    margin-top: 32px;
    font-size: 12px;
    color: var(--dim);
  }
  .credit a {
    color: var(--mut);
    text-decoration: underline;
    text-decoration-color: var(--line-2);
    text-underline-offset: 3px;
  }
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
