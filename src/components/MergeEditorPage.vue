<script setup lang="ts">
import { Compartment, type ChangeSpec, type Extension, type StateField } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import { isolateHistory } from "@codemirror/commands";
import { openSearchPanel } from "@codemirror/search";
import { computed, nextTick, onMounted, onUnmounted, ref, shallowRef, watch } from "vue";
import { getConflictFile } from "../api";
import {
  blameGutter,
  createMergeField,
  createPane,
  heightOfLine,
  lineTop,
  setMerge,
  topLine,
  updateMarks,
  type BlameLabel,
  type MergeSnapshot,
  type PaneMark,
  type SideStates,
} from "../mergeEditor";
import {
  anchorsFor,
  buildBlocks,
  changedChars,
  mapLine,
  resolveSimple,
  resultKind,
  sideKind,
  splitLines,
  type BlockKind,
  type CharRange,
  type LineRange,
  type MergeBlock,
  type SideName,
} from "../mergeModel";
import { notify } from "../notify";
import type { BlameLine, MergeFileContent, MergeState } from "../types";

const props = defineProps<{
  state: MergeState;
  repoPath: string;
  path: string;
  busy: boolean;
}>();

const emit = defineEmits<{
  back: [];
  save: [payload: { path: string; content: string }];
}>();

type Pane = "ours" | "result" | "theirs";

interface Band {
  index: number;
  kind: BlockKind;
  d: string;
  button: number;
}

interface Settings {
  ignoreWhitespace: boolean;
  highlight: "words" | "lines";
  syncScroll: boolean;
  blame: boolean;
}

const SETTINGS_KEY = "rebased.mergeViewer";
const DIVIDER = 30;

function loadSettings(): Settings {
  const fallback: Settings = { ignoreWhitespace: false, highlight: "words", syncScroll: true, blame: false };
  try {
    return { ...fallback, ...JSON.parse(localStorage.getItem(SETTINGS_KEY) ?? "{}") };
  } catch {
    return fallback;
  }
}

const settings = ref<Settings>(loadSettings());
const content = ref<MergeFileContent | null>(null);
const loading = ref(false);
const error = ref("");
const blocks = shallowRef<MergeBlock[]>([]);
const snapshot = shallowRef<MergeSnapshot>({ ranges: [], states: [] });
const bands = shallowRef<{ left: Band[]; right: Band[] }>({ left: [], right: [] });
const lineCounts = ref({ ours: 1, result: 1, theirs: 1 });
const current = ref(-1);
const settingsOpen = ref(false);

const oursHost = ref<HTMLElement | null>(null);
const resultHost = ref<HTMLElement | null>(null);
const theirsHost = ref<HTMLElement | null>(null);
const leftDivider = ref<HTMLElement | null>(null);
const rightDivider = ref<HTMLElement | null>(null);

let views: Record<Pane, EditorView | null> = { ours: null, result: null, theirs: null };
let mergeField: StateField<MergeSnapshot> | null = null;
let lines = { ours: [] as string[], base: [] as string[], theirs: [] as string[] };
let baseText = "";
let wordCache = new Map<string, CharRange[]>();
let simpleCache = new Map<number, string | null>();
let drawFrame = 0;
let scrollDriver: Pane | null = null;
let scrollDriverTimer = 0;
const blameSlots: Record<"ours" | "theirs", Compartment> = { ours: new Compartment(), theirs: new Compartment() };

const editable = computed(() => Boolean(content.value && !content.value.binary && !content.value.tooLarge));

const pendingBlocks = computed(() =>
  blocks.value.filter((block) => {
    const state = snapshot.value.states[block.index];
    return state && (state.ours === "pending" || state.theirs === "pending");
  }),
);
const pendingConflicts = computed(() => pendingBlocks.value.filter((block) => block.conflict).length);
const pendingChanges = computed(() => pendingBlocks.value.length);
const allDone = computed(() => editable.value && blocks.value.length > 0 && pendingChanges.value === 0);
const canResolveSimple = computed(() =>
  pendingBlocks.value.some((block) => block.conflict && untouched(block) && simpleOf(block) !== null),
);
const nonConflicting = computed(() => {
  const list = pendingBlocks.value.filter((block) => !block.conflict);
  return {
    ours: list.some((block) => block.oursChanged),
    theirs: list.some((block) => block.theirsChanged),
    all: list.length > 0,
  };
});
const canPrev = computed(() => pendingBlocks.value.some((block) => block.index < current.value));
const canNext = computed(() => pendingBlocks.value.some((block) => block.index > current.value));

const oursAnchors = computed(() =>
  anchorsFor(
    blocks.value.map((block) => [block.ours, snapshot.value.ranges[block.index] ?? block.base] as [LineRange, LineRange]),
    lineCounts.value.ours,
    lineCounts.value.result,
  ),
);
const theirsAnchors = computed(() =>
  anchorsFor(
    blocks.value.map((block) => [block.theirs, snapshot.value.ranges[block.index] ?? block.base] as [LineRange, LineRange]),
    lineCounts.value.theirs,
    lineCounts.value.result,
  ),
);

const statusText = computed(() => {
  if (!blocks.value.length) return "没有更改";
  if (!pendingChanges.value) return "所有更改均已处理";
  return `${pendingChanges.value} 处更改。${pendingConflicts.value} 个冲突`;
});

watch(settings, (value) => localStorage.setItem(SETTINGS_KEY, JSON.stringify(value)), { deep: true });

watch(
  () => props.path,
  (path) => {
    void loadFile(path);
  },
  { immediate: true },
);

watch([snapshot, () => settings.value.highlight], () => {
  refreshMarks();
  scheduleDraw();
});

watch(
  () => settings.value.blame,
  (on) => {
    for (const side of ["ours", "theirs"] as const) {
      views[side]?.dispatch({ effects: blameSlots[side].reconfigure(blameExtension(side, on)) });
    }
    scheduleDraw();
  },
);

async function loadFile(path: string) {
  loading.value = true;
  error.value = "";
  destroyEditors();
  try {
    content.value = await getConflictFile(props.repoPath, path);
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err);
    content.value = null;
  } finally {
    loading.value = false;
  }
  await nextTick();
  mountEditors();
}

function destroyEditors() {
  for (const view of Object.values(views)) view?.destroy();
  views = { ours: null, result: null, theirs: null };
  mergeField = null;
  blocks.value = [];
  snapshot.value = { ranges: [], states: [] };
  bands.value = { left: [], right: [] };
}

function initialSnapshot(list: MergeBlock[]): MergeSnapshot {
  return {
    ranges: list.map((block) => ({ ...block.base })),
    states: list.map((block) => ({
      ours: block.oursChanged ? "pending" : "none",
      theirs: block.theirsChanged ? "pending" : "none",
    })),
  };
}

function mountEditors() {
  destroyEditors();
  const file = content.value;
  if (!file || !editable.value || !oursHost.value || !resultHost.value || !theirsHost.value) return;
  lines = { ours: splitLines(file.ours), base: splitLines(file.base), theirs: splitLines(file.theirs) };
  baseText = lines.base.join("\n");
  wordCache = new Map();
  simpleCache = new Map();
  current.value = -1;
  const list = buildBlocks(lines.ours, lines.base, lines.theirs, settings.value.ignoreWhitespace);
  const initial = initialSnapshot(list);
  const merge = createMergeField(initial);
  mergeField = merge.field;
  lineCounts.value = { ours: lines.ours.length, result: lines.base.length, theirs: lines.theirs.length };

  const geometry = EditorView.updateListener.of((update) => {
    if (update.geometryChanged || update.viewportChanged || update.docChanged) scheduleDraw();
  });
  views.ours = createPane(oursHost.value, {
    doc: lines.ours.join("\n"),
    path: file.path,
    readOnly: true,
    blame: blameSlots.ours,
    blameExtension: blameExtension("ours", settings.value.blame),
    extensions: [geometry],
  });
  views.theirs = createPane(theirsHost.value, {
    doc: lines.theirs.join("\n"),
    path: file.path,
    readOnly: true,
    blame: blameSlots.theirs,
    blameExtension: blameExtension("theirs", settings.value.blame),
    extensions: [geometry],
  });
  views.result = createPane(resultHost.value, {
    doc: baseText,
    path: file.path,
    readOnly: false,
    blame: new Compartment(),
    blameExtension: [],
    extraKeys: [
      {
        key: "Mod-s",
        run: () => {
          applyCurrent();
          return true;
        },
      },
    ],
    extensions: [
      merge.extension,
      geometry,
      EditorView.updateListener.of((update) => {
        if (!mergeField) return;
        if (update.docChanged) lineCounts.value.result = update.state.doc.lines;
        const next = update.state.field(mergeField);
        if (next !== update.startState.field(mergeField)) snapshot.value = next;
      }),
    ],
  });
  blocks.value = list;
  snapshot.value = initial;
  for (const pane of ["ours", "result", "theirs"] as Pane[]) {
    views[pane]?.scrollDOM.addEventListener("scroll", () => onScroll(pane), { passive: true });
  }
  refreshMarks();
  scheduleDraw();
}

function blameExtension(side: "ours" | "theirs", on: boolean): Extension {
  if (!on) return [];
  const source = side === "ours" ? content.value?.oursBlame : content.value?.theirsBlame;
  if (!source?.length) return [];
  return blameGutter(source.map(blameLabel));
}

function blameLabel(meta?: BlameLine): BlameLabel {
  if (!meta?.author) return { label: "", title: "" };
  const time = meta.timestamp ? new Date(meta.timestamp * 1000) : null;
  const date = time ? time.toLocaleDateString("zh-CN") : "";
  return {
    label: date ? `${date} ${meta.author}` : meta.author,
    title: time ? `${meta.author} · ${time.toLocaleString("zh-CN", { hour12: false })}` : meta.author,
  };
}

function sideRange(block: MergeBlock, side: SideName) {
  return side === "ours" ? block.ours : block.theirs;
}

function sideLines(block: MergeBlock, side: SideName) {
  const range = sideRange(block, side);
  return lines[side].slice(range.start, range.end);
}

function baseLines(block: MergeBlock) {
  return lines.base.slice(block.base.start, block.base.end);
}

function wordsOf(block: MergeBlock, side: SideName) {
  const key = `${block.index}:${side}`;
  let words = wordCache.get(key);
  if (!words) {
    const other: SideName = side === "ours" ? "theirs" : "ours";
    const compare = block.base.start === block.base.end && block.conflict ? sideLines(block, other) : baseLines(block);
    words = changedChars(sideLines(block, side).join("\n"), compare.join("\n"), settings.value.ignoreWhitespace);
    wordCache.set(key, words);
  }
  return words;
}

function simpleOf(block: MergeBlock) {
  if (!simpleCache.has(block.index)) {
    simpleCache.set(
      block.index,
      resolveSimple(sideLines(block, "ours").join("\n"), baseLines(block).join("\n"), sideLines(block, "theirs").join("\n")),
    );
  }
  return simpleCache.get(block.index) ?? null;
}

function untouched(block: MergeBlock) {
  const state = snapshot.value.states[block.index];
  return state?.ours === "pending" && state.theirs === "pending";
}

function sidePending(block: MergeBlock, side: SideName) {
  return snapshot.value.states[block.index]?.[side] === "pending";
}

function refreshMarks() {
  const words = settings.value.highlight === "words";
  for (const side of ["ours", "theirs"] as const) {
    const view = views[side];
    if (!view) continue;
    const marks: PaneMark[] = blocks.value
      .filter((block) => sidePending(block, side))
      .map((block) => ({ range: sideRange(block, side), kind: sideKind(block, side), words: words ? wordsOf(block, side) : [] }));
    updateMarks(view, marks, words);
  }
  const view = views.result;
  if (!view) return;
  const marks: PaneMark[] = pendingBlocks.value.map((block) => ({
    range: snapshot.value.ranges[block.index],
    kind: block.conflict ? "conflict" : resultKind(block),
    words: [],
  }));
  updateMarks(view, marks, false);
}

function scheduleDraw() {
  if (drawFrame) return;
  drawFrame = requestAnimationFrame(() => {
    drawFrame = 0;
    draw();
  });
}

function band(fromView: EditorView, from: LineRange, toView: EditorView, to: LineRange, top: number, height: number) {
  const y = (view: EditorView, line: number) => lineTop(view, line) + view.documentTop - top;
  const l0 = y(fromView, from.start);
  const l1 = from.start === from.end ? l0 : y(fromView, from.end);
  const r0 = y(toView, to.start);
  const r1 = to.start === to.end ? r0 : y(toView, to.end);
  if (Math.max(l1, r1) < -40 || Math.min(l0, r0) > height + 40) return null;
  const m = DIVIDER / 2;
  return {
    d: `M0,${l0} C${m},${l0} ${m},${r0} ${DIVIDER},${r0} L${DIVIDER},${r1} C${m},${r1} ${m},${l1} 0,${l1} Z`,
    l0,
    r0,
  };
}

function draw() {
  const { ours, result, theirs } = views;
  if (!ours || !result || !theirs || !leftDivider.value || !rightDivider.value) return;
  const leftRect = leftDivider.value.getBoundingClientRect();
  const rightRect = rightDivider.value.getBoundingClientRect();
  const left: Band[] = [];
  const right: Band[] = [];
  for (const block of blocks.value) {
    const target = snapshot.value.ranges[block.index];
    if (!target) continue;
    if (sidePending(block, "ours")) {
      const shape = band(ours, block.ours, result, target, leftRect.top, leftRect.height);
      if (shape) left.push({ index: block.index, kind: sideKind(block, "ours"), d: shape.d, button: shape.l0 });
    }
    if (sidePending(block, "theirs")) {
      const shape = band(result, target, theirs, block.theirs, rightRect.top, rightRect.height);
      if (shape) right.push({ index: block.index, kind: sideKind(block, "theirs"), d: shape.d, button: shape.r0 });
    }
  }
  bands.value = { left, right };
}

function holdDriver(pane: Pane) {
  scrollDriver = pane;
  window.clearTimeout(scrollDriverTimer);
  scrollDriverTimer = window.setTimeout(() => {
    scrollDriver = null;
  }, 90);
}

function centerLine(view: EditorView) {
  const dom = view.scrollDOM;
  return topLine(view, dom.scrollTop + dom.clientHeight / 2 - view.documentPadding.top);
}

function scrollToLine(view: EditorView, line: number) {
  const dom = view.scrollDOM;
  dom.scrollTop = Math.max(0, heightOfLine(view, line) + view.documentPadding.top - dom.clientHeight / 2);
}

function onScroll(pane: Pane) {
  scheduleDraw();
  if (!settings.value.syncScroll) return;
  if (scrollDriver && scrollDriver !== pane) return;
  const source = views[pane];
  if (!source) return;
  holdDriver(pane);
  const line = centerLine(source);
  const resultLine = pane === "result" ? line : mapLine(pane === "ours" ? oursAnchors.value : theirsAnchors.value, line, "a");
  for (const other of ["ours", "result", "theirs"] as Pane[]) {
    const view = views[other];
    if (!view || other === pane) continue;
    const target = other === "result" ? resultLine : mapLine(other === "ours" ? oursAnchors.value : theirsAnchors.value, resultLine, "b");
    scrollToLine(view, target);
    view.scrollDOM.scrollLeft = source.scrollDOM.scrollLeft;
  }
}

function lineChange(view: EditorView, range: LineRange, next: string[]): ChangeSpec | null {
  const doc = view.state.doc;
  const text = next.join("\n");
  if (range.start < range.end) {
    const from = doc.line(range.start + 1).from;
    if (range.end < doc.lines) return { from, to: doc.line(range.end + 1).from, insert: next.length ? `${text}\n` : "" };
    if (next.length) return { from, to: doc.length, insert: text };
    return { from: Math.max(0, from - 1), to: doc.length, insert: "" };
  }
  if (!next.length) return null;
  if (range.start < doc.lines) return { from: doc.line(range.start + 1).from, insert: `${text}\n` };
  return { from: doc.length, insert: `\n${text}` };
}

function commit(states: SideStates[], edits: Map<number, string[]>) {
  const view = views.result;
  if (!view || !mergeField) return;
  const before = view.state.field(mergeField);
  const changes: ChangeSpec[] = [];
  const ranges: LineRange[] = [];
  let delta = 0;
  before.ranges.forEach((range, index) => {
    const next = edits.get(index);
    const start = range.start + delta;
    if (!next) {
      ranges.push({ start, end: range.end + delta });
      return;
    }
    const change = lineChange(view, range, next);
    if (change) changes.push(change);
    ranges.push({ start, end: start + next.length });
    delta += next.length - (range.end - range.start);
  });
  view.dispatch({
    changes,
    effects: setMerge.of({ ranges, states }),
    annotations: isolateHistory.of("full"),
    userEvent: "input.merge",
  });
}

function cloneStates() {
  return snapshot.value.states.map((state) => ({ ...state }));
}

function apply(block: MergeBlock, side: SideName) {
  const states = cloneStates();
  const state = states[block.index];
  const other: SideName = side === "ours" ? "theirs" : "ours";
  let next = sideLines(block, side);
  if (block.conflict) {
    if (state[other] === "applied") next = [...sideLines(block, other), ...next];
    state[side] = "applied";
  } else {
    state.ours = block.oursChanged ? "applied" : "none";
    state.theirs = block.theirsChanged ? "applied" : "none";
  }
  current.value = block.index;
  commit(states, new Map([[block.index, next]]));
}

function ignore(block: MergeBlock, side: SideName) {
  const states = cloneStates();
  states[block.index][side] = "ignored";
  if (!block.conflict) {
    const other: SideName = side === "ours" ? "theirs" : "ours";
    if (states[block.index][other] === "pending") states[block.index][other] = "ignored";
  }
  current.value = block.index;
  commit(states, new Map());
}

function onBandClick(index: number, side: SideName, action: "apply" | "ignore") {
  const block = blocks.value[index];
  if (!block) return;
  if (action === "apply") apply(block, side);
  else ignore(block, side);
}

function applyNonConflicting(which: SideName | "all") {
  const states = cloneStates();
  const edits = new Map<number, string[]>();
  for (const block of pendingBlocks.value) {
    if (block.conflict) continue;
    const side: SideName = block.oursChanged ? "ours" : "theirs";
    if (which !== "all" && !(which === "ours" ? block.oursChanged : block.theirsChanged)) continue;
    edits.set(block.index, sideLines(block, side));
    states[block.index] = {
      ours: block.oursChanged ? "applied" : "none",
      theirs: block.theirsChanged ? "applied" : "none",
    };
  }
  if (edits.size) commit(states, edits);
}

function resolveSimpleConflicts() {
  const states = cloneStates();
  const edits = new Map<number, string[]>();
  for (const block of pendingBlocks.value) {
    if (!block.conflict || !untouched(block)) continue;
    const merged = simpleOf(block);
    if (merged === null) continue;
    edits.set(block.index, merged === "" && !sideLines(block, "ours").length ? [] : merged.split("\n"));
    states[block.index] = { ours: "applied", theirs: "applied" };
  }
  if (!edits.size) {
    notify("没有可以自动解决的简单冲突", "error");
    return;
  }
  commit(states, edits);
  notify(`已自动解决 ${edits.size} 个简单冲突`);
}

function go(delta: number) {
  const list = pendingBlocks.value;
  const target = delta > 0 ? list.find((block) => block.index > current.value) : [...list].reverse().find((block) => block.index < current.value);
  if (target) reveal(target);
}

function reveal(block: MergeBlock) {
  const view = views.result;
  const range = snapshot.value.ranges[block.index];
  if (!view || !range) return;
  current.value = block.index;
  const doc = view.state.doc;
  const pos = doc.line(Math.min(doc.lines, range.start + 1)).from;
  holdDriver("result");
  view.dispatch({ selection: { anchor: pos }, effects: EditorView.scrollIntoView(pos, { y: "center" }) });
  view.focus();
}

function revealIndex(index: number) {
  const block = blocks.value[index];
  if (block) reveal(block);
}

function stripeStyle(range: LineRange, total: number) {
  const span = Math.max(1, total);
  return {
    top: `${(Math.min(range.start, span) / span) * 100}%`,
    height: `max(3px, ${((range.end - range.start) / span) * 100}%)`,
  };
}

function stripeMarks(pane: Pane) {
  if (pane === "result") {
    return pendingBlocks.value.map((block) => ({
      index: block.index,
      kind: block.conflict ? "conflict" : resultKind(block),
      range: snapshot.value.ranges[block.index],
      total: lineCounts.value.result,
    }));
  }
  return blocks.value
    .filter((block) => sidePending(block, pane))
    .map((block) => ({ index: block.index, kind: sideKind(block, pane), range: sideRange(block, pane), total: lineCounts.value[pane] }));
}

function resultText() {
  return views.result?.state.doc.toString() ?? "";
}

function dirty() {
  if (!views.result) return false;
  const initial = initialSnapshot(blocks.value).states;
  return resultText() !== baseText || snapshot.value.states.some((state, index) => state.ours !== initial[index].ours || state.theirs !== initial[index].theirs);
}

function applyCurrent() {
  if (props.busy || !editable.value || !views.result) return;
  if (pendingConflicts.value > 0 && !window.confirm(`还有 ${pendingConflicts.value} 个冲突未处理，将按结果栏当前内容保存，确定应用？`)) return;
  emit("save", { path: props.path, content: resultText() });
}

function acceptSide(side: SideName) {
  if (props.busy || !editable.value) return;
  emit("save", { path: props.path, content: lines[side].join("\n") });
}

function goBack() {
  if (dirty() && !window.confirm("当前文件的合并结果尚未应用，关闭后将丢失，确定关闭？")) return;
  emit("back");
}

function changeIgnoreWhitespace(event: Event) {
  const target = event.target as HTMLSelectElement;
  const next = target.value === "ignore";
  if (dirty() && !window.confirm("切换空白处理方式会重新计算差异，当前文件的处理进度将丢失，确定继续？")) {
    target.value = settings.value.ignoreWhitespace ? "ignore" : "none";
    return;
  }
  settings.value.ignoreWhitespace = next;
  mountEditors();
}

function openFind() {
  if (views.result) openSearchPanel(views.result);
}

function onPageKey(event: KeyboardEvent) {
  if (event.defaultPrevented) return;
  if (event.key === "F7") {
    event.preventDefault();
    go(event.shiftKey ? -1 : 1);
  } else if (event.key === "Escape") {
    event.preventDefault();
    if (settingsOpen.value) settingsOpen.value = false;
    else goBack();
  }
}

function closeSettings(event: MouseEvent) {
  if (settingsOpen.value && !(event.target as HTMLElement).closest(".mrg-settings")) settingsOpen.value = false;
}

function onResize() {
  scheduleDraw();
}

onMounted(() => {
  window.addEventListener("keydown", onPageKey);
  window.addEventListener("resize", onResize);
});

onUnmounted(() => {
  window.removeEventListener("keydown", onPageKey);
  window.removeEventListener("resize", onResize);
  window.clearTimeout(scrollDriverTimer);
  cancelAnimationFrame(drawFrame);
  destroyEditors();
});
</script>

<template>
  <Teleport to="body">
    <div class="mrg-page" @mousedown="closeSettings">
      <div class="mrg-title">
        <span>合并修订</span>
        <span class="mrg-title-path" :title="`${repoPath}/${path}`">{{ repoPath.replace(/\\/g, "/") }}/{{ path }}</span>
      </div>

      <div class="mrg-toolbar">
        <button type="button" class="mrg-icon" title="上一个差异 (Shift+F7)" :disabled="!canPrev" @click="go(-1)">
          <svg viewBox="0 0 16 16"><path d="M8 13V3M3.5 7.5 8 3l4.5 4.5" /></svg>
        </button>
        <button type="button" class="mrg-icon" title="下一个差异 (F7)" :disabled="!canNext" @click="go(1)">
          <svg viewBox="0 0 16 16"><path d="M8 3v10M3.5 8.5 8 13l4.5-4.5" /></svg>
        </button>
        <span class="mrg-sep" />
        <button
          type="button"
          class="mrg-icon"
          title="应用左侧的非冲突更改"
          :disabled="busy || !nonConflicting.ours"
          @click="applyNonConflicting('ours')"
        >
          <svg viewBox="0 0 16 16"><path d="M2 4l4 4-4 4M7 4l4 4-4 4M14 3v10" /></svg>
        </button>
        <button
          type="button"
          class="mrg-icon"
          title="应用所有非冲突更改"
          :disabled="busy || !nonConflicting.all"
          @click="applyNonConflicting('all')"
        >
          <svg viewBox="0 0 16 16"><path d="M1.5 4.5l3.5 3.5-3.5 3.5M14.5 4.5 11 8l3.5 3.5M8 3v10" /></svg>
        </button>
        <button
          type="button"
          class="mrg-icon"
          title="应用右侧的非冲突更改"
          :disabled="busy || !nonConflicting.theirs"
          @click="applyNonConflicting('theirs')"
        >
          <svg viewBox="0 0 16 16"><path d="M14 4l-4 4 4 4M9 4 5 8l4 4M2 3v10" /></svg>
        </button>
        <button
          type="button"
          class="mrg-icon"
          title="解决简单冲突"
          :disabled="busy || !canResolveSimple"
          @click="resolveSimpleConflicts"
        >
          <svg viewBox="0 0 16 16"><path d="M2.5 13.5 10 6M9 3.5V1.5M12.5 7h2M11.5 4.5 13 3M4.5 2.5l.6 1.4 1.4.6-1.4.6-.6 1.4-.6-1.4L2.5 4.5l1.4-.6z" /></svg>
        </button>
        <span class="mrg-sep" />
        <select class="mrg-combo" :value="settings.ignoreWhitespace ? 'ignore' : 'none'" title="空白处理" @change="changeIgnoreWhitespace">
          <option value="none">不忽略</option>
          <option value="ignore">忽略空白</option>
        </select>
        <select v-model="settings.highlight" class="mrg-combo" title="高亮方式">
          <option value="words">高亮显示单词</option>
          <option value="lines">高亮显示行</option>
        </select>
        <span class="mrg-sep" />
        <button
          type="button"
          class="mrg-icon"
          :class="{ on: settings.syncScroll }"
          title="同步滚动"
          @click="settings.syncScroll = !settings.syncScroll"
        >
          <svg viewBox="0 0 16 16"><path d="M5 2v12M2.5 4.5 5 2l2.5 2.5M2.5 11.5 5 14l2.5-2.5M11 2v12M8.5 4.5 11 2l2.5 2.5M8.5 11.5 11 14l2.5-2.5" /></svg>
        </button>
        <button type="button" class="mrg-icon" title="查找 (Ctrl+F)" @click="openFind">
          <svg viewBox="0 0 16 16"><path d="M10.5 10.5 14 14M11.5 6.5a5 5 0 1 1-10 0 5 5 0 0 1 10 0z" /></svg>
        </button>
        <span class="mrg-settings">
          <button type="button" class="mrg-icon" :class="{ on: settingsOpen }" title="设置" @click="settingsOpen = !settingsOpen">
            <svg viewBox="0 0 16 16">
              <path d="M8 10.2a2.2 2.2 0 1 0 0-4.4 2.2 2.2 0 0 0 0 4.4z" />
              <path
                d="M6.8 1.5h2.4l.4 1.9 1.3.7 1.8-.7 1.2 2.1-1.4 1.3v1.4l1.4 1.3-1.2 2.1-1.8-.7-1.3.7-.4 1.9H6.8l-.4-1.9-1.3-.7-1.8.7-1.2-2.1 1.4-1.3V6.8L2.1 5.5l1.2-2.1 1.8.7 1.3-.7z"
              />
            </svg>
          </button>
          <div v-if="settingsOpen" class="mrg-menu">
            <label><input v-model="settings.blame" type="checkbox" /> 显示 Blame 注释</label>
            <label><input v-model="settings.syncScroll" type="checkbox" /> 同步滚动</label>
          </div>
        </span>
        <span class="mrg-spacer" />
        <span class="mrg-status" :class="{ conflict: pendingConflicts > 0 }">{{ statusText }}</span>
      </div>

      <div v-if="allDone" class="mrg-banner">
        <span>所有更改均已处理。</span>
        <a href="#" @click.prevent="applyCurrent">保存更改并完成合并</a>
      </div>

      <p v-if="error" class="mrg-hint err">{{ error }}</p>
      <p v-else-if="loading" class="mrg-hint">正在加载三方内容…</p>
      <p v-else-if="content?.binary" class="mrg-hint">二进制文件无法逐行合并，请使用底部的「接受左侧」或「接受右侧」</p>
      <p v-else-if="content?.tooLarge" class="mrg-hint">文件超过 512KB，无法在此合并，请使用底部的「接受左侧」或「接受右侧」</p>

      <div v-show="editable && !loading" class="mrg-work">
        <section class="mrg-col">
          <div class="mrg-head">您的版本<b v-if="state.ours">{{ state.ours }}</b></div>
          <div class="mrg-body">
            <div class="mrg-stripe">
              <i
                v-for="mark in stripeMarks('ours')"
                :key="mark.index"
                :class="`mrg-${mark.kind}`"
                :style="stripeStyle(mark.range, mark.total)"
                @click="revealIndex(mark.index)"
              />
            </div>
            <div ref="oursHost" class="mrg-host" />
          </div>
        </section>

        <section class="mrg-divider">
          <div class="mrg-head" />
          <div ref="leftDivider" class="mrg-divider-body">
            <svg :width="DIVIDER" height="100%">
              <path v-for="item in bands.left" :key="item.index" :class="`mrg-band mrg-${item.kind}`" :d="item.d" />
            </svg>
            <div v-for="item in bands.left" :key="`b${item.index}`" class="mrg-actions left" :style="{ top: `${item.button}px` }">
              <button type="button" class="mrg-act apply" title="应用 (»)" :disabled="busy" @click="onBandClick(item.index, 'ours', 'apply')">»</button>
              <button type="button" class="mrg-act ignore" title="忽略 (×)" :disabled="busy" @click="onBandClick(item.index, 'ours', 'ignore')">×</button>
            </div>
          </div>
        </section>

        <section class="mrg-col">
          <div class="mrg-head center">结果</div>
          <div class="mrg-body">
            <div ref="resultHost" class="mrg-host" />
            <div class="mrg-stripe">
              <i
                v-for="mark in stripeMarks('result')"
                :key="mark.index"
                :class="`mrg-${mark.kind}`"
                :style="stripeStyle(mark.range, mark.total)"
                @click="revealIndex(mark.index)"
              />
            </div>
          </div>
        </section>

        <section class="mrg-divider">
          <div class="mrg-head" />
          <div ref="rightDivider" class="mrg-divider-body">
            <svg :width="DIVIDER" height="100%">
              <path v-for="item in bands.right" :key="item.index" :class="`mrg-band mrg-${item.kind}`" :d="item.d" />
            </svg>
            <div v-for="item in bands.right" :key="`b${item.index}`" class="mrg-actions right" :style="{ top: `${item.button}px` }">
              <button type="button" class="mrg-act ignore" title="忽略 (×)" :disabled="busy" @click="onBandClick(item.index, 'theirs', 'ignore')">×</button>
              <button type="button" class="mrg-act apply" title="应用 («)" :disabled="busy" @click="onBandClick(item.index, 'theirs', 'apply')">«</button>
            </div>
          </div>
        </section>

        <section class="mrg-col">
          <div class="mrg-head">来自 <b>{{ state.theirs || "传入" }}</b> 的更改</div>
          <div class="mrg-body">
            <div ref="theirsHost" class="mrg-host" />
            <div class="mrg-stripe">
              <i
                v-for="mark in stripeMarks('theirs')"
                :key="mark.index"
                :class="`mrg-${mark.kind}`"
                :style="stripeStyle(mark.range, mark.total)"
                @click="revealIndex(mark.index)"
              />
            </div>
          </div>
        </section>
      </div>
      <div v-if="!editable || loading" class="mrg-fill" />

      <footer class="mrg-foot">
        <span
          class="mrg-help"
          title="F7 / Shift+F7：下一个 / 上一个差异&#10;» «：应用这一侧的更改，冲突时先后点两侧可都保留&#10;×：忽略这一侧的更改&#10;Ctrl+S：应用　Ctrl+F：查找　Ctrl+Z：撤销&#10;Esc：取消"
        >?</span>
        <button type="button" class="mrg-btn" :disabled="busy || !content" @click="acceptSide('ours')">接受左侧</button>
        <button type="button" class="mrg-btn" :disabled="busy || !content" @click="acceptSide('theirs')">接受右侧</button>
        <span class="mrg-spacer" />
        <button type="button" class="mrg-btn" :disabled="busy" @click="goBack">取消</button>
        <button type="button" class="mrg-btn primary" :disabled="busy || !editable" @click="applyCurrent">应用</button>
      </footer>
    </div>
  </Teleport>
</template>

<style>
.mrg-page {
  --mrg-conflict: #4a2f2d;
  --mrg-conflict-word: #6e403b;
  --mrg-conflict-edge: #c75450;
  --mrg-mod: #2c3a4b;
  --mrg-mod-word: #3d5573;
  --mrg-mod-edge: #4d7bb8;
  --mrg-add: #29412f;
  --mrg-add-word: #3a5e44;
  --mrg-add-edge: #4f9c5c;
  --mrg-del: #3b3d40;
  --mrg-del-word: #50545a;
  --mrg-del-edge: #868a91;
  --tok-keyword: #cf8e6d;
  --tok-string: #6aab73;
  --tok-number: #2aacb8;
  --tok-comment: #7a7e85;
  --tok-type: #d5b778;
  --tok-property: #c77dbb;
  --tok-meta: #b3ae60;
  --tok-func: #56a8f5;
  position: fixed;
  inset: 0;
  z-index: 1250;
  display: flex;
  flex-direction: column;
  background: var(--bg);
  color: var(--text);
  font-size: 12px;
}

html[data-theme="light"] .mrg-page {
  --mrg-conflict: #ffe1dc;
  --mrg-conflict-word: #ffbfb5;
  --mrg-conflict-edge: #e0574a;
  --mrg-mod: #e3edfb;
  --mrg-mod-word: #c2d7f6;
  --mrg-mod-edge: #5d8fd8;
  --mrg-add: #dff3e2;
  --mrg-add-word: #b9e3c0;
  --mrg-add-edge: #57ad66;
  --mrg-del: #ebebec;
  --mrg-del-word: #d4d5d8;
  --mrg-del-edge: #a2a5ab;
  --tok-keyword: #0033b3;
  --tok-string: #067d17;
  --tok-number: #1750eb;
  --tok-comment: #8c8c8c;
  --tok-type: #0033b3;
  --tok-property: #871094;
  --tok-meta: #9e880d;
  --tok-func: #00627a;
}

.mrg-title {
  flex: none;
  height: 30px;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 0 12px;
  border-bottom: 1px solid var(--border);
  background: var(--bg-hover);
}

.mrg-title-path {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--text-muted);
}

.mrg-toolbar {
  flex: none;
  height: 32px;
  display: flex;
  align-items: center;
  gap: 2px;
  padding: 0 6px;
  border-bottom: 1px solid var(--border);
}

.mrg-icon {
  width: 24px;
  height: 24px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border-radius: 4px;
  color: var(--text-muted);
}

.mrg-icon svg {
  width: 16px;
  height: 16px;
  fill: none;
  stroke: currentColor;
  stroke-width: 1.3;
  stroke-linecap: round;
  stroke-linejoin: round;
}

.mrg-icon:hover:not(:disabled),
.mrg-icon.on {
  background: var(--bg-selected);
  color: var(--text);
}

.mrg-icon:disabled {
  opacity: 0.35;
  cursor: default;
}

.mrg-sep {
  width: 1px;
  height: 16px;
  margin: 0 4px;
  background: var(--border-soft);
}

.mrg-combo {
  height: 24px;
  padding: 0 4px;
  border: 0;
  border-radius: 4px;
  background: transparent;
  color: var(--text-muted);
  font-size: 12px;
  cursor: pointer;
}

.mrg-combo:hover {
  background: var(--bg-selected);
  color: var(--text);
}

.mrg-combo option {
  background: var(--bg-elevated);
  color: var(--text);
}

.mrg-settings {
  position: relative;
}

.mrg-menu {
  position: absolute;
  top: 28px;
  left: 0;
  z-index: 5;
  min-width: 160px;
  padding: 6px 0;
  border: 1px solid var(--border-elevated);
  border-radius: 6px;
  background: var(--bg-elevated);
  box-shadow: var(--shadow);
}

.mrg-menu label {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 5px 12px;
  white-space: nowrap;
  cursor: pointer;
}

.mrg-menu label:hover {
  background: var(--bg-selected);
}

.mrg-spacer {
  flex: 1;
}

.mrg-status {
  padding-right: 8px;
  color: var(--text-muted);
  white-space: nowrap;
}

.mrg-banner {
  flex: none;
  height: 28px;
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 0 12px;
  border-bottom: 1px solid var(--border);
  background: var(--mrg-add);
}

.mrg-banner a {
  color: var(--accent);
}

.mrg-hint {
  margin: 0;
  padding: 24px;
  color: var(--text-muted);
}

.mrg-hint.err {
  color: var(--danger);
}

.mrg-fill {
  flex: 1;
}

.mrg-work {
  flex: 1;
  min-height: 0;
  display: grid;
  grid-template-columns: minmax(0, 1fr) 30px minmax(0, 1fr) 30px minmax(0, 1fr);
}

.mrg-col,
.mrg-divider {
  min-width: 0;
  min-height: 0;
  display: flex;
  flex-direction: column;
}

.mrg-head {
  flex: none;
  height: 26px;
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 0 8px;
  overflow: hidden;
  white-space: nowrap;
  border-bottom: 1px solid var(--border);
  background: var(--bg-hover);
  color: var(--text-muted);
}

.mrg-head b {
  color: var(--text);
  font-weight: 600;
}

.mrg-head.center {
  justify-content: center;
}

.mrg-divider .mrg-head {
  padding: 0;
}

.mrg-body {
  flex: 1;
  min-height: 0;
  display: flex;
}

.mrg-host {
  flex: 1;
  min-width: 0;
  min-height: 0;
}

.mrg-host .cm-editor {
  height: 100%;
  background: var(--bg-deep);
  color: var(--text-editor);
  font-size: 12px;
}

.mrg-host .cm-editor.cm-focused {
  outline: none;
}

.mrg-host .cm-scroller {
  font-family: "JetBrains Mono", Consolas, "Courier New", monospace;
  line-height: 18px;
}

.mrg-host .cm-content {
  caret-color: var(--text);
}

.mrg-host .cm-cursor {
  border-left-color: var(--text);
}

.mrg-host .cm-gutters {
  border: 0;
  background: var(--bg-deep);
  color: var(--text-dim);
}

.mrg-host .cm-lineNumbers .cm-gutterElement {
  min-width: 32px;
  padding: 0 8px 0 4px;
}

.mrg-host .cm-activeLine {
  background: transparent;
}

.mrg-host .cm-focused .cm-activeLine {
  background: var(--bg-hover);
}

.mrg-host .cm-activeLineGutter {
  background: transparent;
  color: var(--text-muted);
}

.mrg-host .cm-selectionBackground,
.mrg-host .cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground {
  background: rgba(53, 116, 240, 0.35);
}

.mrg-host .cm-selectionMatch {
  background: rgba(53, 116, 240, 0.18);
}

.mrg-host .cm-matchingBracket {
  background: rgba(53, 116, 240, 0.25);
  color: inherit;
}

.mrg-host .cm-panels {
  background: var(--bg-hover);
  color: var(--text);
  border-color: var(--border);
}

.mrg-host .cm-panel.cm-search {
  padding: 4px 8px;
  font-family: inherit;
  font-size: 12px;
}

.mrg-host .cm-panel.cm-search input,
.mrg-host .cm-panel.cm-search button {
  font-size: 12px;
}

.mrg-host .cm-textfield {
  border: 1px solid var(--border-soft);
  border-radius: 4px;
  background: var(--bg-input);
  color: var(--text);
}

.mrg-host .cm-button {
  border: 1px solid var(--border-soft);
  border-radius: 4px;
  background: var(--bg-input);
  background-image: none;
  color: var(--text);
}

.mrg-host .cm-searchMatch {
  background: rgba(255, 200, 50, 0.3);
}

.mrg-host .cm-searchMatch-selected {
  background: rgba(255, 200, 50, 0.55);
}

.mrg-host .mrg-blame .cm-gutterElement {
  padding: 0 8px 0 6px;
  color: var(--text-dim);
  font-size: 11px;
  white-space: nowrap;
}

.mrg-line.mrg-conflict { background: var(--mrg-conflict); }
.mrg-line.mrg-mod { background: var(--mrg-mod); }
.mrg-line.mrg-add { background: var(--mrg-add); }
.mrg-line.mrg-del { background: var(--mrg-del); }
.mrg-line.mrg-first.mrg-conflict { box-shadow: inset 0 1px 0 var(--mrg-conflict-edge); }
.mrg-line.mrg-first.mrg-mod { box-shadow: inset 0 1px 0 var(--mrg-mod-edge); }
.mrg-line.mrg-first.mrg-add { box-shadow: inset 0 1px 0 var(--mrg-add-edge); }
.mrg-line.mrg-first.mrg-del { box-shadow: inset 0 1px 0 var(--mrg-del-edge); }
.mrg-line.mrg-last.mrg-conflict { box-shadow: inset 0 -1px 0 var(--mrg-conflict-edge); }
.mrg-line.mrg-last.mrg-mod { box-shadow: inset 0 -1px 0 var(--mrg-mod-edge); }
.mrg-line.mrg-last.mrg-add { box-shadow: inset 0 -1px 0 var(--mrg-add-edge); }
.mrg-line.mrg-last.mrg-del { box-shadow: inset 0 -1px 0 var(--mrg-del-edge); }
.mrg-line.mrg-first.mrg-last.mrg-conflict { box-shadow: inset 0 1px 0 var(--mrg-conflict-edge), inset 0 -1px 0 var(--mrg-conflict-edge); }
.mrg-line.mrg-first.mrg-last.mrg-mod { box-shadow: inset 0 1px 0 var(--mrg-mod-edge), inset 0 -1px 0 var(--mrg-mod-edge); }
.mrg-line.mrg-first.mrg-last.mrg-add { box-shadow: inset 0 1px 0 var(--mrg-add-edge), inset 0 -1px 0 var(--mrg-add-edge); }
.mrg-line.mrg-first.mrg-last.mrg-del { box-shadow: inset 0 1px 0 var(--mrg-del-edge), inset 0 -1px 0 var(--mrg-del-edge); }
.mrg-gap-above.mrg-conflict { box-shadow: inset 0 2px 0 var(--mrg-conflict-edge); }
.mrg-gap-above.mrg-mod { box-shadow: inset 0 2px 0 var(--mrg-mod-edge); }
.mrg-gap-above.mrg-add { box-shadow: inset 0 2px 0 var(--mrg-add-edge); }
.mrg-gap-above.mrg-del { box-shadow: inset 0 2px 0 var(--mrg-del-edge); }
.mrg-gap-below.mrg-conflict { box-shadow: inset 0 -2px 0 var(--mrg-conflict-edge); }
.mrg-gap-below.mrg-mod { box-shadow: inset 0 -2px 0 var(--mrg-mod-edge); }
.mrg-gap-below.mrg-add { box-shadow: inset 0 -2px 0 var(--mrg-add-edge); }
.mrg-gap-below.mrg-del { box-shadow: inset 0 -2px 0 var(--mrg-del-edge); }
.mrg-conflict .mrg-word { background: var(--mrg-conflict-word); }
.mrg-mod .mrg-word { background: var(--mrg-mod-word); }
.mrg-add .mrg-word { background: var(--mrg-add-word); }
.mrg-del .mrg-word { background: var(--mrg-del-word); }

.mrg-host .tok-keyword,
.mrg-host .tok-bool,
.mrg-host .tok-atom { color: var(--tok-keyword); }
.mrg-host .tok-string,
.mrg-host .tok-string2,
.mrg-host .tok-inserted { color: var(--tok-string); }
.mrg-host .tok-number,
.mrg-host .tok-literal { color: var(--tok-number); }
.mrg-host .tok-comment { color: var(--tok-comment); font-style: italic; }
.mrg-host .tok-typeName,
.mrg-host .tok-className,
.mrg-host .tok-namespace { color: var(--tok-type); }
.mrg-host .tok-propertyName { color: var(--tok-property); }
.mrg-host .tok-meta,
.mrg-host .tok-macroName,
.mrg-host .tok-annotation { color: var(--tok-meta); }
.mrg-host .tok-variableName.tok-definition,
.mrg-host .tok-propertyName.tok-definition { color: var(--tok-func); }
.mrg-host .tok-invalid,
.mrg-host .tok-deleted { color: var(--danger); }

.mrg-stripe {
  flex: none;
  position: relative;
  width: 8px;
  background: var(--bg-deep);
}

.mrg-col:first-child .mrg-stripe {
  border-right: 1px solid var(--border-soft);
}

.mrg-col:not(:first-child) .mrg-stripe {
  border-left: 1px solid var(--border-soft);
}

.mrg-stripe i {
  position: absolute;
  left: 1px;
  right: 1px;
  min-height: 3px;
  cursor: pointer;
}

.mrg-stripe i.mrg-conflict { background: var(--mrg-conflict-edge); }
.mrg-stripe i.mrg-mod { background: var(--mrg-mod-edge); }
.mrg-stripe i.mrg-add { background: var(--mrg-add-edge); }
.mrg-stripe i.mrg-del { background: var(--mrg-del-edge); }

.mrg-divider-body {
  position: relative;
  flex: 1;
  min-height: 0;
  overflow: hidden;
  background: var(--bg-hover);
}

.mrg-divider-body svg {
  position: absolute;
  inset: 0;
  overflow: visible;
}

.mrg-band {
  stroke-width: 1;
}

.mrg-band.mrg-conflict { fill: var(--mrg-conflict); stroke: var(--mrg-conflict-edge); }
.mrg-band.mrg-mod { fill: var(--mrg-mod); stroke: var(--mrg-mod-edge); }
.mrg-band.mrg-add { fill: var(--mrg-add); stroke: var(--mrg-add-edge); }
.mrg-band.mrg-del { fill: var(--mrg-del); stroke: var(--mrg-del-edge); }

.mrg-actions {
  position: absolute;
  display: flex;
  margin-top: 1px;
}

.mrg-actions.left {
  left: 0;
}

.mrg-actions.right {
  right: 0;
}

.mrg-act {
  width: 14px;
  height: 16px;
  padding: 0;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border-radius: 3px;
  font-size: 14px;
  font-weight: 700;
  line-height: 1;
  color: var(--text-muted);
}

.mrg-act.ignore {
  font-size: 12px;
  font-weight: 400;
  color: var(--text-dim);
}

.mrg-act:hover:not(:disabled) {
  background: var(--bg-selected);
  color: var(--text);
}

.mrg-act.ignore:hover:not(:disabled) {
  color: var(--danger);
}

.mrg-foot {
  flex: none;
  height: 46px;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 0 12px;
  border-top: 1px solid var(--border);
}

.mrg-help {
  width: 22px;
  height: 22px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border: 1px solid var(--border-soft);
  border-radius: 50%;
  color: var(--text-muted);
  cursor: help;
}

.mrg-btn {
  height: 26px;
  padding: 0 14px;
  border: 1px solid var(--border-soft);
  border-radius: 4px;
  background: var(--bg-input);
  color: var(--text);
  white-space: nowrap;
}

.mrg-btn:hover:not(:disabled) {
  background: var(--bg-selected);
}

.mrg-btn.primary {
  border-color: var(--accent);
  background: var(--accent);
  color: #fff;
}

.mrg-btn.primary:hover:not(:disabled) {
  filter: brightness(1.08);
  background: var(--accent);
}

.mrg-btn:disabled {
  opacity: 0.5;
  cursor: default;
}
</style>
