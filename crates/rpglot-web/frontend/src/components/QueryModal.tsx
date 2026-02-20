import { useState, useEffect, useCallback, useMemo } from "react";
import { createPortal } from "react-dom";
import { X, Copy, Check } from "lucide-react";
import { HighlightedCode } from "./HighlightedCode";

interface QueryModalProps {
  text: string;
  language: "sql" | "plan" | "text";
  title: string;
  onClose: () => void;
}

export function QueryModal({ text, language, title, onClose }: QueryModalProps) {
  const [copied, setCopied] = useState(false);

  const lineCount = useMemo(() => text.split("\n").length, [text]);

  // Capture-phase Escape — close modal without propagating
  useEffect(() => {
    function handleKeyDown(e: KeyboardEvent) {
      if (e.key === "Escape") {
        e.stopPropagation();
        e.preventDefault();
        onClose();
      }
    }
    window.addEventListener("keydown", handleKeyDown, true);
    return () => window.removeEventListener("keydown", handleKeyDown, true);
  }, [onClose]);

  const handleCopy = useCallback(() => {
    navigator.clipboard.writeText(text).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    });
  }, [text]);

  const lineNumbers = useMemo(() => {
    const lines: string[] = [];
    for (let i = 1; i <= lineCount; i++) {
      lines.push(String(i));
    }
    return lines.join("\n");
  }, [lineCount]);

  return createPortal(
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/50"
      onClick={onClose}
    >
      <div
        className="relative w-[90vw] max-w-[1200px] max-h-[90vh] flex flex-col rounded-xl border border-[var(--border-default)] bg-[var(--bg-surface)]"
        style={{ boxShadow: "var(--shadow-md)" }}
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="flex items-center gap-3 px-5 py-3 border-b border-[var(--border-default)] bg-[var(--bg-elevated)] rounded-t-xl">
          <h2 className="flex-1 text-sm font-semibold text-[var(--text-primary)]">
            {title}
          </h2>
          <span className="text-[11px] text-[var(--text-tertiary)] font-mono tabular-nums">
            {lineCount} {lineCount === 1 ? "line" : "lines"}
          </span>
          <button
            onClick={handleCopy}
            className="flex items-center gap-1 text-[11px] px-2 py-1 rounded transition-colors text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-hover)]"
          >
            {copied ? (
              <>
                <Check size={12} className="text-[var(--status-success)]" />
                <span className="text-[var(--status-success)]">copied</span>
              </>
            ) : (
              <>
                <Copy size={12} />
                <span>copy</span>
              </>
            )}
          </button>
          <button
            onClick={onClose}
            className="p-1 rounded text-[var(--text-tertiary)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-hover)] transition-colors"
          >
            <X size={16} />
          </button>
        </div>

        {/* Body */}
        <div className="flex-1 overflow-auto p-0">
          <div className="flex min-h-0">
            {/* Line numbers */}
            <pre className="sticky left-0 select-none px-3 py-4 text-[12px] font-mono text-right text-[var(--text-disabled)] bg-[var(--bg-elevated)] border-r border-[var(--border-default)] leading-[1.6]">
              {lineNumbers}
            </pre>
            {/* Code */}
            <HighlightedCode
              text={text}
              language={language}
              className="flex-1 p-4 text-[13px] font-mono text-[var(--text-primary)] whitespace-pre-wrap break-all leading-[1.6]"
            />
          </div>
        </div>
      </div>
    </div>,
    document.body,
  );
}
