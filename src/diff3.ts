import type { Diff3Row, DiffKind } from "./types";

type Op = { type: "eq" | "del" | "ins"; a?: string; b?: string };

function lcsOps(a: string[], b: string[]): Op[] {
  const n = a.length;
  const m = b.length;
  const dp: number[][] = Array.from({ length: n + 1 }, () => Array(m + 1).fill(0));
  for (let i = n - 1; i >= 0; i -= 1) {
    for (let j = m - 1; j >= 0; j -= 1) {
      dp[i][j] = a[i] === b[j] ? dp[i + 1][j + 1] + 1 : Math.max(dp[i + 1][j], dp[i][j + 1]);
    }
  }
  const ops: Op[] = [];
  let i = 0;
  let j = 0;
  while (i < n && j < m) {
    if (a[i] === b[j]) {
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

export function alignDiff3(oursText: string, baseText: string, theirsText: string): Diff3Row[] {
  const ours = splitLines(oursText);
  const base = splitLines(baseText);
  const theirs = splitLines(theirsText);
  const left = lcsOps(base, ours);
  const right = lcsOps(base, theirs);
  const rows: Diff3Row[] = [];
  let li = 0;
  let ri = 0;

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
      oursLine !== theirsLine;
    rows.push({
      ours: oursLine,
      base: baseLine,
      theirs: theirsLine,
      oursKind: conflict ? "conflict" : oursKind,
      baseKind: conflict ? "conflict" : baseKind,
      theirsKind: conflict ? "conflict" : theirsKind,
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
      const changedOurs = l.a !== l.b;
      const changedTheirs = r.a !== r.b;
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
