<script lang="ts" generics="S extends string">
  import Icon from './Icon.svelte';
  import LibraryFilter from './LibraryFilter.svelte';
  import type { Clip, LibraryFilter as Filter } from '$lib/clips';
  import { t } from '$lib/i18n.svelte';

  let {
    clips,
    query = $bindable(''),
    filters = $bindable([]),
    sort = $bindable(),
    sorts
  }: {
    clips: Clip[];
    query?: string;
    filters?: Filter[];
    sort: S;
    sorts: { value: S; label: string }[];
  } = $props();

  let sortOpen = $state(false);
  let sortEl = $state<HTMLElement | null>(null);
  let searchEl = $state<HTMLInputElement | null>(null);
  let searchFocused = $state(false);

  const current = $derived(sorts.find((o) => o.value === sort) ?? sorts[0]);

  export function focusSearch() {
    searchEl?.focus();
    searchEl?.select();
  }

  $effect(() => {
    if (!sortOpen) return;
    const onDown = (e: MouseEvent) => {
      if (sortEl && !sortEl.contains(e.target as Node)) sortOpen = false;
    };
    window.addEventListener('mousedown', onDown, true);
    return () => window.removeEventListener('mousedown', onDown, true);
  });
</script>

<div class="bar">
  <label class="search" class:open={searchFocused || query}>
    <Icon name="search" size={16} />
    <input
      aria-label={t('clips.search')}
      placeholder={t('clips.search')}
      bind:this={searchEl}
      bind:value={query}
      onfocus={() => (searchFocused = true)}
      onblur={() => (searchFocused = false)}
      onkeydown={(e) => e.key === 'Escape' && searchEl?.blur()}
    />
  </label>
  <LibraryFilter {clips} bind:selected={filters} />
  <div class="sort-dd" class:open={sortOpen} bind:this={sortEl}>
    <button class="ctrl" onclick={() => (sortOpen = !sortOpen)}>
      <Icon name="sort" size={14} />
      {t(current.label)}
      <Icon name="chevron-down" size={13} sw={2} />
    </button>
    {#if sortOpen}
      <div class="sort-menu">
        {#each sorts as o (o.value)}
          <button
            class="sort-item"
            class:on={sort === o.value}
            onclick={() => {
              sort = o.value;
              sortOpen = false;
            }}
          >
            {t(o.label)}
          </button>
        {/each}
      </div>
    {/if}
  </div>
</div>

<style>
  .bar {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .search {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 9px;
    height: 36px;
    width: 36px;
    overflow: hidden;
    cursor: pointer;
    color: var(--text-1);
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    transition: width 0.22s cubic-bezier(0.2, 0.8, 0.2, 1), border-color 0.15s ease, color 0.15s ease;
  }
  .search :global(svg) {
    flex-shrink: 0;
  }
  .search:hover {
    color: var(--text-0);
    border-color: var(--line-strong);
  }
  .search.open {
    width: 220px;
    cursor: text;
  }
  .search:focus-within {
    border-color: var(--line-strong);
  }
  .search input {
    flex: 1;
    min-width: 0;
    background: none;
    border: none;
    outline: none;
    font-size: 13px;
    color: var(--text-0);
  }
  .search input::placeholder {
    color: var(--text-3);
  }
  .ctrl {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    height: 36px;
    padding: 0 12px;
    font-size: 13px;
    color: var(--text-1);
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    transition: color 0.15s ease, border-color 0.15s ease;
  }
  .ctrl:hover {
    color: var(--text-0);
    border-color: var(--line-strong);
  }
  .sort-dd {
    position: relative;
  }
  .sort-dd.open .ctrl {
    border-color: var(--line-strong);
    color: var(--text-0);
  }
  .sort-dd .ctrl > :global(svg:last-child) {
    transition: transform 0.2s ease;
  }
  .sort-dd.open .ctrl > :global(svg:last-child) {
    transform: rotate(180deg);
  }
  .sort-menu {
    position: absolute;
    top: calc(100% + 6px);
    right: 0;
    min-width: 150px;
    display: flex;
    flex-direction: column;
    gap: 1px;
    padding: 5px;
    background: var(--surface);
    border: 1px solid var(--line-strong);
    border-radius: var(--r-sm);
    box-shadow: 0 18px 42px -14px rgba(0, 0, 0, 0.7);
    z-index: 70;
  }
  .sort-item {
    padding: 7px 10px;
    font-size: 13px;
    text-align: left;
    white-space: nowrap;
    color: var(--text-1);
    border-radius: 6px;
    transition: background 0.13s ease, color 0.13s ease;
  }
  .sort-item:hover {
    background: var(--bg-3);
    color: var(--text-0);
  }
  .sort-item.on {
    color: var(--text-0);
    font-weight: 560;
  }
</style>
