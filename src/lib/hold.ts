// Mantener pulsado para confirmar, en vez de un diálogo: el gesto ya es la confirmación. El botón
// se llena mientras se mantiene (ratón, o Espacio/Intro) y actúa al completarse; soltar antes no
// hace nada salvo avisar (`onhint`) de que hay que mantener. Shift confirma al instante, como
// saltaba el diálogo. Es una acción y no un componente para que cada botón conserve sus estilos.
//
// El botón necesita dentro un `<span class="hold-fill"></span>` (estilos en app.css).

export type HoldApi = { press(shift?: boolean): void; release(): void };

type Options = {
  onconfirm: () => void;
  onhint?: (on: boolean) => void;
  duration?: number;
  ref?: (api: HoldApi) => void;
};

export function hold(node: HTMLElement, options: Options) {
  let opts = options;
  let holding = false;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let hintTimer: ReturnType<typeof setTimeout> | undefined;
  const ms = () => opts.duration ?? 900;

  node.dataset.hold = '';

  function setHolding(on: boolean) {
    holding = on;
    if (on) node.dataset.holding = '';
    else delete node.dataset.holding;
  }

  function press(shift = false) {
    if (holding || (node as HTMLButtonElement).disabled) return;
    if (shift) {
      opts.onconfirm();
      return;
    }
    clearTimeout(hintTimer);
    opts.onhint?.(false);
    node.style.setProperty('--hold-ms', `${ms()}ms`);
    setHolding(true);
    timer = setTimeout(() => {
      setHolding(false);
      opts.onconfirm();
    }, ms());
  }

  function release() {
    if (!holding) return;
    clearTimeout(timer);
    setHolding(false);
    opts.onhint?.(true);
    clearTimeout(hintTimer);
    hintTimer = setTimeout(() => opts.onhint?.(false), 1600);
  }

  const onPointerDown = (e: PointerEvent) => {
    if (e.button === 0) press(e.shiftKey);
  };
  const onKeyDown = (e: KeyboardEvent) => {
    if ((e.key === ' ' || e.key === 'Enter') && !e.repeat) {
      e.preventDefault();
      press(e.shiftKey);
    }
  };
  const onKeyUp = (e: KeyboardEvent) => {
    if (e.key === ' ' || e.key === 'Enter') release();
  };
  // El clic llega al soltar: no debe hacer nada (ni cerrar el menú que contiene el botón).
  const onClick = (e: MouseEvent) => e.stopPropagation();

  node.addEventListener('pointerdown', onPointerDown);
  node.addEventListener('pointerup', release);
  node.addEventListener('pointerleave', release);
  node.addEventListener('pointercancel', release);
  node.addEventListener('keydown', onKeyDown);
  node.addEventListener('keyup', onKeyUp);
  node.addEventListener('click', onClick);
  opts.ref?.({ press, release });

  return {
    update(next: Options) {
      opts = next;
    },
    destroy() {
      clearTimeout(timer);
      clearTimeout(hintTimer);
      node.removeEventListener('pointerdown', onPointerDown);
      node.removeEventListener('pointerup', release);
      node.removeEventListener('pointerleave', release);
      node.removeEventListener('pointercancel', release);
      node.removeEventListener('keydown', onKeyDown);
      node.removeEventListener('keyup', onKeyUp);
      node.removeEventListener('click', onClick);
    }
  };
}
