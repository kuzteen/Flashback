<script lang="ts">
  import type { Snippet } from 'svelte';

  let {
    title,
    desc,
    disabled = false,
    sub = false,
    info,
    children
  }: {
    title: string;
    desc?: string;
    disabled?: boolean;
    sub?: boolean;
    info?: Snippet;
    children?: Snippet;
  } = $props();
</script>

<div class="row" class:disabled class:sub inert={disabled}>
  <div class="info">
    <h3>{title}</h3>
    {#if info}{@render info()}{/if}
    {#if desc}<p>{desc}</p>{/if}
  </div>
  {#if children}<div class="control">{@render children()}</div>{/if}
</div>

<style>
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 24px;
    min-height: 72px;
    padding: 16px 0;
  }
  .row + :global(.row) {
    border-top: 1px solid var(--line);
  }
  .disabled {
    opacity: 0.45;
  }
  .sub {
    min-height: 52px;
    padding: 10px 0 10px 18px;
  }
  .sub h3 {
    margin-bottom: 0;
    font-size: 13.5px;
    font-weight: 500;
    color: var(--text-1);
  }
  .info {
    min-width: 0;
  }
  h3 {
    font-size: 14.5px;
    font-weight: 560;
    margin-bottom: 3px;
  }
  p {
    font-size: 12.5px;
    color: var(--text-2);
  }
  .control {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-shrink: 0;
  }
</style>
