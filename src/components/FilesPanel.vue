<script setup lang="ts">
import { computed } from "vue";
import { formatDateTime } from "../path";
import type { GitCommit, GitFile } from "../types";
import Icons from "./Icons.vue";

const props = defineProps<{
  files: GitFile[];
  message?: string;
  commit?: GitCommit | null;
}>();

const grouped = computed(() => {
  const tree = new Map<string, GitFile[]>();
  for (const file of props.files) {
    const parts = file.path.split("/");
    const dir = parts.length > 1 ? parts.slice(0, -1).join("/") : "";
    const list = tree.get(dir) ?? [];
    list.push(file);
    tree.set(dir, list);
  }
  return [...tree.entries()];
});

const fullMessage = computed(() => props.message?.trim() || props.commit?.message || "");

function fileName(path: string) {
  return path.split(/[\\/]/).pop() ?? path;
}
</script>

<template>
  <aside class="panel">
    <div class="head">
      <span>{{ files.length }} 个文件</span>
      <span class="spacer" />
      <button class="icon-btn" title="更多"><Icons name="more" /></button>
    </div>
    <div class="scroll">
      <div v-if="!files.length" class="empty-hint">选择一条提交查看文件</div>
      <template v-for="[dir, items] in grouped" :key="dir || '__root'">
        <div v-if="dir" class="dir">
          <Icons name="folder" :size="13" />
          <span>{{ dir }}</span>
          <span class="muted">{{ items.length }} 个文件</span>
        </div>
        <div v-for="file in items" :key="file.path" class="file">
          <span class="file-status" :class="file.status">{{ file.status }}</span>
          <Icons name="file" :size="12" />
          <span class="name">{{ fileName(file.path) }}</span>
        </div>
      </template>
    </div>
    <div class="detail">
      <template v-if="commit || fullMessage">
        <div v-if="commit" class="meta">
          <span class="hash">{{ commit.shortHash }}</span>
          <span>{{ commit.author }}</span>
          <span>{{ formatDateTime(commit.timestamp) }}</span>
        </div>
        <pre v-if="fullMessage" class="body">{{ fullMessage }}</pre>
      </template>
      <p v-else class="empty-hint">左键选择一条提交查看完整说明</p>
    </div>
  </aside>
</template>

<style scoped>
.panel {
  width: 280px;
  min-width: 220px;
  border-left: 1px solid var(--border);
  display: flex;
  flex-direction: column;
  background: var(--bg);
}

.head {
  height: 34px;
  padding: 0 8px;
  display: flex;
  align-items: center;
  color: var(--text-muted);
  border-bottom: 1px solid var(--border);
}

.spacer {
  flex: 1;
}

.scroll {
  flex: 1;
  min-height: 80px;
  overflow: auto;
}

.detail {
  flex: 1;
  min-height: 96px;
  max-height: 50%;
  overflow: auto;
  padding: 10px 12px;
  border-top: 1px solid var(--border);
  background: var(--bg);
}

.meta {
  display: flex;
  flex-wrap: wrap;
  gap: 8px 12px;
  margin-bottom: 8px;
  font-size: 11px;
  color: var(--text-muted);
}

.hash {
  font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  color: var(--text);
}

.body {
  margin: 0;
  white-space: pre-wrap;
  word-break: break-word;
  font: inherit;
  font-size: 12px;
  line-height: 1.55;
  color: var(--text);
}

.dir,
.file {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 3px 10px;
}

.dir {
  padding-top: 8px;
  color: var(--text);
}

.file {
  padding-left: 22px;
}

.name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
