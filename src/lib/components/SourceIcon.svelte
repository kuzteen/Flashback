<script lang="ts">
  import Icon from './Icon.svelte';
  import { ensureGameIcon, gameIcon } from '$lib/artwork.svelte';
  import { isScreenSource } from '$lib/screen-source';
  import { sourceBadge } from '$lib/source-badge';

  // Imagen del origen de un clip: portada propia, icono del juego, inicial de un nombre
  // inventado, pantalla o importado (ver sourceBadge). La misma en la tarjeta y en "Editar clip".
  let { source, cover = null, size = 16 }: { source: string; cover?: string | null; size?: number } = $props();

  $effect(() => {
    const name = source.trim();
    if (name && !isScreenSource(name)) ensureGameIcon(name);
  });

  const badge = $derived(sourceBadge(source, cover, gameIcon(source.trim())));
</script>

{#if badge.kind === 'cover' || badge.kind === 'game'}
  <img class="ico" src={badge.src} alt="" draggable="false" style:width="{size}px" style:height="{size}px" />
{:else if badge.kind === 'initial'}
  <span class="initial" style:width="{size}px" style:height="{size}px" style:font-size="{Math.round(size * 0.58)}px">
    {badge.letter}
  </span>
{:else if badge.kind === 'screen'}
  <Icon name="monitor-fill" size={size - 1} sw={1.8} />
{:else}
  <Icon name="imported" size={size - 1} />
{/if}

<style>
  .ico {
    flex: none;
    display: block;
    object-fit: cover;
    border-radius: 25%;
  }
  .initial {
    flex: none;
    display: grid;
    place-items: center;
    font-family: var(--font-display);
    font-weight: 700;
    line-height: 1;
    color: var(--text-0);
    background: var(--bg-3);
    border-radius: 25%;
  }
</style>
