import { useState, useRef, useCallback, type ReactNode } from "react";
import { createPortal } from "react-dom";

interface TooltipProps {
  content: string;
  children: ReactNode;
  side?: "top" | "bottom";
}

export function Tooltip({ content, children, side = "top" }: TooltipProps) {
  const [show, setShow] = useState(false);
  const [coords, setCoords] = useState({ x: 0, y: 0 });
  const timeout = useRef<ReturnType<typeof setTimeout>>(undefined);
  const triggerRef = useRef<HTMLSpanElement>(null);

  const handleEnter = useCallback(() => {
    timeout.current = setTimeout(() => {
      if (triggerRef.current) {
        const rect = triggerRef.current.getBoundingClientRect();
        setCoords({
          x: rect.left + rect.width / 2,
          y: side === "top" ? rect.top - 6 : rect.bottom + 6,
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
          <span
            className={`fixed z-[9999] px-2 py-1 text-xs rounded whitespace-nowrap pointer-events-none
              bg-[var(--text-primary)] text-[var(--text-inverse)]`}
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
          </span>,
          document.body,
        )}
    </span>
  );
}
