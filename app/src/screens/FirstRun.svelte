<script lang="ts">
  import { onMount } from "svelte";
  import type { ColorView, FirstRunView, PlanView, Tool } from "../lib/types";
  import { firstRun, message, preview } from "../lib/api";
  import PlanSheet from "../components/PlanSheet.svelte";

  let { palette, onDone }: { palette: ColorView[]; onDone: () => void } = $props();

  const COUNT = ["no", "one", "two"];

  let fr = $state<FirstRunView | null>(null);
  let names = $state<Record<string, string>>({});
  let editing = $state<Record<string, boolean>>({});
  let error = $state<string | null>(null);
  let sheet = $state<{ plan: PlanView; label: string } | null>(null);

  const installed = $derived(fr ? fr.candidates.filter((c) => c.installed) : []);
  const title = $derived(
    installed.length === 0
      ? "No Claude Code or Codex login was found."
      : `You already have ${COUNT[installed.length] ?? installed.length} agent${installed.length === 1 ? "" : "s"}.`,
  );

  onMount(async () => {
    try {
      fr = await firstRun();
      names = Object.fromEntries(fr.candidates.map((c) => [c.tool, c.suggested_name]));
    } catch (e) {
      error = message(e);
    }
  });

  async function open(req: Parameters<typeof preview>[0], label: string) {
    error = null;
    try {
      sheet = { plan: await preview(req), label };
    } catch (e) {
      error = message(e);
    }
  }

  function adopt() {
    const picks = installed.map((c) => [c.tool, names[c.tool].trim()] as [Tool, string]);
    open({ op: "adopt-all", picks }, "Put them on the roster");
  }
</script>

<div class="pad first">
  {#if !fr}
    {#if error}<p class="error">{error}</p>{:else}<p class="hello-s">Looking for your logins…</p>{/if}
  {:else}
    <div class="hello">{title}</div>
    <p class="hello-s">
      {#if installed.length}
        We found these {installed.every((c) => c.signed_in) ? "signed in " : ""}on this computer. Give them names, or keep ours. Nothing changes until you continue.
      {:else}
        Install Claude Code or Codex, sign in once, then open Agent Roster again.
      {/if}
    </p>

    {#if fr.prototype}
      <div class="proto">
        <span>Your earlier agent-roster script's agents were found.</span>
        <button class="btn sm" onclick={() => open({ op: "import" }, "Import")}>Bring them in</button>
      </div>
    {/if}

    {#each fr.candidates as c, i (c.tool)}
      <div class="found" class:missing={!c.installed}>
        {#if c.installed && editing[c.tool]}
          <input
            class="nm2 wide"
            aria-label="New name for {c.suggested_name}"
            style:color={palette[i]?.hex}
            bind:value={names[c.tool]}
            spellcheck="false"
          />
        {:else}
          <span class="nm2 wide" style:color={c.installed ? palette[i]?.hex : "var(--mut)"}>
            {c.installed ? names[c.tool] : c.tool_label}
          </span>
        {/if}
        <span class="meta">
          {#if c.installed}
            <span>{c.tool_label}{c.version ? ` ${c.version}` : ""}</span><br />
            <span>{c.email ?? (c.signed_in ? "signed in" : "not signed in yet")}</span>
          {:else}
            not installed
          {/if}
        </span>
        {#if c.installed && !editing[c.tool]}
          <button class="btn sm" onclick={() => (editing[c.tool] = true)}>Rename</button>
        {/if}
      </div>
    {/each}
    <div class="found missing">
      <span class="nm2 wide" style:color="var(--mut)">Gemini</span><span class="meta">soon</span>
    </div>

    {#if error}<p class="error">{error}</p>{/if}
    <div class="foot">
      <button class="btn pri" onclick={adopt} disabled={installed.length === 0}>Continue</button>
    </div>
  {/if}
</div>

{#if sheet}
  <PlanSheet plan={sheet.plan} confirmLabel={sheet.label} onDone={() => onDone()} onCancel={() => (sheet = null)} />
{/if}

<style>
  .first {
    margin: 0 auto;
    padding-top: 48px;
  }
  .found {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 14px 0;
    border-bottom: 1px solid var(--line);
  }
  .found:last-of-type {
    border-bottom: 0;
  }
  .found.missing {
    opacity: 0.5;
  }
  .nm2 {
    font-size: 22px;
    min-width: 140px;
  }
  input.nm2 {
    width: 160px;
    background: var(--raise);
    border: 1px solid var(--line);
    border-radius: 8px;
    padding: 2px 8px;
    font-family: inherit;
  }
  .meta {
    flex: 1;
    font-size: 12.5px;
    color: var(--mut);
    line-height: 1.5;
  }
  .proto {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    background: var(--well);
    border: 1px solid var(--line);
    border-radius: 10px;
    padding: 10px 14px;
    font-size: 13px;
    margin: 6px 0 8px;
  }
</style>
