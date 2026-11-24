<script setup lang="ts">
export interface EditorTabBarItem {
  path: string;
  name: string;
  dirty: boolean;
}

defineProps<{
  tabs: EditorTabBarItem[];
  activePath: string;
}>();

const emit = defineEmits<{
  select: [path: string];
  close: [path: string];
}>();
</script>

<template>
  <div class="editor-tab-bar" role="tablist">
    <button
      v-for="tab in tabs"
      :key="tab.path"
      type="button"
      role="tab"
      class="editor-tab"
      :class="{ active: tab.path === activePath }"
      :aria-selected="tab.path === activePath"
      :title="tab.path"
      @click="emit('select', tab.path)"
      @click.middle.prevent="emit('close', tab.path)"
    >
      <span v-if="tab.dirty" class="dirty" title="未保存" />
      <span class="name">{{ tab.name }}</span>
      <span
        class="close"
        role="button"
        tabindex="0"
        title="关闭"
        aria-label="关闭"
        @click.stop="emit('close', tab.path)"
        @keydown.enter.stop="emit('close', tab.path)"
        @keydown.space.prevent.stop="emit('close', tab.path)"
      >×</span>
    </button>
  </div>
</template>

<style scoped>
.editor-tab-bar {
  flex: none;
  display: flex;
  min-width: 0;
  height: 32px;
  overflow-x: auto;
  overflow-y: hidden;
  border-bottom: 1px solid var(--border);
  background: var(--bg-tabbar);
}

.editor-tab-bar::-webkit-scrollbar {
  height: 4px;
}

.editor-tab {
  flex: none;
  display: inline-flex;
  align-items: center;
  gap: 7px;
  max-width: 220px;
  height: 32px;
  padding: 0 7px 0 11px;
  border-right: 1px solid var(--border);
  color: var(--text-muted);
  user-select: none;
}

.editor-tab:hover {
  background: var(--bg-hover);
  color: var(--text);
}

.editor-tab.active {
  box-shadow: inset 0 2px 0 var(--accent);
  background: var(--bg-tab-active);
  color: var(--text);
}

.name {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.dirty {
  flex: none;
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: var(--text-warn);
}

.close {
  flex: none;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 18px;
  height: 18px;
  border-radius: 4px;
  color: var(--text-dim);
  font-size: 16px;
  line-height: 1;
}

.editor-tab:hover .close,
.editor-tab.active .close {
  color: var(--text-muted);
}

.close:hover {
  background: var(--bg-selected);
  color: var(--text);
}
</style>
