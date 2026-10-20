<script setup lang="ts">
import { onUnmounted, ref } from "vue";
import { sameRepo } from "../path";
import type { ProjectTab } from "../types";
import Icons from "./Icons.vue";

const props = defineProps<{
  tabs: ProjectTab[];
  activePath: string;
}>();

const emit = defineEmits<{
  batch: [];
  select: [path: string];
  close: [path: string];
  add: [];
  reorder: [from: number, to: number];
}>();

const DRAG_THRESHOLD = 5;

const listEl = ref<HTMLElement>();
const draggingPath = ref("");
let pending: { path: string; x: number; y: number } | null = null;
let suppressClick = false;

function tabIndex(path: string) {
  return props.tabs.findIndex((tab) => sameRepo(tab.path, path));
}

function onPointerDown(event: PointerEvent, path: string) {
  if (event.button !== 0) return;
  pending = { path, x: event.clientX, y: event.clientY };
  window.addEventListener("pointermove", onPointerMove);
  window.addEventListener("pointerup", onPointerUp);
  window.addEventListener("pointercancel", onPointerUp);
}

function onPointerMove(event: PointerEvent) {
  if (!pending) return;
  if (!draggingPath.value) {
    if (Math.hypot(event.clientX - pending.x, event.clientY - pending.y) < DRAG_THRESHOLD) return;
    draggingPath.value = pending.path;
  }
  const from = tabIndex(draggingPath.value);
  const items = listEl.value?.querySelectorAll<HTMLElement>(".tab");
  if (from < 0 || !items) return;
  let insert = items.length;
  for (let i = 0; i < items.length; i += 1) {
    const rect = items[i].getBoundingClientRect();
    if (event.clientX < rect.left + rect.width / 2) {
      insert = i;
      break;
    }
  }
  const to = insert > from ? insert - 1 : insert;
  if (to !== from) emit("reorder", from, to);
}

function onPointerUp() {
  if (draggingPath.value) {
    suppressClick = true;
    window.setTimeout(() => {
      suppressClick = false;
    }, 0);
  }
  pending = null;
  draggingPath.value = "";
  window.removeEventListener("pointermove", onPointerMove);
  window.removeEventListener("pointerup", onPointerUp);
  window.removeEventListener("pointercancel", onPointerUp);
}

function onTabClick(path: string) {
  if (suppressClick) return;
  emit("select", path);
}

onUnmounted(onPointerUp);
</script>

<template>
  <div class="tabs">
    <button type="button" class="icon-btn batch" title="批量 Git 操作中心" @click="emit('batch')">
      <Icons name="branch" :size="13" />
    </button>
    <div ref="listEl" class="list" :class="{ dragging: draggingPath }">
      <button
        v-for="tab in tabs"
        :key="tab.path"
        type="button"
        class="tab"
        :class="{
          active: sameRepo(tab.path, activePath),
          merging: tab.merging,
          lifted: draggingPath && sameRepo(tab.path, draggingPath),
        }"
        :title="tab.path"
        @pointerdown="onPointerDown($event, tab.path)"
        @click="onTabClick(tab.path)"
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
          @pointerdown.stop
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
  background: var(--bg-tabbar);
  border-bottom: 1px solid var(--border);
}

.list {
  display: flex;
  min-width: 0;
  flex: 1;
  overflow-x: auto;
}

.batch {
  margin: 3px 4px;
  flex: none;
  border-right: 1px solid var(--border-soft);
  border-radius: 0;
  padding-right: 5px;
}

.list::-webkit-scrollbar {
  height: 0;
}

.list.dragging,
.list.dragging .tab {
  cursor: grabbing;
}

.tab {
  position: relative;
  display: inline-flex;
  align-items: center;
  gap: 6px;
  max-width: 220px;
  height: 100%;
  padding: 0 6px 0 12px;
  color: var(--text-muted);
  border-right: 1px solid var(--border);
  user-select: none;
  touch-action: none;
}

.tab:hover {
  color: var(--text);
  background: var(--bg-hover);
}

.tab.active {
  color: var(--text);
  background: var(--bg-tab-active);
  font-weight: 600;
  box-shadow: inset 0 3px 0 var(--accent);
}

.tab.lifted {
  opacity: 0.75;
  background: var(--bg-hover);
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

.tab.active .label em {
  color: var(--text-muted);
}

.dirty {
  color: var(--renamed);
  font-weight: 700;
}

.close {
  width: 18px;
  height: 18px;
  border-radius: 4px;
  color: var(--danger);
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex: none;
  cursor: pointer;
  opacity: 0.7;
  transition: opacity 0.12s, background-color 0.12s, color 0.12s;
}

.tab:hover .close,
.tab.active .close {
  opacity: 1;
}

.close:hover {
  color: #fff;
  background: var(--danger);
}

.close:active {
  transform: scale(0.9);
}

.list.dragging .close {
  pointer-events: none;
}

.add {
  margin: 3px 6px;
}
</style>
