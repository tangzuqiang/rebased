import type { BlameLine, Diff3Row, DiffKind } from "./types";

type Op = { type: "eq" | "del" | "ins"; a?: string; b?: string };

export type AlignDiff3Options = {
  blame?: { ours?: BlameLine[]; base?: BlameLine[]; theirs?: BlameLine[] };
  ignoreWhitespace?: boolean;
};

function normalize(line: string, ignoreWhitespace: boolean) {
  return ignoreWhitespace ? line.replace(/\s+/g, " ").trim() : line;
}

function lcsOps(a: string[], b: string[], ignoreWhitespace = false): Op[] {
  const keyA = a.map((line) => normalize(line, ignoreWhitespace));
  const keyB = b.map((line) => normalize(line, ignoreWhitespace));
  const n = a.length;
  const m = b.length;
  const dp: number[][] = Array.from({ length: n + 1 }, () => Array(m + 1).fill(0));
  for (let i = n - 1; i >= 0; i -= 1) {
    for (let j = m - 1; j >= 0; j -= 1) {
      dp[i][j] = keyA[i] === keyB[j] ? dp[i + 1][j + 1] + 1 : Math.max(dp[i + 1][j], dp[i][j + 1]);
    }
  }
  const ops: Op[] = [];
  let i = 0;
  let j = 0;
  while (i < n && j < m) {
    if (keyA[i] === keyB[j]) {
      ops.push({ type: "eq", a: a[i], b: b[j] });
      i += 1;
      j += 1;
    } else if (dp[i + 1][j] >= dp[i][j + 1]) {
      ops.push({ type: "del", a: a[i] });
      i += 1;
    } else {
      ops.push({ type: "ins", b: b[j] });
      j += 1;
    }
  }
  while (i < n) {
    ops.push({ type: "del", a: a[i] });
    i += 1;
  }
  while (j < m) {
    ops.push({ type: "ins", b: b[j] });
    j += 1;
  }
  return ops;
}

function splitLines(text: string) {
  if (!text) return [] as string[];
  return text.replace(/\r\n/g, "\n").replace(/\r/g, "\n").split("\n");
}

function takeBlame(list: BlameLine[] | undefined, index: { value: number }, kind: DiffKind): BlameLine | undefined {
  if (kind === "empty" || kind === "deleted") return undefined;
  const item = list?.[index.value];
  index.value += 1;
  return item;
}

export function alignDiff3(
  oursText: string,
  baseText: string,
  theirsText: string,
  options: AlignDiff3Options = {},
): Diff3Row[] {
  const ignoreWhitespace = Boolean(options.ignoreWhitespace);
  const ours = splitLines(oursText);
  const base = splitLines(baseText);
  const theirs = splitLines(theirsText);
  const left = lcsOps(base, ours, ignoreWhitespace);
  const right = lcsOps(base, theirs, ignoreWhitespace);
  const rows: Diff3Row[] = [];
  let li = 0;
  let ri = 0;
  const oursIndex = { value: 0 };
  const baseIndex = { value: 0 };
  const theirsIndex = { value: 0 };

  const pushRow = (
    oursLine: string,
    baseLine: string,
    theirsLine: string,
    oursKind: DiffKind,
    baseKind: DiffKind,
    theirsKind: DiffKind,
  ) => {
    const conflict =
      oursKind !== "same" &&
      oursKind !== "empty" &&
      theirsKind !== "same" &&
      theirsKind !== "empty" &&
      normalize(oursLine, ignoreWhitespace) !== normalize(theirsLine, ignoreWhitespace);
    rows.push({
      ours: oursLine,
      base: baseLine,
      theirs: theirsLine,
      oursKind: conflict ? "conflict" : oursKind,
      baseKind: conflict ? "conflict" : baseKind,
      theirsKind: conflict ? "conflict" : theirsKind,
      oursBlame: takeBlame(options.blame?.ours, oursIndex, oursKind),
      baseBlame: takeBlame(options.blame?.base, baseIndex, baseKind),
      theirsBlame: takeBlame(options.blame?.theirs, theirsIndex, theirsKind),
    });
  };

  while (li < left.length || ri < right.length) {
    const l = left[li];
    const r = right[ri];
    if (l?.type === "ins" && r?.type === "ins") {
      pushRow(l.b ?? "", "", r.b ?? "", "added", "empty", "added");
      li += 1;
      ri += 1;
      continue;
    }
    if (l?.type === "ins") {
      pushRow(l.b ?? "", "", "", "added", "empty", "empty");
      li += 1;
      continue;
    }
    if (r?.type === "ins") {
      pushRow("", "", r.b ?? "", "empty", "empty", "added");
      ri += 1;
      continue;
    }
    if (l?.type === "del" && r?.type === "del") {
      pushRow("", l.a ?? "", "", "deleted", "same", "deleted");
      li += 1;
      ri += 1;
      continue;
    }
    if (l?.type === "del" && r?.type === "eq") {
      pushRow("", l.a ?? "", r.b ?? "", "deleted", "same", "same");
      li += 1;
      ri += 1;
      continue;
    }
    if (r?.type === "del" && l?.type === "eq") {
      pushRow(l.b ?? "", r.a ?? "", "", "same", "same", "deleted");
      li += 1;
      ri += 1;
      continue;
    }
    if (l?.type === "eq" && r?.type === "eq") {
      const changedOurs = !ignoreWhitespace && l.a !== l.b;
      const changedTheirs = !ignoreWhitespace && r.a !== r.b;
      pushRow(
        l.b ?? "",
        l.a ?? "",
        r.b ?? "",
        changedOurs ? "changed" : "same",
        "same",
        changedTheirs ? "changed" : "same",
      );
      li += 1;
      ri += 1;
      continue;
    }
    if (l) {
      pushRow(l.b ?? l.a ?? "", l.a ?? "", "", l.type === "del" ? "deleted" : "same", "same", "empty");
      li += 1;
      continue;
    }
    if (r) {
      pushRow("", r.a ?? "", r.b ?? r.a ?? "", "empty", "same", r.type === "del" ? "deleted" : "same");
      ri += 1;
    }
  }
  return rows;
}
