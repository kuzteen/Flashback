<script lang="ts">
  let { checked, onchange, label }: { checked: boolean; onchange: (v: boolean) => void; label: string } =
    $props();
</script>

<button class="switch" class:on={checked} onclick={() => onchange(!checked)} role="switch" aria-checked={checked} aria-label={label}>
  <span class="knob"></span>
</button>

<style>
  .switch {
    flex-shrink: 0;
    width: 44px;
    height: 25px;
    border-radius: 999px;
    background: var(--bg-3);
    border: 1px solid var(--line);
    padding: 2px;
    transition: background 0.18s ease, border-color 0.18s ease;
  }
  .knob {
    display: block;
    width: 19px;
    height: 19px;
    border-radius: 999px;
    background: var(--text-2);
    transition: transform 0.18s ease, background 0.18s ease;
  }
  .on {
    background: var(--accent);
    border-color: transparent;
  }
  .on .knob {
    transform: translateX(19px);
    background: var(--on-accent);
  }
</style>
