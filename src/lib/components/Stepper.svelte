<script lang="ts" generics="T extends string | number">
  import { fly } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import Icon from '$lib/components/Icon.svelte';
  import { stepIndex } from '$lib/stepper';

  type Option = { label: string; value: T };

  let {
    value,
    options,
    onchange,
    ariaLabel = ''
  }: {
    value: T;
    options: readonly Option[];
    onchange: (value: T) => void;
    ariaLabel?: string;
  } = $props();

  const index = $derived(options.findIndex((o) => o.value === value));
  const label = $derived(options[index]?.label ?? String(value));
  const canPrev = $derived(stepIndex(options.length, index, -1) !== null);
  const canNext = $derived(stepIndex(options.length, index, 1) !== null);
  let dir = $state<-1 | 1>(1);

  function go(d: -1 | 1) {
    const i = stepIndex(options.length, index, d);
    if (i === null) return;
    dir = d;
    onchange(options[i].value);
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key === 'ArrowLeft' || e.key === 'ArrowDown') go(-1);
    else if (e.key === 'ArrowRight' || e.key === 'ArrowUp') go(1);
    else return;
    e.preventDefault();
  }
</script>

<!-- Un solo punto de foco: las flechas del teclado mueven el valor y los botones quedan para el ratón. -->
<div
  class="stepper"
  role="spinbutton"
  tabindex="0"
  aria-label={ariaLabel}
  aria-valuetext={label}
  aria-valuenow={index}
  aria-valuemin={0}
  aria-valuemax={options.length - 1}
  {onkeydown}
>
  <button class="arrow" tabindex="-1" aria-hidden="true" disabled={!canPrev} onclick={() => go(-1)}>
    <Icon name="chevron-left" size={13} sw={2.2} />
  </button>
  <!-- Todas las opciones ocupan la misma celda, invisibles: el ancho es el de la más larga y no
       salta al cambiar. -->
  <span class="values">
    {#each options as o (o.value)}
      <span class="ghost" aria-hidden="true">{o.label}</span>
    {/each}
    {#key value}
      <span
        class="value"
        in:fly={{ x: dir * 10, duration: 180, easing: cubicOut }}
        out:fly={{ x: dir * -10, duration: 140, easing: cubicOut }}
      >{label}</span>
    {/key}
  </span>
  <button class="arrow" tabindex="-1" aria-hidden="true" disabled={!canNext} onclick={() => go(1)}>
    <Icon name="chevron-right" size={13} sw={2.2} />
  </button>
</div>

<style>
  .stepper {
    display: flex;
    align-items: center;
    flex-shrink: 0;
    min-width: 140px;
    height: 34px;
    padding: 0 3px;
    font-size: 13px;
    color: var(--text-0);
    background: var(--bg-0);
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    transition: border-color 0.14s ease;
    user-select: none;
  }
  .stepper:hover,
  .stepper:focus-visible {
    border-color: var(--line-strong);
  }
  .stepper:focus-visible {
    outline: none;
  }
  .arrow {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 26px;
    height: 26px;
    border-radius: 4px;
    color: var(--text-2);
    cursor: pointer;
    transition: color 0.12s ease, background 0.12s ease;
  }
  .arrow:hover:not(:disabled) {
    color: var(--text-0);
    background: var(--bg-hover);
  }
  .arrow:disabled {
    opacity: 0.3;
    cursor: default;
  }
  .values {
    flex: 1;
    display: grid;
    justify-items: center;
    min-width: 72px;
    padding: 0 6px;
    overflow: hidden;
    font-variant-numeric: tabular-nums;
  }
  .ghost,
  .value {
    grid-area: 1 / 1;
    white-space: nowrap;
    line-height: 1;
    text-box: trim-both cap alphabetic;
    padding-block: 3px;
  }
  .ghost {
    visibility: hidden;
  }
</style>
