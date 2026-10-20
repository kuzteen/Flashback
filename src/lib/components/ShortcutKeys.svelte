<script lang="ts">
  import { labelTokens } from '$lib/hotkeys.svelte';

  let { tokens, down = [] }: { tokens: string[]; down?: string[] } = $props();
</script>

<span class="caps">
  {#each tokens as token, i (token)}
    {#if i > 0}<span class="plus">+</span>{/if}
    <kbd class="cap" class:down={down.includes(token)}>{labelTokens(token)[0]}</kbd>
  {/each}
</span>

<style>
  .caps { display: inline-flex; align-items: center; gap: 5px; }
  .plus { font-size: 11px; color: var(--text-3); line-height: 1; text-box: trim-both cap alphabetic; }
  .cap {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 22px;
    height: 22px;
    padding: 0 6px;
    font-family: inherit;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.03em;
    line-height: 1;
    color: var(--text-0);
    background: var(--bg-2);
    border: 1px solid var(--line-strong);
    border-bottom-width: 3px;
    border-radius: 5px;
    transition: height 0.07s ease, border-bottom-width 0.07s ease, margin-top 0.07s ease;
  }
  .cap.down { height: 20px; border-bottom-width: 1px; margin-top: 2px; background: var(--bg-3); }
  @media (prefers-reduced-motion: reduce) { .cap { transition: none; } }
</style>
