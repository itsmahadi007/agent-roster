<script lang="ts">
  let {
    title,
    sub,
    onBack,
  }: { title: string; sub?: string; onBack?: () => void } = $props();
</script>

<header class="ph">
  {#if onBack}<button class="link" onclick={onBack}>Back to your agents</button>{/if}
  <h3>{title}</h3>
  {#if sub}<p class="lede">{sub}</p>{/if}
</header>

<style>
  .ph {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    margin-bottom: 24px;
  }
  .link {
    margin-bottom: 12px;
  }
  h3 {
    margin: 0 0 4px;
    font-size: 22px;
    line-height: 28px;
    font-weight: 600;
  }
</style>
