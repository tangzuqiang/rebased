<script setup lang="ts">
import { computed, ref, watch } from "vue";
import type { ConflictFile, MergeState } from "../types";
import Modal from "./Modal.vue";

const props = defineProps<{
  state: MergeState;
  busy: boolean;
}>();

const emit = defineEmits<{
  close: [];
  edit: [path: string];
  accept: [payload: { paths: string[]; side: "ours" | "theirs" }];
  continue: [];
  abort: [];
}>();

const selected = ref<string[]>(props.state.files[0] ? [props.state.files[0].path] : []);
const lastClicked = ref(props.state.files[0]?.path ?? "");
const groupByDir = ref(false);
const pendingResolved = ref<string[]>([]);

const visibleFiles = computed(() =>
  props.state.files.filter((file) => !pendingResolved.value.includes(file.path)),
);
const selectedFiles = computed(() =>
  visibleFiles.value.filter((file) => selected.value.includes(file.path)),
);
const canMergeOne = computed(() => selected.value.length === 1 && Boolean(selected.value[0]));

const tableRows = computed(() => {
  if (!groupByDir.value) {
    return visibleFiles.value.map((file) => ({ type: "file" as const, file }));
  }
  const groups = new Map<string, ConflictFile[]>();
  for (const file of visibleFiles.value) {
    const dir = file.path.includes("/") ? file.path.split("/").slice(0, -1).join("/") : "";
    const list = groups.get(dir) ?? [];
    list.push(file);
    groups.set(dir, list);
  }
  const resultRows: Array<{ type: "dir" | "file"; dir?: string; file?: ConflictFile; paths?: string[] }> = [];
  for (const [dir, files] of groups) {
    resultRows.push({ type: "dir", dir: dir || "仓库根目录", paths: files.map((file) => file.path) });
    for (const file of files) {
      resultRows.push({ type: "file", file });
    }
  }
  return resultRows;
});

watch(
  () => props.state.files.map((file) => file.path).join("\n"),
  () => {
    const alive = new Set(props.state.files.map((file) => file.path));
    pendingResolved.value = pendingResolved.value.filter((path) => alive.has(path));
    selected.value = selected.value.filter((path) =>
      visibleFiles.value.some((file) => file.path === path),
    );
    if (!selected.value.length && visibleFiles.value[0]) {
      selected.value = [visibleFiles.value[0].path];
    }
    if (!visibleFiles.value.some((file) => file.path === lastClicked.value)) {
      lastClicked.value = selected.value[0] ?? "";
    }
  },
);

watch(
  () => props.busy,
  (busy, wasBusy) => {
    if (wasBusy && !busy) {
      const alive = new Set(props.state.files.map((file) => file.path));
      pendingResolved.value = pendingResolved.value.filter((path) => !alive.has(path));
    }
  },
);

function fileName(path: string) {
  return path.split(/[\\/]/).pop() ?? path;
}

function dirName(path: string) {
  const parts = path.split(/[\\/]/);
  return parts.length > 1 ? parts.slice(0, -1).join("/") : "";
}

function selectFile(path: string, event: MouseEvent) {
  const paths = visibleFiles.value.map((file) => file.path);
  if (event.shiftKey && lastClicked.value) {
    const from = paths.indexOf(lastClicked.value);
    const to = paths.indexOf(path);
    if (from >= 0 && to >= 0) {
      selected.value = paths.slice(Math.min(from, to), Math.max(from, to) + 1);
      lastClicked.value = path;
      return;
    }
  }
  if (event.ctrlKey || event.metaKey) {
    selected.value = selected.value.includes(path)
      ? selected.value.filter((item) => item !== path)
      : [...selected.value, path];
    lastClicked.value = path;
    return;
  }
  selected.value = [path];
  lastClicked.value = path;
}

function openMerge(path?: string) {
  const target = path || (selected.value.length === 1 ? selected.value[0] : "");
  if (!target) return;
  emit("edit", target);
}

function selectDir(paths: string[] = []) {
  selected.value = [...paths];
  lastClicked.value = paths[0] ?? "";
}

function hideResolved(paths: string[]) {
  pendingResolved.value = [...new Set([...pendingResolved.value, ...paths])];
  selected.value = selected.value.filter((item) => !paths.includes(item));
  if (!selected.value.length && visibleFiles.value[0]) {
    selected.value = [visibleFiles.value[0].path];
  }
}

function accept(side: "ours" | "theirs", paths?: string[]) {
  const list = paths?.length ? paths : [...selected.value];
  if (!list.length) return;
  hideResolved(list);
  emit("accept", { paths: list, side });
}

function cancelMerge() {
  if (props.busy) return;
  if (!window.confirm("确定取消本次合并？工作区将恢复到合并前的状态。")) return;
  emit("abort");
}

function finishMerge() {
  if (visibleFiles.value.length) return;
  emit("continue");
}
</script>

<template>
  <Modal title="冲突文件" medium @close="emit('close')">
    <div class="conflicts">
      <p class="subtitle">
        正在将分支 <strong>{{ state.theirs }}</strong> 合并到分支 <strong>{{ state.ours }}</strong>
        。单击选中文件，双击或点「合并」进入合并页；也可直接使用您的 / 他们的覆盖该文件。
      </p>
      <div class="list">
        <div class="head">
          <span>冲突文件</span>
          <span>本地</span>
          <span>传入</span>
          <span>操作</span>
        </div>
        <div class="scroll body">
          <template v-for="row in tableRows" :key="row.type === 'dir' ? row.dir : row.file?.path">
            <button v-if="row.type === 'dir'" type="button" class="dir" @click="selectDir(row.paths)">
              {{ row.dir }}
            </button>
            <div
              v-else-if="row.file"
              class="row"
              :class="{ selected: selected.includes(row.file.path) }"
              @click="selectFile(row.file.path, $event)"
              @dblclick="openMerge(row.file.path)"
            >
              <span class="name" :title="row.file.path">
                {{ fileName(row.file.path) }}
                <em v-if="!groupByDir">{{ dirName(row.file.path) }}</em>
              </span>
              <span>{{ row.file.oursStatus }}</span>
              <span>{{ row.file.theirsStatus }}</span>
              <span class="actions" @click.stop @dblclick.stop>
                <button type="button" class="mini" :disabled="busy" @click="accept('ours', [row.file.path])">
                  使用您的
                </button>
                <button type="button" class="mini" :disabled="busy" @click="accept('theirs', [row.file.path])">
                  使用他们的
                </button>
                <button type="button" class="mini primary" :disabled="busy" @click="openMerge(row.file.path)">
                  合并
                </button>
              </span>
            </div>
          </template>
          <div v-if="!visibleFiles.length" class="empty-hint">所有冲突已处理，可以完成合并</div>
        </div>
        <label class="group">
          <input v-model="groupByDir" type="checkbox" />
          按目录分组
        </label>
      </div>
    </div>
    <template #footer>
      <button type="button" class="danger-btn" :disabled="busy" @click="cancelMerge">取消合并</button>
      <span class="spacer" />
      <button type="button" class="secondary-btn" :disabled="busy || !selectedFiles.length" @click="accept('ours')">
        使用您的
      </button>
      <button type="button" class="secondary-btn" :disabled="busy || !selectedFiles.length" @click="accept('theirs')">
        使用他们的
      </button>
      <button type="button" class="secondary-btn" :disabled="busy || !canMergeOne" @click="openMerge()">
        合并
      </button>
      <button type="button" class="primary-btn" :disabled="busy || visibleFiles.length > 0" @click="finishMerge">
        完成合并
      </button>
    </template>
  </Modal>
</template>

<style scoped>
.conflicts {
  display: flex;
  flex-direction: column;
  gap: 10px;
  min-height: 360px;
  height: 100%;
}

.subtitle {
  margin: 0;
}

.list {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  border: 1px solid var(--border-soft);
  border-radius: 6px;
  overflow: hidden;
}

.head,
.row {
  display: grid;
  grid-template-columns: minmax(0, 1.4fr) 56px 56px 220px;
  align-items: center;
}

.head {
  height: 28px;
  padding: 0 10px;
  color: var(--text-dim);
  border-bottom: 1px solid var(--border);
  background: var(--bg-hover);
}

.body {
  flex: 1;
}

.dir,
.row {
  width: 100%;
  text-align: left;
}

.dir {
  padding: 6px 10px 2px;
  color: var(--text-muted);
}

.row {
  min-height: 32px;
  padding: 0 10px;
  cursor: pointer;
}

.row:hover,
.dir:hover,
.row.selected {
  background: var(--bg-selected);
}

.name {
  display: flex;
  gap: 8px;
  overflow: hidden;
}

.name em {
  color: var(--text-dim);
  font-style: normal;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.actions {
  display: flex;
  justify-content: flex-end;
  gap: 4px;
}

.mini {
  height: 22px;
  padding: 0 6px;
  border: 1px solid var(--border-soft);
  border-radius: 4px;
  background: var(--bg-input);
  font-size: 11px;
}

.mini.primary {
  background: var(--accent);
  border-color: var(--accent);
  color: #fff;
}

.mini:hover:not(:disabled) {
  background: var(--bg-hover);
}

.mini.primary:hover:not(:disabled) {
  background: var(--accent-hover);
}

.group {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 6px 10px;
  color: var(--text-muted);
  border-top: 1px solid var(--border);
}

.spacer {
  flex: 1;
}

.danger-btn {
  height: 26px;
  border-radius: 4px;
  padding: 0 12px;
  font-weight: 500;
  background: var(--danger);
  color: #fff;
}
</style>
