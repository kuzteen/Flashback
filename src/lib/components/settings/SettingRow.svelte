<script lang="ts">
  import type { Snippet } from 'svelte';

  let {
    title,
    desc,
    disabled = false,
    sub = false,
    muted = false,
    lead,
    info,
    children
  }: {
    title: string;
    desc?: string;
    disabled?: boolean;
    sub?: boolean;
    // Atenúa el texto y el icono pero deja el control usable (un juego con la captura apagada).
    muted?: boolean;
    lead?: Snippet;
    info?: Snippet;
    children?: Snippet;
  } = $props();
</script>

<div class="row" class:disabled class:sub class:muted inert={disabled}>
  {#if lead}<div class="lead">{@render lead()}</div>{/if}
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
    min-height: 70px;
    padding: 14px 0;
  }
  .disabled {
    opacity: 0.45;
  }
  .sub {
    min-height: 56px;
    padding: 8px 0 8px 20px;
  }
  .sub h3 {
    margin-bottom: 0;
    font-size: 15px;
    font-weight: 500;
    color: var(--text-1);
  }
  .lead {
    display: flex;
    flex-shrink: 0;
    margin-right: -8px;
  }
  .info {
    flex: 1;
    min-width: 0;
  }
  .muted .lead,
  .muted .info {
    opacity: 0.55;
  }
  .lead,
  .info {
    transition: opacity 0.15s ease;
  }
  h3 {
    font-size: 16px;
    font-weight: 560;
    margin-bottom: 4px;
  }
  p {
    font-size: 14px;
    color: var(--text-2);
  }
  .control {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-shrink: 0;
  }
</style>
