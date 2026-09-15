<script setup lang="ts">
import { computed } from "vue";
import type { GitFile } from "../types";
import Icons from "./Icons.vue";

const props = defineProps<{
  files: GitFile[];
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
