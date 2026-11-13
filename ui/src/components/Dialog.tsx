// Modal dialog frame. Escape and the close button call onClose; Enter in a
// single-line input triggers onSubmit when given.
import { useEffect, useRef } from "react";
import type { ReactNode, CSSProperties } from "react";

export interface DialogProps {
  title: ReactNode;
  children: ReactNode;
  footer?: ReactNode;
  onClose: () => void;
  onSubmit?: () => void;
  width?: number | string;
  height?: number | string;
  className?: string;
  style?: CSSProperties;
}

export function Dialog({ title, children, footer, onClose, onSubmit, width, height, className, style }: DialogProps) {
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const first = el.querySelector<HTMLElement>("[autofocus], input, select, textarea, button.primary");
    first?.focus();
  }, []);

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") {
      e.stopPropagation();
      onClose();
    } else if (
      e.key === "Enter" &&
      onSubmit &&
      !e.shiftKey &&
      (e.target as HTMLElement).tagName === "INPUT" &&
      (e.target as HTMLInputElement).type !== "checkbox"
    ) {
      e.preventDefault();
      onSubmit();
    }
  };

  return (
    <div className="dialog-backdrop" onMouseDown={(e) => e.target === e.currentTarget && e.stopPropagation()}>
      <div
        ref={ref}
        className={`dialog ${className ?? ""}`}
        role="dialog"
        aria-modal="true"
        style={{ width, height, ...style }}
        onKeyDown={onKeyDown}
      >
        <div className="dialog-title">
          <span>{title}</span>
          <button className="icon close" aria-label="Close" onClick={onClose}>
            ✕
          </button>
        </div>
        <div className="dialog-body grow">{children}</div>
        {footer && <div className="dialog-footer">{footer}</div>}
      </div>
    </div>
  );
}
