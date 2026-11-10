<script lang="ts">
  import { onDestroy, onMount, type Snippet } from "svelte";

  let {
    kind,
    label,
    title,
    sub,
    head,
    foot,
    children,
    onClose,
    onEscape,
  }: {
    /** A side panel from the right, or a centered dialog. */
    kind: "panel" | "dialog";
    /** The accessible name. */
    label: string;
    /** A heading in the header, when `head` isn't given. */
    title?: string;
    sub?: string;
    head?: Snippet;
    foot?: Snippet;
    children: Snippet;
    /** ✕, a click on the dimmed list, and Escape (unless `onEscape` says otherwise). */
    onClose: () => void;
    /** Escape: defaults to `onClose`; null when the content handles Escape itself. */
    onEscape?: (() => void) | null;
  } = $props();

  let box: HTMLDivElement | undefined = $state();
  let body: HTMLDivElement | undefined = $state();
  /** Where focus was when this opened: it goes back there on close. */
  const before = typeof document !== "undefined" ? (document.activeElement as HTMLElement | null) : null;

  const FOCUSABLE =
    'a[href], button:not(:disabled), input:not(:disabled), select:not(:disabled), textarea:not(:disabled), [tabindex]:not([tabindex="-1"])';

  onMount(() => {
    // Anything that opens starts at its top.
    if (body) body.scrollTop = 0;
    // The content may already have put focus inside (Add agent focuses its name).
    if (!box?.contains(document.activeElement)) box?.focus({ preventScroll: true });
  });

  onDestroy(() => {
    if (before && before.isConnected && before !== document.body) before.focus({ preventScroll: true });
  });

  function keydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      // An open menu closes first.
      if (document.querySelector('[role="menu"]')) return;
      const esc = onEscape === undefined ? onClose : onEscape;
      if (!esc) return;
      e.preventDefault();
      esc();
      return;
    }
    if (e.key !== "Tab" || !box) return;
    // Keep focus inside while it is open.
    const items = [...box.querySelectorAll<HTMLElement>(FOCUSABLE)].filter((el) => !el.closest("[hidden]"));
    if (items.length === 0) {
      e.preventDefault();
      box.focus();
      return;
    }
    const first = items[0];
    const last = items[items.length - 1];
    const at = document.activeElement;
    if (e.shiftKey && (at === first || at === box)) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && at === last) {
      e.preventDefault();
      first.focus();
    }
  }
</script>

<svelte:window onkeydown={keydown} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="scrim" data-testid="scrim" aria-hidden="true" onclick={onClose}></div>
<div class="ov {kind}" role="dialog" aria-modal="true" aria-label={label} tabindex="-1" bind:this={box}>
  <header class="oh">
    <div class="who">
      {#if head}
        {@render head()}
      {:else}
        <h2>{title ?? label}</h2>
        {#if sub}<p class="sub">{sub}</p>{/if}
      {/if}
    </div>
    <button class="x" aria-label="Close" title="Close (Esc)" onclick={onClose}>
      <svg width="12" height="12" viewBox="0 0 12 12" aria-hidden="true"><path d="M1 1l10 10M11 1 1 11" /></svg>
    </button>
  </header>
  <div class="ob" bind:this={body}>
    {@render children()}
  </div>
  {#if foot}<footer class="of">{@render foot()}</footer>{/if}
</div>

<style>
  .scrim {
    position: fixed;
    inset: var(--bar-h, 0px) 0 0 0;
    background: var(--scrim);
    z-index: 30;
    animation: fade 180ms var(--ease);
  }
  .ov {
    position: fixed;
    z-index: 31;
    background: var(--panel);
    border: 1px solid var(--panel-line);
    display: flex;
    flex-direction: column;
    box-shadow: var(--shadow);
    outline: none;
  }
  .ov.panel {
    top: var(--bar-h, 0px);
    right: 0;
    bottom: 0;
    width: 430px;
    border-width: 0 0 0 1px;
    animation: slide 200ms var(--ease);
  }
  /* A narrow window: the panel fills it, ✕ still top right. */
  @media (max-width: 600px) {
    .ov.panel {
      width: 100%;
      border-left: 0;
    }
  }
  .ov.dialog {
    left: 50%;
    top: calc(50% + var(--bar-h, 0px) / 2);
    transform: translate(-50%, -50%);
    width: min(560px, calc(100vw - 32px));
    max-height: calc(100vh - var(--bar-h, 0px) - 32px);
    border-radius: 14px;
    animation: pop 180ms var(--ease);
  }
  .oh {
    flex: none;
    display: flex;
    align-items: flex-start;
    gap: 12px;
    padding: 16px 18px 12px;
  }
  .panel .oh {
    border-bottom: 1px solid var(--line);
  }
  .dialog .oh {
    padding: 16px 20px 8px;
  }
  .who {
    flex: 1;
    min-width: 0;
  }
  h2 {
    margin: 0;
    font-size: 18px;
    line-height: 24px;
    font-weight: 600;
  }
  .sub {
    margin: 2px 0 0;
    font-size: 12px;
    line-height: 16px;
    color: var(--mut);
  }
  .x {
    flex: none;
    width: 28px;
    height: 28px;
    margin: -2px -6px 0 0;
    display: grid;
    place-items: center;
    border: 0;
    border-radius: 6px;
    background: none;
    color: var(--mut);
    transition:
      background-color 150ms var(--ease),
      color 150ms var(--ease);
  }
  .x svg {
    fill: none;
    stroke: currentColor;
    stroke-width: 1.5;
    stroke-linecap: round;
  }
  .x:hover {
    color: var(--txt);
    background: var(--raise);
  }
  .x:focus-visible {
    outline: 2px solid var(--txt);
    outline-offset: 0;
  }
  .ob {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
  }
  .panel .ob {
    padding: 14px 18px 0;
  }
  .dialog .ob {
    padding: 0 20px 4px;
  }
  .of {
    flex: none;
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    padding: 14px 20px 18px;
  }
  @keyframes fade {
    from {
      opacity: 0;
    }
  }
  @keyframes slide {
    from {
      transform: translateX(100%);
    }
  }
  @keyframes pop {
    from {
      opacity: 0;
      transform: translate(-50%, -48%) scale(0.98);
    }
  }
</style>
