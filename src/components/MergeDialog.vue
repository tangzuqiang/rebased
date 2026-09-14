<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { getMergeFile, getMergePreview } from "../api";
import { alignDiff3 } from "../diff3";
import type { BranchInfo, Diff3Row, MergeFileContent, MergePreview, RepoInfo } from "../types";
import BranchFilterList from "./BranchFilterList.vue";
import Modal from "./Modal.vue";

const props = defineProps<{
  repo: RepoInfo;
  branches: BranchInfo[];
  busy: boolean;
}>();

const emit = defineEmits<{
  close: [];
  merge: [theirs: string];
}>();

const theirs = ref("");
const pickerOpen = ref(false);
const preview = ref<MergePreview | null>(null);
const selectedPath = ref("");
const content = ref<MergeFileContent | null>(null);
const loading = ref(false);
const error = ref("");
const scrollLeft = ref<HTMLElement | null>(null);
const scrollBase = ref<HTMLElement | null>(null);
const scrollRight = ref<HTMLElement | null>(null);

const exclude = computed(() => [props.repo.branch]);

function chooseTheirs(name: string) {
  theirs.value = name;
  pickerOpen.value = false;
}

function onDocMouseDown(event: MouseEvent) {
  const target = event.target as HTMLElement;
  if (!target.closest(".picker")) {
    pickerOpen.value = false;
  }
}

onMounted(() => document.addEventListener("mousedown", onDocMouseDown));
onUnmounted(() => document.removeEventListener("mousedown", onDocMouseDown));

const rows = computed<Diff3Row[]>(() => {
  if (!content.value || content.value.binary) return [];
  const ours = content.value.ours;
  const base = content.value.base;
  const incoming = content.value.theirs;
  const total = ours.split("\n").length + base.split("\n").length + incoming.split("\n").length;
  if (total > 6000) {
    const max = Math.max(ours.split("\n").length, base.split("\n").length, incoming.split("\n").length);
    const o = ours.split("\n");
    const b = base.split("\n");
    const t = incoming.split("\n");
    return Array.from({ length: max }, (_, index) => ({
      ours: o[index] ?? "",
      base: b[index] ?? "",
      theirs: t[index] ?? "",
      oursKind: "same" as const,
      baseKind: "same" as const,
      theirsKind: "same" as const,
    }));
  }
  return alignDiff3(ours, base, incoming);
});

const selectedFile = computed(() => preview.value?.files.find((file) => file.path === selectedPath.value));

async function loadPreview() {
  if (!theirs.value) {
    preview.value = null;
    content.value = null;
    return;
  }
  loading.value = true;
  error.value = "";
  try {
    preview.value = await getMergePreview(props.repo.path, theirs.value);
    selectedPath.value = preview.value.files[0]?.path ?? "";
    await loadFile();
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err);
    preview.value = null;
    content.value = null;
  } finally {
    loading.value = false;
  }
}

async function loadFile() {
  if (!theirs.value || !selectedPath.value) {
    content.value = null;
    return;
  }
  loading.value = true;
  error.value = "";
  try {
    content.value = await getMergeFile(props.repo.path, theirs.value, selectedPath.value);
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err);
    content.value = null;
  } finally {
    loading.value = false;
  }
}

function fileName(path: string) {
  return path.split(/[\\/]/).pop() ?? path;
}

function syncScroll(source: HTMLElement, targets: Array<HTMLElement | null>) {
  for (const target of targets) {
    if (!target || target === source) continue;
    target.scrollTop = source.scrollTop;
    target.scrollLeft = source.scrollLeft;
  }
}

watch(theirs, () => {
  void loadPreview();
});

watch(selectedPath, () => {
  void loadFile();
});
</script>

<template>
  <Modal title="合并（三端对比）" wide @close="emit('close')">
    <div class="merge">
      <div class="toolbar">
        <label>
          当前分支
          <strong>{{ repo.branch }}</strong>
        </label>
        <div class="grow picker">
          <span>合并入</span>
          <button type="button" class="picker-value" @click="pickerOpen = !pickerOpen">
            {{ theirs || "选择要合并的分支" }}
          </button>
          <div v-if="pickerOpen" class="picker-panel">
            <BranchFilterList
              :branches="branches"
              :exclude="exclude"
              :selected="theirs"
              :enable-actions="false"
              :default-open="['local', 'remote']"
              @select="chooseTheirs"
            />
          </div>
        </div>
        <span v-if="preview" class="muted">
          共同祖先 {{ preview.baseShort }} · {{ preview.files.length }} 个文件
        </span>
      </div>

      <p v-if="error" class="err">{{ error }}</p>
      <p v-else-if="loading" class="muted">正在加载三端内容…</p>

      <div v-if="preview" class="workspace">
        <aside class="files">
          <header>变更文件</header>
          <button
            v-for="file in preview.files"
            :key="file.path"
            type="button"
            :class="{ selected: file.path === selectedPath, conflict: file.conflict }"
            :title="file.path"
            @click="selectedPath = file.path"
          >
            <span class="file-status" :class="file.status">{{ file.status }}</span>
            <span>{{ fileName(file.path) }}</span>
          </button>
          <div v-if="!preview.files.length" class="empty-hint">已与目标分支一致</div>
        </aside>

        <div class="panes">
          <div class="pane-head ours">本地 · {{ preview.ours }}</div>
          <div class="pane-head base">共同祖先 · {{ preview.baseShort }}</div>
          <div class="pane-head theirs">传入 · {{ preview.theirs }}</div>

          <template v-if="content?.binary">
            <div class="empty-hint span3">该文件是二进制，无法文本对比</div>
          </template>
          <template v-else>
            <div
              ref="scrollLeft"
              class="pane"
              @scroll="syncScroll($event.currentTarget as HTMLElement, [scrollBase, scrollRight])"
            >
              <div
                v-for="(row, index) in rows"
                :key="`o-${index}`"
                class="line"
                :class="row.oursKind"
              >
                <span class="ln">{{ index + 1 }}</span>
                <span class="code">{{ row.ours }}</span>
              </div>
            </div>
            <div
              ref="scrollBase"
              class="pane"
              @scroll="syncScroll($event.currentTarget as HTMLElement, [scrollLeft, scrollRight])"
            >
              <div
                v-for="(row, index) in rows"
                :key="`b-${index}`"
                class="line"
                :class="row.baseKind"
              >
                <span class="ln">{{ index + 1 }}</span>
                <span class="code">{{ row.base }}</span>
              </div>
            </div>
            <div
              ref="scrollRight"
              class="pane"
              @scroll="syncScroll($event.currentTarget as HTMLElement, [scrollLeft, scrollBase])"
            >
              <div
                v-for="(row, index) in rows"
                :key="`t-${index}`"
                class="line"
                :class="row.theirsKind"
              >
                <span class="ln">{{ index + 1 }}</span>
                <span class="code">{{ row.theirs }}</span>
              </div>
            </div>
          </template>
        </div>
      </div>

      <p v-if="selectedFile?.conflict" class="warn">
        该文件两侧都有修改，合并时可能产生冲突。
      </p>
    </div>
    <template #footer>
      <button type="button" class="secondary-btn" @click="emit('close')">取消</button>
      <button
        type="button"
        class="primary-btn"
        :disabled="busy || !theirs || loading"
        @click="emit('merge', theirs)"
      >
        执行合并
      </button>
    </template>
  </Modal>
</template>

<style scoped>
.merge {
  height: 100%;
  display: flex;
  flex-direction: column;
  gap: 10px;
  min-height: 0;
}

.toolbar {
  display: flex;
  align-items: center;
  gap: 16px;
}

.grow {
  flex: 1;
  min-width: 0;
}

label,
.picker {
  display: flex;
  align-items: center;
  gap: 8px;
}

.picker {
  position: relative;
}

.picker-value {
  min-width: 280px;
  max-width: 420px;
  height: 28px;
  padding: 0 10px;
  border: 1px solid var(--border-soft);
  background: var(--bg-input);
  border-radius: 4px;
  text-align: left;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.picker-panel {
  position: absolute;
  top: calc(100% + 4px);
  left: 48px;
  z-index: 5;
  width: min(420px, 70vw);
  max-height: 360px;
  overflow: auto;
  background: #3c3f41;
  border: 1px solid #54575b;
  border-radius: 6px;
  box-shadow: var(--shadow);
}

.workspace {
  flex: 1;
  min-height: 0;
  display: grid;
  grid-template-columns: 220px 1fr;
  border: 1px solid var(--border-soft);
  border-radius: 6px;
  overflow: hidden;
}

.files {
  border-right: 1px solid var(--border);
  background: #2b2d30;
  overflow: auto;
}

.files header {
  padding: 8px 10px;
  color: var(--text-muted);
}

.files button {
  width: 100%;
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 4px 10px;
  text-align: left;
}

.files button.selected {
  background: var(--bg-selected);
}

.files button.conflict {
  color: #e0b0ff;
}

.panes {
  display: grid;
  grid-template-columns: 1fr 1fr 1fr;
  grid-template-rows: 28px 1fr;
  min-width: 0;
  min-height: 0;
}

.pane-head {
  display: flex;
  align-items: center;
  padding: 0 8px;
  border-bottom: 1px solid var(--border);
  color: var(--text-muted);
}

.pane-head.ours { background: #243246; }
.pane-head.base { background: #2d2f33; }
.pane-head.theirs { background: #2a3a2c; }

.pane {
  overflow: auto;
  background: #1e1f22;
  border-right: 1px solid var(--border);
  font-family: Consolas, "Courier New", monospace;
  font-size: 12px;
  line-height: 18px;
}

.pane:last-child {
  border-right: 0;
}

.line {
  display: flex;
  min-height: 18px;
  white-space: pre;
}

.ln {
  width: 42px;
  flex: none;
  padding: 0 6px;
  color: #6f737a;
  text-align: right;
  user-select: none;
}

.code {
  flex: 1;
  padding-right: 8px;
}

.line.changed { background: #3d3420; }
.line.added { background: #1f3d28; }
.line.deleted { background: #3d2224; }
.line.conflict { background: #4a2a4a; }
.line.empty { background: transparent; }

.span3 {
  grid-column: 1 / -1;
}

.err {
  color: var(--danger);
  margin: 0;
}

.warn {
  margin: 0;
  color: #e0b0ff;
}
</style>
