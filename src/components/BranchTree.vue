<script setup lang="ts">
import { computed, ref } from "vue";
import type { BranchInfo } from "../types";
import Icons from "./Icons.vue";

const props = defineProps<{
  branches: BranchInfo[];
  currentBranch: string;
}>();

const emit = defineEmits<{
  change: [refs: string[]];
}>();

type Group = "local" | "remote" | "tag";

const open = ref<Record<Group, boolean>>({ local: false, remote: false, tag: false });
const active = ref<string[]>(["HEAD"]);
const highlighted = ref("HEAD");
const anchor = ref("HEAD");
const query = ref("");
const searching = computed(() => query.value.trim().length > 0);

function matchName(name: string) {
  const keyword = query.value.trim().toLowerCase();
  return !keyword || name.toLowerCase().includes(keyword);
}

const localBranches = computed(() =>
  props.branches.filter(
    (item) => item.kind !== "tag" && item.kind !== "remote" && !item.remote && matchName(item.name),
  ),
);
const remoteBranches = computed(() =>
  props.branches.filter((item) => (item.kind === "remote" || item.remote) && matchName(item.name)),
);
const tags = computed(() =>
  props.branches.filter((item) => item.kind === "tag" && matchName(item.name)),
);

const groups = computed(() =>
  [
    { key: "local" as Group, label: "本地", items: localBranches.value },
    { key: "remote" as Group, label: "远程", items: remoteBranches.value },
    { key: "tag" as Group, label: "标签", items: tags.value },
  ].filter((group) => !searching.value || group.items.length > 0),
);

const visibleNames = computed(() => [
  ...(searching.value ? [] : ["HEAD"]),
  ...groups.value.flatMap((group) => (isOpen(group.key) ? group.items.map((item) => item.name) : [])),
]);

function isOpen(group: Group) {
  return searching.value || open.value[group];
}

function toggleGroup(group: Group) {
  if (searching.value) return;
  open.value[group] = !open.value[group];
}

function setActive(names: string[]) {
  active.value = names.length ? names : ["HEAD"];
  const refs = active.value.map((name) => (name === "HEAD" ? props.currentBranch : name)).filter(Boolean);
  emit("change", [...new Set(refs)]);
}

function show(name: string) {
  highlighted.value = name;
  anchor.value = name;
  setActive([name]);
}

function onRowClick(event: MouseEvent, name: string) {
  highlighted.value = name;
  if (event.shiftKey) {
    const names = visibleNames.value;
    const from = names.indexOf(anchor.value);
    const to = names.indexOf(name);
    if (from >= 0 && to >= 0) {
      const range = names.slice(Math.min(from, to), Math.max(from, to) + 1);
      const keep = event.ctrlKey || event.metaKey ? active.value : [];
      setActive([...new Set([...keep, ...range])]);
      return;
    }
  }
  anchor.value = name;
  if (event.ctrlKey || event.metaKey) {
    setActive(active.value.includes(name) ? active.value.filter((item) => item !== name) : [...active.value, name]);
  }
}

function onRowDblClick(event: MouseEvent, name: string) {
  if (event.ctrlKey || event.metaKey || event.shiftKey) return;
  show(name);
}

function onSearchKey(event: KeyboardEvent) {
  const names = visibleNames.value;
  if (event.key === "Enter") {
    const target = names.includes(highlighted.value) ? highlighted.value : names[0];
    if (target) show(target);
  } else if (event.key === "ArrowDown" || event.key === "ArrowUp") {
    event.preventDefault();
    if (!names.length) return;
    const index = names.indexOf(highlighted.value);
    const next = event.key === "ArrowDown" ? Math.min(names.length - 1, index + 1) : Math.max(0, index - 1);
    highlighted.value = names[index < 0 ? 0 : next];
  } else if (event.key === "Escape" && query.value) {
    event.stopPropagation();
    query.value = "";
  }
}
</script>

<template>
  <aside class="tree">
    <div class="search">
      <Icons name="search" :size="12" />
      <input v-model="query" placeholder="过滤分支，回车查看" @keydown="onSearchKey" />
      <button v-if="query" type="button" class="clear" title="清空" @click="query = ''">
        <Icons name="close" :size="10" />
      </button>
    </div>
    <div class="scroll">
      <button
        v-if="!searching"
        type="button"
        class="row head-row"
        :class="{ on: highlighted === 'HEAD', active: active.includes('HEAD') }"
        title="双击查看当前分支的提交，Ctrl / Shift 点击可多选"
        @click="onRowClick($event, 'HEAD')"
        @dblclick="onRowDblClick($event, 'HEAD')"
      >
        <span>HEAD（当前分支）</span>
      </button>
      <div v-for="group in groups" :key="group.key" class="group">
        <button type="button" class="row group-row" @click="toggleGroup(group.key)">
          <Icons name="chevron" :size="10" :class="{ folded: !isOpen(group.key) }" />
          <span>{{ group.label }}</span>
          <em>{{ group.items.length }}</em>
        </button>
        <template v-if="isOpen(group.key)">
          <button
            v-for="item in group.items"
            :key="item.name"
            type="button"
            class="row item"
            :class="{ on: highlighted === item.name, active: active.includes(item.name), current: item.current }"
            :title="`${item.name}\n双击查看该分支的提交，Ctrl / Shift 点击可多选`"
            @click="onRowClick($event, item.name)"
            @dblclick="onRowDblClick($event, item.name)"
          >
            <span>{{ item.name }}</span>
          </button>
        </template>
      </div>
      <p v-if="searching && !groups.length" class="empty">没有匹配的分支</p>
    </div>
    <div v-if="active.length > 1" class="multi">
      <span>已选 {{ active.length }} 个分支</span>
      <button type="button" @click="show('HEAD')">只看当前分支</button>
    </div>
  </aside>
</template>

<style scoped>
.tree {
  width: 200px;
  min-width: 160px;
  border-right: 1px solid var(--border);
  display: flex;
  flex-direction: column;
  background: var(--bg);
}

.search {
  height: 28px;
  margin: 6px;
  padding: 0 6px;
  display: flex;
  align-items: center;
  gap: 4px;
  border: 1px solid var(--border-soft);
  border-radius: 4px;
  background: var(--bg-input);
  color: var(--text-dim);
}

.search input {
  flex: 1;
  min-width: 0;
  background: transparent;
  border: 0;
  outline: none;
}

.clear {
  flex: none;
  width: 16px;
  height: 16px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border-radius: 3px;
  color: var(--text-dim);
}

.clear:hover {
  color: var(--text);
}

.group-row em {
  margin-left: auto;
  font-style: normal;
  font-size: 11px;
  color: var(--text-dim);
}

.row.active {
  box-shadow: inset 2px 0 0 var(--accent);
  background: color-mix(in srgb, var(--accent) 14%, transparent);
}

.multi {
  flex: none;
  height: 28px;
  padding: 0 8px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 6px;
  border-top: 1px solid var(--border);
  color: var(--text-muted);
  font-size: 11px;
}

.multi button {
  color: var(--link);
  font-size: 11px;
}

.empty {
  margin: 0;
  padding: 12px;
  color: var(--text-dim);
  text-align: center;
}

.scroll {
  flex: 1;
  overflow: auto;
}

.row {
  width: 100%;
  min-height: 22px;
  padding: 0 8px;
  display: flex;
  align-items: center;
  gap: 6px;
  text-align: left;
  color: var(--text);
  user-select: none;
}

.row:hover,
.row.on {
  background: var(--bg-selected);
}

.group-row {
  color: var(--text-muted);
}

.item {
  padding-left: 28px;
}

.item span,
.group-row span,
.head-row span {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.current {
  color: var(--link);
}

.folded {
  transform: rotate(-90deg);
}
</style>
