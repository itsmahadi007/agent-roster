<script lang="ts">
  import type { PlanView } from "../lib/types";
  import { apply, message } from "../lib/api";

  let {
    plan,
    confirmLabel,
    onDone,
    onCancel,
  }: {
    plan: PlanView;
    confirmLabel: string;
    onDone: (opId: number | null) => void;
    onCancel: () => void;
  } = $props();

  const MARK = { add: "+", change: "~", remove: "-", note: " " } as const;
  let busy = $state(false);
  let error = $state<string | null>(null);

  async function confirm() {
    busy = true;
    error = null;
    try {
      onDone(await apply(plan.token));
    } catch (e) {
      error = message(e);
    } finally {
      busy = false;
    }
  }
</script>

<div class="backdrop">
  <div class="sheet" role="dialog" aria-modal="true" aria-label={plan.summary}>
    <h4>{plan.summary}</h4>
    <ul>
      {#each plan.steps as s, i (i)}
        <li data-kind={s.kind}><span class="mark mono">{MARK[s.kind]}</span><span>{s.text}</span></li>
      {/each}
    </ul>
    {#if error}<p class="error">{error}</p>{/if}
    <div class="foot">
      <button class="btn" onclick={onCancel} disabled={busy}>Cancel</button>
      <button class="btn pri" onclick={confirm} disabled={busy}>{confirmLabel}</button>
    </div>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(5, 9, 14, 0.62);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 10;
  }
  .sheet {
    width: min(520px, calc(100vw - 48px));
    background: var(--base);
    border: 1px solid #2b3746;
    border-radius: 14px;
    padding: 22px;
    box-shadow: 0 30px 70px rgba(0, 0, 0, 0.55);
  }
  h4 {
    margin: 0 0 12px;
    font-size: 16px;
    font-weight: 700;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 12px 14px;
    background: var(--well);
    border: 1px solid var(--line);
    border-radius: 10px;
    font-size: 12.5px;
    line-height: 1.75;
    color: var(--mut);
  }
  li {
    display: flex;
    gap: 10px;
  }
  .mark {
    width: 10px;
    flex: none;
  }
  [data-kind="add"] .mark {
    color: var(--jade);
  }
  [data-kind="change"] .mark {
    color: #f0b429;
  }
  [data-kind="remove"] .mark {
    color: var(--coral);
  }
  [data-kind="add"],
  [data-kind="change"],
  [data-kind="remove"] {
    color: var(--txt);
  }
  .error {
    margin: 12px 0 0;
  }
</style>
