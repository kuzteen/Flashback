<script lang="ts">
  import { confirmState, closeConfirm } from '$lib/confirm.svelte';
  import { t } from '$lib/i18n.svelte';

  const req = $derived(confirmState.req);

  let confirmEl = $state<HTMLButtonElement | null>(null);

  $effect(() => {
    if (req) confirmEl?.focus();
  });

  function onKeyDown(e: KeyboardEvent) {
    if (!confirmState.req || e.key !== 'Escape') return;
    // Igual que en ShareDialog: el editor también escucha Escape en window y cerraría los dos
    // con una sola tecla si el evento siguiera propagándose.
    e.preventDefault();
    e.stopImmediatePropagation();
    closeConfirm(false);
  }
</script>

<svelte:window onkeydown={onKeyDown} />

{#if req}
  <div
    class="backdrop"
    role="presentation"
    onclick={(e) => {
      if (e.target === e.currentTarget) closeConfirm(false);
    }}
  >
    <div class="card" role="alertdialog" aria-modal="true" aria-label={req.title}>
      <h2 class="title">{req.title}</h2>
      <p class="message">{req.message}</p>
      {#if req.hint}<p class="hint">{req.hint}</p>{/if}
      <div class="actions">
        <button class="btn" onclick={() => closeConfirm(false)}>{t('confirm.cancel')}</button>
        <button class="btn danger" bind:this={confirmEl} onclick={() => closeConfirm(true)}>
          {req.confirmLabel}
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 220;
    display: grid;
    place-items: center;
    background: rgba(0, 0, 0, 0.62);
  }
  .card {
    width: 360px;
    max-width: calc(100vw - 40px);
    padding: 20px 22px 18px;
    background: var(--bg-1);
    border: 1px solid var(--line-strong);
    border-radius: var(--r-md);
    box-shadow: 0 24px 60px -18px rgba(0, 0, 0, 0.8);
    text-align: center;
    animation: confirm-in 0.16s ease-out;
  }
  @keyframes confirm-in {
    from {
      opacity: 0;
      transform: translateY(8px) scale(0.97);
    }
    to {
      opacity: 1;
      transform: translateY(0) scale(1);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .card {
      animation: none;
    }
  }

  .title {
    font-family: var(--font-mono);
    font-size: 12.5px;
    font-weight: 600;
    letter-spacing: 0.09em;
    text-transform: uppercase;
    color: var(--text-0);
  }
  .message {
    margin-top: 12px;
    font-size: 13.5px;
    line-height: 1.45;
    color: var(--text-1);
    overflow-wrap: anywhere;
  }
  .hint {
    margin-top: 6px;
    font-size: 12px;
    line-height: 1.4;
    color: var(--text-2);
  }

  .actions {
    display: flex;
    justify-content: center;
    gap: 8px;
    margin-top: 20px;
  }
  .btn {
    min-width: 116px;
    padding: 8px 14px;
    font-size: 13px;
    color: var(--text-1);
    background: var(--bg-0);
    border: 1px solid var(--line);
    border-radius: var(--r-sm);
    cursor: pointer;
    transition: color 0.14s ease, background 0.14s ease, border-color 0.14s ease;
  }
  .btn:hover {
    color: var(--text-0);
    border-color: var(--line-strong);
  }
  .btn.danger {
    color: var(--rec);
    border-color: rgba(255, 91, 91, 0.35);
  }
  .btn.danger:hover {
    color: var(--rec);
    background: rgba(255, 91, 91, 0.14);
    border-color: var(--rec);
  }
</style>
