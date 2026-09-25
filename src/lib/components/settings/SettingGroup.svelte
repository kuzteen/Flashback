<script lang="ts">
  import type { Snippet } from 'svelte';

  let { title, children }: { title?: string; children: Snippet } = $props();
</script>

<section class="group">
  {#if title}<h2 class="label title">{title}</h2>{/if}
  <div class="panel">{@render children()}</div>
</section>

<style>
  .group + :global(.group) {
    margin-top: 26px;
  }
  .title {
    margin: 0 0 10px 2px;
    font-weight: 500;
  }
  .panel {
    background: var(--bg-1);
    border: 1px solid var(--line);
    border-radius: var(--r-lg);
    padding: 0 20px;
  }
</style>
