<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from "vue";
import { getConflictFile } from "../api";
import { alignDiff3 } from "../diff3";
import { commentPrefix, findInTextarea, handleIdeEditorKey, writeTextarea } from "../editorKeys";
import { highlightCode } from "../highlight";
import { joinLines, remapLines } from "../mergeLines";
import "highlight.js/styles/atom-one-dark.css";
import { notify } from "../notify";
import type { BlameLine, Diff3Row, DiffKind, MergeFileContent, MergeState } from "../types";
import Icons from "./Icons.vue";

const props = defineProps<{
  state: MergeState;
  repoPath: string;
  path: string;
  busy: boolean;
}>();

const emit = defineEmits<{
  back: [];
  save: [payload: { path: string; content: string }];
  abort: [];
}>();

interface Hunk {
  start: number;
  end: number;
  ours: boolean;
  theirs: boolean;
  conflict: boolean;
}

interface ViewerRow {
  isFold: false;
  oursHtml: string;
  theirsHtml: string;
  oursNo: number | "";
  theirsNo: number | "";
  oursKind: string;
  theirsKind: string;
  oursBlame?: BlameLine;
  theirsBlame?: BlameLine;
  rowIndex: number;
  hunkStart: boolean;
  hunk?: Hunk;
  conflictBorder: boolean;
  blameOursAnno: string;
  blameTheirsAnno: string;
}

interface FoldRow {
  isFold: true;
  count: number;
  start: number;
}

type VRow = ViewerRow | FoldRow;
type Side = "ours" | "theirs";
type Choice = Side | "base" | "both";

const ROW_HEIGHT = 18;

const content = ref<MergeFileContent | null>(null);
const result = ref("");
const seededText = ref("");
const resultLines = ref<Array<string | null>>([]);
const choice = ref<Record<string, Choice>>({});
const settled = ref<Record<string, { ours: boolean; theirs: boolean }>>({});
const loading = ref(false);
const error = ref("");
const ignoreWhitespace = ref(false);
const showBlame = ref(true);
const overwrite = ref<"ours" | "theirs" | "">("");
const currentHunk = ref(0);
const mergeReady = ref(false);
const completeOpen = ref(false);
const completeShown = ref(false);
const oursPane = ref<HTMLElement | null>(null);
const resultPane = ref<HTMLElement | null>(null);
const theirsPane = ref<HTMLElement | null>(null);
const leftGutter = ref<HTMLElement | null>(null);
const rightGutter = ref<HTMLElement | null>(null);
const resultEditor = ref<HTMLTextAreaElement | null>(null);
const findOpen = ref(false);
const findQuery = ref("");
const findHint = ref("");
const expandedFolds = ref<Set<number>>(new Set());

const FOLD_THRESHOLD = 8;

const editable = computed(() => Boolean(content.value && !content.value.binary && !content.value.tooLarge));
const dirty = computed(() => result.value !== seededText.value || Object.keys(choice.value).length > 0);

const rows = computed<Diff3Row[]>(() => {
  if (!content.value || !editable.value) return [];
  return alignDiff3(content.value.ours, content.value.base, content.value.theirs, {
    ignoreWhitespace: ignoreWhitespace.value,
    blame: {
      ours: content.value.oursBlame,
      base: content.value.baseBlame,
      theirs: content.value.theirsBlame,
    },
  });
});

const hunks = computed(() => {
  const list: Hunk[] = [];
  const all = rows.value;
  let index = 0;
  while (index < all.length) {
    const ours = isSideChange(all[index].oursKind);
    const theirs = isSideChange(all[index].theirsKind);
    if (!ours && !theirs) {
      index += 1;
      continue;
    }
    const start = index;
    const kind = hunkKind(all[index]);
    index += 1;
    while (index < all.length && hunkKind(all[index]) === kind) {
      index += 1;
    }
    list.push({
      start,
      end: index,
      ours: kind === "ours" || kind === "conflict",
      theirs: kind === "theirs" || kind === "conflict",
      conflict: kind === "conflict",
    });
  }
  return list;
});

const unresolvedConflicts = computed(() => hunks.value.filter((h) => h.conflict && !hunkDone(h)));
const conflictCount = computed(() => unresolvedConflicts.value.length);
const totalConflicts = computed(() => hunks.value.filter((h) => h.conflict).length);
const resolvedCount = computed(() => totalConflicts.value - conflictCount.value);
const currentStart = computed(() => hunks.value[currentHunk.value]?.start ?? -1);
const canGoUp = computed(() => unresolvedConflicts.value.some((h) => h.start < currentStart.value));
const canGoDown = computed(() => unresolvedConflicts.value.some((h) => h.start > currentStart.value));
const nonConflictCount = computed(() => hunks.value.filter((h) => !h.conflict && !hunkDone(h)).length);

const highlighted = computed(() => {
  const all = rows.value;
  if (!all.length || !content.value || !editable.value)
    return { ours: [] as string[], theirs: [] as string[] };
  const oursLines = all.map((r) => r.ours);
  const theirsLines = all.map((r) => r.theirs);
  return {
    ours: splitHighlighted(oursLines, content.value!.path),
    theirs: splitHighlighted(theirsLines, content.value!.path),
  };
});

const wordDiffMap = computed(() => {
  const cache = new Map<string, WordSeg[]>();
  const result = new Map<number, WordSeg[]>();
  for (const hunk of hunks.value) {
    if (!hunk.conflict) continue;
    for (let i = hunk.start; i < hunk.end; i++) {
      const row = rows.value[i];
      if (!row || row.oursKind !== "conflict") continue;
      const key = `${row.ours}\x00${row.theirs}`;
      let segs = cache.get(key);
      if (!segs) {
        segs = computeWordDiff(row.ours, row.theirs);
        cache.set(key, segs);
      }
      result.set(i, segs);
    }
  }
  return result;
});

const oursNos = computed(() => numberSide(rows.value, (r) => r.oursKind));
const theirsNos = computed(() => numberSide(rows.value, (r) => r.theirsKind));

const viewerRows = computed<VRow[]>(() => {
  const all = rows.value;
  const hl = highlighted.value;
  const result: VRow[] = [];
  let i = 0;

  while (i < all.length) {
    const row = all[i];
    const oursChanged = isSideChange(row.oursKind);
    const theirsChanged = isSideChange(row.theirsKind);

    if (oursChanged || theirsChanged) {
      const hunk = hunkOf(i);
      result.push(makeViewerRow(i, hl, hunk));
      i++;
    } else {
      let start = i;
      while (i < all.length && !isSideChange(all[i].oursKind) && !isSideChange(all[i].theirsKind)) {
        i++;
      }
      const count = i - start;

      if (count > FOLD_THRESHOLD) {
        result.push({ isFold: true, count, start });
        if (expandedFolds.value.has(start)) {
          for (let j = start; j < i; j++) result.push(makeViewerRow(j, hl, undefined));
        }
      } else {
        for (let j = start; j < i; j++) result.push(makeViewerRow(j, hl, undefined));
      }
    }
  }
  return result;
});

const rowToVisual = computed(() => {
  const map: number[] = new Array(rows.value.length).fill(0);
  viewerRows.value.forEach((vr, visual) => {
    if (!vr.isFold) {
      map[vr.rowIndex] = visual;
    } else if (!expandedFolds.value.has(vr.start)) {
      for (let r = vr.start; r < vr.start + vr.count; r++) map[r] = visual;
    }
  });
  return map;
});

const resultLayout = computed(() => {
  const starts: number[] = [];
  const owners: number[] = [];
  resultLines.value.forEach((line, row) => {
    starts.push(owners.length);
    if (line === null) return;
    const count = line.split("\n").length;
    for (let k = 0; k < count; k++) owners.push(row);
  });
  starts.push(owners.length);
  return { starts, owners };
});

const resultLineCount = computed(() => Math.max(1, result.value.split("\n").length));

watch(
  () => props.path,
  (path) => {
    void loadFile(path);
  },
  { immediate: true },
);

watch([() => content.value?.path, ignoreWhitespace], () => {
  if (!content.value?.path) return;
  seedResult();
  currentHunk.value = 0;
  mergeReady.value = false;
  completeOpen.value = false;
  completeShown.value = false;
  overwrite.value = "";
  expandedFolds.value = new Set();
});

watch(resultEditor, (el) => {
  if (el) writeTextarea(el, result.value);
});

async function loadFile(path: string) {
  loading.value = true;
  error.value = "";
  try {
    content.value = await getConflictFile(props.repoPath, path);
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err);
    content.value = null;
  } finally {
    loading.value = false;
  }
}

function isSideChange(kind: DiffKind) {
  return kind === "changed" || kind === "added" || kind === "deleted" || kind === "conflict";
}

function hunkKind(row: Diff3Row): "ours" | "theirs" | "conflict" | "" {
  if (row.forceConflict) return "conflict";
  const ours = isSideChange(row.oursKind);
  const theirs = isSideChange(row.theirsKind);
  if (ours && theirs) return "conflict";
  if (ours) return "ours";
  if (theirs) return "theirs";
  return "";
}

function numberSide(list: Diff3Row[], kindOf: (row: Diff3Row) => DiffKind) {
  const numbers: Array<number | ""> = [];
  let line = 0;
  for (const row of list) {
    const kind = kindOf(row);
    if (kind === "empty" || kind === "deleted") numbers.push("");
    else {
      line += 1;
      numbers.push(line);
    }
  }
  return numbers;
}

function lineFrom(row: Diff3Row, side: "ours" | "theirs" | "base") {
  const kind = side === "ours" ? row.oursKind : side === "theirs" ? row.theirsKind : row.baseKind;
  const text = side === "ours" ? row.ours : side === "theirs" ? row.theirs : row.base;
  if (kind === "empty" || kind === "deleted") return null;
  return text;
}

function defaultResultLine(row: Diff3Row) {
  if (row.forceConflict) return lineFrom(row, "ours");
  const oursChange = isSideChange(row.oursKind);
  const theirsChange = isSideChange(row.theirsKind);
  if (theirsChange && !oursChange) return lineFrom(row, "theirs");
  return lineFrom(row, "ours");
}

function seedResult() {
  resultLines.value = rows.value.map(defaultResultLine);
  choice.value = {};
  settled.value = {};
  seededText.value = joinLines(resultLines.value);
  setResultText(seededText.value);
}

function markComplete() {
  mergeReady.value = true;
  if (completeShown.value) return;
  completeShown.value = true;
  completeOpen.value = true;
}

function checkConflictsResolved() {
  const conflicts = hunks.value.filter((h) => h.conflict);
  if (!conflicts.length) return;
  if (conflicts.every((h) => hunkDone(h))) markComplete();
}

function sideSettled(hunk: Hunk, side: "ours" | "theirs") {
  return Boolean(settled.value[hunkKey(hunk)]?.[side]);
}

function hunkDone(hunk: Hunk) {
  const key = hunkKey(hunk);
  const state = settled.value[key];
  if (!state) return false;
  if (hunk.conflict) {
    const picked = choice.value[key];
    return picked === "ours" || picked === "theirs" || picked === "both" || (state.ours && state.theirs);
  }
  return (hunk.ours ? state.ours : true) && (hunk.theirs ? state.theirs : true);
}

function markSettled(hunk: Hunk, patch: { ours?: boolean; theirs?: boolean }) {
  const key = hunkKey(hunk);
  const prev = settled.value[key] ?? { ours: false, theirs: false };
  settled.value = {
    ...settled.value,
    [key]: { ours: patch.ours ?? prev.ours, theirs: patch.theirs ?? prev.theirs },
  };
}

function hunkKey(hunk: Hunk) {
  return `${hunk.start}:${hunk.end}`;
}

function hunkOf(index: number) {
  return hunks.value.find((h) => index >= h.start && index < h.end);
}

function fillHunk(hunk: Hunk, lines: Array<string | null>, picked: Choice) {
  const next = [...resultLines.value];
  for (let i = hunk.start; i < hunk.end; i += 1) {
    next[i] = lines[i - hunk.start] ?? null;
  }
  resultLines.value = next;
  choice.value = { ...choice.value, [hunkKey(hunk)]: picked };
  setResultText(joinLines(next));
  currentHunk.value = Math.max(0, hunks.value.findIndex((item) => item.start === hunk.start));
}

function applyHunk(hunk: Hunk, side: Side | "base") {
  const source = rows.value;
  const lines: Array<string | null> = [];
  for (let i = hunk.start; i < hunk.end; i += 1) lines.push(lineFrom(source[i], side));
  fillHunk(hunk, lines, side);
  checkConflictsResolved();
}

function applyBoth(hunk: Hunk, first: Side, second: Side) {
  const source = rows.value;
  const merged: string[] = [];
  for (const side of [first, second]) {
    for (let i = hunk.start; i < hunk.end; i += 1) {
      const line = lineFrom(source[i], side);
      if (line !== null) merged.push(line);
    }
  }
  const lines: Array<string | null> = new Array(hunk.end - hunk.start).fill(null);
  if (merged.length) lines[0] = merged.join("\n");
  fillHunk(hunk, lines, "both");
}

function setResultText(text: string) {
  result.value = text;
  writeTextarea(resultEditor.value, text);
}

function onResultInput(event: Event) {
  const target = event.target;
  if (!(target instanceof HTMLTextAreaElement)) return;
  result.value = target.value;
  resultLines.value = remapLines(resultLines.value, result.value);
}

function acceptHunk(hunk: Hunk, side: Side) {
  const other: Side = side === "ours" ? "theirs" : "ours";
  if (hunk.conflict && choice.value[hunkKey(hunk)] === other && sideSettled(hunk, other)) {
    applyBoth(hunk, other, side);
  } else {
    applyHunk(hunk, side);
  }
  markSettled(hunk, { [side]: true });
  checkConflictsResolved();
}

function rejectHunk(hunk: Hunk, side: Side) {
  const current = choice.value[hunkKey(hunk)];
  const other: Side = side === "ours" ? "theirs" : "ours";
  applyHunk(hunk, current === other || current === "both" ? other : "base");
  markSettled(hunk, { [side]: true });
  checkConflictsResolved();
}

function applyNonConflicts() {
  let applied = 0;
  for (const hunk of hunks.value) {
    if (hunk.conflict || hunkDone(hunk)) continue;
    if (hunk.theirs && !hunk.ours) {
      applyHunk(hunk, "theirs");
      markSettled(hunk, { theirs: true, ours: true });
      applied += 1;
    } else if (hunk.ours && !hunk.theirs) {
      applyHunk(hunk, "ours");
      markSettled(hunk, { ours: true, theirs: true });
      applied += 1;
    }
  }
  const left = unresolvedConflicts.value.length;
  if (!applied) {
    notify(left ? "没有可自动应用的非冲突更改，请手动处理冲突" : "没有可应用的非冲突更改", "error");
    return;
  }
  notify(left ? `已应用 ${applied} 处非冲突更改，仍有 ${left} 处冲突待处理` : `已应用 ${applied} 处非冲突更改`);
  if (!left) markComplete();
}

function soften(kind: DiffKind): DiffKind {
  return kind === "conflict" ? "changed" : kind;
}

function sideKind(index: number, side: "ours" | "theirs"): string {
  const row = rows.value[index];
  const raw = side === "ours" ? row.oursKind : row.theirsKind;
  const hunk = hunkOf(index);
  if (!hunk) return raw;
  if (sideSettled(hunk, side)) {
    const picked = choice.value[hunkKey(hunk)];
    if (picked !== side && picked !== "both") return raw === "empty" ? "empty" : "discarded";
    return soften(raw);
  }
  return raw;
}

function formatBlame(meta?: BlameLine) {
  if (!meta?.author) return "";
  const time = meta.timestamp
    ? new Date(meta.timestamp * 1000).toLocaleString("zh-CN", { hour12: false })
    : "";
  return time ? `${meta.author} · ${time}` : meta.author;
}

function blameDateAuthor(meta?: BlameLine) {
  if (!meta?.author) return "";
  const date = meta.timestamp
    ? new Date(meta.timestamp * 1000).toLocaleDateString("zh-CN")
    : "";
  return date ? `${date} ${meta.author}` : meta.author;
}

function blameAnno(index: number, side: "ours" | "theirs") {
  if (!showBlame.value) return "";
  const row = rows.value[index];
  const meta = side === "ours" ? row.oursBlame : row.theirsBlame;
  const label = blameDateAuthor(meta);
  if (!label) return "";
  if (index > 0) {
    const prevRow = rows.value[index - 1];
    const prevMeta = side === "ours" ? prevRow.oursBlame : prevRow.theirsBlame;
    if (blameDateAuthor(prevMeta) === label) return "";
  }
  return label;
}

function makeViewerRow(index: number, hl: { ours: string[]; theirs: string[] }, hunk: Hunk | undefined): ViewerRow {
  const row = rows.value[index];
  const sk_ours = sideKind(index, "ours");
  const sk_theirs = sideKind(index, "theirs");
  let oursHtml = hl.ours[index] ?? escapeHtml(row.ours);
  let theirsHtml = hl.theirs[index] ?? escapeHtml(row.theirs);

  if (hunk?.conflict && sk_ours === "conflict") {
    const segs = wordDiffMap.value.get(index);
    if (segs) oursHtml = renderWordHtml(segs, "ours");
  }
  if (hunk?.conflict && sk_theirs === "conflict") {
    const segs = wordDiffMap.value.get(index);
    if (segs) theirsHtml = renderWordHtml(segs, "theirs");
  }

  return {
    isFold: false,
    oursHtml,
    theirsHtml,
    oursNo: oursNos.value[index],
    theirsNo: theirsNos.value[index],
    oursKind: sk_ours,
    theirsKind: sk_theirs,
    oursBlame: row.oursBlame,
    theirsBlame: row.theirsBlame,
    rowIndex: index,
    hunkStart: hunk !== undefined && hunk.start === index,
    hunk,
    conflictBorder: hunk !== undefined && hunk.conflict && !hunkDone(hunk),
    blameOursAnno: blameAnno(index, "ours"),
    blameTheirsAnno: blameAnno(index, "theirs"),
  };
}

function sidePanes() {
  return [oursPane.value, theirsPane.value, leftGutter.value, rightGutter.value];
}

function codePanes() {
  return [oursPane.value, resultPane.value, theirsPane.value];
}

let scrollDriver: HTMLElement | null = null;
let scrollDriverTimer = 0;

function holdScrollDriver(el: HTMLElement) {
  scrollDriver = el;
  window.clearTimeout(scrollDriverTimer);
  scrollDriverTimer = window.setTimeout(() => {
    scrollDriver = null;
  }, 80);
}

function sideTopToResult(top: number) {
  const visual = Math.floor(top / ROW_HEIGHT);
  const vr = viewerRows.value[visual];
  if (!vr) return resultLayout.value.owners.length * ROW_HEIGHT;
  const row = vr.isFold ? vr.start : vr.rowIndex;
  const frac = vr.isFold || resultLines.value[row] === null ? 0 : top - visual * ROW_HEIGHT;
  return resultLayout.value.starts[row] * ROW_HEIGHT + frac;
}

function resultTopToSide(top: number) {
  const line = Math.floor(top / ROW_HEIGHT);
  const { owners, starts } = resultLayout.value;
  const row = owners[line];
  if (row === undefined) return viewerRows.value.length * ROW_HEIGHT;
  const frac = line === starts[row] ? top - line * ROW_HEIGHT : 0;
  return rowToVisual.value[row] * ROW_HEIGHT + frac;
}

function syncScroll(event: Event) {
  const source = event.target;
  if (!(source instanceof HTMLElement)) return;
  if (scrollDriver && scrollDriver !== source) return;
  holdScrollDriver(source);
  if (source === resultPane.value) {
    const top = resultTopToSide(source.scrollTop);
    for (const pane of sidePanes()) if (pane) pane.scrollTop = top;
  } else {
    for (const pane of sidePanes()) if (pane && pane !== source) pane.scrollTop = source.scrollTop;
    if (resultPane.value) resultPane.value.scrollTop = sideTopToResult(source.scrollTop);
  }
  if (codePanes().includes(source)) {
    for (const pane of codePanes()) if (pane && pane !== source) pane.scrollLeft = source.scrollLeft;
  }
}

async function goHunk(delta: number) {
  const list = unresolvedConflicts.value;
  if (!list.length) return;
  const start = currentStart.value;
  const target =
    delta < 0
      ? [...list].reverse().find((h) => h.start < start)
      : list.find((h) => h.start > start);
  if (!target) return;
  currentHunk.value = Math.max(0, hunks.value.findIndex((h) => h.start === target.start));
  await nextTick();
  if (resultPane.value) holdScrollDriver(resultPane.value);
  const sideTop = Math.max(0, rowToVisual.value[target.start] * ROW_HEIGHT - 80);
  for (const pane of sidePanes()) if (pane) pane.scrollTop = sideTop;
  if (resultPane.value) {
    resultPane.value.scrollTop = Math.max(0, resultLayout.value.starts[target.start] * ROW_HEIGHT - 80);
  }
}

function coverAll(side: "ours" | "theirs") {
  overwrite.value = side;
  for (const hunk of hunks.value) {
    applyHunk(hunk, side);
    markSettled(hunk, { ours: true, theirs: true });
  }
  notify(side === "ours" ? "已用本地版本覆盖全部改动" : "已用传入版本覆盖全部改动");
  if (!unresolvedConflicts.value.length) mergeReady.value = true;
}

function applyCurrent() {
  if (props.busy || !editable.value) return;
  if (
    conflictCount.value > 0 &&
    !window.confirm(`还有 ${conflictCount.value} 处冲突未通过按钮处理，将按结果区当前内容保存，确定应用？`)
  ) {
    return;
  }
  emit("save", { path: props.path, content: result.value });
}

function goBack() {
  if (dirty.value && !window.confirm("当前文件的处理结果尚未应用，返回后将丢失，确定返回？")) return;
  emit("back");
}

function toggleIgnoreWhitespace(event: Event) {
  const target = event.target as HTMLInputElement;
  if (dirty.value && !window.confirm("切换「忽略空白」会重新生成结果，当前文件的处理进度将丢失，确定继续？")) {
    target.checked = ignoreWhitespace.value;
    return;
  }
  ignoreWhitespace.value = target.checked;
}

function onPageKey(event: KeyboardEvent) {
  if (event.key !== "Escape" || event.defaultPrevented) return;
  event.preventDefault();
  if (completeOpen.value) {
    completeOpen.value = false;
  } else if (findOpen.value) {
    findOpen.value = false;
  } else {
    goBack();
  }
}

onMounted(() => window.addEventListener("keydown", onPageKey));
onUnmounted(() => {
  window.removeEventListener("keydown", onPageKey);
  window.clearTimeout(scrollDriverTimer);
});

function runFind(reverse = false) {
  const el = resultEditor.value;
  if (!el || !findQuery.value) return;
  findHint.value = findInTextarea(el, findQuery.value, reverse) ? "" : "未找到";
}

function onResultKey(event: KeyboardEvent) {
  if (event.key === "Escape" && findOpen.value) {
    event.preventDefault();
    findOpen.value = false;
    return;
  }
  if ((event.ctrlKey || event.metaKey) && event.key === "F3") {
    event.preventDefault();
    runFind(event.shiftKey);
    return;
  }
  handleIdeEditorKey(event, {
    onSave: applyCurrent,
    onFind: () => {
      findOpen.value = true;
      findHint.value = "";
    },
    commentPrefix: commentPrefix(props.path),
  });
}

function cancelMerge() {
  if (props.busy) return;
  if (!window.confirm("确定取消本次合并？工作区将恢复到合并前的状态。")) return;
  emit("abort");
}

function toggleFold(start: number) {
  const next = new Set(expandedFolds.value);
  if (next.has(start)) next.delete(start);
  else next.add(start);
  expandedFolds.value = next;
}

function escapeHtml(text: string) {
  return text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

function splitHighlighted(lines: string[], filePath: string): string[] {
  if (!lines.length) return [];
  const full = highlightCode(lines.join("\n"), filePath);
  return splitHtmlLines(full);
}

function splitHtmlLines(html: string): string[] {
  const result: string[] = [];
  let current = "";
  const openStack: string[] = [];
  const tagRegex = /(<\/?[^>]+>)/g;
  const parts = html.split(tagRegex).filter(Boolean);

  for (const part of parts) {
    if (part.startsWith("<")) {
      if (part.startsWith("</")) {
        openStack.pop();
      } else if (!part.endsWith("/>")) {
        openStack.push(part);
      }
      current += part;
    } else {
      const segments = part.split("\n");
      for (let i = 0; i < segments.length; i++) {
        if (i > 0) {
          result.push(current);
          current = openStack.join("");
        }
        current += segments[i];
      }
    }
  }
  result.push(current);
  return result;
}

interface WordSeg {
  text: string;
  type: "same" | "oursOnly" | "theirsOnly";
}

function tokenize(text: string): string[] {
  return text.match(/\s+|\w+|[^\s\w]/g) ?? [];
}

function computeWordDiff(ours: string, theirs: string): WordSeg[] {
  const a = tokenize(ours);
  const b = tokenize(theirs);
  const n = a.length;
  const m = b.length;

  if (n === 0 && m === 0) return [];
  if (n === 0) return b.map((text) => ({ text, type: "theirsOnly" as const }));
  if (m === 0) return a.map((text) => ({ text, type: "oursOnly" as const }));

  const maxLen = Math.max(...a.map((t) => t.length), ...b.map((t) => t.length), 1);
  if (n * m > 500000 || maxLen > 200) {
    return a.map((text) => ({ text, type: "same" as const }));
  }

  const dp: number[][] = Array.from({ length: n + 1 }, () => Array(m + 1).fill(0));
  for (let i = n - 1; i >= 0; i--) {
    for (let j = m - 1; j >= 0; j--) {
      dp[i][j] = a[i] === b[j] ? dp[i + 1][j + 1] + 1 : Math.max(dp[i + 1][j], dp[i][j + 1]);
    }
  }

  const segs: WordSeg[] = [];
  let i = 0;
  let j = 0;
  while (i < n && j < m) {
    if (a[i] === b[j]) {
      segs.push({ text: a[i], type: "same" });
      i++;
      j++;
    } else if (dp[i + 1][j] >= dp[i][j + 1]) {
      segs.push({ text: a[i], type: "oursOnly" });
      i++;
    } else {
      segs.push({ text: b[j], type: "theirsOnly" });
      j++;
    }
  }
  while (i < n) {
    segs.push({ text: a[i], type: "oursOnly" });
    i++;
  }
  while (j < m) {
    segs.push({ text: b[j], type: "theirsOnly" });
    j++;
  }
  return segs;
}

function renderWordHtml(segs: WordSeg[], side: "ours" | "theirs"): string {
  const includeType = side === "ours" ? "oursOnly" : "theirsOnly";
  return segs
    .filter((s) => s.type === "same" || s.type === includeType)
    .map((s) => {
      const text = escapeHtml(s.text);
      if (s.type === includeType && s.text.trim()) {
        return `<span class="wd">${text}</span>`;
      }
      return text;
    })
    .join("");
}
</script>

<template>
  <Teleport to="body">
    <div class="page">
      <header class="toolbar">
        <button type="button" class="tb-btn back-btn" title="返回冲突列表 (Esc)" @click="goBack">
          <Icons name="arrow-left" :size="14" />
          <span>返回</span>
        </button>
        <div class="tb-sep" />
        <button type="button" class="tb-btn" title="上一个冲突" :disabled="!canGoUp" @click="goHunk(-1)">
          <Icons name="arrow-up" :size="14" />
        </button>
        <button type="button" class="tb-btn" title="下一个冲突" :disabled="!canGoDown" @click="goHunk(1)">
          <Icons name="arrow-down" :size="14" />
        </button>
        <div class="tb-sep" />
        <div class="conflict-badge" :class="{ resolved: conflictCount === 0 }">
          <span v-if="conflictCount > 0" class="badge-icon">&#x26A0;</span>
          <span v-else class="badge-icon">&#x2713;</span>
          <span v-if="conflictCount > 0">{{ conflictCount }} 处冲突待处理</span>
          <span v-else>所有冲突已处理</span>
          <span v-if="totalConflicts > 0" class="badge-progress">{{ resolvedCount }}/{{ totalConflicts }}</span>
        </div>
        <button
          type="button"
          class="tb-btn apply-btn"
          :disabled="busy || !nonConflictCount"
          title="自动应用所有非冲突更改"
          @click="applyNonConflicts"
        >
          <Icons name="apply-both" :size="14" />
          <span>应用非冲突</span>
        </button>
        <div class="tb-sep" />
        <span class="tb-label">覆盖全部:</span>
        <button
          type="button"
          class="tb-btn cover-btn"
          :class="{ on: overwrite === 'ours' }"
          :disabled="busy"
          title="用本地版本覆盖全部"
          @click="coverAll('ours')"
        >
          本地
        </button>
        <button
          type="button"
          class="tb-btn cover-btn"
          :class="{ on: overwrite === 'theirs' }"
          :disabled="busy"
          title="用传入版本覆盖全部"
          @click="coverAll('theirs')"
        >
          传入
        </button>
        <span class="file-path" :title="path">{{ path }}</span>
        <label class="tb-check">
          <input v-model="showBlame" type="checkbox" />
          <span>Blame</span>
        </label>
        <label class="tb-check">
          <input :checked="ignoreWhitespace" type="checkbox" @change="toggleIgnoreWhitespace" />
          <span>忽略空白</span>
        </label>
        <span class="tb-count">{{ hunks.length }} 个更改</span>
      </header>

      <p v-if="error" class="err">{{ error }}</p>
      <p v-else-if="loading" class="empty-hint">正在加载三端内容和行历史…</p>
      <div v-else-if="content?.binary" class="empty-hint">二进制文件请返回列表后使用「使用您的」或「使用他们的」</div>
      <div v-else-if="content?.tooLarge" class="empty-hint">
        文件超过 512KB，无法在此合并，请返回列表后使用「使用您的」或「使用他们的」
      </div>
      <div v-else class="workspace">
        <section class="pane-col ours-col">
          <div class="pane-head ours-head">
            <span class="head-badge ours-badge">本地</span>
            <span class="head-branch">{{ state.ours }}</span>
          </div>
          <div ref="oursPane" class="pane-body" @scroll="syncScroll">
            <template v-for="(vr, vi) in viewerRows" :key="`o-${vi}`">
              <div v-if="vr.isFold" class="fold-bar" @click="toggleFold(vr.start)">
                <span class="fold-icon">&#x22EE;</span>
                <span class="fold-text">
                  {{ expandedFolds.has(vr.start) ? `收起 ${vr.count} 行未更改` : `${vr.count} 行未更改，点击展开` }}
                </span>
              </div>
              <div
                v-else
                class="line"
                :class="[vr.oursKind, { current: vr.hunk?.start === hunks[currentHunk]?.start, 'conflict-border': vr.conflictBorder }]"
                :title="formatBlame(vr.oursBlame)"
              >
                <span class="no">{{ vr.oursNo }}</span>
                <span v-if="showBlame" class="blame" :title="formatBlame(vr.oursBlame)">{{ vr.blameOursAnno }}</span>
                <span class="code" v-html="vr.oursHtml" />
              </div>
            </template>
          </div>
        </section>

        <section class="gutter-col">
          <div class="gutter-head" />
          <div ref="leftGutter" class="gutter-body" @scroll="syncScroll">
            <template v-for="(vr, vi) in viewerRows" :key="`lg-${vi}`">
              <div v-if="vr.isFold" class="fold-gutter" />
              <div v-else class="gutter-row" :class="{ 'hunk-start': vr.hunkStart }">
                <template v-if="vr.hunkStart && vr.hunk?.ours && !sideSettled(vr.hunk, 'ours')">
                  <button type="button" class="accept-btn" title="接受本地此段 (»)" @click.stop="acceptHunk(vr.hunk!, 'ours')">
                    <svg viewBox="0 0 12 12" width="12" height="12"><path d="M3 1 L8 6 L3 11" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>
                  </button>
                  <button type="button" class="reject-btn" title="舍弃本地此段" @click.stop="rejectHunk(vr.hunk!, 'ours')">
                    <svg viewBox="0 0 12 12" width="10" height="10"><path d="M2 2 L10 10 M10 2 L2 10" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/></svg>
                  </button>
                </template>
              </div>
            </template>
          </div>
        </section>

        <section class="pane-col result-col">
          <div class="pane-head result-head">
            <span class="head-badge result-badge">结果</span>
            <span v-if="findOpen" class="findbar">
              <input
                v-model="findQuery"
                placeholder="查找…"
                @keydown.enter.prevent="runFind($event.shiftKey)"
                @keydown.escape.prevent="findOpen = false"
              />
              <button type="button" title="上一个 (Shift+F3)" @click="runFind(true)">
                <Icons name="arrow-up" :size="12" />
              </button>
              <button type="button" title="下一个 (F3)" @click="runFind(false)">
                <Icons name="arrow-down" :size="12" />
              </button>
              <em v-if="findHint">{{ findHint }}</em>
            </span>
          </div>
          <div ref="resultPane" class="pane-body result-pane-body" @scroll="syncScroll">
            <div class="result-wrap">
              <div class="result-lines-col">
                <div v-for="n in resultLineCount" :key="n" class="result-no">{{ n }}</div>
              </div>
              <textarea
                ref="resultEditor"
                class="result-editor"
                spellcheck="false"
                :style="{ minHeight: `${resultLineCount * 18}px` }"
                @input="onResultInput"
                @keydown="onResultKey"
              />
            </div>
          </div>
        </section>

        <section class="gutter-col">
          <div class="gutter-head" />
          <div ref="rightGutter" class="gutter-body" @scroll="syncScroll">
            <template v-for="(vr, vi) in viewerRows" :key="`rg-${vi}`">
              <div v-if="vr.isFold" class="fold-gutter" />
              <div v-else class="gutter-row" :class="{ 'hunk-start': vr.hunkStart }">
                <template v-if="vr.hunkStart && vr.hunk?.theirs && !sideSettled(vr.hunk, 'theirs')">
                  <button type="button" class="accept-btn" title="接受传入此段 («)" @click.stop="acceptHunk(vr.hunk!, 'theirs')">
                    <svg viewBox="0 0 12 12" width="12" height="12"><path d="M9 1 L4 6 L9 11" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>
                  </button>
                  <button type="button" class="reject-btn" title="舍弃传入此段" @click.stop="rejectHunk(vr.hunk!, 'theirs')">
                    <svg viewBox="0 0 12 12" width="10" height="10"><path d="M2 2 L10 10 M10 2 L2 10" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/></svg>
                  </button>
                </template>
              </div>
            </template>
          </div>
        </section>

        <section class="pane-col theirs-col">
          <div class="pane-head theirs-head">
            <span class="head-badge theirs-badge">传入</span>
            <span class="head-branch">{{ state.theirs }}</span>
          </div>
          <div ref="theirsPane" class="pane-body" @scroll="syncScroll">
            <template v-for="(vr, vi) in viewerRows" :key="`t-${vi}`">
              <div v-if="vr.isFold" class="fold-bar" @click="toggleFold(vr.start)">
                <span class="fold-icon">&#x22EE;</span>
                <span class="fold-text">
                  {{ expandedFolds.has(vr.start) ? `收起 ${vr.count} 行未更改` : `${vr.count} 行未更改，点击展开` }}
                </span>
              </div>
              <div
                v-else
                class="line"
                :class="[vr.theirsKind, { current: vr.hunk?.start === hunks[currentHunk]?.start, 'conflict-border': vr.conflictBorder }]"
                :title="formatBlame(vr.theirsBlame)"
              >
                <span class="no">{{ vr.theirsNo }}</span>
                <span v-if="showBlame" class="blame" :title="formatBlame(vr.theirsBlame)">{{ vr.blameTheirsAnno }}</span>
                <span class="code" v-html="vr.theirsHtml" />
              </div>
            </template>
          </div>
        </section>
      </div>

      <footer class="foot">
        <button type="button" class="secondary-btn" @click="goBack">返回列表</button>
        <span class="spacer" />
        <button
          type="button"
          class="primary-btn apply-current"
          :disabled="busy || !editable"
          :title="conflictCount > 0 ? `还有 ${conflictCount} 处冲突未通过按钮处理` : ''"
          @click="applyCurrent"
        >
          <Icons name="check" :size="14" />
          应用
        </button>
        <button type="button" class="secondary-btn" :disabled="busy" @click="cancelMerge">取消合并</button>
      </footer>
    </div>

    <div v-if="completeOpen" class="complete-mask" @mousedown.self="completeOpen = false">
      <div class="complete-box">
        <div class="complete-icon">&#x2713;</div>
        <h3>冲突已全部处理</h3>
        <p>当前文件的冲突已处理完，可以点击「应用」保存结果。</p>
        <button type="button" class="primary-btn" @click="completeOpen = false">确定</button>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.page {
  position: fixed;
  inset: 0;
  z-index: 1250;
  display: flex;
  flex-direction: column;
  background: var(--bg);
}

/* ── Toolbar ────────────────────────────────────────── */
.toolbar,
.foot {
  flex: none;
  display: flex;
  align-items: center;
  gap: 6px;
  min-height: 38px;
  padding: 0 10px;
  border-bottom: 1px solid var(--border);
  background: var(--bg-hover);
}

.foot {
  border-bottom: 0;
  border-top: 1px solid var(--border);
}

.tb-btn {
  height: 26px;
  padding: 0 8px;
  border-radius: 4px;
  display: inline-flex;
  align-items: center;
  gap: 4px;
  color: var(--text-muted);
  font-size: 12px;
  white-space: nowrap;
}

.tb-btn:hover:not(:disabled) {
  background: var(--bg-selected);
  color: var(--text);
}

.back-btn {
  font-weight: 500;
}

.tb-sep {
  width: 1px;
  height: 18px;
  background: var(--border-soft);
  margin: 0 2px;
}

.conflict-badge {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 26px;
  padding: 0 10px;
  border-radius: 4px;
  background: rgba(224, 108, 117, 0.15);
  color: var(--danger);
  font-weight: 600;
  font-size: 12px;
  white-space: nowrap;
}

.conflict-badge.resolved {
  background: rgba(152, 195, 121, 0.15);
  color: var(--ok);
}

.badge-icon {
  font-size: 14px;
}

.badge-progress {
  color: var(--text-muted);
  font-weight: 400;
  margin-left: 2px;
}

.apply-btn {
  color: var(--accent);
}

.apply-btn:hover:not(:disabled) {
  background: rgba(53, 116, 240, 0.12);
  color: var(--accent);
}

.tb-label {
  color: var(--text-muted);
  font-size: 11px;
  margin-left: 2px;
}

.cover-btn {
  border: 1px solid var(--border-soft);
  background: var(--bg-input);
}

.cover-btn.on,
.cover-btn:hover:not(:disabled) {
  background: var(--accent);
  border-color: var(--accent);
  color: #fff;
}

.file-path {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--text-muted);
  text-align: center;
  font-size: 12px;
}

.tb-check {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  color: var(--text-muted);
  font-size: 11px;
  white-space: nowrap;
  cursor: pointer;
}

.tb-count {
  color: var(--text-dim);
  font-size: 11px;
  white-space: nowrap;
}

/* ── Workspace Grid ─────────────────────────────────── */
.workspace {
  flex: 1;
  min-height: 0;
  display: grid;
  grid-template-columns: minmax(0, 1fr) 36px minmax(0, 1fr) 36px minmax(0, 1fr);
}

.pane-col,
.gutter-col {
  min-width: 0;
  min-height: 0;
  display: flex;
  flex-direction: column;
}

.pane-head {
  flex: none;
  height: 30px;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 0 10px;
  font-size: 12px;
}

.ours-head {
  background: var(--diff-ours-head);
}

.result-head {
  background: var(--diff-base-head);
  justify-content: space-between;
}

.theirs-head {
  background: var(--diff-theirs-head);
}

.head-badge {
  display: inline-flex;
  align-items: center;
  height: 20px;
  padding: 0 8px;
  border-radius: 3px;
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.02em;
}

.ours-badge {
  background: rgba(53, 116, 240, 0.2);
  color: #6da3f7;
}

.result-badge {
  background: rgba(134, 138, 145, 0.2);
  color: var(--text-muted);
}

.theirs-badge {
  background: rgba(73, 156, 84, 0.2);
  color: #6aab73;
}

.head-branch {
  color: var(--text-muted);
  font-size: 11px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.findbar {
  display: inline-flex;
  align-items: center;
  gap: 4px;
}

.findbar input {
  width: 140px;
  height: 22px;
  padding: 0 6px;
  border: 1px solid var(--border-soft);
  border-radius: 4px;
  background: var(--bg-input);
  color: var(--text);
  font-size: 12px;
}

.findbar button {
  width: 22px;
  height: 22px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  color: var(--text-muted);
  border-radius: 3px;
}

.findbar button:hover {
  background: var(--bg-hover);
  color: var(--text);
}

.findbar em {
  color: var(--danger);
  font-style: normal;
  font-size: 11px;
}

/* ── Pane Body ──────────────────────────────────────── */
.pane-body {
  flex: 1;
  min-height: 0;
  overflow: scroll;
  scrollbar-gutter: stable both-edges;
  background: var(--bg-deep);
  font-family: Consolas, "Courier New", monospace;
  font-size: 12px;
  line-height: 18px;
}

.pane-body::-webkit-scrollbar {
  width: 10px;
  height: 10px;
}

.pane-body::-webkit-scrollbar-thumb {
  background: var(--scrollbar);
  border-radius: 8px;
}

.pane-body::-webkit-scrollbar-track {
  background: transparent;
}

.result-pane-body {
  padding: 0;
}

/* ── Code Lines ─────────────────────────────────────── */
.line {
  display: flex;
  min-height: 18px;
  height: 18px;
}

.line.current {
  box-shadow: inset 3px 0 0 var(--accent);
}

.line.conflict-border {
  box-shadow: inset 3px 0 0 #c754c7, inset -3px 0 0 #c754c7;
  background: var(--diff-conflict-bg) !important;
}

.no {
  flex: none;
  width: 40px;
  padding: 0 6px 0 4px;
  color: var(--text-dim);
  text-align: right;
  user-select: none;
}

.blame {
  flex: none;
  width: 120px;
  padding-right: 8px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--text-dim);
  font-size: 11px;
  user-select: none;
}

.code {
  flex: 1;
  min-width: 0;
  padding-right: 8px;
  white-space: pre;
  color: var(--text-editor);
}

/* ── Diff Backgrounds ───────────────────────────────── */
.line.changed { background: var(--diff-changed); }
.line.added { background: var(--diff-added-bg); }
.line.deleted { background: var(--diff-deleted-bg); }
.line.conflict { background: var(--diff-conflict-bg); }
.line.discarded { opacity: 0.35; background: transparent; }

/* ── Word Diff ──────────────────────────────────────── */
:deep(.wd) {
  background: rgba(255, 200, 50, 0.35);
  border-radius: 2px;
  padding: 0 1px;
}

/* ── Fold Bar ───────────────────────────────────────── */
.fold-bar {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  box-sizing: border-box;
  min-height: 18px;
  height: 18px;
  background: var(--bg-hover);
  border-top: 1px solid var(--border);
  border-bottom: 1px solid var(--border);
  color: var(--text-muted);
  font-size: 11px;
  cursor: pointer;
  user-select: none;
}

.fold-bar:hover {
  background: var(--bg-selected);
  color: var(--text);
}

.fold-icon {
  font-size: 14px;
  line-height: 1;
  color: var(--text-dim);
}

.fold-text {
  white-space: nowrap;
}

.fold-gutter {
  min-height: 18px;
  height: 18px;
}

/* ── Gutter ─────────────────────────────────────────── */
.gutter-head {
  flex: none;
  height: 30px;
  background: var(--bg-hover);
}

.gutter-body {
  flex: 1;
  min-height: 0;
  overflow: hidden auto;
  scrollbar-width: none;
  background: var(--bg-deep);
  border-left: 1px solid var(--border);
  border-right: 1px solid var(--border);
}

.gutter-body::-webkit-scrollbar {
  width: 0;
  height: 0;
}

.gutter-row {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 2px;
  min-height: 18px;
  height: 18px;
}

.gutter-row.hunk-start {
  position: relative;
}

.accept-btn,
.reject-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border-radius: 3px;
  cursor: pointer;
  transition: background 0.15s, color 0.15s;
}

.accept-btn {
  width: 18px;
  height: 18px;
  color: var(--accent);
}

.accept-btn:hover {
  background: var(--accent);
  color: #fff;
}

.reject-btn {
  width: 16px;
  height: 16px;
  color: var(--text-dim);
}

.reject-btn:hover {
  background: var(--danger);
  color: #fff;
}

/* ── Result Editor ──────────────────────────────────── */
.result-wrap {
  display: flex;
  min-height: 100%;
}

.result-lines-col {
  flex: none;
  width: 40px;
  padding: 0 4px 0 0;
  text-align: right;
  color: var(--text-dim);
  font-family: Consolas, "Courier New", monospace;
  font-size: 12px;
  line-height: 18px;
  user-select: none;
}

.result-no {
  height: 18px;
  min-height: 18px;
}

.result-editor {
  display: block;
  flex: 1;
  box-sizing: border-box;
  min-width: 0;
  min-height: 100%;
  margin: 0;
  padding: 0 8px 0 0;
  border: 0;
  outline: none;
  resize: none;
  overflow: hidden;
  color: var(--text-editor);
  background: transparent;
  font-family: Consolas, "Courier New", monospace;
  font-size: 12px;
  line-height: 18px;
  white-space: pre;
  tab-size: 4;
}

/* ── Footer ─────────────────────────────────────────── */
.spacer { flex: 1; }

.apply-current {
  gap: 4px;
}

/* ── Complete Dialog ────────────────────────────────── */
.complete-mask {
  position: fixed;
  inset: 0;
  z-index: 1400;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(0, 0, 0, 0.45);
}

.complete-box {
  width: min(420px, calc(100vw - 48px));
  padding: 24px 28px;
  border-radius: 8px;
  background: var(--bg-elevated);
  border: 1px solid var(--border-elevated);
  box-shadow: var(--shadow);
  text-align: center;
}

.complete-icon {
  font-size: 36px;
  color: var(--ok);
  margin-bottom: 8px;
}

.complete-box h3 {
  margin: 0 0 8px;
  font-size: 16px;
}

.complete-box p {
  margin: 0 0 20px;
  color: var(--text-muted);
}

/* ── Utility ────────────────────────────────────────── */
.err {
  padding: 16px;
  color: var(--danger);
}
</style>
