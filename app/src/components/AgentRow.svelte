<script lang="ts">
  import type { AgentView } from "../lib/types";
  import { message, signIn } from "../lib/api";

  let {
    agent,
    onLimits = () => {},
  }: { agent: AgentView; onLimits?: (name: string, on: boolean) => void } = $props();

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
</script>

<div class="ag" data-agent={agent.name}>
  <span class="stripe" style:background={agent.color}></span>
  <div class="who">
    <div class="nm wide" style:color={agent.color}>{agent.name}</div>
    <div class="sub">
      <span class="tool">{agent.tool_label}</span>
      {#if agent.email}
        <span>{agent.email}</span>
      {:else if agent.state === "sign-in"}
        <span>not signed in yet</span>
      {:else if agent.plan}
        <span>{agent.plan}</span>
      {/if}
      {#if agent.default_label}<span>{agent.default_label}</span>{/if}
    </div>
  </div>
  <div class="use">
    {#each agent.windows as w (w.label)}
      <div class="ul" data-window={w.label}>
        <span>{w.label}</span>
        <b><i class:hot={w.hot} style:width="{w.used_percent}%" style:background={w.hot ? "var(--coral)" : agent.color}></i></b>
        <em class="num" class:hot={w.hot}>{w.used_percent}%</em>
      </div>
    {/each}
    {#if agent.reset_text}
      <div class="rs">
        <span class="num" class:hot={agent.reset_hot}>{agent.reset_text}</span>
      </div>
    {/if}
    {#if agent.limits_note}
      <div class="rs note">
        {agent.limits_note}
        {#if agent.claude_limits === false && agent.state !== "sign-in" && agent.state !== "not-installed"}
          <button class="link" onclick={() => onLimits(agent.name, true)}>Turn on limits</button>
        {/if}
      </div>
    {/if}
  </div>
  <div class="st">
    {#if agent.state === "sign-in"}
      <button class="btn sm" style:border-color={agent.color} style:color={agent.color} onclick={doSignIn}>Sign in</button>
    {:else}
      <span class="state" style:color={active ? agent.color : agent.state === "not-installed" ? "var(--dim)" : "var(--mut)"}>
        <i style:background={active ? agent.color : "var(--line-2)"}></i>{agent.status_text}
      </span>
    {/if}
    {#if agent.claude_limits === true}
      <button class="link off" onclick={() => onLimits(agent.name, false)}>Turn off limits</button>
    {/if}
    {#if error}<span class="error" title={error}>{error}</span>{/if}
  </div>
  <div class="jack" style:color={agent.color}><b class:live={agent.live}></b></div>
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
    font-size: 30px;
    letter-spacing: -0.01em;
    line-height: 32px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
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
    box-shadow: 0 0 0 4px rgba(255, 255, 255, 0.04);
  }
</style>
