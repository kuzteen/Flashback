<script lang="ts">
  import { untrack } from 'svelte';
  import { digitsOnly, parseSliderValue } from '$lib/slider-value';

  // Campo para escribir a mano el valor de un deslizador. Se superpone al número (el padre lo
  // envuelve en una caja relativa) para no mover la fila. Enter o salir aplica; Escape cancela.
  let {
    value,
    min,
    max,
    label,
    ondone,
    center = false,
    width = 52
  }: {
    value: number;
    min: number;
    max: number;
    label: string;
    ondone: (v: number | null) => void;
    center?: boolean;
    width?: number;
  } = $props();

  let text = $state(String(untrack(() => value)));
  let closed = false;

  function finish(save: boolean) {
    if (closed) return;
    closed = true;
    ondone(save ? parseSliderValue(text, min, max) : null);
  }

  function mount(node: HTMLInputElement) {
    node.focus();
    node.select();
  }
</script>

<input
  class="value-input mono"
  class:center
  style:width="{width}px"
  aria-label={label}
  inputmode="numeric"
  maxlength={String(Math.max(Math.abs(min), Math.abs(max))).length + (min < 0 ? 1 : 0)}
  spellcheck="false"
  value={text}
  oninput={(e) => {
    text = digitsOnly(e.currentTarget.value, min < 0);
    e.currentTarget.value = text;
  }}
  use:mount
  onkeydown={(e) => {
    // El editor no debe ver estas teclas: espacio reproduce y las flechas mueven el fotograma.
    e.stopPropagation();
    if (e.key === 'Enter') {
      e.preventDefault();
      finish(true);
    } else if (e.key === 'Escape') {
      e.preventDefault();
      finish(false);
    }
  }}
  onblur={() => finish(true)}
/>

<style>
  .value-input {
    position: absolute;
    top: 50%;
    right: -6px;
    translate: 0 -50%;
    height: 22px;
    padding: 0 6px;
    font-size: 11.5px;
    text-align: right;
    color: var(--text-0);
    background: var(--field);
    border: 1px solid var(--line-strong);
    border-radius: 4px;
    outline: none;
  }
  .value-input.center {
    right: auto;
    left: 50%;
    translate: -50% -50%;
    text-align: center;
  }
  .value-input:focus {
    border-color: var(--text-3);
  }
</style>
