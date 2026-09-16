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

const openLocal = ref(false);
const openRemote = ref(false);
const openTags = ref(false);
const selected = ref<string[]>(["HEAD"]);
const query = ref("");

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

function isOn(name: string) {
  return selected.value.includes(name);
}

function emitRefs() {
  const names = selected.value.filter((name) => name !== "HEAD");
  if (!names.length || (selected.value.length === 1 && selected.value[0] === "HEAD")) {
    emit("change", props.currentBranch ? [props.currentBranch] : []);
    return;
  }
  emit("change", names);
}

function toggle(name: string) {
  if (name === "HEAD") {
    selected.value = ["HEAD"];
    emitRefs();
    return;
  }
  selected.value = selected.value.filter((item) => item !== "HEAD");
  if (isOn(name)) {
    selected.value = selected.value.filter((item) => item !== name);
  } else {
    selected.value = [...selected.value, name];
  }
  if (!selected.value.length) {
    selected.value = ["HEAD"];
  }
  emitRefs();
}

function toggleGroup(kind: "local" | "remote" | "tag") {
  const list =
    kind === "local" ? localBranches.value : kind === "remote" ? remoteBranches.value : tags.value;
  const names = list.map((item) => item.name);
  const allOn = names.length > 0 && names.every((name) => isOn(name));
  selected.value = selected.value.filter((item) => item !== "HEAD" && !names.includes(item));
  if (!allOn) {
    selected.value = [...selected.value, ...names];
  }
  if (!selected.value.length) {
    selected.value = ["HEAD"];
  }
  emitRefs();
}

function groupChecked(kind: "local" | "remote" | "tag") {
  const list =
    kind === "local" ? localBranches.value : kind === "remote" ? remoteBranches.value : tags.value;
  return list.length > 0 && list.every((item) => isOn(item.name));
}
</script>

<template>
  <aside class="tree">
    <div class="search">
      <Icons name="search" :size="12" />
      <input v-model="query" placeholder="过滤分支" />
    </div>
    <div class="scroll">
      <button type="button" class="row head-row" :class="{ on: isOn('HEAD') }" @click="toggle('HEAD')">
        <input type="checkbox" :checked="isOn('HEAD')" @click.stop="toggle('HEAD')" />
        <span>HEAD (当前分支)</span>
      </button>
      <div class="group">
        <button type="button" class="row group-row" @click="openLocal = !openLocal">
          <Icons name="chevron" :size="10" :class="{ folded: !openLocal }" />
          <input type="checkbox" :checked="groupChecked('local')" @click.stop="toggleGroup('local')" />
          <span>本地</span>
        </button>
        <template v-if="openLocal">
          <button
            v-for="item in localBranches"
            :key="item.name"
            type="button"
            class="row item"
            :class="{ on: isOn(item.name), current: item.current }"
            :title="item.name"
            @click="toggle(item.name)"
          >
            <input type="checkbox" :checked="isOn(item.name)" @click.stop="toggle(item.name)" />
            <span>{{ item.name }}</span>
          </button>
        </template>
      </div>
      <div class="group">
        <button type="button" class="row group-row" @click="openRemote = !openRemote">
          <Icons name="chevron" :size="10" :class="{ folded: !openRemote }" />
          <input type="checkbox" :checked="groupChecked('remote')" @click.stop="toggleGroup('remote')" />
          <span>远程</span>
        </button>
        <template v-if="openRemote">
          <button
            v-for="item in remoteBranches"
            :key="item.name"
            type="button"
            class="row item"
            :class="{ on: isOn(item.name) }"
            :title="item.name"
            @click="toggle(item.name)"
          >
            <input type="checkbox" :checked="isOn(item.name)" @click.stop="toggle(item.name)" />
            <span>{{ item.name }}</span>
          </button>
        </template>
      </div>
      <div class="group">
        <button type="button" class="row group-row" @click="openTags = !openTags">
          <Icons name="chevron" :size="10" :class="{ folded: !openTags }" />
          <input type="checkbox" :checked="groupChecked('tag')" @click.stop="toggleGroup('tag')" />
          <span>标签</span>
        </button>
        <template v-if="openTags">
          <button
            v-for="item in tags"
            :key="item.name"
            type="button"
            class="row item"
            :class="{ on: isOn(item.name) }"
            :title="item.name"
            @click="toggle(item.name)"
          >
            <input type="checkbox" :checked="isOn(item.name)" @click.stop="toggle(item.name)" />
            <span>{{ item.name }}</span>
          </button>
        </template>
      </div>
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
  width: 100%;
  background: transparent;
  border: 0;
  outline: none;
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

input[type="checkbox"] {
  accent-color: var(--accent);
}
</style>
