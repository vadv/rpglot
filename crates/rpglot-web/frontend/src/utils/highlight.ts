export interface Token {
  type:
    | "keyword"
    | "string"
    | "number"
    | "comment"
    | "operator"
    | "param"
    | "plain"
    | "node"
    | "cost"
    | "arrow"
    | "condition";
  text: string;
  severity?: "warn" | "critical";
}

const SQL_KEYWORDS = new Set([
  "SELECT",
  "FROM",
  "WHERE",
  "AND",
  "OR",
  "NOT",
  "IN",
  "EXISTS",
  "BETWEEN",
  "LIKE",
  "ILIKE",
  "IS",
  "NULL",
  "TRUE",
  "FALSE",
  "AS",
  "ON",
  "JOIN",
  "LEFT",
  "RIGHT",
  "INNER",
  "OUTER",
  "FULL",
  "CROSS",
  "LATERAL",
  "NATURAL",
  "USING",
  "INSERT",
  "INTO",
  "VALUES",
  "UPDATE",
  "SET",
  "DELETE",
  "CREATE",
  "ALTER",
  "DROP",
  "TABLE",
  "INDEX",
  "VIEW",
  "SCHEMA",
  "DATABASE",
  "TRIGGER",
  "FUNCTION",
  "PROCEDURE",
  "SEQUENCE",
  "TYPE",
  "EXTENSION",
  "IF",
  "THEN",
  "ELSE",
  "ELSIF",
  "END",
  "CASE",
  "WHEN",
  "BEGIN",
  "DECLARE",
  "RETURN",
  "RETURNS",
  "LOOP",
  "FOR",
  "WHILE",
  "EXIT",
  "CONTINUE",
  "GROUP",
  "BY",
  "ORDER",
  "HAVING",
  "LIMIT",
  "OFFSET",
  "FETCH",
  "FIRST",
  "NEXT",
  "ROWS",
  "ONLY",
  "UNION",
  "ALL",
  "INTERSECT",
  "EXCEPT",
  "DISTINCT",
  "WITH",
  "RECURSIVE",
  "ASC",
  "DESC",
  "NULLS",
  "PARTITION",
  "OVER",
  "WINDOW",
  "RANGE",
  "UNBOUNDED",
  "PRECEDING",
  "FOLLOWING",
  "CURRENT",
  "ROW",
  "FILTER",
  "WITHIN",
  "GRANT",
  "REVOKE",
  "TRUNCATE",
  "VACUUM",
  "ANALYZE",
  "EXPLAIN",
  "EXECUTE",
  "PREPARE",
  "DEALLOCATE",
  "LISTEN",
  "NOTIFY",
  "UNLISTEN",
  "COPY",
  "DO",
  "PERFORM",
  "RAISE",
  "EXCEPTION",
  "NOTICE",
  "CONSTRAINT",
  "PRIMARY",
  "KEY",
  "FOREIGN",
  "REFERENCES",
  "UNIQUE",
  "CHECK",
  "DEFAULT",
  "CASCADE",
  "RESTRICT",
  "NO",
  "ACTION",
  "DEFERRABLE",
  "DEFERRED",
  "IMMEDIATE",
  "CONCURRENTLY",
  "TEMPORARY",
  "TEMP",
  "UNLOGGED",
  "MATERIALIZED",
  "REFRESH",
  "REINDEX",
  "CLUSTER",
  "COMMIT",
  "ROLLBACK",
  "SAVEPOINT",
  "RELEASE",
  "LOCK",
  "SHARE",
  "EXCLUSIVE",
  "ACCESS",
  "NOWAIT",
  "SKIP",
  "LOCKED",
  "COALESCE",
  "GREATEST",
  "LEAST",
  "CAST",
  "ANY",
  "SOME",
  "ARRAY",
  "UNNEST",
  "LATERAL",
  "TABLESAMPLE",
  "RETURNING",
  "CONFLICT",
  "NOTHING",
  "GENERATED",
  "ALWAYS",
  "IDENTITY",
  "OVERRIDING",
  "SYSTEM",
  "VALUE",
  "REPLACING",
  "OWNER",
  "TO",
  "RENAME",
  "COLUMN",
  "ADD",
  "BOOLEAN",
  "INTEGER",
  "INT",
  "BIGINT",
  "SMALLINT",
  "TEXT",
  "VARCHAR",
  "CHAR",
  "NUMERIC",
  "DECIMAL",
  "REAL",
  "FLOAT",
  "DOUBLE",
  "PRECISION",
  "DATE",
  "TIME",
  "TIMESTAMP",
  "INTERVAL",
  "JSON",
  "JSONB",
  "UUID",
  "BYTEA",
  "SERIAL",
  "BIGSERIAL",
  "MONEY",
  "INET",
  "CIDR",
  "MACADDR",
  "BIT",
  "VARBIT",
  "XML",
  "POINT",
  "LINE",
  "LSEG",
  "BOX",
  "PATH",
  "POLYGON",
  "CIRCLE",
  "TSVECTOR",
  "TSQUERY",
  "OID",
  "REGCLASS",
  "VOID",
  "RECORD",
  "SETOF",
  "LANGUAGE",
  "PLPGSQL",
  "SQL",
  "VOLATILE",
  "STABLE",
  "IMMUTABLE",
  "STRICT",
  "SECURITY",
  "DEFINER",
  "INVOKER",
  "PARALLEL",
  "SAFE",
  "UNSAFE",
  "COST",
  "CALLED",
  "INPUT",
  "REPLACE",
  "COUNT",
  "SUM",
  "AVG",
  "MIN",
  "MAX",
  "STRING_AGG",
  "ARRAY_AGG",
  "BOOL_AND",
  "BOOL_OR",
  "EVERY",
  "RANK",
  "DENSE_RANK",
  "ROW_NUMBER",
  "NTILE",
  "LAG",
  "LEAD",
  "FIRST_VALUE",
  "LAST_VALUE",
  "NTH_VALUE",
  "CUME_DIST",
  "PERCENT_RANK",
  "GROUPING",
  "ROLLUP",
  "CUBE",
  "SETS",
  "EXTRACT",
  "EPOCH",
  "YEAR",
  "MONTH",
  "DAY",
  "HOUR",
  "MINUTE",
  "SECOND",
  "AGE",
  "NOW",
  "CURRENT_TIMESTAMP",
  "CURRENT_DATE",
  "CURRENT_TIME",
  "LOCALTIME",
  "LOCALTIMESTAMP",
  "UPPER",
  "LOWER",
  "LENGTH",
  "TRIM",
  "SUBSTRING",
  "POSITION",
  "OVERLAY",
  "SIMILAR",
  "ESCAPE",
  "COLLATE",
  "VARYING",
  "ZONE",
  "WITHOUT",
  "WORK",
  "TRANSACTION",
  "ISOLATION",
  "LEVEL",
  "READ",
  "WRITE",
  "COMMITTED",
  "UNCOMMITTED",
  "REPEATABLE",
  "SERIALIZABLE",
  "ONLY",
  "FORCE",
  "LOCAL",
  "SESSION",
  "GLOBAL",
  "SHOW",
  "RESET",
  "DISCARD",
  "COMMENT",
  "SECURITY",
  "LABEL",
  "POLICY",
  "ENABLE",
  "DISABLE",
  "RULE",
  "ALSO",
  "INSTEAD",
]);

const OPERATORS = new Set([
  "=",
  "<>",
  "!=",
  "<",
  ">",
  "<=",
  ">=",
  "+",
  "-",
  "*",
  "/",
  "%",
  "||",
  "::",
  ".",
  ",",
  ";",
  "(",
  ")",
  "[",
  "]",
  "~",
  "!~",
  "~*",
  "!~*",
  "@>",
  "<@",
  "?",
  "?|",
  "?&",
  "#>",
  "#>>",
  "->",
  "->>",
  "&&",
]);

// Multi-char operators sorted longest-first for greedy matching
const MULTI_OPS = [
  "<>",
  "!=",
  "<=",
  ">=",
  "||",
  "::",
  "!~*",
  "!~",
  "~*",
  "@>",
  "<@",
  "?|",
  "?&",
  "#>>",
  "#>",
  "->>",
  "->",
  "&&",
];

function isWordChar(ch: string): boolean {
  return /[a-zA-Z0-9_]/.test(ch);
}

function isDigit(ch: string): boolean {
  return ch >= "0" && ch <= "9";
}

export function tokenizeSQL(text: string): Token[] {
  const tokens: Token[] = [];
  const len = text.length;
  let i = 0;

  function emit(type: Token["type"], t: string) {
    if (t) tokens.push({ type, text: t });
  }

  while (i < len) {
    const ch = text[i];

    // Single-line comment --
    if (ch === "-" && i + 1 < len && text[i + 1] === "-") {
      const start = i;
      i += 2;
      while (i < len && text[i] !== "\n") i++;
      emit("comment", text.slice(start, i));
      continue;
    }

    // Multi-line comment /* */
    if (ch === "/" && i + 1 < len && text[i + 1] === "*") {
      const start = i;
      i += 2;
      while (i < len - 1 && !(text[i] === "*" && text[i + 1] === "/")) i++;
      if (i < len - 1) i += 2;
      else i = len; // unclosed — emit to end
      emit("comment", text.slice(start, i));
      continue;
    }

    // String literal 'xxx'
    if (ch === "'") {
      const start = i;
      i++;
      while (i < len) {
        if (text[i] === "'" && i + 1 < len && text[i + 1] === "'") {
          i += 2; // escaped quote
        } else if (text[i] === "'") {
          i++;
          break;
        } else {
          i++;
        }
      }
      emit("string", text.slice(start, i));
      continue;
    }

    // Dollar-quoted string $tag$...$tag$
    if (ch === "$" && i + 1 < len) {
      // Check if this is a parameter $1, $2
      if (isDigit(text[i + 1])) {
        const start = i;
        i++;
        while (i < len && isDigit(text[i])) i++;
        emit("param", text.slice(start, i));
        continue;
      }
      // Dollar-quoted string $$ or $tag$
      const tagStart = i;
      let j = i + 1;
      while (j < len && (isWordChar(text[j]) || text[j] === "$")) {
        if (text[j] === "$") {
          j++;
          break;
        }
        j++;
      }
      if (j > tagStart + 1 && text[j - 1] === "$") {
        const tag = text.slice(tagStart, j);
        const bodyStart = j;
        const endIdx = text.indexOf(tag, bodyStart);
        if (endIdx !== -1) {
          i = endIdx + tag.length;
        } else {
          i = len; // unclosed
        }
        emit("string", text.slice(tagStart, i));
        continue;
      }
    }

    // Parameter $1, $2 (standalone $ that didn't match above)
    if (ch === "$" && i + 1 < len && isDigit(text[i + 1])) {
      const start = i;
      i++;
      while (i < len && isDigit(text[i])) i++;
      emit("param", text.slice(start, i));
      continue;
    }

    // Number
    if (isDigit(ch) || (ch === "." && i + 1 < len && isDigit(text[i + 1]))) {
      const start = i;
      // Check prev char — if it's a word char, this is part of identifier
      if (start > 0 && isWordChar(text[start - 1])) {
        // Part of a word, will be handled by word branch below
      } else {
        i++;
        while (i < len && (isDigit(text[i]) || text[i] === ".")) i++;
        // Scientific notation
        if (i < len && (text[i] === "e" || text[i] === "E")) {
          i++;
          if (i < len && (text[i] === "+" || text[i] === "-")) i++;
          while (i < len && isDigit(text[i])) i++;
        }
        emit("number", text.slice(start, i));
        continue;
      }
    }

    // Word (keyword or identifier)
    if (isWordChar(ch) && !isDigit(ch)) {
      const start = i;
      while (i < len && isWordChar(text[i])) i++;
      const word = text.slice(start, i);
      if (SQL_KEYWORDS.has(word.toUpperCase())) {
        emit("keyword", word);
      } else {
        emit("plain", word);
      }
      continue;
    }

    // Digits that are part of identifiers (caught by the check above)
    if (isDigit(ch)) {
      const start = i;
      while (i < len && isWordChar(text[i])) i++;
      emit("plain", text.slice(start, i));
      continue;
    }

    // Multi-char operators
    let matched = false;
    for (const op of MULTI_OPS) {
      if (text.startsWith(op, i)) {
        emit("operator", op);
        i += op.length;
        matched = true;
        break;
      }
    }
    if (matched) continue;

    // Single-char operators
    if (OPERATORS.has(ch)) {
      emit("operator", ch);
      i++;
      continue;
    }

    // Whitespace and other characters
    const start = i;
    while (
      i < len &&
      !isWordChar(text[i]) &&
      text[i] !== "'" &&
      text[i] !== "$" &&
      text[i] !== "-" &&
      text[i] !== "/" &&
      !OPERATORS.has(text[i])
    ) {
      i++;
    }
    if (i > start) {
      emit("plain", text.slice(start, i));
    } else {
      // Safety: advance at least one character to avoid infinite loop
      emit("plain", text[i]);
      i++;
    }
  }

  return tokens;
}

// EXPLAIN plan node types
const PLAN_NODES = [
  "Seq Scan",
  "Index Scan",
  "Index Only Scan",
  "Bitmap Heap Scan",
  "Bitmap Index Scan",
  "Tid Scan",
  "Tid Range Scan",
  "Subquery Scan",
  "Function Scan",
  "Table Function Scan",
  "Values Scan",
  "CTE Scan",
  "Named Tuplestore Scan",
  "WorkTable Scan",
  "Foreign Scan",
  "Custom Scan",
  "Nested Loop",
  "Merge Join",
  "Hash Join",
  "Hash",
  "Materialize",
  "Memoize",
  "Sort",
  "Incremental Sort",
  "Group",
  "Aggregate",
  "GroupAggregate",
  "HashAggregate",
  "MixedAggregate",
  "WindowAgg",
  "Unique",
  "SetOp",
  "Lock Rows",
  "Limit",
  "Offset",
  "Result",
  "ProjectSet",
  "ModifyTable",
  "Insert",
  "Update",
  "Delete",
  "Merge",
  "Append",
  "Merge Append",
  "Recursive Union",
  "BitmapAnd",
  "BitmapOr",
  "Gather",
  "Gather Merge",
  "Parallel",
];

// Sorted longest-first for greedy matching
const SORTED_NODES = [...PLAN_NODES].sort((a, b) => b.length - a.length);

const CONDITION_LABELS = [
  "Filter:",
  "Hash Cond:",
  "Join Filter:",
  "Index Cond:",
  "Recheck Cond:",
  "Merge Cond:",
  "Sort Key:",
  "Group Key:",
  "Output:",
  "One-Time Filter:",
  "Rows Removed by Filter:",
  "Rows Removed by Index Recheck:",
  "Buffers:",
  "Planning Time:",
  "Execution Time:",
  "Planning:",
  "Workers Planned:",
  "Workers Launched:",
  "Heap Fetches:",
  "Cache Key:",
  "Cache Mode:",
  "Hits:",
  "Misses:",
  "Evictions:",
  "Overflows:",
  "Peak Memory Usage:",
  "Original Hash Batches:",
  "Original Hash Buckets:",
  "Hash Batches:",
  "Hash Buckets:",
  "Memory Usage:",
  "Disk Usage:",
  "Relations:",
  "Remote SQL:",
  "Subplans Removed:",
  "Order By:",
  "Presorted Key:",
  "Full-sort Groups:",
  "Pre-sorted Groups:",
  "Trigger:",
];

const SORTED_CONDITIONS = [...CONDITION_LABELS].sort(
  (a, b) => b.length - a.length,
);

// --- EXPLAIN problem detection helpers ---

type Severity = "warn" | "critical";

interface Range {
  start: number;
  end: number;
  type: Token["type"];
  severity?: Severity;
}

/** Parse estimated rows from (cost=...) and actual rows from (actual time=...) on the same line */
function computeRowSeverity(text: string): Severity | undefined {
  const estMatch = /\(cost=[\d.]+\.\.[\d.]+ rows=(\d+) width=\d+\)/.exec(text);
  const actMatch =
    /\(actual time=[\d.]+\.\.[\d.]+ rows=(\d+) loops=(\d+)\)/.exec(text);
  if (!estMatch || !actMatch) return undefined;
  const est = parseInt(estMatch[1], 10);
  const act = parseInt(actMatch[1], 10) * parseInt(actMatch[2], 10);
  const ratio = Math.max(est, act) / Math.max(Math.min(est, act), 1);
  if (ratio >= 100) return "critical";
  if (ratio >= 10) return "warn";
  return undefined;
}

/** Split a cost/actual block into sub-ranges, marking rows=N with severity if needed */
function addCostBlockRanges(
  ranges: Range[],
  matchStart: number,
  matchText: string,
  rowSeverity: Severity | undefined,
) {
  if (!rowSeverity) {
    ranges.push({ start: matchStart, end: matchStart + matchText.length, type: "cost" });
    return;
  }
  // Find rows=N within the block
  const rowsRe = /rows=\d+/g;
  let rm;
  let last = 0;
  while ((rm = rowsRe.exec(matchText)) !== null) {
    if (rm.index > last) {
      ranges.push({
        start: matchStart + last,
        end: matchStart + rm.index,
        type: "cost",
      });
    }
    ranges.push({
      start: matchStart + rm.index,
      end: matchStart + rm.index + rm[0].length,
      type: "cost",
      severity: rowSeverity,
    });
    last = rm.index + rm[0].length;
  }
  if (last < matchText.length) {
    ranges.push({
      start: matchStart + last,
      end: matchStart + matchText.length,
      type: "cost",
    });
  }
}

/** Tokenize remaining text on a plan line with problem analysis */
function tokenizeRemainingWithAnalysis(
  text: string,
  tokens: Token[],
  condLabel?: string,
) {
  const ranges: Range[] = [];

  // 1. Row estimation severity for the whole line
  const rowSeverity = computeRowSeverity(text);

  // 2. Cost/actual blocks
  const costRe =
    /\(cost=[\d.]+\.\.[\d.]+ rows=\d+ width=\d+\)|\(actual time=[\d.]+\.\.[\d.]+ rows=\d+ loops=\d+\)/g;
  let m;
  while ((m = costRe.exec(text)) !== null) {
    addCostBlockRanges(ranges, m.index, m[0], rowSeverity);
  }

  // 3. Disk sort: "Sort Method: ... Disk: NkB"
  const diskRe = /Disk:\s*(\d+)kB/g;
  while ((m = diskRe.exec(text)) !== null) {
    const kb = parseInt(m[1], 10);
    const sev: Severity = kb >= 102400 ? "critical" : "warn";
    ranges.push({ start: m.index, end: m.index + m[0].length, type: "cost", severity: sev });
  }

  // 4. Temp I/O: "temp read=N" or "temp written=N"
  const tempRe = /temp (?:read|written)=(\d+)/g;
  while ((m = tempRe.exec(text)) !== null) {
    const val = parseInt(m[1], 10);
    if (val > 0) {
      ranges.push({ start: m.index, end: m.index + m[0].length, type: "cost", severity: "warn" });
    }
  }

  // 5. Rows Removed by Filter / Index Recheck — number after the condition label
  if (
    condLabel === "Rows Removed by Filter:" ||
    condLabel === "Rows Removed by Index Recheck:"
  ) {
    const numRe = /^\s*(\d+)/;
    const nm = numRe.exec(text);
    if (nm) {
      const val = parseInt(nm[1], 10);
      let sev: Severity | undefined;
      if (val >= 100000) sev = "critical";
      else if (val >= 10000) sev = "warn";
      if (sev) {
        const numStart = nm.index + nm[0].indexOf(nm[1]);
        ranges.push({
          start: numStart,
          end: numStart + nm[1].length,
          type: "cost",
          severity: sev,
        });
      }
    }
  }

  // Sort ranges by start position, then dedupe overlaps (keep first)
  ranges.sort((a, b) => a.start - b.start);
  const deduped: Range[] = [];
  for (const r of ranges) {
    if (deduped.length > 0 && r.start < deduped[deduped.length - 1].end) continue;
    deduped.push(r);
  }

  // Emit tokens, filling gaps with plain
  let pos = 0;
  for (const r of deduped) {
    if (r.start > pos) {
      tokens.push({ type: "plain", text: text.slice(pos, r.start) });
    }
    const tok: Token = { type: r.type, text: text.slice(r.start, r.end) };
    if (r.severity) tok.severity = r.severity;
    tokens.push(tok);
    pos = r.end;
  }
  if (pos < text.length) {
    tokens.push({ type: "plain", text: text.slice(pos) });
  }
}

export function tokenizePlan(text: string): Token[] {
  const lines = text.split("\n");
  const tokens: Token[] = [];

  for (let li = 0; li < lines.length; li++) {
    if (li > 0) tokens.push({ type: "plain", text: "\n" });
    const line = lines[li];
    let pos = 0;

    // Consume leading whitespace
    while (pos < line.length && (line[pos] === " " || line[pos] === "\t")) {
      pos++;
    }
    if (pos > 0) {
      tokens.push({ type: "plain", text: line.slice(0, pos) });
    }

    const rest = line.slice(pos);

    // Check for arrow ->
    if (rest.startsWith("->")) {
      tokens.push({ type: "arrow", text: "->" });
      pos += 2;
      // Space after arrow
      let spaceEnd = pos;
      while (
        spaceEnd < line.length &&
        (line[spaceEnd] === " " || line[spaceEnd] === "\t")
      ) {
        spaceEnd++;
      }
      if (spaceEnd > pos) {
        tokens.push({ type: "plain", text: line.slice(pos, spaceEnd) });
        pos = spaceEnd;
      }
    }

    // Try to match node type
    const afterArrow = line.slice(pos);
    let nodeMatched = false;
    for (const node of SORTED_NODES) {
      if (afterArrow.startsWith(node)) {
        // Ensure word boundary after node name
        const nextCh = afterArrow[node.length];
        if (
          nextCh === undefined ||
          nextCh === " " ||
          nextCh === "\t" ||
          nextCh === "("
        ) {
          tokens.push({ type: "node", text: node });
          pos += node.length;
          nodeMatched = true;
          break;
        }
      }
    }

    // Try to match condition label
    let matchedCondLabel: string | undefined;
    if (!nodeMatched) {
      const trimmedRest = line.slice(pos);
      for (const cond of SORTED_CONDITIONS) {
        if (trimmedRest.startsWith(cond)) {
          tokens.push({ type: "condition", text: cond });
          pos += cond.length;
          matchedCondLabel = cond;
          break;
        }
      }
    }

    // Process remainder of line with problem analysis
    const remaining = line.slice(pos);
    if (remaining) {
      tokenizeRemainingWithAnalysis(remaining, tokens, matchedCondLabel);
    }
  }

  return tokens;
}
