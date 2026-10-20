import type { DiffLine } from "./types";

export interface Segment {
  text: string;
  changed: boolean;
}

export interface SideLine {
  no: string;
  text: string;
  parts?: Segment[];
}

export type DiffSegment =
  | { type: "same"; lines: Array<{ leftNo: string; rightNo: string; text: string }> }
  | { type: "change"; block: number; left: SideLine[]; right: SideLine[] }
  | { type: "gap"; text: string };

export type BlockKind = "add" | "del" | "mod";

export type ViewRow =
  | { type: "line"; no: string; text: string; parts?: Segment[]; kind: "ctx" | BlockKind; block: number }
  | { type: "fold"; key: number; count: number }
  | { type: "gap"; text: string };

/** 差异块在两侧视图中的位置，均为视图行号的半开区间 [start, end)。 */
export interface BlockBox {
  block: number;
  kind: BlockKind;
  l0: number;
  l1: number;
  r0: number;
  r1: number;
}

const FOLD_CONTEXT = 3;
const MAX_WORD_TOKENS = 400;

function tokenize(text: string) {
  return text.match(/[\p{L}\p{N}_]+|\s+|[^\p{L}\p{N}_\s]/gu) ?? [];
}

/** 前后缀对齐后，中间不同的部分标记为改动；超长行不做词级比较。 */
export function wordSegments(oldText: string, newText: string): [Segment[], Segment[]] {
  const a = tokenize(oldText);
  const b = tokenize(newText);
  if (a.length > MAX_WORD_TOKENS || b.length > MAX_WORD_TOKENS) {
    return [[{ text: oldText, changed: true }], [{ text: newText, changed: true }]];
  }
  let prefix = 0;
  while (prefix < a.length && prefix < b.length && a[prefix] === b[prefix]) prefix += 1;
  let suffix = 0;
  while (
    suffix < a.length - prefix &&
    suffix < b.length - prefix &&
    a[a.length - 1 - suffix] === b[b.length - 1 - suffix]
  ) {
    suffix += 1;
  }
  const build = (tokens: string[]) => {
    const parts: Segment[] = [
      { text: tokens.slice(0, prefix).join(""), changed: false },
      { text: tokens.slice(prefix, tokens.length - suffix).join(""), changed: true },
      { text: tokens.slice(tokens.length - suffix).join(""), changed: false },
    ];
    return parts.filter((part) => part.text.length);
  };
  return [build(a), build(b)];
}

/** 把补丁方向反过来：增删互换、行号互换，并保持每个改动段内"先删后增"的顺序。 */
export function reverseLines(lines: DiffLine[]): DiffLine[] {
  const result: DiffLine[] = [];
  let dels: DiffLine[] = [];
  let adds: DiffLine[] = [];
  const flush = () => {
    result.push(...dels, ...adds);
    dels = [];
    adds = [];
  };
  for (const line of lines) {
    if (line.kind === "add") {
      dels.push({ ...line, kind: "del", oldNo: line.newNo, newNo: "" });
    } else if (line.kind === "del") {
      adds.push({ ...line, kind: "add", oldNo: "", newNo: line.oldNo });
    } else {
      flush();
      result.push({ ...line, oldNo: line.newNo, newNo: line.oldNo });
    }
  }
  flush();
  return result;
}

/** 把统一格式补丁切成"相同段 / 差异块 / 截断提示"，左侧为 old，右侧为 new。 */
export function buildSegments(lines: DiffLine[], words: boolean) {
  const segments: DiffSegment[] = [];
  let block = -1;
  let dels: DiffLine[] = [];
  let adds: DiffLine[] = [];
  let seenHunk = false;

  const flush = () => {
    if (!dels.length && !adds.length) return;
    block += 1;
    const left: SideLine[] = dels.map((line) => ({ no: line.oldNo, text: line.text }));
    const right: SideLine[] = adds.map((line) => ({ no: line.newNo, text: line.text }));
    if (words) {
      for (let index = 0; index < Math.min(left.length, right.length); index += 1) {
        const [oldParts, newParts] = wordSegments(left[index].text, right[index].text);
        left[index].parts = oldParts;
        right[index].parts = newParts;
      }
    }
    segments.push({ type: "change", block, left, right });
    dels = [];
    adds = [];
  };

  for (const line of lines) {
    if (line.kind === "del") {
      if (adds.length) flush();
      dels.push(line);
      continue;
    }
    if (line.kind === "add") {
      adds.push(line);
      continue;
    }
    flush();
    if (line.kind === "hunk") {
      if (seenHunk || /差异过大/.test(line.text)) segments.push({ type: "gap", text: line.text });
      seenHunk = true;
      continue;
    }
    const last = segments[segments.length - 1];
    const entry = { leftNo: line.oldNo, rightNo: line.newNo, text: line.text };
    if (last?.type === "same") last.lines.push(entry);
    else segments.push({ type: "same", lines: [entry] });
  }
  flush();
  return { segments, blockCount: block + 1 };
}

/**
 * 生成左右两侧各自的视图行（不补空行），并记录每个差异块在两侧的位置。
 * 相同段两侧行数相等，折叠时两侧同步折叠。
 */
export function layoutSegments(segments: DiffSegment[], collapse: boolean, expanded: Set<number>) {
  const left: ViewRow[] = [];
  const right: ViewRow[] = [];
  const boxes: BlockBox[] = [];
  segments.forEach((segment, key) => {
    if (segment.type === "gap") {
      left.push({ type: "gap", text: segment.text });
      right.push({ type: "gap", text: segment.text });
      return;
    }
    if (segment.type === "change") {
      const kind: BlockKind = !segment.left.length ? "add" : !segment.right.length ? "del" : "mod";
      const box: BlockBox = { block: segment.block, kind, l0: left.length, l1: 0, r0: right.length, r1: 0 };
      for (const line of segment.left) {
        left.push({ type: "line", ...line, kind: kind === "mod" ? "mod" : "del", block: segment.block });
      }
      for (const line of segment.right) {
        right.push({ type: "line", ...line, kind: kind === "mod" ? "mod" : "add", block: segment.block });
      }
      box.l1 = left.length;
      box.r1 = right.length;
      boxes.push(box);
      return;
    }
    const push = (from: number, to: number) => {
      for (const line of segment.lines.slice(from, to)) {
        left.push({ type: "line", no: line.leftNo, text: line.text, kind: "ctx", block: -1 });
        right.push({ type: "line", no: line.rightNo, text: line.text, kind: "ctx", block: -1 });
      }
    };
    const count = segment.lines.length;
    const head = key === 0 ? 0 : FOLD_CONTEXT;
    const tail = key === segments.length - 1 ? 0 : FOLD_CONTEXT;
    const hidden = count - head - tail;
    if (collapse && hidden > 1 && !expanded.has(key)) {
      push(0, head);
      left.push({ type: "fold", key, count: hidden });
      right.push({ type: "fold", key, count: hidden });
      push(count - tail, count);
    } else {
      push(0, count);
    }
  });
  return { left, right, boxes };
}

/** 两侧视图行之间的对应点，用于按差异块同步滚动。 */
export function scrollAnchors(boxes: BlockBox[], leftCount: number, rightCount: number) {
  const anchors: Array<[number, number]> = [[0, 0]];
  for (const box of boxes) {
    anchors.push([box.l0, box.r0], [box.l1, box.r1]);
  }
  anchors.push([leftCount, rightCount]);
  return anchors;
}

/** 按对应点做分段线性映射，side 为 0 表示从左映射到右。 */
export function mapRow(anchors: Array<[number, number]>, row: number, side: 0 | 1) {
  const other = side === 0 ? 1 : 0;
  for (let index = 1; index < anchors.length; index += 1) {
    const prev = anchors[index - 1];
    const next = anchors[index];
    if (row <= next[side]) {
      const span = next[side] - prev[side];
      const ratio = span > 0 ? (row - prev[side]) / span : 1;
      return prev[other] + ratio * (next[other] - prev[other]);
    }
  }
  const last = anchors[anchors.length - 1];
  return last[other] + (row - last[side]);
}

/** 以左侧全文为底，把指定差异块（null 表示全部）替换成右侧内容，返回新的左侧全文。 */
export function applyToLeft(segments: DiffSegment[], blocks: Set<number> | null): string {
  const out: string[] = [];
  for (const segment of segments) {
    if (segment.type === "same") {
      for (const line of segment.lines) out.push(line.text);
    } else if (segment.type === "change") {
      const take = blocks === null || blocks.has(segment.block) ? segment.right : segment.left;
      for (const line of take) out.push(line.text);
    }
  }
  return out.join("\n");
}
