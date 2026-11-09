<script lang="ts">
  import type { OverviewView } from "../lib/types";

  let { view }: { view: OverviewView } = $props();
</script>

<aside class="side">
  <section>
    <h4>On the line now</h4>
    {#each view.calls as c (c.from_id + c.to_id)}
      <div class="call" style:border-color={c.from_color}>
        <div class="who">
          <b class="wide" style:color={c.from_color}>{c.from_name}</b> asked
          <b class="wide" style:color={c.to_color}>{c.to_name}</b>
        </div>
        {#if c.task}<div class="what">{c.task}</div>{/if}
        <div class="t">{c.detail}</div>
      </div>
    {:else}
      <p class="quiet">Nobody is on the line.</p>
    {/each}
  </section>
  <section>
    <h4>Type these in any terminal</h4>
    <div class="cmds mono">
      {#each view.agents as a (a.id)}<span style:color={a.color}>{a.name}</span>{/each}
    </div>
  </section>
  <section>
    <h4>Limits</h4>
    {#each view.warnings as w (w.name + w.text)}
      <p class="warn"><b class="wide hot">{w.name}</b> {w.text}</p>
    {:else}
      <p class="warn">No agent is near its limit.</p>
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
    padding: 22px 22px 18px 68px;
    display: flex;
    flex-direction: column;
    gap: 18px;
    overflow-y: auto;
  }
  h4 {
    margin: 0 0 8px;
    font-size: 13px;
    color: var(--mut);
    font-weight: 600;
  }
  section p {
    margin: 0;
  }
  .call {
    border-left: 2px solid;
    padding: 2px 0 2px 14px;
    margin-bottom: 10px;
  }
  .call .who {
    font-size: 13px;
    color: var(--mut);
  }
  .call .who b {
    font-size: 14px;
  }
  .call .what {
    font-size: 15px;
    line-height: 1.4;
    margin: 6px 0 8px;
    user-select: text;
  }
  .call .t {
    font-size: 12px;
    color: var(--dim);
  }
  .cmds {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .cmds span {
    font-size: 12.5px;
    padding: 5px 9px;
    border-radius: 7px;
    background: var(--raise);
    border: 1px solid var(--line);
    user-select: text;
  }
  .warn {
    font-size: 13px;
    line-height: 1.5;
    color: var(--txt);
    margin-bottom: 6px !important;
  }
  .quiet {
    font-size: 12px;
    color: var(--dim);
  }
  .safe {
    margin-top: auto;
    font-size: 12.5px;
    color: var(--mut);
    line-height: 1.5;
  }
  .safe b {
    color: var(--jade);
    font-weight: 600;
  }
</style>
