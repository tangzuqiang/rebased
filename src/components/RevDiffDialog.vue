<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { revDiffFile, revDiffFiles, writeWorktreeFile } from "../api";
import { parsePatch } from "../patch";
import {
  applyToLeft,
  buildSegments,
  layoutSegments,
  mapRow,
  reverseLines,
  scrollAnchors,
  type BlockBox,
  type ViewRow,
} from "../splitDiff";
import type { DiffLine, GitFile } from "../types";
import DiffFileTree from "./DiffFileTree.vue";
import Modal from "./Modal.vue";

const ROW_HEIGHT = 18;
const GUTTER_WIDTH = 56;
const FULL_CONTEXT = 1_000_000;
const SETTINGS_KEY = "rebased.diffViewer";

type ViewMode = "split" | "unified";
type Side = "left" | "right";
interface ViewerSettings {
  mode: ViewMode;
  ignoreWhitespace: boolean;
  words: boolean;
  collapse: boolean;
}

function loadSettings(): ViewerSettings {
  const fallback: ViewerSettings = { mode: "split", ignoreWhitespace: false, words: true, collapse: false };
  try {
    return { ...fallback, ...JSON.parse(localStorage.getItem(SETTINGS_KEY) || "{}") };
  } catch {
    return fallback;
  }
}

const props = defineProps<{
  repoPath: string;
  title: string;
  /** 旧版本（补丁的 − 侧）。 */
  from: string;
  /** 新版本（补丁的 + 侧）；空字符串表示工作树。 */
  to: string;
  fromLabel: string;
  toLabel: string;
  /** 把 to 放在左侧、from 放在右侧显示。 */
  reverse?: boolean;
}>();

const emit = defineEmits<{
  close: [];
}>();

const leftLabel = computed(() => (props.reverse ? props.toLabel : props.fromLabel));
const rightLabel = computed(() => (props.reverse ? props.fromLabel : props.toLabel));
const rootName = computed(() => props.repoPath.split(/[\\/]/).filter(Boolean).pop() ?? props.repoPath);

const files = ref<GitFile[]>([]);
const loadingFiles = ref(false);
const listError = ref("");
const filter = ref("");
const active = ref<GitFile | null>(null);
const lines = ref<DiffLine[]>([]);
const binary = ref(false);
const loadingDiff = ref(false);
const diffError = ref("");
const tree = ref<InstanceType<typeof DiffFileTree> | null>(null);
let diffToken = 0;
let pendingBlock = -1;

const porting = ref(false);
const portError = ref("");
const undoStack = ref<Array<{ path: string; content: string }>>([]);

const settings = ref<ViewerSettings>(loadSettings());
const expanded = ref(new Set<number>());
const currentBlock = ref(-1);
const leftPane = ref<HTMLElement | null>(null);
const rightPane = ref<HTMLElement | null>(null);
const unifiedPane = ref<HTMLElement | null>(null);
const leftTop = ref(0);
const rightTop = ref(0);
const viewHeight = ref(0);

const addCount = computed(() => lines.value.filter((line) => line.kind === "add").length);
const delCount = computed(() => lines.value.filter((line) => line.kind === "del").length);

const built = computed(() => buildSegments(lines.value, settings.value.words));
const layout = computed(() => layoutSegments(built.value.segments, settings.value.collapse, expanded.value));
const anchors = computed(() =>
  scrollAnchors(layout.value.boxes, layout.value.left.length, layout.value.right.length),
);
/** 统一视图中每个差异块的起始行。 */
const unifiedStarts = computed(() => {
  const starts: number[] = [];
  lines.value.forEach((line, index) => {
    const changed = line.kind === "add" || line.kind === "del";
    const prev = lines.value[index - 1];
    if (changed && !(prev && (prev.kind === "add" || prev.kind === "del"))) starts.push(index);
  });
  return starts;
});
const blockCount = computed(() =>
  settings.value.mode === "split" ? layout.value.boxes.length : unifiedStarts.value.length,
);

/** 视口内可见的差异块，用于中间连接带和移植箭头。 */
const visibleBoxes = computed(() => {
  const rows = viewHeight.value / ROW_HEIGHT + 2;
  const leftStart = leftTop.value / ROW_HEIGHT - 1;
  const rightStart = rightTop.value / ROW_HEIGHT - 1;
  return layout.value.boxes.filter(
    (box) =>
      (box.l1 >= leftStart && box.l0 <= leftStart + rows) || (box.r1 >= rightStart && box.r0 <= rightStart + rows),
  );
});

watch(settings, (value) => localStorage.setItem(SETTINGS_KEY, JSON.stringify(value)), { deep: true });
watch(
  () => [settings.value.mode, settings.value.ignoreWhitespace],
  () => void loadDiff(active.value),
);

function displayPath(path: string) {
  const parts = path.split(" → ");
  return props.reverse && parts.length === 2 ? `${parts[1]} → ${parts[0]}` : path;
}

function targetPath(file: GitFile) {
  return file.path.split(" → ").pop() ?? file.path;
}

function sidePath(path: string, side: Side) {
  const parts = displayPath(path).split(" → ");
  return side === "left" ? parts[0] : parts[parts.length - 1];
}

function statusLabel(file: GitFile) {
  if (props.reverse) return file.status === "A" ? "D" : file.status === "D" ? "A" : file.status;
  return file.untracked && file.status === "A" ? "U" : file.status;
}

function rowsOf(side: Side) {
  return side === "left" ? layout.value.left : layout.value.right;
}

function rowClass(row: ViewRow) {
  if (row.type !== "line") return row.type;
  return [row.kind === "ctx" ? "" : row.kind, { current: row.block >= 0 && row.block === currentBlock.value }];
}

/** 某一侧行数为 0 的差异块，在该侧画一条插入线。 */
function insertMarks(side: Side) {
  return side === "left" ? layout.value.boxes.filter((box) => box.l0 === box.l1) : [];
}

function expandFold(key: number) {
  expanded.value = new Set([...expanded.value, key]);
}

function setPane(side: Side, el: unknown) {
  const pane = el instanceof HTMLElement ? el : null;
  if (side === "left") leftPane.value = pane;
  else rightPane.value = pane;
  if (pane) viewHeight.value = pane.clientHeight;
}

let scrollDriver: Side | null = null;
let driverTimer = 0;

function onPaneScroll(side: Side) {
  const source = side === "left" ? leftPane.value : rightPane.value;
  const target = side === "left" ? rightPane.value : leftPane.value;
  if (!source) return;
  leftTop.value = leftPane.value?.scrollTop ?? 0;
  rightTop.value = rightPane.value?.scrollTop ?? 0;
  viewHeight.value = source.clientHeight;
  if (scrollDriver && scrollDriver !== side) return;
  scrollDriver = side;
  window.clearTimeout(driverTimer);
  driverTimer = window.setTimeout(() => (scrollDriver = null), 80);
  if (!target) return;
  const half = source.clientHeight / 2;
  const center = (source.scrollTop + half) / ROW_HEIGHT;
  const mapped = mapRow(anchors.value, center, side === "left" ? 0 : 1);
  target.scrollTop = Math.max(0, mapped * ROW_HEIGHT - half);
  target.scrollLeft = source.scrollLeft;
}

function connectorPath(box: BlockBox) {
  const l0 = box.l0 * ROW_HEIGHT - leftTop.value;
  const l1 = box.l1 * ROW_HEIGHT - leftTop.value;
  const r0 = box.r0 * ROW_HEIGHT - rightTop.value;
  const r1 = box.r1 * ROW_HEIGHT - rightTop.value;
  const w = GUTTER_WIDTH;
  const m = w / 2;
  return `M0,${l0} C${m},${l0} ${m},${r0} ${w},${r0} L${w},${r1} C${m},${r1} ${m},${l1} 0,${l1} Z`;
}

function goBlock(delta: number) {
  const count = blockCount.value;
  if (!count) return;
  let target: number;
  if (currentBlock.value < 0) {
    target = delta > 0 ? 0 : count - 1;
    if (settings.value.mode === "split" && delta > 0 && leftPane.value) {
      const top = leftPane.value.scrollTop / ROW_HEIGHT;
      const found = layout.value.boxes.findIndex((box) => box.l0 >= top);
      if (found >= 0) target = found;
    }
  } else {
    target = currentBlock.value + delta;
  }
  jumpTo(target);
}

function jumpTo(target: number) {
  const count = blockCount.value;
  if (!count) return;
  const index = Math.min(count - 1, Math.max(0, target));
  currentBlock.value = index;
  if (settings.value.mode === "split") {
    const box = layout.value.boxes[index];
    scrollDriver = null;
    leftPane.value?.scrollTo({ top: Math.max(0, (box.l0 - 4) * ROW_HEIGHT) });
    rightPane.value?.scrollTo({ top: Math.max(0, (box.r0 - 4) * ROW_HEIGHT) });
  } else {
    unifiedPane.value?.scrollTo({ top: Math.max(0, (unifiedStarts.value[index] - 4) * ROW_HEIGHT) });
  }
}

function onViewerKey(event: KeyboardEvent) {
  const next = event.key === "F7" ? !event.shiftKey : event.altKey && event.key === "ArrowDown";
  const prev = event.key === "F7" ? event.shiftKey : event.altKey && event.key === "ArrowUp";
  if (!next && !prev) return;
  event.preventDefault();
  goBlock(next ? 1 : -1);
}

/** 左侧是工作树时，才能把右侧的代码写进当前分支。 */
const portBlocker = computed(() => {
  if (!props.reverse || props.to !== "") return "左侧不是工作树，不能写入";
  if (settings.value.mode !== "split") return "请切换到并排查看";
  if (settings.value.ignoreWhitespace) return "忽略空白时无法精确移植，请切换为不忽略空白";
  if (built.value.segments.some((segment) => segment.type === "gap")) return "文件过大，差异被截断，无法移植";
  return "";
});
const canWrite = computed(() => props.reverse && props.to === "");
const portable = computed(() => !portBlocker.value && !binary.value && built.value.blockCount > 0);

async function port(block: number | null) {
  const file = active.value;
  if (!file || !portable.value || porting.value) return;
  if (block === null && !window.confirm(`确定用「${rightLabel.value}」的版本覆盖当前工作树中的 ${targetPath(file)}？`)) return;
  porting.value = true;
  portError.value = "";
  const segments = built.value.segments;
  const before = applyToLeft(segments, new Set());
  const after = applyToLeft(segments, block === null ? null : new Set([block]));
  try {
    await writeWorktreeFile(props.repoPath, targetPath(file), after);
    undoStack.value.push({ path: file.path, content: before });
    pendingBlock = block ?? 0;
    await reloadKeeping(file.path);
  } catch (error) {
    portError.value = error instanceof Error ? error.message : String(error);
  } finally {
    porting.value = false;
  }
}

async function undoPort() {
  const last = undoStack.value[undoStack.value.length - 1];
  if (!last || porting.value) return;
  porting.value = true;
  portError.value = "";
  try {
    await writeWorktreeFile(props.repoPath, last.path.split(" → ").pop() ?? last.path, last.content);
    undoStack.value.pop();
    pendingBlock = Math.max(0, currentBlock.value);
    await reloadKeeping(last.path);
  } catch (error) {
    portError.value = error instanceof Error ? error.message : String(error);
  } finally {
    porting.value = false;
  }
}

async function reloadKeeping(path: string) {
  const list = await revDiffFiles(props.repoPath, props.from, props.to);
  const index = files.value.findIndex((file) => file.path === path);
  files.value = list;
  const same = list.find((file) => file.path === path);
  if (!same) pendingBlock = -1;
  active.value = same ?? list[Math.min(Math.max(index, 0), list.length - 1)] ?? null;
}

async function loadFiles() {
  loadingFiles.value = true;
  listError.value = "";
  files.value = [];
  active.value = null;
  try {
    files.value = await revDiffFiles(props.repoPath, props.from, props.to);
    active.value = files.value[0] ?? null;
  } catch (error) {
    listError.value = error instanceof Error ? error.message : String(error);
  } finally {
    loadingFiles.value = false;
  }
}

async function loadDiff(file: GitFile | null) {
  const token = ++diffToken;
  lines.value = [];
  binary.value = false;
  diffError.value = "";
  expanded.value = new Set();
  currentBlock.value = -1;
  if (!file) return;
  loadingDiff.value = true;
  try {
    const diff = await revDiffFile(props.repoPath, props.from, props.to, file.path, file.untracked, {
      context: settings.value.mode === "split" ? FULL_CONTEXT : 3,
      ignoreWhitespace: settings.value.ignoreWhitespace,
    });
    if (token !== diffToken) return;
    binary.value = diff.binary;
    const parsed = diff.binary ? [] : parsePatch(diff.patch);
    lines.value = props.reverse ? reverseLines(parsed) : parsed;
  } catch (error) {
    if (token !== diffToken) return;
    diffError.value = error instanceof Error ? error.message : String(error);
  } finally {
    if (token === diffToken) loadingDiff.value = false;
  }
  const target = pendingBlock;
  pendingBlock = -1;
  if (token !== diffToken || !lines.value.length) return;
  await nextTick();
  leftTop.value = 0;
  rightTop.value = 0;
  if (target >= 0) jumpTo(target);
  else goBlock(1);
}

function onResize() {
  viewHeight.value = (leftPane.value ?? unifiedPane.value)?.clientHeight ?? viewHeight.value;
}

onMounted(() => window.addEventListener("resize", onResize));
onBeforeUnmount(() => {
  window.removeEventListener("resize", onResize);
  window.clearTimeout(driverTimer);
});

watch(() => [props.repoPath, props.from, props.to], () => void loadFiles(), { immediate: true });
watch(active, (file) => void loadDiff(file));
</script>

<template>
  <Modal :title="title" wide flush allow-fullscreen no-footer @close="emit('close')">
    <div class="rev-diff">
      <aside class="left">
        <div class="direction">
          <span class="side old" :title="leftLabel">{{ leftLabel }}</span>
          <span class="arrow">↔</span>
          <span class="side new" :title="rightLabel">{{ rightLabel }}</span>
        </div>
        <div class="tree-tools">
          <input
            v-model="filter"
            class="filter"
            placeholder="筛选文件"
            @keydown.down.prevent="tree?.move(1)"
            @keydown.up.prevent="tree?.move(-1)"
          />
          <button type="button" class="tool" title="全部展开" @click="tree?.expandAll()">⊞</button>
          <button type="button" class="tool" title="全部折叠" @click="tree?.collapseAll()">⊟</button>
        </div>
        <div class="summary">
          {{ loadingFiles ? "正在比较…" : `${files.length} 个文件有差异` }}
        </div>
        <div class="scroll files">
          <div v-if="listError" class="empty-hint">{{ listError }}</div>
          <div v-else-if="!loadingFiles && !files.length" class="empty-hint">没有差异</div>
          <DiffFileTree
            v-else-if="files.length"
            ref="tree"
            :files="files"
            :active-path="active?.path"
            :root-name="rootName"
            :filter="filter"
            :status-of="statusLabel"
            :path-of="(file) => targetPath(file)"
            @select="active = $event"
          />
        </div>
      </aside>

      <section class="diff" tabindex="-1" @keydown="onViewerKey">
        <div class="toolbar">
          <button type="button" class="tool" title="上一处差异（Shift+F7）" :disabled="!blockCount" @click="goBlock(-1)">↑</button>
          <button type="button" class="tool" title="下一处差异（F7）" :disabled="!blockCount" @click="goBlock(1)">↓</button>
          <span class="sep" />
          <select v-model="settings.mode" class="tool-select" title="查看方式">
            <option value="split">并排查看</option>
            <option value="unified">统一查看</option>
          </select>
          <select v-model="settings.ignoreWhitespace" class="tool-select" title="空白处理">
            <option :value="false">不忽略空白</option>
            <option :value="true">忽略空白</option>
          </select>
          <label v-if="settings.mode === 'split'" class="tool-check">
            <input v-model="settings.words" type="checkbox" /> 高亮单词
          </label>
          <label v-if="settings.mode === 'split'" class="tool-check">
            <input v-model="settings.collapse" type="checkbox" /> 折叠未更改
          </label>
          <template v-if="canWrite">
            <span class="sep" />
            <button
              type="button"
              class="tool-btn"
              :disabled="!portable || porting"
              :title="portBlocker || `用「${rightLabel}」的整个文件覆盖工作树`"
              @click="port(null)"
            >« 全部移植</button>
            <button
              type="button"
              class="tool-btn"
              :disabled="!undoStack.length || porting"
              title="撤销上一次移植"
              @click="undoPort"
            >撤销</button>
            <span v-if="portError" class="port-error" :title="portError">{{ portError }}</span>
            <span v-else-if="portBlocker && lines.length" class="muted hint" :title="portBlocker">{{ portBlocker }}</span>
          </template>
          <span class="muted count">
            <template v-if="blockCount">
              {{ currentBlock >= 0 ? `${currentBlock + 1}/` : "" }}{{ blockCount }} 处差异 ·
            </template>
            <span class="plus">+{{ addCount }}</span> <span class="minus">−{{ delCount }}</span>
          </span>
        </div>
        <div v-if="settings.mode === 'split' && active && !binary" class="pane-heads">
          <div class="pane-head old" :title="`${leftLabel}  ${sidePath(active.path, 'left')}`">
            <strong>{{ leftLabel }}</strong><span>{{ sidePath(active.path, "left") }}</span>
          </div>
          <div class="pane-head-gap" />
          <div class="pane-head new" :title="`${rightLabel}  ${sidePath(active.path, 'right')}`">
            <strong>{{ rightLabel }}</strong><span>{{ sidePath(active.path, "right") }}</span>
          </div>
        </div>
        <header v-else>
          <span class="path" :title="active ? displayPath(active.path) : ''">{{ active ? displayPath(active.path) : "差异预览" }}</span>
        </header>

        <div v-if="loadingDiff || diffError || !active || binary || !lines.length" class="diff-body">
          <div v-if="loadingDiff" class="empty-hint">正在加载差异…</div>
          <div v-else-if="diffError" class="empty-hint">{{ diffError }}</div>
          <div v-else-if="!active" class="empty-hint">选择一个文件查看差异</div>
          <div v-else-if="binary" class="empty-hint">二进制文件，无法预览差异</div>
          <div v-else class="empty-hint">
            {{ settings.ignoreWhitespace ? "忽略空白后内容相同" : "内容相同（可能只有文件权限变化）" }}
          </div>
        </div>
        <div v-else-if="settings.mode === 'split'" class="diff-body split" :style="{ '--gutter': `${GUTTER_WIDTH}px` }">
          <template v-for="side in (['left', 'right'] as const)" :key="side">
            <div
              :ref="(el) => setPane(side, el)"
              class="pane code-font"
              :class="side"
              @scroll="onPaneScroll(side)"
            >
              <div class="pane-inner">
                <div v-for="(row, index) in rowsOf(side)" :key="index" class="srow" :class="rowClass(row)">
                  <template v-if="row.type === 'line'">
                    <span class="no">{{ row.no }}</span>
                    <span class="code"><template v-if="row.parts"><span
                          v-for="(part, partIndex) in row.parts"
                          :key="partIndex"
                          :class="{ word: part.changed }"
                        >{{ part.text }}</span></template><template v-else>{{ row.text }}</template></span>
                  </template>
                  <button v-else-if="row.type === 'fold'" type="button" class="fold" @click="expandFold(row.key)">
                    ⋯ {{ row.count }} 行未更改，点击展开
                  </button>
                  <span v-else class="gap">{{ row.text }}</span>
                </div>
                <div
                  v-for="box in insertMarks(side)"
                  :key="`m${box.block}`"
                  class="insert-mark"
                  :class="[box.kind, { current: box.block === currentBlock }]"
                  :style="{ top: `${(side === 'left' ? box.l0 : box.r0) * ROW_HEIGHT - 1}px` }"
                />
              </div>
            </div>
            <div v-if="side === 'left'" class="gutter">
              <svg :width="GUTTER_WIDTH" height="100%">
                <path
                  v-for="box in visibleBoxes.filter((item) => item.kind !== 'del')"
                  :key="box.block"
                  :d="connectorPath(box)"
                  class="band"
                  :class="[box.kind, { current: box.block === currentBlock }]"
                />
              </svg>
              <template v-if="portable">
                <button
                  v-for="box in visibleBoxes.filter((item) => item.kind !== 'del')"
                  :key="`p${box.block}`"
                  type="button"
                  class="port"
                  :disabled="porting"
                  :style="{ top: `${box.r0 * ROW_HEIGHT - rightTop}px` }"
                  title="把右侧这处代码移植到左侧（写入当前工作树）"
                  @click="port(box.block)"
                >«</button>
              </template>
            </div>
          </template>
        </div>
        <div v-else ref="unifiedPane" class="scroll diff-body">
          <div class="patch code-font">
            <div
              v-for="(line, index) in lines"
              :key="index"
              class="dline"
              :class="[line.kind, { current: unifiedStarts[currentBlock] === index }]"
            >
              <span class="no">{{ line.oldNo }}</span>
              <span class="no">{{ line.newNo }}</span>
              <span class="mark">{{ line.kind === "add" ? "+" : line.kind === "del" ? "-" : line.kind === "hunk" ? "" : " " }}</span>
              <span class="code">{{ line.text }}</span>
            </div>
          </div>
        </div>
      </section>
    </div>
  </Modal>
</template>

<style scoped>
.rev-diff {
  flex: 1;
  min-height: 0;
  display: grid;
  grid-template-columns: minmax(240px, 22%) 1fr;
}

.left,
.diff {
  min-width: 0;
  min-height: 0;
  display: flex;
  flex-direction: column;
}

.left {
  border-right: 1px solid var(--border);
}

.direction {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 10px 12px 6px;
  min-width: 0;
}

.side {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-weight: 600;
}

.side.old { color: #e6c07b; }
.side.new { color: #7cb7ff; }

.arrow {
  color: var(--text-dim);
}

.tree-tools {
  display: flex;
  align-items: center;
  gap: 2px;
  padding: 0 8px 6px 12px;
}

.filter {
  flex: 1;
  min-width: 0;
  height: 26px;
  padding: 0 8px;
  border: 1px solid var(--border-soft);
  border-radius: 4px;
  background: var(--bg-input);
}

.summary {
  padding: 0 12px 4px;
  color: var(--text-dim);
}

.files {
  flex: 1;
  min-height: 0;
}

.diff {
  outline: none;
}

.toolbar {
  height: 34px;
  padding: 0 8px;
  display: flex;
  align-items: center;
  gap: 6px;
  border-bottom: 1px solid var(--border);
  white-space: nowrap;
}

.tool {
  width: 26px;
  height: 24px;
  flex: none;
  border-radius: 4px;
  font-size: 14px;
}

.tool:hover:not(:disabled) {
  background: var(--bg-selected);
}

.tool:disabled {
  opacity: 0.4;
}

.sep {
  width: 1px;
  height: 18px;
  margin: 0 4px;
  background: var(--border);
}

.tool-select {
  height: 24px;
  padding: 0 6px;
  border: 1px solid var(--border-soft);
  border-radius: 4px;
  background: var(--bg-input);
}

.tool-check {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  margin-left: 4px;
}

.tool-btn {
  height: 24px;
  padding: 0 8px;
  border: 1px solid var(--border-soft);
  border-radius: 4px;
}

.tool-btn:hover:not(:disabled) {
  background: var(--bg-selected);
}

.tool-btn:disabled {
  opacity: 0.45;
}

.port-error,
.hint {
  min-width: 0;
  max-width: 260px;
  overflow: hidden;
  text-overflow: ellipsis;
}

.port-error {
  color: var(--danger);
}

.count {
  margin-left: auto;
}

.plus { color: #8fd19e; }
.minus { color: #e8a0a4; }

.diff header,
.pane-heads {
  height: 28px;
  border-bottom: 1px solid var(--border);
}

.diff header {
  padding: 0 12px;
  display: flex;
  align-items: center;
}

.pane-heads {
  display: grid;
  grid-template-columns: 1fr 56px 1fr;
}

.pane-head {
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 0 10px;
  overflow: hidden;
  white-space: nowrap;
}

.pane-head span {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  color: var(--text-dim);
}

.pane-head.old strong { color: #e6c07b; }
.pane-head.new strong { color: #7cb7ff; }

.path {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.diff-body {
  flex: 1;
  min-height: 0;
  background: var(--bg-deep);
}

.code-font {
  font-family: Consolas, "Courier New", monospace;
  font-size: 12px;
  line-height: 18px;
}

.split {
  display: grid;
  grid-template-columns: minmax(0, 1fr) var(--gutter) minmax(0, 1fr);
}

.pane {
  min-width: 0;
  min-height: 0;
  overflow: auto;
}

.pane-inner {
  position: relative;
  width: max-content;
  min-width: 100%;
}

.gutter {
  position: relative;
  overflow: hidden;
  border-left: 1px solid var(--border);
  border-right: 1px solid var(--border);
  background: var(--bg-deep);
}

.gutter svg {
  position: absolute;
  inset: 0;
}

.band { stroke-width: 1; }
.band.mod { fill: rgba(70, 120, 190, 0.35); stroke: rgba(110, 160, 230, 0.7); }
.band.add { fill: rgba(70, 150, 90, 0.35); stroke: rgba(110, 200, 130, 0.7); }
.band.del { fill: rgba(150, 150, 150, 0.25); stroke: rgba(180, 180, 180, 0.6); }
.band.current { stroke-width: 2; }

.port {
  position: absolute;
  right: 2px;
  width: 22px;
  height: 18px;
  padding: 0;
  border: none;
  background: transparent;
  color: #9a9da3;
  font-size: 20px;
  font-weight: 700;
  line-height: 14px;
  letter-spacing: -2px;
}

.port:hover:not(:disabled) {
  color: #e8eaed;
}

.port:disabled {
  opacity: 0.4;
}

.srow {
  height: 18px;
  display: flex;
  white-space: pre;
}

.srow .no {
  position: sticky;
  left: 0;
  flex: none;
  width: 48px;
  background: var(--bg-deep);
  border-right: 1px solid var(--border-soft);
}

.srow .code {
  flex: 1;
  padding-left: 8px;
}

.srow.del { background: rgba(130, 130, 130, 0.22); }
.srow.add { background: #1f3d28; }
.srow.mod { background: #22324a; }
.srow.del .word { background: rgba(170, 170, 170, 0.35); border-radius: 2px; }
.srow.add .word { background: #2f6b3d; border-radius: 2px; }
.srow.mod .word { background: #2f5e8a; border-radius: 2px; }
.srow.current .no { box-shadow: inset 2px 0 0 var(--accent, #4a8cff); }

.insert-mark {
  position: absolute;
  left: 0;
  right: 0;
  height: 2px;
  pointer-events: none;
}

.insert-mark.add { background: rgba(110, 200, 130, 0.8); }
.insert-mark.del { background: rgba(180, 180, 180, 0.7); }
.insert-mark.current { height: 3px; }

.srow.fold,
.srow.gap {
  background: #243246;
  color: #7cb7ff;
}

.fold {
  width: 100%;
  padding-left: 56px;
  text-align: left;
  color: inherit;
}

.gap {
  padding-left: 56px;
}

.patch .dline {
  display: grid;
  grid-template-columns: 42px 42px 16px 1fr;
  height: 18px;
  white-space: pre;
}

.no,
.mark {
  color: #6f737a;
  text-align: right;
  padding-right: 6px;
  user-select: none;
}

.code {
  padding-right: 12px;
}

.dline.add { background: #1f3d28; }
.dline.del { background: #3d2224; }
.dline.hunk { color: #7cb7ff; background: #243246; }
.dline.current { box-shadow: inset 2px 0 0 var(--accent, #4a8cff); }
</style>
