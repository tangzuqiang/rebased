<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { getConflictFile } from "../api";
import { alignDiff3 } from "../diff3";
import type { Diff3Row, MergeFileContent } from "../types";
import Modal from "./Modal.vue";

const props = defineProps<{
  repoPath: string;
  path: string;
  oursLabel: string;
  theirsLabel: string;
  busy: boolean;
}>();

const emit = defineEmits<{
  close: [];
  accept: [side: "ours" | "theirs"];
  save: [content: string];
}>();

const content = ref<MergeFileContent | null>(null);
const result = ref("");
const loading = ref(false);
const error = ref("");

const rows = computed<Diff3Row[]>(() => {
  if (!content.value || content.value.binary) return [];
  return alignDiff3(content.value.ours, content.value.base, content.value.theirs);
});

onMounted(async () => {
  loading.value = true;
  try {
    content.value = await getConflictFile(props.repoPath, props.path);
    result.value = content.value.ours;
  } catch (err) {
    error.value = err instanceof Error ? err.message : String(err);
  } finally {
    loading.value = false;
  }
});

function fileName(path: string) {
  return path.split(/[\\/]/).pop() ?? path;
}
</script>

<template>
  <Modal :title="`合并文件 - ${fileName(path)}`" wide allow-fullscreen flush @close="emit('close')">
    <div class="resolve">
      <p class="muted path">{{ path }}</p>
      <p v-if="error" class="err">{{ error }}</p>
      <p v-else-if="loading" class="muted">正在加载三端内容…</p>
      <div v-else-if="content?.binary" class="empty-hint">二进制文件请使用「接受本地」或「接受传入」</div>
      <template v-else>
        <div class="panes">
          <div class="pane-head ours">本地 · {{ oursLabel }}</div>
          <div class="pane-head base">共同祖先</div>
          <div class="pane-head theirs">传入 · {{ theirsLabel }}</div>
          <div class="pane">
            <div v-for="(row, index) in rows" :key="`o-${index}`" class="line" :class="row.oursKind">
              <span class="code">{{ row.ours }}</span>
            </div>
          </div>
          <div class="pane">
            <div v-for="(row, index) in rows" :key="`b-${index}`" class="line" :class="row.baseKind">
              <span class="code">{{ row.base }}</span>
            </div>
          </div>
          <div class="pane">
            <div v-for="(row, index) in rows" :key="`t-${index}`" class="line" :class="row.theirsKind">
              <span class="code">{{ row.theirs }}</span>
            </div>
          </div>
        </div>
        <label class="result">
          <span>合并结果（可编辑后标记已解决）</span>
          <textarea v-model="result" />
        </label>
      </template>
    </div>
    <template #footer>
      <button type="button" class="secondary-btn" @click="emit('close')">返回列表</button>
      <button type="button" class="secondary-btn" :disabled="busy" @click="emit('accept', 'ours')">接受本地</button>
      <button type="button" class="secondary-btn" :disabled="busy" @click="emit('accept', 'theirs')">接受传入</button>
      <button
        type="button"
        class="secondary-btn"
        :disabled="busy || !content || content.binary"
        @click="result = content?.theirs ?? result"
      >
        用传入覆盖结果
      </button>
      <button
        type="button"
        class="primary-btn"
        :disabled="busy || !content || content.binary"
        @click="emit('save', result)"
      >
        保存并标记已解决
      </button>
    </template>
  </Modal>
</template>

<style scoped>
.resolve {
  height: 100%;
  display: flex;
  flex-direction: column;
  gap: 8px;
  min-height: 0;
  padding: 10px 12px 0;
}

.path {
  margin: 0;
}

.err {
  color: var(--danger);
}

.panes {
  flex: 1;
  min-height: 180px;
  display: grid;
  grid-template-columns: 1fr 1fr 1fr;
  grid-template-rows: 28px 1fr;
  border: 1px solid var(--border-soft);
  border-radius: 6px;
  overflow: hidden;
}

.pane-head,
.pane {
  min-width: 0;
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
  background: var(--bg-deep);
  border-right: 1px solid var(--border);
  font-family: Consolas, "Courier New", monospace;
  font-size: 12px;
  line-height: 18px;
}

.pane:nth-child(6) {
  border-right: 0;
}

.line {
  min-height: 18px;
  white-space: pre;
  padding: 0 8px;
}

.line.changed { background: #3d3420; }
.line.added { background: #1f3d28; }
.line.deleted { background: #3d2224; }
.line.conflict { background: #4a2a4a; }

.result {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

textarea {
  min-height: 120px;
  resize: vertical;
  background: var(--bg-input);
  border: 1px solid var(--border-soft);
  border-radius: 4px;
  padding: 8px;
  font-family: Consolas, "Courier New", monospace;
}
</style>
