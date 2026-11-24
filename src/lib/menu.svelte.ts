// Всплывающее меню: «Ещё», папки, правый клик по заметке.
export type MenuItem =
  | { label: string; icon?: string; hint?: string; danger?: boolean; checked?: boolean; disabled?: boolean; run: () => void }
  | { sep: true }
  | { header: string };

export const menu = $state<{ current: { x: number; y: number; items: MenuItem[]; up: boolean } | null }>({ current: null });

/** Открывает меню у элемента (под ним) или в точке курсора. */
export function openMenu(at: MouseEvent | HTMLElement | DOMRect, items: MenuItem[]) {
  let x: number, y: number, up = false;
  if (at instanceof MouseEvent) {
    at.preventDefault();
    x = at.clientX;
    y = at.clientY;
  } else {
    const r = at instanceof DOMRect ? at : at.getBoundingClientRect();
    x = r.left;
    y = r.bottom + 4;
    if (y > window.innerHeight - 160) { y = r.top - 4; up = true; }
  }
  menu.current = { x, y, items, up };
}

export const closeMenu = () => (menu.current = null);
