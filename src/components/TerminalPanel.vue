<script setup lang="ts">
import { onUnmounted, ref, watch } from "vue";
import Icons from "./Icons.vue";
import TerminalSession from "./TerminalSession.vue";

interface TermTab {
  id: string;
  seq: number;
  title: string;
  cwd: string;
}

const props = defineProps<{
  repoPath: string;
  visible: boolean;
  startCwd?: string;
  spawnSeq?: number;
}>();

const emit = defineEmits<{
  close: [];
}>();

const height = ref(240);
const errorText = ref("");
const tabs = ref<TermTab[]>([]);
const activeId = ref("");
let seq = 0;
let resizing = false;

function tabTitle(index: number) {
  return index === 1 ? "本地" : `本地 (${index})`;
}

function addTab(cwd = props.repoPath) {
  seq += 1;
  const tab: TermTab = {
    id: `${cwd}::${seq}`,
    seq,
    title: tabTitle(seq),
    cwd,
  };
  tabs.value.push(tab);
  activeId.value = tab.id;
  errorText.value = "";
}

function selectTab(id: string) {
  activeId.value = id;
  errorText.value = "";
}

function closeTab(id: string) {
  const index = tabs.value.findIndex((tab) => tab.id === id);
  if (index < 0) return;
  tabs.value.splice(index, 1);
  if (!tabs.value.length) {
    activeId.value = "";
    emit("close");
    return;
  }
  if (activeId.value === id) {
    activeId.value = (tabs.value[index] ?? tabs.value[index - 1]).id;
  }
}

function onResizeMove(event: MouseEvent) {
  if (!resizing) return;
  const bottom = window.innerHeight - 24;
  height.value = Math.min(Math.max(bottom - event.clientY, 140), Math.floor(window.innerHeight * 0.7));
}

function onResizeEnd() {
  if (!resizing) return;
  resizing = false;
  window.removeEventListener("mousemove", onResizeMove);
  window.removeEventListener("mouseup", onResizeEnd);
}

function startResize(event: MouseEvent) {
  if ((event.target as HTMLElement).closest("button")) return;
  resizing = true;
  window.addEventListener("mousemove", onResizeMove);
  window.addEventListener("mouseup", onResizeEnd);
}

if (!tabs.value.length) {
  addTab(props.startCwd || props.repoPath);
}

watch(
  () => props.visible,
  (visible) => {
    if (visible && !tabs.value.length) {
      addTab(props.startCwd || props.repoPath);
    }
  },
);

watch(
  () => props.spawnSeq,
  (value) => {
    if (!value) return;
    addTab(props.startCwd || props.repoPath);
  },
);

onUnmounted(() => {
  window.removeEventListener("mousemove", onResizeMove);
  window.removeEventListener("mouseup", onResizeEnd);
});
</script>

<template>
  <section class="terminal" :style="{ height: `${height}px` }">
    <header @mousedown="startResize">
      <div class="tabs">
        <button
          v-for="tab in tabs"
          :key="tab.id"
          type="button"
          class="tab"
          :class="{ active: tab.id === activeId }"
          @click="selectTab(tab.id)"
          @click.middle.prevent="closeTab(tab.id)"
        >
          <span>{{ tab.title }}</span>
          <span class="close" title="关闭终端" @click.stop="closeTab(tab.id)">
            <Icons name="close" :size="10" />
          </span>
        </button>
        <button type="button" class="icon-btn add" title="新建终端" @click="addTab()">
          <Icons name="plus" :size="12" />
        </button>
      </div>
      <span class="muted">Alt+F12</span>
      <span v-if="errorText" class="err">{{ errorText }}</span>
      <button type="button" class="icon-btn" title="隐藏终端" @click="emit('close')">✕</button>
    </header>
    <div class="sessions">
      <TerminalSession
        v-for="tab in tabs"
        v-show="tab.id === activeId"
        :key="tab.id"
        :id="tab.id"
        :cwd="tab.cwd || repoPath"
        :active="visible && tab.id === activeId"
        @error="errorText = $event"
      />
    </div>
  </section>
</template>

<style scoped>
.terminal {
  flex: none;
  min-height: 140px;
  display: flex;
  flex-direction: column;
  border-top: 1px solid var(--border);
  background: var(--bg-deep);
}

header {
  height: 28px;
  padding: 0 8px 0 0;
  display: flex;
  align-items: center;
  gap: 8px;
  cursor: ns-resize;
  border-bottom: 1px solid var(--border);
  background: var(--bg);
}

.tabs {
  flex: 1;
  min-width: 0;
  height: 100%;
  display: flex;
  align-items: stretch;
  overflow-x: auto;
}

.tabs::-webkit-scrollbar {
  height: 0;
}

.tab {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 100%;
  padding: 0 6px 0 10px;
  color: var(--text-muted);
  border-right: 1px solid var(--border);
}

.tab.active {
  color: var(--text);
  background: var(--bg-deep);
}

.tab:hover {
  color: var(--text);
}

.close {
  width: 18px;
  height: 18px;
  border-radius: 4px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  color: var(--danger);
  cursor: pointer;
  opacity: 0.7;
  transition: opacity 0.12s, background-color 0.12s, color 0.12s;
}

.tab:hover .close,
.tab.active .close {
  opacity: 1;
}

.close:hover {
  background: var(--danger);
  color: #fff;
}

.close:active {
  transform: scale(0.9);
}

.add {
  width: 24px;
  height: 24px;
  margin: auto 4px;
}

.muted {
  flex: none;
}

.err {
  max-width: 280px;
  color: var(--danger);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.sessions {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
}
</style>
