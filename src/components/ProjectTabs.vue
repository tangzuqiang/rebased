<script setup lang="ts">
import { sameRepo } from "../path";
import type { ProjectTab } from "../types";
import Icons from "./Icons.vue";

defineProps<{
  tabs: ProjectTab[];
  activePath: string;
}>();

const emit = defineEmits<{
  select: [path: string];
  close: [path: string];
  add: [];
}>();
</script>

<template>
  <div class="tabs">
    <div class="list">
      <button
        v-for="tab in tabs"
        :key="tab.path"
        type="button"
        class="tab"
        :class="{ active: sameRepo(tab.path, activePath), merging: tab.merging }"
        :title="tab.path"
        @click="emit('select', tab.path)"
        @click.middle.prevent="emit('close', tab.path)"
      >
        <span class="label">
          <span v-if="tab.dirty" class="dirty">*</span>
          {{ tab.name }}
          <em v-if="tab.branch">{{ tab.branch }}</em>
        </span>
        <span
          class="close"
          title="关闭"
          @click.stop="emit('close', tab.path)"
        >
          <Icons name="close" :size="10" />
        </span>
      </button>
    </div>
    <button type="button" class="icon-btn add" title="打开仓库" @click="emit('add')">
      <Icons name="plus" :size="12" />
    </button>
  </div>
</template>

<style scoped>
.tabs {
  height: 32px;
  flex: none;
  display: flex;
  align-items: stretch;
  background: #1e1f22;
  border-bottom: 1px solid var(--border);
}

.list {
  display: flex;
  min-width: 0;
  flex: 1;
  overflow-x: auto;
}

.list::-webkit-scrollbar {
  height: 0;
}

.tab {
  position: relative;
  display: inline-flex;
  align-items: center;
  gap: 6px;
  max-width: 220px;
  height: 100%;
  padding: 0 8px 0 12px;
  color: var(--text-muted);
  border-right: 1px solid #2b2d30;
}

.tab:hover,
.tab.active {
  color: var(--text);
  background: #2b2d30;
}

.tab.active {
  box-shadow: inset 0 2px 0 var(--accent);
}

.tab.merging {
  color: #ffd6d6;
}

.label {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.label em {
  margin-left: 6px;
  color: var(--text-dim);
  font-style: normal;
}

.dirty {
  color: #c9a227;
  font-weight: 700;
}

.close {
  width: 16px;
  height: 16px;
  border-radius: 3px;
  color: var(--text-dim);
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex: none;
}

.close:hover {
  color: var(--text);
  background: var(--bg-hover);
}

.add {
  margin: 3px 6px;
}
</style>
