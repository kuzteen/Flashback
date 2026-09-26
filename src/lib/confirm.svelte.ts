import { t } from '$lib/i18n.svelte';

type Request = {
  title: string;
  message: string;
  hint?: string;
  confirmLabel: string;
};

export const confirmState = $state<{ req: Request | null }>({ req: null });

let resolver: ((ok: boolean) => void) | null = null;

function ask(req: Request): Promise<boolean> {
  // Si quedara uno abierto, se resuelve como cancelado: quien lo esperaba no debe quedarse
  // colgado ni actuar sobre una respuesta que el usuario no ha dado.
  resolver?.(false);
  confirmState.req = req;
  return new Promise((resolve) => (resolver = resolve));
}

export function closeConfirm(ok: boolean) {
  confirmState.req = null;
  const resolve = resolver;
  resolver = null;
  resolve?.(ok);
}

// Borrar una playlist no manda nada a la papelera (es solo metadato), así que el aviso de los
// clips no vale: ahí se promete que se pueden restaurar desde Windows y aquí no hay vuelta atrás.
export function confirmDeletePlaylist(name: string): Promise<boolean> {
  return ask({
    title: t('confirm.plDeleteTitle'),
    message: t('confirm.plDeleteOne', { name }),
    hint: t('confirm.plDeleteHint'),
    confirmLabel: t('confirm.delete')
  });
}
