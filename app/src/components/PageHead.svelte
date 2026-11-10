<script lang="ts">
  let { title, sub }: { title: string; sub?: string } = $props();
</script>

<header class="ph">
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
  h3 {
    margin: 0 0 4px;
    font-size: 22px;
    line-height: 28px;
    font-weight: 600;
  }
</style>
