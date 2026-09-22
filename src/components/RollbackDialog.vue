<script setup lang="ts">
import { computed, ref, watch } from "vue";
import type { GitFile, RepoInfo } from "../types";
import Modal from "./Modal.vue";

const props = defineProps<{
  repo: RepoInfo;
  changes: GitFile[];
  unversioned: GitFile[];
  busy: boolean;
}>();

const emit = defineEmits<{
  close: [];
  discard: [paths: { path: string; untracked: boolean }[]];
}>();

const selected = ref<string[]>([]);
const submitting = ref(false);
const locked = computed(() => props.busy || submitting.value);

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
      selected.value = [...paths];
    }
  },
  { immediate: true },
);

const changeChecked = computed(() =>
  props.changes.length > 0 && props.changes.every((file) => selected.value.includes(file.path)),
);
const unversionedChecked = computed(() =>
  props.unversioned.length > 0 && props.unversioned.every((file) => selected.value.includes(file.path)),
);

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

function confirmDiscard() {
  if (locked.value || !selected.value.length) return;
  const known = new Map(allFiles.value.map((file) => [file.path, file.untracked]));
  const paths = selected.value.map((path) => ({
    path,
    untracked: known.get(path) ?? false,
  }));
  const tracked = paths.filter((item) => !item.untracked).length;
  const untracked = paths.length - tracked;
  const detail =
    untracked > 0 && tracked > 0
      ? `其中 ${tracked} 个已跟踪文件将恢复为 HEAD 版本，${untracked} 个未跟踪文件将被删除。`
      : untracked > 0
        ? "未跟踪文件将被永久删除。"
        : "已跟踪文件将恢复为 HEAD 版本，并取消暂存。";
  if (
    !window.confirm(
      `确定回滚选中的 ${paths.length} 个文件？\n${detail}\n此操作不可撤销。`,
    )
  ) {
    return;
  }
  submitting.value = true;
  emit("discard", paths);
}

watch(
  () => props.busy,
  (busy, wasBusy) => {
    if (!busy && wasBusy) submitting.value = false;
  },
);
</script>

<template>
  <Modal :title="`回滚 - ${repo.name}`" wide @close="emit('close')">
    <p class="lead">
      勾选要回滚的文件。已跟踪文件恢复为上次提交；未跟踪文件会被删除。
    </p>
    <div v-if="!allFiles.length" class="empty-hint">工作区没有可回滚的变更</div>
    <div v-else class="scroll files">
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
      <label
        v-for="file in changes"
        :key="file.path + String(file.staged)"
        class="file"
      >
        <input
          type="checkbox"
          :checked="selected.includes(file.path)"
          @change="toggle(file.path)"
        />
        <span class="file-status" :class="file.status">{{ file.status }}</span>
        <span class="name" :title="file.path">{{ fileName(file.path) }}</span>
        <span class="dir">{{ dirName(file.path) }}</span>
      </label>

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
      <label v-for="file in unversioned" :key="file.path" class="file">
        <input
          type="checkbox"
          :checked="selected.includes(file.path)"
          @change="toggle(file.path)"
        />
        <span class="file-status U">U</span>
        <span class="name" :title="file.path">{{ fileName(file.path) }}</span>
        <span class="dir">{{ dirName(file.path) }}</span>
      </label>
    </div>
    <template #footer>
      <button type="button" class="secondary-btn" :disabled="locked" @click="emit('close')">取消</button>
      <button
        type="button"
        class="primary-btn danger"
        :class="{ 'is-loading': submitting }"
        :disabled="locked || !selected.length || !allFiles.length"
        @click="confirmDiscard"
      >
        <span v-if="submitting" class="btn-spinner" />
        {{ submitting ? "正在回滚…" : "回滚选中" }}
      </button>
    </template>
  </Modal>
</template>

<style scoped>
.lead {
  margin: 0 0 12px;
  color: var(--text-muted);
  font-size: 12px;
  line-height: 1.5;
}

.scroll.files {
  max-height: min(420px, 55vh);
  overflow: auto;
  border: 1px solid var(--border);
  border-radius: 6px;
}

.group {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 12px 4px;
  color: var(--text);
}

.file {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 4px 12px 4px 16px;
  cursor: pointer;
}

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

.primary-btn.danger {
  background: var(--danger);
  border-color: var(--danger);
}

.primary-btn.danger:hover:not(:disabled) {
  filter: brightness(1.05);
}
</style>
