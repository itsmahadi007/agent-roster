<script lang="ts">
  import { onMount } from "svelte";
  import { getVersion } from "@tauri-apps/api/app";
  import { check, type Update } from "@tauri-apps/plugin-updater";
  import { relaunch } from "@tauri-apps/plugin-process";

  /** Looks for a new version on GitHub at start and every 6 hours. */
  const EVERY = 6 * 60 * 60 * 1000;

  let version = $state("");
  let update = $state<Update | null>(null);
  let dismissed = $state<string | null>(null);
  let status = $state<string | null>(null);
  let busy = $state(false);
  let progress = $state<number | null>(null);

  async function look(asked: boolean) {
    if (busy) return;
    if (asked) status = "Checking…";
    try {
      const u = await check();
      update = u;
      if (asked) {
        dismissed = null;
        status = u ? null : "You have the latest version";
      }
    } catch {
      // Offline or GitHub unreachable: only say so when the person asked.
      if (asked) status = "Couldn't check for updates. Try again later.";
    }
  }

  async function install() {
    if (!update) return;
    busy = true;
    status = "Downloading…";
    let total = 0;
    let got = 0;
    try {
      await update.downloadAndInstall((e) => {
        if (e.event === "Started") total = e.data.contentLength ?? 0;
        if (e.event === "Progress") {
          got += e.data.chunkLength;
          progress = total ? Math.min(100, Math.round((got / total) * 100)) : null;
          status = progress === null ? "Downloading…" : `Downloading… ${progress}%`;
        }
        if (e.event === "Finished") status = "Restarting…";
      });
      await relaunch();
    } catch (e) {
      status = `The update didn't install: ${e instanceof Error ? e.message : String(e)}`;
      busy = false;
    }
  }

  onMount(() => {
    getVersion()
      .then((v) => (version = v))
      .catch(() => {});
    look(false);
    const t = setInterval(() => look(false), EVERY);
    return () => clearInterval(t);
  });

  const offer = $derived(update && update.version !== dismissed ? update : null);
</script>

{#if offer}
  <div class="offer" role="status">
    <b>Agent Roster {offer.version} is ready</b>
    <span>Installs in a few seconds, then the app restarts. Your agents and settings stay as they are.</span>
    {#if busy}
      <span class="st">{status}</span>
    {:else}
      <div class="acts">
        <button class="btn sm pri" onclick={install}>Update now</button>
        <button class="btn sm" onclick={() => (dismissed = offer.version)}>Later</button>
      </div>
      {#if status}<span class="st">{status}</span>{/if}
    {/if}
  </div>
{/if}

<div class="ver">
  {#if version}<span class="num">Version {version}</span>{/if}
  {#if !offer && status}
    <span class="st">{status}</span>
  {:else if !offer}
    <button class="link" onclick={() => look(true)}>Check for updates</button>
  {/if}
</div>

<style>
  .offer {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 10px 12px;
    margin-bottom: 4px;
    border: 1px solid var(--line-2);
    border-radius: 8px;
    background: var(--raise);
    font-size: 12px;
    line-height: 16px;
    color: var(--mut);
  }
  .offer b {
    color: var(--txt);
    font-weight: 500;
    font-size: 13px;
  }
  .acts {
    display: flex;
    gap: 8px;
    margin-top: 2px;
  }
  .ver {
    display: flex;
    gap: 12px;
    align-items: baseline;
    font-size: 11px;
    color: var(--dim);
  }
  .ver .link {
    font-size: 11px;
  }
  .st {
    color: var(--mut);
  }
</style>
