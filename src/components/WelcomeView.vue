<script setup lang="ts">
import { ref } from "vue";
import { formatOpenedAt } from "../path";
import type { RecentProject } from "../types";
import Icons from "./Icons.vue";

defineProps<{
  recent: RecentProject[];
  version?: string;
}>();

const emit = defineEmits<{
  open: [];
  openPath: [path: string];
  removeRecent: [path: string];
}>();

const path = ref("");
</script>

<template>
  <div class="welcome">
    <img src="/rebased.svg" width="72" height="72" alt="Rebased" />
    <h1>Rebased <span v-if="version" class="ver">{{ version }}</span></h1>
    <p>基于原项目界面还原的轻量 Git 客户端</p>
    <div class="actions">
      <button class="primary-btn" @click="emit('open')">打开仓库…</button>
    </div>
    <div class="path-row">
      <input
        v-model="path"
        placeholder="或粘贴目录路径后回车"
        @keydown.enter="path.trim() && emit('openPath', path.trim())"
      />
    </div>
    <section v-if="recent.length" class="recent">
      <h2>最近项目</h2>
      <div class="list">
        <div v-for="item in recent" :key="item.path" class="row">
          <button type="button" class="item" :title="item.path" @click="emit('openPath', item.path)">
            <Icons name="folder" :size="14" />
            <span class="meta">
              <strong>{{ item.name }}</strong>
              <em>{{ item.path }}</em>
            </span>
            <span class="when">{{ formatOpenedAt(item.lastOpened) }}</span>
          </button>
          <button
            type="button"
            class="icon-btn"
            title="从最近项目中移除"
            @click="emit('removeRecent', item.path)"
          >
            <Icons name="close" :size="10" />
          </button>
        </div>
      </div>
    </section>
  </div>
</template>

<style scoped>
.welcome {
  height: 100%;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 10px;
  background:
    radial-gradient(circle at top, var(--bg-hover) 0%, var(--bg) 45%, var(--bg-deep) 100%);
}

h1 {
  margin: 8px 0 0;
  font-size: 28px;
  font-weight: 560;
}

.ver {
  margin-left: 8px;
  font-size: 14px;
  font-weight: 500;
  color: var(--text-muted);
}

p {
  margin: 0;
  color: var(--text-muted);
}

.actions {
  margin-top: 12px;
}

.primary-btn {
  height: 32px;
  padding: 0 18px;
}

.path-row input {
  margin-top: 8px;
  width: 360px;
  height: 30px;
  border: 1px solid var(--border-soft);
  background: var(--bg-input);
  border-radius: 4px;
  padding: 0 10px;
  outline: none;
}

.path-row input:focus {
  border-color: var(--accent);
}

.recent {
  width: min(520px, calc(100vw - 48px));
  margin-top: 20px;
}

h2 {
  margin: 0 0 8px;
  font-size: 12px;
  font-weight: 600;
  color: var(--text-muted);
}

.list {
  max-height: 240px;
  overflow: auto;
  border: 1px solid var(--border-soft);
  border-radius: 6px;
  background: var(--bg);
}

.row {
  display: flex;
  align-items: center;
  border-bottom: 1px solid var(--border-soft);
}

.row:last-child {
  border-bottom: 0;
}

.item {
  flex: 1;
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 10px;
  text-align: left;
}

.item:hover {
  background: var(--bg-selected);
}

.meta {
  min-width: 0;
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.meta strong {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.meta em,
.when {
  color: var(--text-dim);
  font-style: normal;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.when {
  flex: none;
  font-size: 11px;
}

.row .icon-btn {
  margin-right: 6px;
}
</style>
