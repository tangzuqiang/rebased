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

type AnnOp = Op & { baseIndex: number };

function annotate(ops: Op[]): AnnOp[] {
  let baseIndex = 0;
  return ops.map((op) => {
    if (op.type === "ins") return { ...op, baseIndex };
    const annotated = { ...op, baseIndex };
    baseIndex += 1;
    return annotated;
  });
}

function inRegion(op: AnnOp, prev: number, next: number) {
  if (op.type === "ins") return op.baseIndex > prev && op.baseIndex <= next;
  return op.baseIndex > prev && op.baseIndex < next;
}

function sideLines(ops: AnnOp[]) {
  const lines: string[] = [];
  for (const op of ops) {
    if (op.type === "ins" || op.type === "eq") lines.push(op.b ?? op.a ?? "");
  }
  return lines;
}

function baseLines(ops: AnnOp[]) {
  const lines: string[] = [];
  for (const op of ops) {
    if (op.type === "eq" || op.type === "del") lines.push(op.a ?? "");
  }
  return lines;
}

function sameLines(a: string[], b: string[], ignoreWhitespace: boolean) {
  if (a.length !== b.length) return false;
  return a.every((line, index) => normalize(line, ignoreWhitespace) === normalize(b[index] ?? "", ignoreWhitespace));
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
  const left = annotate(lcsOps(base, ours, ignoreWhitespace));
  const right = annotate(lcsOps(base, theirs, ignoreWhitespace));
  const rows: Diff3Row[] = [];
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
    forceConflict = false,
  ) => {
    const autoConflict =
      !forceConflict &&
      oursKind !== "same" &&
      oursKind !== "empty" &&
      theirsKind !== "same" &&
      theirsKind !== "empty" &&
      normalize(oursLine, ignoreWhitespace) !== normalize(theirsLine, ignoreWhitespace);
    rows.push({
      ours: oursLine,
      base: baseLine,
      theirs: theirsLine,
      oursKind: autoConflict ? "conflict" : oursKind,
      baseKind: autoConflict ? "conflict" : baseKind,
      theirsKind: autoConflict ? "conflict" : theirsKind,
      forceConflict: forceConflict || autoConflict,
      oursBlame: takeBlame(options.blame?.ours, oursIndex, oursKind),
      baseBlame: takeBlame(options.blame?.base, baseIndex, baseKind),
      theirsBlame: takeBlame(options.blame?.theirs, theirsIndex, theirsKind),
    });
  };

  const zip = (opsL: AnnOp[], opsR: AnnOp[]) => {
    let li = 0;
    let ri = 0;
    while (li < opsL.length || ri < opsR.length) {
      const l = opsL[li];
      const r = opsR[ri];
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
  };

  const emitStable = (l: AnnOp, r: AnnOp) => {
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
  };

  const emitAgreed = (ops: AnnOp[]) => {
    for (const op of ops) {
      if (op.type === "del") {
        baseIndex.value += 1;
        continue;
      }
      const text = op.b ?? op.a ?? "";
      pushRow(text, op.type === "eq" ? (op.a ?? "") : "", text, "same", op.type === "eq" ? "same" : "empty", "same");
    }
  };

  const emitConflict = (leftOps: AnnOp[], rightOps: AnnOp[]) => {
    const oursLines = sideLines(leftOps);
    const theirsLines = sideLines(rightOps);
    const ancestor = baseLines(leftOps);
    const count = Math.max(oursLines.length, theirsLines.length, ancestor.length);
    for (let index = 0; index < count; index += 1) {
      const hasOurs = index < oursLines.length;
      const hasTheirs = index < theirsLines.length;
      const hasBase = index < ancestor.length;
      pushRow(
        hasOurs ? oursLines[index] : "",
        hasBase ? ancestor[index] : "",
        hasTheirs ? theirsLines[index] : "",
        hasOurs ? "conflict" : "empty",
        hasBase ? "same" : "empty",
        hasTheirs ? "conflict" : "empty",
        true,
      );
    }
  };

  const emitRegion = (leftOps: AnnOp[], rightOps: AnnOp[]) => {
    const leftChanged = leftOps.some((op) => op.type !== "eq");
    const rightChanged = rightOps.some((op) => op.type !== "eq");
    if (!leftChanged && !rightChanged) return;
    if (leftChanged && rightChanged) {
      const oursResult = sideLines(leftOps);
      const theirsResult = sideLines(rightOps);
      if (sameLines(oursResult, theirsResult, ignoreWhitespace)) emitAgreed(leftOps);
      else emitConflict(leftOps, rightOps);
      return;
    }
    zip(leftOps, rightOps);
  };

  const leftEq = new Set(left.filter((op) => op.type === "eq").map((op) => op.baseIndex));
  const rightEq = new Set(right.filter((op) => op.type === "eq").map((op) => op.baseIndex));
  const sync: number[] = [];
  for (let index = 0; index < base.length; index += 1) {
    if (leftEq.has(index) && rightEq.has(index)) sync.push(index);
  }
  sync.push(base.length);

  let prev = -1;
  let li = 0;
  let ri = 0;
  for (const next of sync) {
    const leftRegion: AnnOp[] = [];
    while (li < left.length && inRegion(left[li], prev, next)) {
      leftRegion.push(left[li]);
      li += 1;
    }
    const rightRegion: AnnOp[] = [];
    while (ri < right.length && inRegion(right[ri], prev, next)) {
      rightRegion.push(right[ri]);
      ri += 1;
    }
    emitRegion(leftRegion, rightRegion);
    if (next < base.length) {
      const l = left[li];
      const r = right[ri];
      if (l?.type === "eq" && l.baseIndex === next && r?.type === "eq" && r.baseIndex === next) {
        emitStable(l, r);
        li += 1;
        ri += 1;
      }
    }
    prev = next;
  }
  if (li < left.length || ri < right.length) zip(left.slice(li), right.slice(ri));
  return rows;
}
