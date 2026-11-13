// A positioned popup menu with nested submenus, used for the menu bar and
// for right-click menus. Closes on outside click, Escape, or selection.
import { useEffect, useLayoutEffect, useRef, useState } from "react";

export type MenuEntry =
  | { type?: "item"; label: string; shortcut?: string; disabled?: boolean; checked?: boolean; onSelect?: () => void; submenu?: MenuEntry[] }
  | { type: "separator" };

export interface PopupMenuProps {
  x: number;
  y: number;
  items: MenuEntry[];
  onClose: () => void;
  /** Keep the menu open when this element is clicked (the menu-bar title). */
  anchor?: HTMLElement | null;
}

export function PopupMenu({ x, y, items, onClose, anchor }: PopupMenuProps) {
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState({ x, y });
  const [openSub, setOpenSub] = useState<number | null>(null);
  const [subPos, setSubPos] = useState({ x: 0, y: 0 });
  const [active, setActive] = useState(-1);

  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const r = el.getBoundingClientRect();
    setPos({
      x: Math.max(0, Math.min(x, window.innerWidth - r.width - 4)),
      y: Math.max(0, Math.min(y, window.innerHeight - r.height - 4)),
    });
  }, [x, y]);

  useEffect(() => {
    const onDown = (e: MouseEvent) => {
      const t = e.target as Node;
      if (ref.current?.contains(t) || anchor?.contains(t)) return;
      if ((t as HTMLElement).closest?.(".menu-popup")) return;
      onClose();
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        onClose();
      }
    };
    window.addEventListener("mousedown", onDown, true);
    window.addEventListener("keydown", onKey, true);
    window.addEventListener("blur", onClose);
    return () => {
      window.removeEventListener("mousedown", onDown, true);
      window.removeEventListener("keydown", onKey, true);
      window.removeEventListener("blur", onClose);
    };
  }, [onClose, anchor]);

  const selectable = items
    .map((it, i) => ({ it, i }))
    .filter(({ it }) => it.type !== "separator" && !it.disabled);

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const idx = selectable.findIndex((s) => s.i === active);
      const next = e.key === "ArrowDown" ? idx + 1 : idx - 1;
      const wrapped = (next + selectable.length) % Math.max(1, selectable.length);
      setActive(selectable[wrapped]?.i ?? -1);
    } else if (e.key === "Enter" && active >= 0) {
      e.preventDefault();
      const it = items[active];
      if (it.type !== "separator" && it.onSelect && !it.submenu) {
        onClose();
        it.onSelect();
      }
    }
  };

  useEffect(() => ref.current?.focus(), []);

  return (
    <>
      <div
        ref={ref}
        className="menu-popup"
        role="menu"
        tabIndex={-1}
        style={{ left: pos.x, top: pos.y, outline: "none" }}
        onKeyDown={onKeyDown}
        onContextMenu={(e) => e.preventDefault()}
      >
        {items.map((it, i) =>
          it.type === "separator" ? (
            <div key={i} className="menu-sep" />
          ) : (
            <div
              key={i}
              role="menuitem"
              aria-disabled={it.disabled}
              className={`menu-item${it.disabled ? " disabled" : ""}${openSub === i || active === i ? " open" : ""}`}
              onMouseEnter={(e) => {
                setActive(i);
                if (it.submenu && !it.disabled) {
                  const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
                  setSubPos({ x: r.right - 2, y: r.top - 4 });
                  setOpenSub(i);
                } else {
                  setOpenSub(null);
                }
              }}
              onClick={() => {
                if (it.disabled || it.submenu) return;
                onClose();
                it.onSelect?.();
              }}
            >
              <span style={{ width: 14, display: "inline-block" }}>{it.checked ? "✓" : ""}</span>
              <span>{it.label}</span>
              {it.shortcut && <span className="shortcut">{it.shortcut}</span>}
              {it.submenu && <span className="submenu-arrow">▸</span>}
            </div>
          ),
        )}
      </div>
      {openSub !== null && items[openSub].type !== "separator" && (items[openSub] as { submenu?: MenuEntry[] }).submenu && (
        <PopupMenu
          x={subPos.x}
          y={subPos.y}
          items={(items[openSub] as { submenu: MenuEntry[] }).submenu}
          onClose={onClose}
          anchor={ref.current}
        />
      )}
    </>
  );
}

/** Menu bar: titles that open dropdown menus; hovering switches menus while one is open. */
export function MenuBar({ menus }: { menus: { title: string; items: () => MenuEntry[] }[] }) {
  const [open, setOpen] = useState<number | null>(null);
  const refs = useRef<(HTMLButtonElement | null)[]>([]);
  const at = (i: number) => {
    const r = refs.current[i]?.getBoundingClientRect();
    return { x: r?.left ?? 0, y: r?.bottom ?? 0 };
  };
  return (
    <div className="menubar" role="menubar">
      {menus.map((m, i) => (
        <button
          key={m.title}
          ref={(el) => {
            refs.current[i] = el;
          }}
          className={`menubar-title${open === i ? " open" : ""}`}
          onMouseDown={(e) => {
            e.preventDefault();
            setOpen(open === i ? null : i);
          }}
          onMouseEnter={() => open !== null && open !== i && setOpen(i)}
        >
          {m.title}
        </button>
      ))}
      {open !== null && (
        <PopupMenu
          key={open}
          {...at(open)}
          items={menus[open].items()}
          onClose={() => setOpen(null)}
          anchor={refs.current[open]}
        />
      )}
    </div>
  );
}
