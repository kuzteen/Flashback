export function focusTrap(node: HTMLElement, { initial = '[autofocus]', restore = false } = {}) {
  const previous = document.activeElement;
  queueMicrotask(() => {
    if (node.isConnected) (node.querySelector<HTMLElement>(initial) ?? node).focus({ preventScroll: true });
  });

  const onKey = (event: KeyboardEvent) => {
    if (event.key !== 'Tab' || node.inert) return;
    const controls = [...node.querySelectorAll<HTMLElement>(
      'button, a[href], input, select, textarea, [tabindex]'
    )].filter((el) => el.tabIndex >= 0 && !el.matches(':disabled') && !el.closest('[inert]') && el.getClientRects().length);
    const first = controls[0];
    const last = controls.at(-1);
    if (!first || !last) { event.preventDefault(); return; }
    const active = document.activeElement;
    if (!controls.includes(active as HTMLElement) || (event.shiftKey ? active === first : active === last)) {
      event.preventDefault();
      (event.shiftKey ? last : first).focus();
    }
  };
  node.addEventListener('keydown', onKey);
  return {
    destroy() {
      node.removeEventListener('keydown', onKey);
      if (restore && previous instanceof HTMLElement && previous.isConnected) previous.focus({ preventScroll: true });
    }
  };
}
