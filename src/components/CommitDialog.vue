<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { generateCommitMessage, getWorkingDiff } from "../api";
import { parsePatch } from "../patch";
import type { DiffLine, GitFile, RepoInfo } from "../types";
import Icons from "./Icons.vue";
import Modal from "./Modal.vue";

const props = defineProps<{
  repo: RepoInfo;
  changes: GitFile[];
  unversioned: GitFile[];
  busy: boolean;
  startAmend?: boolean;
}>();

const emit = defineEmits<{
  close: [];
  commit: [payload: { message: string; amend: boolean; paths: string[]; push: boolean }];
}>();

const tab = ref<"commit" | "shelve">("commit");
const message = ref("");
const amend = ref(Boolean(props.startAmend));
const selected = ref<string[]>([]);
const activePath = ref("");
const activeUntracked = ref(false);
const lines = ref<DiffLine[]>([]);
const binary = ref(false);
const loadingDiff = ref(false);
const diffError = ref("");
const generating = ref(false);
const generateHint = ref("");
const submitting = ref<"" | "commit" | "commit-push">("");
const locked = computed(() => props.busy || Boolean(submitting.value));

const allFiles = computed(() => [
  ...props.changes.map((file) => ({ ...file, untracked: false })),
  ...props.unversioned,
]);

watch(
  allFiles,
  (files) => {
    const paths = files.map((file) => file.path);
    selected.value = selected.value.filter((path) => paths.includes(path));
    if (!selected.value.length) {
      selected.value = props.changes.map((file) => file.path);
    }
    if (!activePath.value || !paths.includes(activePath.value)) {
      const first = files[0];
      activePath.value = first?.path ?? "";
      activeUntracked.value = first?.untracked ?? false;
    }
  },
  { immediate: true },
);

const included = computed(() => selected.value);
const changeChecked = computed(() =>
  props.changes.length > 0 && props.changes.every((file) => selected.value.includes(file.path)),
);
const unversionedChecked = computed(() =>
  props.unversioned.length > 0 && props.unversioned.every((file) => selected.value.includes(file.path)),
);
const addCount = computed(() => lines.value.filter((line) => line.kind === "add").length);
const delCount = computed(() => lines.value.filter((line) => line.kind === "del").length);

function toggle(path: string) {
  selected.value = selected.value.includes(path)
    ? selected.value.filter((item) => item !== path)
    : [...selected.value, path];
}

function toggleGroup(files: GitFile[], checked: boolean) {
  const paths = files.map((file) => file.path);
  selected.value = checked
    ? [...new Set([...selected.value, ...paths])]
    : selected.value.filter((path) => !paths.includes(path));
}

function fileName(path: string) {
  return path.split(/[\\/]/).pop() ?? path;
}

function dirName(path: string) {
  const parts = path.split(/[\\/]/);
  return parts.length > 1 ? parts.slice(0, -1).join("/") : "";
}

async function preview(file: GitFile) {
  activePath.value = file.path;
  activeUntracked.value = file.untracked;
}

async function loadDiff() {
  if (!activePath.value) {
    lines.value = [];
    return;
  }
  loadingDiff.value = true;
  diffError.value = "";
  try {
    const diff = await getWorkingDiff(props.repo.path, activePath.value, activeUntracked.value);
    binary.value = diff.binary;
    lines.value = diff.binary ? [] : parsePatch(diff.patch);
  } catch (error) {
    diffError.value = error instanceof Error ? error.message : String(error);
    lines.value = [];
  } finally {
    loadingDiff.value = false;
  }
}

async function generateAi() {
  if (!included.value.length) {
    generateHint.value = "请先勾选要提交的文件";
    return;
  }
  generating.value = true;
  generateHint.value = "正在生成提交信息…";
  try {
    const known = new Map(allFiles.value.map((file) => [file.path, file.untracked]));
    const result = await generateCommitMessage(
      props.repo.path,
      included.value.map((path) => ({
        path,
        untracked: known.get(path) ?? false,
      })),
    );
    message.value = result.message;
    generateHint.value =
      result.source === "ai"
        ? `已由 ${result.model || "AI"} 根据 ${included.value.length} 个文件的差异生成，可继续修改`
        : result.message;
  } catch (error) {
    generateHint.value = error instanceof Error ? error.message : String(error);
  } finally {
    generating.value = false;
  }
}

function submit(push: boolean) {
  if (locked.value) return;
  submitting.value = push ? "commit-push" : "commit";
  emit("commit", {
    message: message.value,
    amend: amend.value,
    paths: included.value,
    push,
  });
}

watch(
  () => props.busy,
  (busy, wasBusy) => {
    if (!busy && wasBusy) submitting.value = "";
  },
);

watch([activePath, activeUntracked], () => {
  void loadDiff();
});
</script>

<template>
  <Modal
    :title="`提交 - ${repo.name}`"
    wide
    flush
    allow-fullscreen
    no-footer
    @close="emit('close')"
  >
    <div class="commit-ui">
      <aside class="left">
        <div class="tabs">
          <button type="button" :class="{ active: tab === 'commit' }" @click="tab = 'commit'">提交</button>
          <button type="button" :class="{ active: tab === 'shelve' }" @click="tab = 'shelve'">储藏</button>
        </div>

        <div v-if="tab === 'shelve'" class="empty-hint">储藏功能暂未纳入本次基础能力</div>
        <template v-else>
          <div class="scroll files">
            <label class="group">
              <input
                type="checkbox"
                :checked="changeChecked"
                :disabled="!changes.length"
                @change="toggleGroup(changes, ($event.target as HTMLInputElement).checked)"
              />
              更改 {{ changes.length }} 个文件
            </label>
            <div v-if="!changes.length" class="empty-hint short">没有已跟踪的变更</div>
            <button
              v-for="file in changes"
              :key="file.path + file.staged"
              type="button"
              class="file"
              :class="{ active: file.path === activePath }"
              @click="preview(file)"
            >
              <input
                type="checkbox"
                :checked="selected.includes(file.path)"
                @click.stop
                @change="toggle(file.path)"
              />
              <span class="file-status" :class="file.status">{{ file.status }}</span>
              <span class="name" :title="file.path">{{ fileName(file.path) }}</span>
              <span class="dir">{{ dirName(file.path) }}</span>
            </button>

            <label class="group">
              <input
                type="checkbox"
                :checked="unversionedChecked"
                :disabled="!unversioned.length"
                @change="toggleGroup(unversioned, ($event.target as HTMLInputElement).checked)"
              />
              未跟踪 {{ unversioned.length }} 个文件
            </label>
            <div v-if="!unversioned.length" class="empty-hint short">没有未跟踪文件</div>
            <button
              v-for="file in unversioned"
              :key="file.path"
              type="button"
              class="file"
              :class="{ active: file.path === activePath }"
              @click="preview(file)"
            >
              <input
                type="checkbox"
                :checked="selected.includes(file.path)"
                @click.stop
                @change="toggle(file.path)"
              />
              <span class="file-status U">U</span>
              <span class="name" :title="file.path">{{ fileName(file.path) }}</span>
              <span class="dir">{{ dirName(file.path) }}</span>
            </button>
          </div>

          <div class="composer">
            <div class="amend-row">
              <label class="amend">
                <input v-model="amend" type="checkbox" />
                修正(M)
              </label>
              <button
                type="button"
                class="ai-btn"
                :disabled="generating || busy || !included.length"
                title="根据勾选文件的差异生成提交说明"
                @click="generateAi"
              >
                <Icons name="sparkle" :size="13" />
                {{ generating ? "正在生成…" : "AI 生成提交信息" }}
              </button>
            </div>
            <p v-if="generateHint" class="hint">{{ generateHint }}</p>
            <textarea
              v-model="message"
              :placeholder="amend ? '留空则沿用上次提交说明' : '提交消息'"
            />
            <div class="actions">
              <button
                type="button"
                class="secondary-btn"
                :class="{ 'is-loading': submitting === 'commit' }"
                :disabled="locked || (!included.length && !amend)"
                @click="submit(false)"
              >
                <span v-if="submitting === 'commit'" class="btn-spinner" />
                {{ submitting === "commit" ? "正在提交…" : "提交" }}
              </button>
              <button
                type="button"
                class="primary-btn"
                :class="{ 'is-loading': submitting === 'commit-push' }"
                :disabled="locked || (!included.length && !amend)"
                @click="submit(true)"
              >
                <span v-if="submitting === 'commit-push'" class="btn-spinner" />
                {{ submitting === "commit-push" ? "正在提交并推送…" : "提交并推送" }}
              </button>
            </div>
          </div>
        </template>
      </aside>

      <section class="diff">
        <header>
          <span>{{ activePath ? fileName(activePath) : "差异预览" }}</span>
          <span class="muted">{{ addCount }} 个添加 · {{ delCount }} 个删除</span>
        </header>
        <div class="scroll diff-body">
          <div v-if="loadingDiff" class="empty-hint">正在加载差异…</div>
          <div v-else-if="diffError" class="empty-hint">{{ diffError }}</div>
          <div v-else-if="binary" class="empty-hint">二进制文件，无法预览差异</div>
          <div v-else-if="!activePath" class="empty-hint">选择一个文件查看差异</div>
          <div v-else-if="!lines.length" class="empty-hint">没有可显示的差异</div>
          <div v-else class="patch">
            <div v-for="(line, index) in lines" :key="index" class="dline" :class="line.kind">
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
.commit-ui {
  flex: 1;
  min-height: 0;
  display: grid;
  grid-template-columns: minmax(320px, 38%) 1fr;
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

.tabs {
  display: flex;
  gap: 2px;
  padding: 8px 10px 0;
}

.tabs button {
  height: 26px;
  padding: 0 10px;
  border-radius: 4px 4px 0 0;
  color: var(--text-muted);
}

.tabs button.active {
  color: var(--text);
  box-shadow: inset 0 -2px 0 var(--accent);
}

.files {
  flex: 1;
  min-height: 0;
  padding: 8px 0;
}

.group {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 12px 4px;
  color: var(--text);
}

.file {
  width: 100%;
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 4px 12px 4px 28px;
  text-align: left;
}

.file.active,
.file:hover {
  background: var(--bg-selected);
}

.name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.dir {
  margin-left: auto;
  color: var(--text-dim);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 45%;
}

.short {
  padding: 6px 16px;
}

.composer {
  border-top: 1px solid var(--border);
  padding: 8px 10px 10px;
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.amend-row {
  display: flex;
  align-items: center;
  gap: 10px;
}

.amend {
  display: flex;
  align-items: center;
  gap: 6px;
  color: var(--text-muted);
}

.ai-btn {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  height: 24px;
  padding: 0 8px;
  border-radius: 4px;
  color: #c7b0ff;
  background: #3a3350;
}

.ai-btn:hover:not(:disabled) {
  background: #4a3d68;
}

.hint {
  margin: 0;
  color: var(--text-dim);
}

textarea {
  min-height: 160px;
  resize: vertical;
  background: var(--bg-deep);
  border: 1px solid var(--accent);
  border-radius: 4px;
  padding: 8px;
  outline: none;
}

.actions {
  display: flex;
  gap: 8px;
}

.diff header {
  height: 34px;
  padding: 0 12px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  border-bottom: 1px solid var(--border);
}

.diff-body {
  flex: 1;
  min-height: 0;
  background: #1e1f22;
}

.patch {
  font-family: Consolas, "Courier New", monospace;
  font-size: 12px;
  line-height: 18px;
}

.dline {
  display: grid;
  grid-template-columns: 42px 42px 16px 1fr;
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
</style>
