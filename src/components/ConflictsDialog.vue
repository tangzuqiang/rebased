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
  accept: [payload: { paths: string[]; side: "ours" | "theirs" }];
  mergeFile: [path: string];
  continue: [];
  abort: [];
}>();

const selected = ref<string[]>(props.state.files[0] ? [props.state.files[0].path] : []);
const lastClicked = ref(props.state.files[0]?.path ?? "");
const groupByDir = ref(false);

const selectedFiles = computed(() =>
  props.state.files.filter((file) => selected.value.includes(file.path)),
);

const rows = computed(() => {
  if (!groupByDir.value) {
    return props.state.files.map((file) => ({ type: "file" as const, file }));
  }
  const groups = new Map<string, ConflictFile[]>();
  for (const file of props.state.files) {
    const dir = file.path.includes("/") ? file.path.split("/").slice(0, -1).join("/") : "";
    const list = groups.get(dir) ?? [];
    list.push(file);
    groups.set(dir, list);
  }
  const result: Array<{ type: "dir" | "file"; dir?: string; file?: ConflictFile; paths?: string[] }> = [];
  for (const [dir, files] of groups) {
    result.push({ type: "dir", dir: dir || "仓库根目录", paths: files.map((file) => file.path) });
    for (const file of files) {
      result.push({ type: "file", file });
    }
  }
  return result;
});

watch(
  () => props.state.files.map((file) => file.path).join("\n"),
  () => {
    const alive = new Set(props.state.files.map((file) => file.path));
    selected.value = selected.value.filter((path) => alive.has(path));
    if (!selected.value.length && props.state.files[0]) {
      selected.value = [props.state.files[0].path];
    }
    if (!alive.has(lastClicked.value)) {
      lastClicked.value = selected.value[0] ?? "";
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
  const paths = props.state.files.map((file) => file.path);
  if (event.shiftKey && lastClicked.value) {
    const from = paths.indexOf(lastClicked.value);
    const to = paths.indexOf(path);
    if (from >= 0 && to >= 0) {
      const [start, end] = from < to ? [from, to] : [to, from];
      selected.value = paths.slice(start, end + 1);
      return;
    }
  }
  if (event.ctrlKey || event.metaKey) {
    selected.value = selected.value.includes(path)
      ? selected.value.filter((item) => item !== path)
      : [...selected.value, path];
  } else {
    selected.value = [path];
  }
  lastClicked.value = path;
}

function selectDir(paths: string[] = []) {
  selected.value = [...paths];
  lastClicked.value = paths[0] ?? "";
}

function accept(side: "ours" | "theirs") {
  if (!selected.value.length) return;
  emit("accept", { paths: [...selected.value], side });
}

function mergeSelected() {
  const path = selected.value[0];
  if (path) emit("mergeFile", path);
}
</script>

<template>
  <Modal title="冲突" medium @close="emit('close')">
    <div class="conflicts">
      <p class="subtitle">
        正在将分支 <strong>{{ state.theirs }}</strong> 合并到分支 <strong>{{ state.ours }}</strong>
      </p>
      <div class="table-wrap">
        <div class="head">
          <span>名称</span>
          <span>本地（{{ state.ours }}）</span>
          <span>传入（{{ state.theirs }}）</span>
        </div>
        <div class="scroll body">
          <template v-for="row in rows" :key="row.type === 'dir' ? row.dir : row.file?.path">
            <button
              v-if="row.type === 'dir'"
              type="button"
              class="dir"
              @click="selectDir(row.paths)"
            >
              {{ row.dir }}
            </button>
            <button
              v-else-if="row.file"
              type="button"
              class="row"
              :class="{ selected: selected.includes(row.file.path) }"
              @click="selectFile(row.file.path, $event)"
              @dblclick="emit('mergeFile', row.file.path)"
            >
              <span class="name" :title="row.file.path">
                {{ fileName(row.file.path) }}
                <em v-if="!groupByDir">{{ dirName(row.file.path) }}</em>
              </span>
              <span>{{ row.file.oursStatus }}</span>
              <span>{{ row.file.theirsStatus }}</span>
            </button>
          </template>
          <div v-if="!state.files.length" class="empty-hint">所有冲突已处理，可以继续合并</div>
        </div>
      </div>
      <label class="group">
        <input v-model="groupByDir" type="checkbox" />
        按目录分组
      </label>
    </div>
    <template #footer>
      <button type="button" class="secondary-btn" :disabled="busy" @click="emit('abort')">中止合并</button>
      <span class="spacer" />
      <button type="button" class="secondary-btn" :disabled="busy || !selectedFiles.length" @click="accept('ours')">
        接受本地
      </button>
      <button type="button" class="secondary-btn" :disabled="busy || !selectedFiles.length" @click="accept('theirs')">
        接受传入
      </button>
      <button type="button" class="primary-btn" :disabled="busy || selectedFiles.length !== 1" @click="mergeSelected">
        合并…
      </button>
      <button type="button" class="primary-btn" :disabled="busy || state.files.length > 0" @click="emit('continue')">
        继续合并
      </button>
    </template>
  </Modal>
</template>

<style scoped>
.conflicts {
  flex: 1;
  min-height: 0;
  height: 100%;
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.subtitle {
  margin: 0;
}

.table-wrap {
  flex: 1;
  min-height: 260px;
  border: 1px solid var(--border-soft);
  border-radius: 6px;
  overflow: hidden;
  display: flex;
  flex-direction: column;
}

.head,
.row {
  display: grid;
  grid-template-columns: minmax(0, 1.4fr) 160px 160px;
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
  min-height: 28px;
  padding: 0 10px;
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

.group {
  display: flex;
  align-items: center;
  gap: 6px;
  color: var(--text-muted);
}

.spacer {
  flex: 1;
}
</style>
