import { useState, useRef, type ReactNode } from "react";

interface TooltipProps {
    content: string;
    children: ReactNode;
    side?: "top" | "bottom";
}

export function Tooltip({ content, children, side = "top" }: TooltipProps) {
    const [show, setShow] = useState(false);
    const timeout = useRef<ReturnType<typeof setTimeout>>(undefined);

    const handleEnter = () => {
        timeout.current = setTimeout(() => setShow(true), 400);
    };
    const handleLeave = () => {
        clearTimeout(timeout.current);
        setShow(false);
    };

    return (
        <span
            className="relative inline-flex"
            onMouseEnter={handleEnter}
            onMouseLeave={handleLeave}
        >
            {children}
            {show && (
                <span
                    className={`absolute z-50 px-2 py-1 text-xs rounded whitespace-nowrap pointer-events-none
                        bg-[var(--text-primary)] text-[var(--text-inverse)]
                        ${side === "top" ? "bottom-full mb-1.5 left-1/2 -translate-x-1/2" : "top-full mt-1.5 left-1/2 -translate-x-1/2"}
                    `}
                    style={{ boxShadow: "var(--shadow-md)" }}
                >
                    {content}
                </span>
            )}
        </span>
    );
}
