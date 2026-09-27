<script lang="ts">
  import type { Snippet } from 'svelte';

  let { title, id, children }: { title?: string; id?: string; children: Snippet } = $props();
</script>

<section class="group" {id}>
  {#if title}<h2 class="title">{title}</h2>{/if}
  <div class="panel">{@render children()}</div>
</section>

<style>
  .group + :global(.group) {
    margin-top: 30px;
    padding-top: 30px;
    border-top: 1px solid var(--line);
  }
  .title {
    margin-bottom: 10px;
    font-size: 22px;
    font-weight: 600;
    letter-spacing: -0.01em;
    color: var(--text-0);
  }
</style>
