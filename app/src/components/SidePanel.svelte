<script lang="ts">
  import type { OverviewView } from "../lib/types";

  let { view }: { view: OverviewView } = $props();
</script>

<aside class="side">
  <section>
    <h4>On the line now</h4>
    {#each view.calls as c, i (i)}
      <div class="call" style:border-color={c.from_color}>
        <div class="who">
          <b class="agent-name ink" style:color={c.from_color} style:--c={c.from_color}>{c.from_name}</b> asked
          <b class="agent-name ink" style:color={c.to_color} style:--c={c.to_color}>{c.to_name}</b>
        </div>
        {#if c.task}<div class="what">{c.task}</div>{/if}
        <div class="t">{c.detail}</div>
      </div>
    {:else}
      <p class="warn">Nobody is on the line</p>
      <p class="quiet">When one agent asks another for help, the call shows here</p>
    {/each}
  </section>
  <section>
    <h4>Type these in any terminal</h4>
    <p class="quiet hint">Each name opens its own account. Inside a session, ask for another agent by name to hand it work.</p>
    <div class="cmds mono">
      {#each view.agents as a (a.id)}<span class="ink" style:color={a.color} style:--c={a.color}>{a.name}</span>{/each}
    </div>
  </section>
  <section>
    <h4>Limits</h4>
    {#each view.warnings as w, i (i)}
      <p class="warn"><b class="agent-name hot">{w.name}</b> {w.text}</p>
    {:else}
      <p class="warn">No agent is near its limit</p>
    {/each}
    {#if view.limits_updated}
      <p class="quiet">Updated each time an agent is used. {view.limits_updated}</p>
    {/if}
  </section>
  <p class="safe">
    Your original Claude and Codex setup is untouched. <b>No hooks added.</b> Every change can be undone.
  </p>
</aside>

<style>
  .side {
    background: var(--well);
    border-left: 1px solid var(--line);
    /* The left 64px is where the cable loops out of the jacks. */
    padding: 24px 24px 24px 64px;
    display: flex;
    flex-direction: column;
    gap: 24px;
    overflow-y: auto;
  }
  h4 {
    margin: 0 0 8px;
    font-size: 13px;
    line-height: 20px;
    color: var(--mut);
    font-weight: 500;
  }
  section p {
    margin: 0;
  }
  .call {
    border-left: 2px solid;
    padding: 0 0 0 12px;
    margin-bottom: 8px;
  }
  .call .who {
    font-size: 13px;
    color: var(--mut);
  }
  .call .who b {
    font-size: 14px;
  }
  .call .what {
    font-size: 14px;
    line-height: 20px;
    margin: 4px 0;
    user-select: text;
  }
  .call .t {
    font-size: 12px;
    line-height: 16px;
    color: var(--dim);
  }
  .cmds {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .cmds span {
    font-size: 12px;
    line-height: 16px;
    padding: 4px 8px;
    border-radius: 6px;
    background: var(--raise);
    border: 1px solid var(--line);
    user-select: text;
  }
  .warn {
    font-size: 13px;
    line-height: 20px;
    color: var(--txt);
  }
  .warn + .warn,
  .warn + .quiet {
    margin-top: 4px;
  }
  .hint {
    margin-bottom: 8px !important;
  }
  .quiet {
    font-size: 12px;
    line-height: 16px;
    color: var(--dim);
  }
  .safe {
    margin-top: auto;
    font-size: 12px;
    line-height: 20px;
    color: var(--mut);
  }
  .safe b {
    color: var(--jade);
    font-weight: 500;
  }
</style>
