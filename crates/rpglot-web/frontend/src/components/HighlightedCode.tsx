import { useMemo } from "react";
import { tokenizeSQL, tokenizePlan } from "../utils/highlight";
import type { Token } from "../utils/highlight";

interface Props {
  text: string;
  language: "sql" | "plan" | "text";
  className?: string;
}

const TOKEN_CLASS: Partial<Record<Token["type"], string>> = {
  keyword: "hl-keyword",
  string: "hl-string",
  number: "hl-number",
  comment: "hl-comment",
  operator: "hl-operator",
  param: "hl-param",
  node: "hl-node",
  cost: "hl-cost",
  arrow: "hl-arrow",
  condition: "hl-condition",
};

export function HighlightedCode({ text, language, className }: Props) {
  const tokens = useMemo(() => {
    if (language === "sql") return tokenizeSQL(text);
    if (language === "plan") return tokenizePlan(text);
    return null;
  }, [text, language]);

  if (!tokens) {
    return (
      <pre className={className}>
        {text}
      </pre>
    );
  }

  return (
    <pre className={className}>
      {tokens.map((tok, i) => {
        const cls = TOKEN_CLASS[tok.type];
        if (!cls) return tok.text;
        return (
          <span key={i} className={cls}>
            {tok.text}
          </span>
        );
      })}
    </pre>
  );
}
