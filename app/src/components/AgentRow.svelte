<script lang="ts">
  import type { AgentView } from "../lib/types";
  import { message, signIn } from "../lib/api";

  let {
    agent,
    onLimits = () => {},
    onEdit = () => {},
  }: {
    agent: AgentView;
    onLimits?: (name: string, on: boolean) => void;
    onEdit?: (name: string) => void;
  } = $props();

  /** "5 hr" reads as "5-hour", "week" as "weekly". */
  const limitName = (label: string) => (label === "week" ? "weekly" : label.replace(/ hr$/, "-hour"));

  let error = $state<string | null>(null);

  async function doSignIn() {
    error = null;
    try {
      await signIn(agent.name);
    } catch (e) {
      error = message(e);
    }
  }

  const active = $derived(agent.state === "working" || agent.live);

  /** Hover text for the line shown where the bars will be. */
  const noteTitle = $derived.by(() => {
    if (agent.state === "sign-in") return `Sign in first. Then use ${agent.name} once and its numbers show here.`;
    if (agent.claude_limits === false) return "Claude Code shows its use in its status line. Agent Roster reads it from there once you click Show usage.";
    if (agent.state === "not-installed") return `${agent.tool_label} isn't installed on this computer`;
    return `${agent.tool_label} writes down its 5-hour and weekly use while it runs. Type ${agent.name} in a terminal and send one message, and the numbers show here.`;
  });
</script>

<div class="ag" data-agent={agent.name}>
  <span class="stripe" style:background={agent.color}></span>
  <div class="who">
    <button
      class="nm agent-name ink"
      style:color={agent.color} style:--c={agent.color}
      aria-label="Edit {agent.name}"
      title="Rename, recolor, set its role, make it the default or remove it"
      onclick={() => onEdit(agent.name)}>{agent.name}</button
    >
    <div class="sub">
      <span class="tool">{agent.tool_label}</span>
      {#if agent.email}
        <span>{agent.email}</span>
      {:else if agent.state === "sign-in"}
        <span>not signed in yet</span>
      {:else if agent.plan}
        <span>{agent.plan}</span>
      {/if}
      {#if agent.default_label}<span title="A plain {agent.tool} command opens this account in new terminals">{agent.default_label}</span>{/if}
    </div>
  </div>
  <div class="use">
    {#each agent.windows as w (w.label)}
      <div class="ul" data-window={w.label} title="{w.used_percent}% of the {limitName(w.label)} limit used">
        <span>{w.label}</span>
        <b><i class:hot={w.hot} style:width="{w.used_percent}%" style:background={w.hot ? "var(--coral)" : agent.color}></i></b>
        <em class="num" class:hot={w.hot}>{w.used_percent}%</em>
      </div>
    {/each}
    {#if agent.reset_text}
      <div class="rs" title="When the limit starts again from 0%">
        <span class="num" class:hot={agent.reset_hot}>{agent.reset_text}</span>
      </div>
    {/if}
    {#if agent.limits_note}
      <div class="rs note">
        <span title={noteTitle}>{agent.limits_note}</span>
        {#if agent.claude_limits === false && agent.state !== "sign-in" && agent.state !== "not-installed"}
          <button
            class="link"
            title="Lets Agent Roster read the 5-hour and weekly use that Claude Code shows in its status line. It never limits the agent. You see a preview first."
            onclick={() => onLimits(agent.name, true)}>Show usage</button
          >
        {/if}
      </div>
    {/if}
  </div>
  <div class="st">
    {#if agent.state === "sign-in"}
      <button class="btn sm ink" style:border-color={agent.color} style:color={agent.color} style:--c={agent.color} title="Opens a terminal with {agent.tool_label}'s own sign-in for this account" onclick={doSignIn}>Sign in</button>
    {:else}
      <span class="state" class:ink={active} style:color={active ? agent.color : agent.state === "not-installed" ? "var(--dim)" : "var(--mut)"} style:--c={agent.color}>
        <i style:background={active ? agent.color : "var(--line-2)"}></i>{agent.status_text}
      </span>
    {/if}
    {#if agent.claude_limits === true}
      <button class="link off" title="Stops reading this account's usage and puts your old status line back" onclick={() => onLimits(agent.name, false)}>Hide usage</button>
    {/if}
    {#if error}<span class="error" title={error}>{error}</span>{/if}
  </div>
  <div class="jack" style:color={agent.color} title={agent.live ? "On a call with another agent" : "Not on a call. Lights up when this agent calls or is called by another."}>
    <b class:live={agent.live}></b>
  </div>
</div>

<style>
  .ag {
    display: grid;
    grid-template-columns: 4px 1fr 208px 128px 44px;
    align-items: center;
    gap: 16px;
    height: 76px;
    margin-bottom: 8px;
    background: var(--raise);
    border: 1px solid var(--line);
    border-right: 0;
    border-radius: 12px 0 0 12px;
    transition: border-color 150ms var(--ease);
  }
  .ag:hover {
    border-color: var(--line-2);
  }
  .stripe {
    height: 100%;
    border-radius: 11px 0 0 11px;
  }
  .who {
    min-width: 0;
  }
  .nm {
    display: block;
    max-width: 100%;
    background: none;
    border: 0;
    padding: 0;
    font-family: inherit;
    text-align: left;
    cursor: pointer;
    font-size: 30px;
    line-height: 34px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .nm:hover {
    text-decoration: underline dotted;
    text-decoration-thickness: 2px;
    text-underline-offset: 6px;
  }
  .nm:focus-visible {
    outline: 2px solid var(--txt);
    outline-offset: 2px;
    border-radius: 4px;
  }
  .sub {
    display: flex;
    gap: 12px;
    margin-top: 4px;
    font-size: 13px;
    line-height: 20px;
    color: var(--mut);
    white-space: nowrap;
    overflow: hidden;
  }
  .tool {
    color: var(--txt);
    font-weight: 500;
  }
  .use {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 12px;
    line-height: 16px;
    color: var(--mut);
  }
  .ul {
    display: grid;
    grid-template-columns: 40px 1fr 36px;
    align-items: center;
    gap: 8px;
  }
  .ul b {
    height: 6px;
    border-radius: 3px;
    background: var(--hole);
    border: 1px solid var(--line);
    overflow: hidden;
    display: block;
  }
  .ul b i {
    display: block;
    height: 100%;
    border-radius: 2px;
  }
  .ul em {
    font-style: normal;
    text-align: right;
    color: var(--txt);
    font-weight: 500;
  }
  .rs {
    font-size: 12px;
    line-height: 16px;
    color: var(--dim);
    padding-left: 48px;
    white-space: nowrap;
  }
  .rs.note {
    padding-left: 0;
    white-space: normal;
    display: flex;
    flex-wrap: wrap;
    gap: 4px 8px;
  }
  .off {
    position: absolute;
    bottom: 8px;
    left: 0;
    opacity: 0;
    transition: opacity 150ms var(--ease);
  }
  .ag:hover .off,
  .off:focus-visible {
    opacity: 1;
  }
  .st {
    align-self: stretch;
    position: relative;
    display: flex;
    flex-direction: column;
    justify-content: center;
    align-items: flex-start;
    gap: 4px;
    padding-right: 8px;
    min-width: 0;
  }
  .state {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    font-weight: 500;
  }
  .state i {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    display: block;
    flex: none;
  }
  .st .error {
    font-size: 12px;
    max-width: 120px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .jack {
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--well);
    border-left: 1px solid var(--line);
  }
  .jack b {
    width: 16px;
    height: 16px;
    border-radius: 50%;
    border: 2px solid var(--line-2);
    background: var(--hole);
    display: block;
  }
  .jack b.live {
    border-color: currentColor;
    box-shadow: 0 0 0 4px var(--glow);
  }
</style>
