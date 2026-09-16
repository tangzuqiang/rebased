<script setup lang="ts">
import { computed, nextTick, ref, watch } from "vue";
import { getConflictFile } from "../api";
import { alignDiff3 } from "../diff3";
import { commentPrefix, findInTextarea, handleIdeEditorKey, writeTextarea } from "../editorKeys";
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

const content = ref<MergeFileContent | null>(null);
const result = ref("");
const resultLines = ref<Array<string | null>>([]);
const choice = ref<Record<string, "ours" | "theirs" | "base">>({});
const settled = ref<Record<string, { ours: boolean; theirs: boolean }>>({});
const loading = ref(false);
const error = ref("");
const ignoreWhitespace = ref(false);
const showBlame = ref(true);
const overwrite = ref<"ours" | "theirs" | "">("");
const blameMenu = ref({ open: false, x: 0, y: 0 });
const currentHunk = ref(0);
const mergeReady = ref(false);
const completeOpen = ref(false);
const completeShown = ref(false);
const syncing = ref(false);
const oursPane = ref<HTMLElement | null>(null);
const resultPane = ref<HTMLElement | null>(null);
const theirsPane = ref<HTMLElement | null>(null);
const leftGutter = ref<HTMLElement | null>(null);
const rightGutter = ref<HTMLElement | null>(null);
const resultEditor = ref<HTMLTextAreaElement | null>(null);
const findOpen = ref(false);
const findQuery = ref("");
const findHint = ref("");

const rows = computed<Diff3Row[]>(() => {
  if (!content.value || content.value.binary) return [];
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

const unresolvedConflicts = computed(() => hunks.value.filter((hunk) => hunk.conflict && !hunkDone(hunk)));
const conflictCount = computed(() => unresolvedConflicts.value.length);
const currentStart = computed(() => hunks.value[currentHunk.value]?.start ?? -1);
const canGoUp = computed(() => unresolvedConflicts.value.some((hunk) => hunk.start < currentStart.value));
const canGoDown = computed(() => unresolvedConflicts.value.some((hunk) => hunk.start > currentStart.value));
const nonConflictCount = computed(() => hunks.value.filter((hunk) => !hunk.conflict && !hunkDone(hunk)).length);
const oursNos = computed(() => numberSide(rows.value, (row) => row.oursKind));
const theirsNos = computed(() => numberSide(rows.value, (row) => row.theirsKind));

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
  return lineFrom(row, "ours");
}

function joinResult(lines: Array<string | null>) {
  return lines.filter((line): line is string => line !== null).join("\n");
}

function seedResult() {
  resultLines.value = rows.value.map(defaultResultLine);
  choice.value = {};
  settled.value = {};
  setResultText(joinResult(resultLines.value));
}

function markComplete() {
  mergeReady.value = true;
  if (completeShown.value) return;
  completeShown.value = true;
  completeOpen.value = true;
}

function checkConflictsResolved() {
  const conflicts = hunks.value.filter((hunk) => hunk.conflict);
  if (!conflicts.length) return;
  if (conflicts.every((hunk) => hunkDone(hunk))) markComplete();
}

function sideSettled(hunk: Hunk, side: "ours" | "theirs") {
  return Boolean(settled.value[hunkKey(hunk)]?.[side]);
}

function hunkDone(hunk: Hunk) {
  const key = hunkKey(hunk);
  const state = settled.value[key];
  if (!state) return false;
  if (hunk.conflict) {
    return Boolean(choice.value[key] === "ours" || choice.value[key] === "theirs" || (state.ours && state.theirs));
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

function hunkAt(index: number) {
  return hunks.value.find((hunk) => hunk.start === index);
}

function hunkOf(index: number) {
  return hunks.value.find((hunk) => index >= hunk.start && index < hunk.end);
}

function applyHunk(hunk: Hunk, side: "ours" | "theirs" | "base") {
  syncLinesFromResult();
  const source = rows.value;
  const next = source.map((row, index) => resultLines.value[index] ?? defaultResultLine(row));
  for (let index = hunk.start; index < hunk.end; index += 1) {
    next[index] = lineFrom(source[index], side);
  }
  resultLines.value = next;
  choice.value = { ...choice.value, [hunkKey(hunk)]: side };
  setResultText(joinResult(next));
  currentHunk.value = Math.max(0, hunks.value.findIndex((item) => item.start === hunk.start));
  checkConflictsResolved();
}

function setResultText(text: string) {
  result.value = text;
  writeTextarea(resultEditor.value, text);
}

function syncLinesFromResult() {
  const visual = result.value.split("\n");
  let offset = 0;
  resultLines.value = resultLines.value.map((line) => {
    if (line === null) return null;
    const value = visual[offset] ?? line;
    offset += 1;
    return value;
  });
}

function onResultInput(event: Event) {
  const target = event.target;
  if (!(target instanceof HTMLTextAreaElement)) return;
  result.value = target.value;
}

function acceptHunk(hunk: Hunk, side: "ours" | "theirs") {
  applyHunk(hunk, side);
  markSettled(hunk, { [side]: true });
  checkConflictsResolved();
}

function rejectHunk(hunk: Hunk, side: "ours" | "theirs") {
  const current = choice.value[hunkKey(hunk)];
  const other = side === "ours" ? "theirs" : "ours";
  applyHunk(hunk, current === other ? other : "base");
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
    if (picked !== side) return raw === "empty" ? "empty" : "discarded";
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

function blameMeta(index: number, side: "ours" | "theirs") {
  const row = rows.value[index];
  return side === "ours" ? row.oursBlame : row.theirsBlame;
}

function blameAnno(index: number, side: "ours" | "theirs") {
  if (!showBlame.value) return "";
  const label = blameDateAuthor(blameMeta(index, side));
  if (!label) return "";
  if (index > 0 && blameDateAuthor(blameMeta(index - 1, side)) === label) return "";
  return label;
}

function cellTitle(kind: DiffKind, meta?: BlameLine, text = "") {
  const blame = formatBlame(meta);
  if (kind === "same" || kind === "empty") return blame || text;
  return blame ? `${text}\n${blame}` : text;
}

function openBlameMenu(event: MouseEvent) {
  event.preventDefault();
  blameMenu.value = { open: true, x: event.clientX, y: event.clientY };
}

function closeBlameMenu() {
  blameMenu.value.open = false;
}

function toggleBlame() {
  showBlame.value = !showBlame.value;
  closeBlameMenu();
}

function panes() {
  return [oursPane.value, resultPane.value, theirsPane.value, leftGutter.value, rightGutter.value];
}

function syncScroll(event: Event) {
  if (syncing.value) return;
  const source = event.target;
  if (!(source instanceof HTMLElement)) return;
  syncing.value = true;
  for (const pane of panes()) {
    if (pane && pane !== source) {
      pane.scrollTop = source.scrollTop;
      if (pane === oursPane.value || pane === resultPane.value || pane === theirsPane.value) {
        pane.scrollLeft = source.scrollLeft;
      }
    }
  }
  syncing.value = false;
}

async function goHunk(delta: number) {
  const list = unresolvedConflicts.value;
  if (!list.length) return;
  const start = currentStart.value;
  const target =
    delta < 0
      ? [...list].reverse().find((hunk) => hunk.start < start)
      : list.find((hunk) => hunk.start > start);
  if (!target) return;
  currentHunk.value = Math.max(0, hunks.value.findIndex((hunk) => hunk.start === target.start));
  await nextTick();
  const top = Math.max(0, target.start * 18 - 80);
  for (const pane of panes()) {
    if (pane) pane.scrollTop = top;
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
  if (conflictCount.value > 0 || !content.value || content.value.binary) return;
  emit("save", { path: props.path, content: result.value });
}

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
</script>

<template>
  <Teleport to="body">
    <div class="page">
      <header class="toolbar">
        <button type="button" class="icon-btn" title="返回冲突列表" @click="emit('back')">←</button>
        <button type="button" class="icon-btn" title="上一个冲突" :disabled="!canGoUp" @click="goHunk(-1)">
          <Icons name="arrow-up" :size="14" />
        </button>
        <button type="button" class="icon-btn" title="下一个冲突" :disabled="!canGoDown" @click="goHunk(1)">
          <Icons name="arrow-down" :size="14" />
        </button>
        <button
          type="button"
          class="icon-btn"
          title="应用非冲突更改"
          :disabled="busy || !nonConflictCount"
          @click="applyNonConflicts"
        >
          <Icons name="apply-both" :size="16" />
        </button>
        <strong class="remain" :class="{ zero: conflictCount === 0 }">
          {{ conflictCount > 0 ? `还有 ${conflictCount} 处冲突` : "冲突已处理完" }}
        </strong>
        <span class="cover-label">覆盖</span>
        <button
          type="button"
          class="cover-btn"
          :class="{ on: overwrite === 'ours' }"
          :disabled="busy"
          title="用本地版本覆盖全部改动"
          @click="coverAll('ours')"
        >
          本地
        </button>
        <button
          type="button"
          class="cover-btn"
          :class="{ on: overwrite === 'theirs' }"
          :disabled="busy"
          title="用传入版本覆盖全部改动"
          @click="coverAll('theirs')"
        >
          传入
        </button>
        <span class="file-path" :title="path">{{ path }}</span>
        <label class="ws">
          <input v-model="showBlame" type="checkbox" />
          使用 Git 追溯注解
        </label>
        <label class="ws">
          <input v-model="ignoreWhitespace" type="checkbox" />
          忽略空白变更
        </label>
        <span class="counts">{{ hunks.length }}个更改</span>
      </header>

      <p v-if="error" class="err">{{ error }}</p>
      <p v-else-if="loading" class="empty-hint">正在加载三端内容和行历史…</p>
      <div v-else-if="content?.binary" class="empty-hint">二进制文件请返回列表后使用「接受左边」或「接受右边」</div>
      <div v-else class="workspace" @contextmenu="openBlameMenu">
        <section class="col">
          <div class="pane-head ours">本地 · {{ state.ours }} 更改处</div>
          <div ref="oursPane" class="pane scroll" @scroll="syncScroll">
            <div
              v-for="(row, index) in rows"
              :key="`o-${index}`"
              class="line"
              :class="[sideKind(index, 'ours'), { current: hunkOf(index)?.start === hunks[currentHunk]?.start }]"
              :title="cellTitle(row.oursKind, row.oursBlame, row.ours)"
            >
              <span class="no">{{ oursNos[index] }}</span>
              <span v-if="showBlame" class="blame" :title="formatBlame(row.oursBlame)">{{ blameAnno(index, 'ours') }}</span>
              <span class="code">{{ row.ours }}</span>
            </div>
          </div>
        </section>

        <section class="gutter-col">
          <div class="pane-head" />
          <div ref="leftGutter" class="gutter-body" @scroll="syncScroll">
            <div v-for="(_row, index) in rows" :key="`lg-${index}`" class="gutter-row">
              <template v-if="hunkAt(index)?.ours && !sideSettled(hunkAt(index)!, 'ours')">
                <button type="button" class="arrow" title="选择左侧此段" @click.stop="acceptHunk(hunkAt(index)!, 'ours')">»</button>
                <button type="button" class="x" title="舍弃左侧此段" @click.stop="rejectHunk(hunkAt(index)!, 'ours')">×</button>
              </template>
            </div>
          </div>
        </section>

        <section class="col">
          <div class="pane-head result">
            <span>结果</span>
            <span v-if="findOpen" class="findbar">
              <input
                v-model="findQuery"
                placeholder="查找"
                @keydown.enter.prevent="runFind($event.shiftKey)"
                @keydown.escape.prevent="findOpen = false"
              />
              <button type="button" title="上一个" @click="runFind(true)">↑</button>
              <button type="button" title="下一个" @click="runFind(false)">↓</button>
              <em v-if="findHint">{{ findHint }}</em>
            </span>
          </div>
          <div ref="resultPane" class="pane scroll result-pane" @scroll="syncScroll">
            <textarea
              ref="resultEditor"
              class="result-editor"
              spellcheck="false"
              :style="{ minHeight: `${Math.max(rows.length, result.split('\n').length) * 18}px` }"
              @input="onResultInput"
              @keydown="onResultKey"
            />
          </div>
        </section>

        <section class="gutter-col">
          <div class="pane-head" />
          <div ref="rightGutter" class="gutter-body" @scroll="syncScroll">
            <div v-for="(_row, index) in rows" :key="`rg-${index}`" class="gutter-row">
              <template v-if="hunkAt(index)?.theirs && !sideSettled(hunkAt(index)!, 'theirs')">
                <button type="button" class="arrow" title="选择右侧此段" @click.stop="acceptHunk(hunkAt(index)!, 'theirs')">«</button>
                <button type="button" class="x" title="舍弃右侧此段" @click.stop="rejectHunk(hunkAt(index)!, 'theirs')">×</button>
              </template>
            </div>
          </div>
        </section>

        <section class="col">
          <div class="pane-head theirs">传入 · {{ state.theirs }} 更改处</div>
          <div ref="theirsPane" class="pane scroll" @scroll="syncScroll">
            <div
              v-for="(row, index) in rows"
              :key="`t-${index}`"
              class="line"
              :class="[sideKind(index, 'theirs'), { current: hunkOf(index)?.start === hunks[currentHunk]?.start }]"
              :title="cellTitle(row.theirsKind, row.theirsBlame, row.theirs)"
            >
              <span class="no">{{ theirsNos[index] }}</span>
              <span v-if="showBlame" class="blame" :title="formatBlame(row.theirsBlame)">{{ blameAnno(index, 'theirs') }}</span>
              <span class="code">{{ row.theirs }}</span>
            </div>
          </div>
        </section>
      </div>

      <footer class="foot">
        <button type="button" class="secondary-btn" @click="emit('back')">返回列表</button>
        <span class="spacer" />
        <button type="button" class="primary-btn" :disabled="busy || conflictCount > 0" @click="applyCurrent">应用</button>
        <button type="button" class="secondary-btn" :disabled="busy" @click="cancelMerge">取消合并</button>
      </footer>
    </div>

    <div v-if="blameMenu.open" class="ctx-mask" @mousedown="closeBlameMenu">
      <div class="ctx" :style="{ top: `${blameMenu.y}px`, left: `${blameMenu.x}px` }" @mousedown.stop>
        <button type="button" class="ctx-item" @click="toggleBlame">
          <span class="mark">{{ showBlame ? "✓" : "" }}</span>
          使用 Git 追溯注解
        </button>
      </div>
    </div>
    <div v-if="completeOpen" class="complete-mask" @mousedown.self="completeOpen = false">
      <div class="complete-box">
        <h3>已经完成合并</h3>
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

.toolbar,
.foot {
  flex: none;
  display: flex;
  align-items: center;
  gap: 8px;
  min-height: 36px;
  padding: 0 12px;
  border-bottom: 1px solid var(--border);
  background: var(--bg-hover);
}

.foot {
  border-bottom: 0;
  border-top: 1px solid var(--border);
}

.toolbar-btn {
  height: 24px;
  padding: 0 8px;
  border: 1px solid var(--border-soft);
  border-radius: 4px;
  background: var(--bg-input);
}

.remain {
  margin-left: 8px;
  font-size: 18px;
  font-weight: 700;
  color: var(--danger);
  white-space: nowrap;
}

.remain.zero {
  color: var(--ok);
}

.cover-label {
  margin-left: 10px;
  color: var(--text-muted);
}

.cover-btn {
  height: 24px;
  padding: 0 10px;
  border: 1px solid var(--border-soft);
  border-radius: 4px;
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
}

.ws,
.counts {
  color: var(--text-muted);
  white-space: nowrap;
}

.ws {
  display: flex;
  align-items: center;
  gap: 6px;
}

.workspace {
  flex: 1;
  min-height: 0;
  display: grid;
  grid-template-columns: minmax(0, 1fr) 40px minmax(0, 1fr) 40px minmax(0, 1fr);
}

.col,
.gutter-col {
  min-width: 0;
  min-height: 0;
  display: flex;
  flex-direction: column;
}

.pane-head {
  flex: none;
  height: 28px;
  display: flex;
  align-items: center;
  padding: 0 8px;
  color: var(--text-muted);
}

.pane-head.ours { background: var(--diff-ours-head); }
.pane-head.result {
  background: var(--diff-base-head);
  justify-content: space-between;
  gap: 8px;
}

.findbar {
  display: inline-flex;
  align-items: center;
  gap: 4px;
}

.findbar input {
  width: 140px;
  height: 20px;
  padding: 0 6px;
  border: 1px solid var(--border-soft);
  border-radius: 4px;
  background: var(--bg-input);
  color: var(--text);
}

.findbar button {
  height: 20px;
  min-width: 20px;
  color: var(--text-muted);
}

.findbar em {
  color: var(--danger);
  font-style: normal;
}

.result-pane {
  padding: 0;
}

.result-editor {
  display: block;
  box-sizing: border-box;
  width: 100%;
  min-height: 100%;
  margin: 0;
  padding: 0 8px;
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
.pane-head.theirs { background: var(--diff-theirs-head); }

.pane,
.gutter-body {
  flex: 1;
  min-height: 0;
}

.pane {
  overflow: scroll;
  scrollbar-gutter: stable both-edges;
  scrollbar-width: auto;
  background: var(--bg-deep);
  font-family: Consolas, "Courier New", monospace;
  font-size: 12px;
  line-height: 18px;
}

.pane::-webkit-scrollbar {
  width: 12px;
  height: 12px;
}

.pane::-webkit-scrollbar-thumb {
  background: var(--scrollbar);
  border-radius: 8px;
}

.pane::-webkit-scrollbar-track {
  background: var(--bg-hover);
}

.gutter-body {
  overflow: hidden auto;
  scrollbar-width: none;
}

.gutter-body::-webkit-scrollbar {
  width: 0;
  height: 0;
}

.line,
.gutter-row {
  min-height: 18px;
  height: 18px;
}

.line {
  display: flex;
}

.line.current {
  box-shadow: inset 2px 0 0 var(--accent);
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
  width: 118px;
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
}

.line.changed { background: var(--diff-changed); }
.line.added { background: var(--diff-added-bg); }
.line.deleted { background: var(--diff-deleted-bg); }
.line.conflict { background: var(--diff-conflict-bg); }
.line.discarded {
  opacity: 0.4;
  background: transparent;
}

.gutter-row {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 2px;
}

.arrow,
.x {
  width: 18px;
  height: 18px;
  line-height: 16px;
  border-radius: 3px;
  color: var(--accent);
  font-size: 15px;
  cursor: pointer;
}

.arrow:hover,
.arrow.on {
  background: var(--accent);
  color: #fff;
}

.x {
  color: var(--text-muted);
}

.x:hover {
  background: var(--danger);
  color: #fff;
}

.err,
.empty-hint {
  padding: 16px;
}

.err {
  color: var(--danger);
}

.spacer {
  flex: 1;
}

.ctx-mask {
  position: fixed;
  inset: 0;
  z-index: 1380;
}

.ctx {
  position: fixed;
  min-width: 220px;
  padding: 4px 0;
  background: var(--bg-elevated);
  border: 1px solid var(--border-elevated);
  border-radius: 6px;
  box-shadow: var(--shadow);
}

.ctx-item {
  display: flex;
  align-items: center;
  gap: 6px;
  width: 100%;
  min-height: 28px;
  padding: 0 12px;
  text-align: left;
}

.ctx-item:hover {
  background: var(--bg-selected);
}

.mark {
  width: 14px;
  color: var(--ok);
}

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
  padding: 18px 20px;
  border-radius: 8px;
  background: var(--bg-elevated);
  border: 1px solid var(--border-elevated);
  box-shadow: var(--shadow);
}

.complete-box h3 {
  margin: 0 0 8px;
}

.complete-box p {
  margin: 0 0 16px;
  color: var(--text-muted);
}
</style>
