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
    if (!nodeMatched) {
      const trimmedRest = line.slice(pos);
      for (const cond of SORTED_CONDITIONS) {
        if (trimmedRest.startsWith(cond)) {
          tokens.push({ type: "condition", text: cond });
          pos += cond.length;
          break;
        }
      }
    }

    // Process remainder of line
    let remaining = line.slice(pos);
    if (remaining) {
      // Find cost blocks (cost=... rows=... width=...)
      const costRe =
        /\(cost=[\d.]+\.\.[\d.]+ rows=\d+ width=\d+\)|\(actual time=[\d.]+\.\.[\d.]+ rows=\d+ loops=\d+\)/g;
      let lastEnd = 0;
      let match;
      const parts: Token[] = [];

      while ((match = costRe.exec(remaining)) !== null) {
        if (match.index > lastEnd) {
          parts.push({ type: "plain", text: remaining.slice(lastEnd, match.index) });
        }
        parts.push({ type: "cost", text: match[0] });
        lastEnd = match.index + match[0].length;
      }
      if (lastEnd < remaining.length) {
        parts.push({ type: "plain", text: remaining.slice(lastEnd) });
      }
      tokens.push(...parts);
    }
  }

  return tokens;
}
