<script lang="ts">
  import { onMount } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import type { Chrome } from "../lib/platform";
  import icon from "../assets/icon.png";

  let { chrome }: { chrome: Chrome } = $props();

  /** Room for the Mac's own buttons (trafficLightPosition x 16 in tauri.macos.conf.json). */
  const MAC_INSET = 76;

  let maximized = $state(false);

  onMount(() => {
    if (chrome !== "windows") return;
    const w = getCurrentWindow();
    const sync = async () => (maximized = await w.isMaximized());
    sync();
    const off = w.onResized(sync);
    return () => {
      off.then((f) => f());
    };
  });

  const w = () => getCurrentWindow();
</script>

<!-- The window's own title bar: decorations are off on Windows; on the Mac the system's buttons
     sit on top of it (tauri.macos.conf.json), so it leaves room for them and draws none. -->
{#if chrome !== "none"}
  <header class="bar {chrome}" data-tauri-drag-region>
    {#if chrome === "mac"}
      <span class="lights" style:width="{MAC_INSET}px" data-tauri-drag-region></span>
    {:else}
      <img class="icon" src={icon} alt="" width="18" height="18" data-tauri-drag-region />
    {/if}
    <span class="title" data-tauri-drag-region>Agent Roster</span>
    <span class="drag" data-tauri-drag-region></span>
    {#if chrome === "windows"}
      <div class="ctl">
        <button aria-label="Minimize" title="Minimize" onclick={() => w().minimize()}>
          <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><path d="M0 5.5h10" /></svg>
        </button>
        <button
          aria-label={maximized ? "Restore" : "Maximize"}
          title={maximized ? "Restore" : "Maximize"}
          onclick={() => w().toggleMaximize()}
        >
          {#if maximized}
            <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
              <path d="M2.5 2.5V.5h7v7h-2M.5 2.5h7v7h-7z" />
            </svg>
          {:else}
            <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><path d="M.5 .5h9v9h-9z" /></svg>
          {/if}
        </button>
        <button class="close" aria-label="Close" title="Close. Agent Roster stays in the tray." onclick={() => w().close()}>
          <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><path d="M.5 .5l9 9M9.5 .5l-9 9" /></svg>
        </button>
      </div>
    {/if}
  </header>
{/if}

<style>
  /* Part of the app: same background, no strip under it. */
  .bar {
    flex: none;
    height: 40px;
    display: flex;
    align-items: center;
    background: var(--base);
    -webkit-user-select: none;
    user-select: none;
  }
  .bar.windows {
    padding-left: 16px;
  }
  /* The Mac's buttons sit at x 16, centered at y 20: the title starts right after them. */
  .lights {
    flex: none;
    align-self: stretch;
  }
  .icon {
    width: 18px;
    height: 18px;
    border-radius: 5px;
    margin-right: 9px;
    pointer-events: auto;
  }
  .title {
    font-size: 13px;
    line-height: 16px;
    font-weight: 600;
    color: var(--txt);
    white-space: nowrap;
  }
  .drag {
    flex: 1;
    align-self: stretch;
  }
  .ctl {
    height: 100%;
    display: flex;
  }
  .ctl button {
    width: 46px;
    height: 100%;
    border: 0;
    background: none;
    color: var(--mut);
    display: grid;
    place-items: center;
    cursor: default;
    transition: background-color 120ms var(--ease);
  }
  .ctl svg {
    fill: none;
    stroke: currentColor;
    stroke-width: 1;
  }
  .ctl button:hover {
    background: var(--raise);
    color: var(--txt);
  }
  .ctl button.close:hover {
    background: #c42b1c;
    color: #ffffff;
  }
  .ctl button:focus-visible {
    outline: 2px solid var(--txt);
    outline-offset: -2px;
  }
</style>
