<script setup lang="ts">
import { computed, ref, watch } from "vue";
import type { GitFile } from "../types";
import Icons from "./Icons.vue";

const props = defineProps<{
  files: GitFile[];
  activePath?: string;
  rootName: string;
  filter: string;
  statusOf: (file: GitFile) => string;
  pathOf: (file: GitFile) => string;
}>();

const emit = defineEmits<{
  select: [file: GitFile];
}>();

interface DirNode {
  key: string;
  name: string;
  dirs: Map<string, DirNode>;
  files: GitFile[];
  count: number;
}

type TreeItem =
  | { kind: "dir"; key: string; name: string; depth: number; count: number; open: boolean }
  | { kind: "file"; file: GitFile; name: string; depth: number };

const ROOT = "";
const AUTO_EXPAND_LIMIT = 40;
const expanded = ref(new Set<string>([ROOT]));

function newDir(key: string, name: string): DirNode {
  return { key, name, dirs: new Map(), files: [], count: 0 };
}

const filtered = computed(() => {
  const keyword = props.filter.trim().toLowerCase();
  return keyword ? props.files.filter((file) => props.pathOf(file).toLowerCase().includes(keyword)) : props.files;
});

const tree = computed(() => {
  const root = newDir(ROOT, props.rootName);
  for (const file of filtered.value) {
    const parts = props.pathOf(file).split(/[\\/]/);
    parts.pop();
    let node = root;
    node.count += 1;
    for (const part of parts) {
      const key = node.key ? `${node.key}/${part}` : part;
      let child = node.dirs.get(part);
      if (!child) {
        child = newDir(key, part);
        node.dirs.set(part, child);
      }
      child.count += 1;
      node = child;
    }
    node.files.push(file);
  }
  return root;
});

/** 只有一个子目录且没有文件的目录合并显示为 a/b。 */
function compress(node: DirNode): DirNode {
  let current = node;
  let name = node.name;
  while (current.files.length === 0 && current.dirs.size === 1) {
    const only = [...current.dirs.values()][0];
    name = `${name}/${only.name}`;
    current = only;
  }
  return current === node ? node : { ...current, name };
}

function fileName(file: GitFile) {
  return props.pathOf(file).split(/[\\/]/).pop() ?? file.path;
}

const items = computed<TreeItem[]>(() => {
  const result: TreeItem[] = [];
  const searching = Boolean(props.filter.trim());
  const walk = (node: DirNode, depth: number) => {
    const dirs = [...node.dirs.values()].map(compress).sort((a, b) => a.name.localeCompare(b.name));
    for (const dir of dirs) {
      const open = searching || expanded.value.has(dir.key);
      result.push({ kind: "dir", key: dir.key, name: dir.name, depth, count: dir.count, open });
      if (open) walk(dir, depth + 1);
    }
    const files = [...node.files].sort((a, b) => fileName(a).localeCompare(fileName(b)));
    for (const file of files) result.push({ kind: "file", file, name: fileName(file), depth });
  };
  const root = tree.value;
  const rootOpen = searching || expanded.value.has(ROOT);
  result.push({ kind: "dir", key: ROOT, name: root.name, depth: 0, count: root.count, open: rootOpen });
  if (rootOpen) walk(root, 1);
  return result;
});

const visibleFiles = computed(() =>
  items.value.flatMap((item) => (item.kind === "file" ? [item.file] : [])),
);

function allDirKeys(node: DirNode, keys: string[] = []) {
  keys.push(node.key);
  node.dirs.forEach((child) => allDirKeys(compress(child), keys));
  return keys;
}

function toggle(key: string) {
  const next = new Set(expanded.value);
  if (next.has(key)) next.delete(key);
  else next.add(key);
  expanded.value = next;
}

function expandAll() {
  expanded.value = new Set(allDirKeys(tree.value));
}

function collapseAll() {
  expanded.value = new Set([ROOT]);
}

/** 让当前文件所在的目录都展开。 */
function reveal(file: GitFile) {
  const parts = props.pathOf(file).split(/[\\/]/);
  parts.pop();
  const next = new Set(expanded.value);
  let key = "";
  for (const part of parts) {
    key = key ? `${key}/${part}` : part;
    next.add(key);
  }
  expanded.value = next;
}

function move(delta: number) {
  const list = visibleFiles.value;
  if (!list.length) return;
  const index = list.findIndex((file) => file.path === props.activePath);
  const next = list[Math.min(list.length - 1, Math.max(0, index < 0 ? 0 : index + delta))];
  if (next) emit("select", next);
}

watch(
  () => props.files,
  (files) => {
    expanded.value = files.length <= AUTO_EXPAND_LIMIT ? new Set(allDirKeys(tree.value)) : new Set([ROOT]);
    const active = files.find((file) => file.path === props.activePath) ?? files[0];
    if (active) reveal(active);
  },
  { immediate: true },
);

defineExpose({ move, expandAll, collapseAll });
</script>

<template>
  <div class="tree" tabindex="0" @keydown.down.prevent="move(1)" @keydown.up.prevent="move(-1)">
    <template v-for="item in items" :key="item.kind === 'dir' ? `d:${item.key}` : `f:${item.file.path}`">
      <button
        v-if="item.kind === 'dir'"
        type="button"
        class="row dir"
        :style="{ paddingLeft: `${8 + item.depth * 14}px` }"
        :title="item.key || rootName"
        @click="toggle(item.key)"
      >
        <Icons name="chevron" :size="10" class="chevron" :class="{ folded: !item.open }" />
        <Icons name="folder" :size="13" class="folder" />
        <span class="name">{{ item.name }}</span>
        <span class="count">{{ item.count }} 个文件</span>
      </button>
      <button
        v-else
        type="button"
        class="row file"
        :class="{ active: item.file.path === activePath, [`s-${statusOf(item.file)}`]: true }"
        :style="{ paddingLeft: `${22 + item.depth * 14}px` }"
        :title="pathOf(item.file)"
        @click="emit('select', item.file)"
      >
        <span class="badge">{{ statusOf(item.file) }}</span>
        <span class="name">{{ item.name }}</span>
      </button>
    </template>
  </div>
</template>

<style scoped>
.tree {
  outline: none;
  padding: 2px 0 8px;
}

.row {
  width: 100%;
  height: 22px;
  display: flex;
  align-items: center;
  gap: 5px;
  padding-right: 10px;
  text-align: left;
  white-space: nowrap;
}

.row:hover {
  background: var(--bg-hover, var(--bg-selected));
}

.row.active {
  background: var(--bg-selected);
}

.chevron {
  flex: none;
  transition: transform 0.12s;
}

.chevron.folded {
  transform: rotate(-90deg);
}

.folder {
  flex: none;
  color: #c9a45c;
}

.name {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
}

.count {
  flex: none;
  color: var(--text-dim);
  font-size: 12px;
}

.badge {
  flex: none;
  width: 14px;
  font-size: 11px;
  font-weight: 700;
  text-align: center;
}

.s-A .badge,
.s-A .name { color: #8fd19e; }
.s-D .badge,
.s-D .name { color: #e8a0a4; }
.s-M .badge,
.s-M .name { color: #7cb7ff; }
.s-R .badge,
.s-R .name { color: #d7b56d; }
.s-U .badge,
.s-U .name { color: #c0c45c; }
</style>
