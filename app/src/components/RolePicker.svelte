<script lang="ts">
  import { ROLE_PRESETS } from "../lib/types";

  let { value, onChange }: { value: string; onChange: (role: string) => void } = $props();

  const preset = $derived(ROLE_PRESETS.find((p) => p.toLowerCase() === value.trim().toLowerCase()) ?? null);
  // svelte-ignore state_referenced_locally
  let custom = $state(value.trim() !== "" && !ROLE_PRESETS.some((p) => p.toLowerCase() === value.trim().toLowerCase()));
</script>

<div class="rp">
  <div class="presets">
    {#each ROLE_PRESETS as p (p)}
      <button
        type="button"
        aria-pressed={preset === p}
        class:on={preset === p}
        onclick={() => {
          custom = false;
          onChange(preset === p ? "" : p);
        }}>{p}</button
      >
    {/each}
    <button type="button" aria-pressed={custom} class:on={custom} onclick={() => (custom = !custom)}>Custom…</button>
  </div>
  {#if custom}
    <input
      class="field"
      aria-label="Role"
      spellcheck="false"
      placeholder="what others should ask it for"
      {value}
      oninput={(e) => onChange(e.currentTarget.value)}
    />
  {/if}
</div>

<style>
  .rp {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .presets {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .presets button {
    height: 28px;
    padding: 0 12px;
    border-radius: 14px;
    border: 1px solid var(--line-2);
    background: none;
    color: var(--mut);
    font-size: 12px;
    font-weight: 500;
    transition:
      border-color 150ms var(--ease),
      color 150ms var(--ease);
  }
  .presets button:hover {
    color: var(--txt);
  }
  .presets button.on {
    border-color: var(--txt);
    color: var(--txt);
    background: var(--raise);
  }
  .presets button:focus-visible {
    outline: 2px solid var(--txt);
    outline-offset: 2px;
  }
  .field {
    height: 32px;
    background: var(--raise);
    border: 1px solid var(--line);
    border-radius: 8px;
    padding: 0 12px;
    color: var(--txt);
    font: inherit;
    font-size: 13px;
  }
  .field::placeholder {
    color: var(--dim);
  }
</style>
