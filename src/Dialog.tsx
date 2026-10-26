import { ReactNode, useEffect, useId, useRef } from "react";

export default function Dialog({ title, children, onClose, busy = false }: { title: string; children: ReactNode; onClose: () => void; busy?: boolean }) {
  const ref = useRef<HTMLDivElement>(null);
  const titre = useId();
  const courant = useRef({ onClose, busy });
  courant.current = { onClose, busy };
  useEffect(() => {
    const precedent = document.activeElement as HTMLElement | null;
    ref.current?.focus();
    const clavier = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !courant.current.busy) { e.preventDefault(); courant.current.onClose(); }
      if (e.key !== "Tab" || !ref.current) return;
      const controles = Array.from(ref.current.querySelectorAll<HTMLElement>('a[href],button:not([disabled]),input:not([disabled]),select:not([disabled]),textarea:not([disabled]),[tabindex="0"]'));
      const premier = controles[0], dernier = controles[controles.length - 1];
      if (!premier) { e.preventDefault(); return; }
      if (e.shiftKey && (document.activeElement === premier || document.activeElement === ref.current)) { e.preventDefault(); dernier.focus(); }
      else if (!e.shiftKey && (document.activeElement === dernier || document.activeElement === ref.current)) { e.preventDefault(); premier.focus(); }
    };
    document.addEventListener("keydown", clavier);
    return () => { document.removeEventListener("keydown", clavier); if (precedent?.isConnected) precedent.focus(); };
  }, []);
  return <div className="overlay" onClick={() => !busy && onClose()}><div className="modal" ref={ref} role="dialog" aria-modal="true" aria-labelledby={titre} tabIndex={-1} onClick={(e) => e.stopPropagation()}><h2 id={titre}>{title}</h2>{children}</div></div>;
}
