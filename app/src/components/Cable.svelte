<script lang="ts">
  import type { CallView } from "../lib/types";

  let { calls, order }: { calls: CallView[]; order: string[] } = $props();

  const ROW = 84; // row height 76 + gap 8
  const MID = 38;
  const y = (id: string) => order.indexOf(id) * ROW + MID;
  const cables = $derived(
    calls
      .filter((c) => order.includes(c.from_id) && order.includes(c.to_id))
      .map((c) => {
        const [y1, y2] = [y(c.from_id), y(c.to_id)];
        return { c, d: `M0 ${y1} C 66 ${y1}, 66 ${y2}, 0 ${y2}` };
      }),
  );
</script>

{#if cables.length}
  <svg class="cable" width="80" height={order.length * ROW} viewBox="0 0 80 {order.length * ROW}" aria-hidden="true">
    <defs>
      {#each cables as { c }, i (i)}
        <linearGradient id="cg{i}" x1="0" y1={c.from_id === c.to_id ? 0 : y(c.from_id) < y(c.to_id) ? 0 : 1} x2="0" y2={y(c.from_id) < y(c.to_id) ? 1 : 0}>
          <stop offset="0" stop-color={c.from_color} />
          <stop offset="1" stop-color={c.to_color} />
        </linearGradient>
      {/each}
    </defs>
    {#each cables as { d }, i (i)}
      <path {d} fill="none" stroke="url(#cg{i})" stroke-width="4" stroke-linecap="round" />
      <path class="pulse" {d} fill="none" stroke="#ffffff" stroke-width="4" stroke-linecap="round" />
    {/each}
  </svg>
{/if}

<style>
  .cable {
    position: absolute;
    right: -60px;
    top: 0;
    overflow: visible;
    pointer-events: none;
  }
  .pulse {
    stroke-dasharray: 6 220;
    animation: run 2.4s linear infinite;
  }
  @keyframes run {
    from {
      stroke-dashoffset: 226;
    }
    to {
      stroke-dashoffset: 0;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .pulse {
      animation: none;
      opacity: 0;
    }
  }
</style>
