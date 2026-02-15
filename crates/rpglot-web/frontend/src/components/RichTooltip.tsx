import { useState, useRef, useCallback, type ReactNode } from "react";
import { createPortal } from "react-dom";

interface RichTooltipProps {
  content: ReactNode;
  children: ReactNode;
  side?: "top" | "bottom";
}

export function RichTooltip({
  content,
  children,
  side = "top",
}: RichTooltipProps) {
  const [show, setShow] = useState(false);
  const [coords, setCoords] = useState({ x: 0, y: 0 });
  const timeout = useRef<ReturnType<typeof setTimeout>>(undefined);
  const triggerRef = useRef<HTMLSpanElement>(null);

  const handleEnter = useCallback(() => {
    timeout.current = setTimeout(() => {
      if (triggerRef.current) {
        const rect = triggerRef.current.getBoundingClientRect();
        const x = Math.min(
          rect.left + rect.width / 2,
          window.innerWidth - 170,
        );
        setCoords({
          x: Math.max(170, x),
          y: side === "top" ? rect.top - 8 : rect.bottom + 8,
        });
      }
      setShow(true);
    }, 400);
  }, [side]);

  const handleLeave = useCallback(() => {
    clearTimeout(timeout.current);
    setShow(false);
  }, []);

  return (
    <span
      ref={triggerRef}
      className="inline-flex"
      onMouseEnter={handleEnter}
      onMouseLeave={handleLeave}
    >
      {children}
      {show &&
        createPortal(
          <div
            className="fixed z-[9999] px-3 py-2 text-xs rounded-lg max-w-xs pointer-events-none
              bg-[var(--bg-elevated)] text-[var(--text-primary)] border border-[var(--border-default)]"
            style={{
              left: coords.x,
              top: coords.y,
              transform:
                side === "top"
                  ? "translate(-50%, -100%)"
                  : "translate(-50%, 0)",
              boxShadow: "var(--shadow-md)",
            }}
          >
            {content}
          </div>,
          document.body,
        )}
    </span>
  );
}
