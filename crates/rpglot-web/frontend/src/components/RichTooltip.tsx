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
  const enterTimeout = useRef<ReturnType<typeof setTimeout>>(undefined);
  const leaveTimeout = useRef<ReturnType<typeof setTimeout>>(undefined);
  const triggerRef = useRef<HTMLSpanElement>(null);

  const showTooltip = useCallback(() => {
    clearTimeout(leaveTimeout.current);
    enterTimeout.current = setTimeout(() => {
      if (triggerRef.current) {
        const rect = triggerRef.current.getBoundingClientRect();
        const x = Math.min(rect.left + rect.width / 2, window.innerWidth - 170);
        setCoords({
          x: Math.max(170, x),
          y: side === "top" ? rect.top - 8 : rect.bottom + 8,
        });
      }
      setShow(true);
    }, 400);
  }, [side]);

  const hideTooltip = useCallback(() => {
    clearTimeout(enterTimeout.current);
    leaveTimeout.current = setTimeout(() => {
      setShow(false);
    }, 150);
  }, []);

  const handleTooltipEnter = useCallback(() => {
    clearTimeout(leaveTimeout.current);
  }, []);

  const handleTooltipLeave = useCallback(() => {
    leaveTimeout.current = setTimeout(() => {
      setShow(false);
    }, 150);
  }, []);

  return (
    <span
      ref={triggerRef}
      className="inline-flex"
      onMouseEnter={showTooltip}
      onMouseLeave={hideTooltip}
    >
      {children}
      {show &&
        createPortal(
          <div
            className="fixed z-[9999] px-3 py-2 text-xs rounded-lg max-w-xs
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
            onMouseEnter={handleTooltipEnter}
            onMouseLeave={handleTooltipLeave}
          >
            {content}
          </div>,
          document.body,
        )}
    </span>
  );
}
