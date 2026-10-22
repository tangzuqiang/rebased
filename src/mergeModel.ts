export interface LineRange {
  start: number;
  end: number;
}

export type BlockKind = "conflict" | "mod" | "add" | "del";
export type SideName = "ours" | "theirs";
export type SideState = "none" | "pending" | "applied" | "ignored";

export interface MergeBlock {
  index: number;
  base: LineRange;
  ours: LineRange;
  theirs: LineRange;
  oursChanged: boolean;
  theirsChanged: boolean;
  conflict: boolean;
}

export interface CharRange {
  from: number;
  to: number;
}

interface Hunk {
  baseStart: number;
  baseEnd: number;
  sideStart: number;
  sideEnd: number;
  side: 0 | 1;
}

export function splitLines(text: string) {
  return text.replace(/\r\n?/g, "\n").split("\n");
}

function normalize(line: string, ignoreWhitespace: boolean) {
  return ignoreWhitespace ? line.replace(/\s+/g, "") : line;
}

function toIds(lists: string[][], key: (item: string) => string) {
  const ids = new Map<string, number>();
  return lists.map((list) =>
    list.map((item) => {
      const k = key(item);
      let id = ids.get(k);
      if (id === undefined) {
        id = ids.size;
        ids.set(k, id);
      }
      return id;
    }),
  );
}

export function lcsPairs(a: number[], b: number[]): Array<[number, number]> {
  const pairs: Array<[number, number]> = [];
  const size = a.length + b.length + 4;
  const vf = new Int32Array(size * 2 + 4);
  const vb = new Int32Array(size * 2 + 4);

  function middleSnake(a0: number, a1: number, b0: number, b1: number): [number, number, number, number] {
    const n = a1 - a0;
    const m = b1 - b0;
    const delta = n - m;
    const odd = (delta & 1) !== 0;
    const maxD = Math.ceil((n + m) / 2);
    const off = maxD + 1;
    vf[off + 1] = 0;
    vb[off + 1] = 0;
    for (let d = 0; d <= maxD; d += 1) {
      for (let k = -d; k <= d; k += 2) {
        let x = k === -d || (k !== d && vf[off + k - 1] < vf[off + k + 1]) ? vf[off + k + 1] : vf[off + k - 1] + 1;
        let y = x - k;
        const sx = x;
        const sy = y;
        while (x < n && y < m && a[a0 + x] === b[b0 + y]) {
          x += 1;
          y += 1;
        }
        vf[off + k] = x;
        if (odd && k >= delta - (d - 1) && k <= delta + (d - 1) && x + vb[off + delta - k] >= n) {
          return [a0 + sx, b0 + sy, a0 + x, b0 + y];
        }
      }
      for (let k = -d; k <= d; k += 2) {
        let x = k === -d || (k !== d && vb[off + k - 1] < vb[off + k + 1]) ? vb[off + k + 1] : vb[off + k - 1] + 1;
        let y = x - k;
        const sx = x;
        const sy = y;
        while (x < n && y < m && a[a1 - 1 - x] === b[b1 - 1 - y]) {
          x += 1;
          y += 1;
        }
        vb[off + k] = x;
        if (!odd && delta - k >= -d && delta - k <= d && x + vf[off + delta - k] >= n) {
          return [a1 - x, b1 - y, a1 - sx, b1 - sy];
        }
      }
    }
    return [a0, b0, a0, b0];
  }

  function walk(a0: number, a1: number, b0: number, b1: number) {
    while (a0 < a1 && b0 < b1 && a[a0] === b[b0]) {
      pairs.push([a0, b0]);
      a0 += 1;
      b0 += 1;
    }
    let ea = a1;
    let eb = b1;
    while (ea > a0 && eb > b0 && a[ea - 1] === b[eb - 1]) {
      ea -= 1;
      eb -= 1;
    }
    if (a0 < ea && b0 < eb) {
      const [x, y, u, v] = middleSnake(a0, ea, b0, eb);
      const progress = !(x === a0 && y === b0 && u === ea && v === eb) && !(u === a0 && v === b0);
      if (progress) {
        walk(a0, x, b0, y);
        for (let i = 0; i < u - x; i += 1) pairs.push([x + i, y + i]);
        walk(u, ea, v, eb);
      }
    }
    for (let i = 0; i < a1 - ea; i += 1) pairs.push([ea + i, eb + i]);
  }

  walk(0, a.length, 0, b.length);
  return pairs;
}

function hunksOf(pairs: Array<[number, number]>, baseLen: number, sideLen: number, side: 0 | 1) {
  const hunks: Hunk[] = [];
  let pa = -1;
  let pb = -1;
  for (const [i, j] of [...pairs, [baseLen, sideLen] as [number, number]]) {
    if (i > pa + 1 || j > pb + 1) {
      hunks.push({ baseStart: pa + 1, baseEnd: i, sideStart: pb + 1, sideEnd: j, side });
    }
    pa = i;
    pb = j;
  }
  return hunks;
}

interface Group {
  base: LineRange;
  sides: [LineRange, LineRange];
  changed: [boolean, boolean];
}

function mapThrough(hunks: Hunk[], pos: number) {
  let result = pos;
  for (const hunk of hunks) {
    if (hunk.baseEnd > pos) break;
    result = pos - hunk.baseEnd + hunk.sideEnd;
  }
  return result;
}

function groupHunks(left: Hunk[], right: Hunk[]): Group[] {
  const all = [...left, ...right].sort((x, y) => x.baseStart - y.baseStart || x.baseEnd - y.baseEnd);
  const groups: Group[] = [];
  let index = 0;
  while (index < all.length) {
    const members = [all[index]];
    let end = all[index].baseEnd;
    index += 1;
    while (index < all.length) {
      const next = all[index];
      const touching = next.baseStart < end || (next.baseStart === end && members.some((h) => h.side !== next.side));
      if (!touching) break;
      members.push(next);
      end = Math.max(end, next.baseEnd);
      index += 1;
    }
    const start = Math.min(...members.map((h) => h.baseStart));
    const sides = [0, 1].map((side) => {
      const own = members.filter((h) => h.side === side);
      const list = side === 0 ? left : right;
      if (!own.length) return { start: mapThrough(list, start), end: mapThrough(list, end) };
      const first = own[0];
      const last = own[own.length - 1];
      return {
        start: first.sideStart - (first.baseStart - start),
        end: last.sideEnd + (end - last.baseEnd),
      };
    }) as [LineRange, LineRange];
    groups.push({
      base: { start, end },
      sides,
      changed: [members.some((h) => h.side === 0), members.some((h) => h.side === 1)],
    });
  }
  return groups;
}

function sameSlice(a: number[], ra: LineRange, b: number[], rb: LineRange) {
  if (ra.end - ra.start !== rb.end - rb.start) return false;
  for (let i = 0; i < ra.end - ra.start; i += 1) if (a[ra.start + i] !== b[rb.start + i]) return false;
  return true;
}

export function buildBlocks(ours: string[], base: string[], theirs: string[], ignoreWhitespace: boolean): MergeBlock[] {
  const [o, b, t] = toIds([ours, base, theirs], (line) => normalize(line, ignoreWhitespace));
  const left = hunksOf(lcsPairs(b, o), b.length, o.length, 0);
  const right = hunksOf(lcsPairs(b, t), b.length, t.length, 1);
  return groupHunks(left, right).map((group, index) => {
    const both = group.changed[0] && group.changed[1];
    return {
      index,
      base: group.base,
      ours: group.sides[0],
      theirs: group.sides[1],
      oursChanged: group.changed[0],
      theirsChanged: group.changed[1],
      conflict: both && !sameSlice(o, group.sides[0], t, group.sides[1]),
    };
  });
}

export function sideKind(block: MergeBlock, side: SideName): BlockKind {
  if (block.conflict) return "conflict";
  const range = side === "ours" ? block.ours : block.theirs;
  if (block.base.start === block.base.end) return "add";
  if (range.start === range.end) return "del";
  return "mod";
}

export function resultKind(block: MergeBlock): BlockKind {
  return sideKind(block, block.oursChanged ? "ours" : "theirs");
}

function tokenize(text: string) {
  return text.match(/\n|[^\S\n]+|[\p{L}\p{N}_$]+|[^\s\p{L}\p{N}_$]/gu) ?? [];
}

const WORD_LIMIT = 20_000;

export function changedChars(side: string, other: string, ignoreWhitespace: boolean): CharRange[] {
  if (side.length > WORD_LIMIT || other.length > WORD_LIMIT || !side || !other) return [];
  const a = tokenize(side);
  const b = tokenize(other);
  const [ia, ib] = toIds([a, b], (token) => (ignoreWhitespace && !token.trim() ? " " : token));
  const matched = new Set(lcsPairs(ia, ib).map(([i]) => i));
  const ranges: CharRange[] = [];
  let pos = 0;
  a.forEach((token, index) => {
    const from = pos;
    pos += token.length;
    if (matched.has(index) || token === "\n" || (ignoreWhitespace && !token.trim())) return;
    const last = ranges[ranges.length - 1];
    if (last && last.to === from) last.to = pos;
    else ranges.push({ from, to: pos });
  });
  return ranges;
}

export function resolveSimple(ours: string, base: string, theirs: string): string | null {
  if (ours.length > WORD_LIMIT || base.length > WORD_LIMIT || theirs.length > WORD_LIMIT) return null;
  const o = tokenize(ours);
  const b = tokenize(base);
  const t = tokenize(theirs);
  const [io, ib, it] = toIds([o, b, t], (token) => token);
  const left = hunksOf(lcsPairs(ib, io), b.length, o.length, 0);
  const right = hunksOf(lcsPairs(ib, it), b.length, t.length, 1);
  const out: string[] = [];
  let pos = 0;
  for (const group of groupHunks(left, right)) {
    out.push(...b.slice(pos, group.base.start));
    if (group.changed[0] && group.changed[1] && !sameSlice(io, group.sides[0], it, group.sides[1])) return null;
    const [from, range] = group.changed[0] ? [o, group.sides[0]] : [t, group.sides[1]];
    out.push(...from.slice(range.start, range.end));
    pos = group.base.end;
  }
  out.push(...b.slice(pos));
  return out.join("");
}

export interface Anchor {
  a: number;
  b: number;
}

export function anchorsFor(pairs: Array<[LineRange, LineRange]>, endA: number, endB: number): Anchor[] {
  const anchors: Anchor[] = [{ a: 0, b: 0 }];
  for (const [ra, rb] of pairs) {
    anchors.push({ a: ra.start, b: rb.start }, { a: ra.end, b: rb.end });
  }
  anchors.push({ a: endA, b: endB });
  return anchors;
}

export function mapLine(anchors: Anchor[], line: number, from: "a" | "b") {
  const to = from === "a" ? "b" : "a";
  for (let i = 1; i < anchors.length; i += 1) {
    const prev = anchors[i - 1];
    const next = anchors[i];
    if (line > next[from] && i < anchors.length - 1) continue;
    const span = next[from] - prev[from];
    if (span <= 0) return next[to];
    const ratio = Math.min(1, Math.max(0, (line - prev[from]) / span));
    return prev[to] + ratio * (next[to] - prev[to]);
  }
  return line;
}
