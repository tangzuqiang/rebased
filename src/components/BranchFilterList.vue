<script setup lang="ts">
import { computed, ref } from "vue";
import type { BranchInfo } from "../types";
import Icons from "./Icons.vue";

export type BranchMenuAction =
  | "checkout"
  | "new-branch"
  | "diff"
  | "update"
  | "push"
  | "track"
  | "rename";

type GroupKey = "recent" | "local" | "remote" | "tag";

const props = withDefaults(
  defineProps<{
    branches: BranchInfo[];
    exclude?: string[];
    selected?: string;
    enableActions?: boolean;
    recent?: string[];
    defaultOpen?: GroupKey[];
  }>(),
  { enableActions: true, defaultOpen: () => ["recent"] },
);

const emit = defineEmits<{
  select: [name: string];
  action: [payload: { action: BranchMenuAction; branch: BranchInfo }];
}>();

const query = ref("");
const open = ref<Record<GroupKey, boolean>>({
  recent: props.defaultOpen.includes("recent"),
  local: props.defaultOpen.includes("local"),
  remote: props.defaultOpen.includes("remote"),
  tag: props.defaultOpen.includes("tag"),
});
const menu = ref<{ open: boolean; x: number; y: number; branch: BranchInfo | null }>({
  open: false,
  x: 0,
  y: 0,
  branch: null,
});

const searching = computed(() => Boolean(query.value.trim()));

const visibleBranches = computed(() => {
  const keyword = query.value.trim().toLowerCase();
  const skip = new Set(props.exclude ?? []);
  return props.branches.filter((branch) => {
    if (skip.has(branch.name)) return false;
    return !keyword || branch.name.toLowerCase().includes(keyword);
  });
});

const localBranches = computed(() =>
  visibleBranches.value.filter((branch) => branch.kind !== "tag" && !branch.remote),
);
const remoteBranches = computed(() =>
  visibleBranches.value.filter((branch) => branch.kind !== "tag" && branch.remote),
);
const tagBranches = computed(() => visibleBranches.value.filter((branch) => branch.kind === "tag"));
const recentBranches = computed(() => {
  const keyword = query.value.trim().toLowerCase();
  return (props.recent ?? [])
    .slice(0, 4)
    .map((name) => visibleBranches.value.find((item) => item.name === name)
      ?? props.branches.find((item) => item.name === name)
      ?? {
        name,
        current: name === props.selected,
        remote: name.includes("/"),
        kind: name.includes("/") ? "remote" : "local",
      } as BranchInfo)
    .filter((branch) => {
      if ((props.exclude ?? []).includes(branch.name)) return false;
      return !keyword || branch.name.toLowerCase().includes(keyword);
    });
});

const trackTarget = computed(() => {
  const branch = menu.value.branch;
  if (!branch) return "";
  if (branch.upstream) return branch.upstream;
  if (branch.remote) return branch.name;
  const short = branch.name;
  const remote = props.branches.find((item) => item.remote && (item.name === `origin/${short}` || item.name.endsWith(`/${short}`)));
  return remote?.name ?? `origin/${short}`;
});

function isOpen(key: GroupKey) {
  return searching.value || open.value[key];
}

function toggleGroup(key: GroupKey) {
  open.value[key] = !open.value[key];
}

function upstreamOf(branch: BranchInfo) {
  if (branch.kind === "tag") return "";
  if (branch.upstream) return branch.upstream;
  if (branch.remote) return "";
  const short = branch.name;
  return props.branches.find((item) => item.remote && (item.name === `origin/${short}` || item.name.endsWith(`/${short}`)))?.name ?? "";
}

const highlighted = ref("");

function onLeafClick(_event: MouseEvent, branch: BranchInfo) {
  highlighted.value = branch.name;
  if (props.enableActions) {
    return;
  }
  emit("select", branch.name);
}

function openMenu(event: MouseEvent, branch: BranchInfo) {
  event.preventDefault();
  event.stopPropagation();
  highlighted.value = branch.name;
  menu.value = { open: true, x: event.clientX, y: event.clientY, branch };
}

function closeMenu() {
  menu.value.open = false;
}

function run(action: BranchMenuAction) {
  const branch = menu.value.branch;
  closeMenu();
  if (branch) emit("action", { action, branch });
}

defineExpose({
  reset() {
    query.value = "";
    highlighted.value = "";
    open.value = {
      recent: props.defaultOpen.includes("recent"),
      local: props.defaultOpen.includes("local"),
      remote: props.defaultOpen.includes("remote"),
      tag: props.defaultOpen.includes("tag"),
    };
    closeMenu();
  },
});
</script>

<template>
  <div class="branch-filter" @mousedown.stop>
    <label class="search">
      <Icons name="search" :size="13" />
      <input v-model="query" placeholder="过滤分支名称" />
    </label>

    <div class="tree">
      <div v-if="recent" class="group">
        <button type="button" class="group-row" @click="toggleGroup('recent')">
          <Icons name="chevron" :size="10" :class="{ folded: !isOpen('recent') }" />
          <span>最近</span>
        </button>
        <template v-if="isOpen('recent')">
          <button
            v-for="branch in recentBranches"
            :key="`recent:${branch.name}`"
            type="button"
            class="leaf"
            :class="{ current: branch.current, selected: branch.name === (highlighted || selected) }"
            :title="branch.name"
            @click="onLeafClick($event, branch)"
            @contextmenu="enableActions && openMenu($event, branch)"
          >
            <Icons :name="branch.current ? 'star' : 'circle'" :size="11" class="mark" />
            <span class="name">{{ branch.name }}</span>
            <span v-if="upstreamOf(branch)" class="track">{{ upstreamOf(branch) }}</span>
          </button>
          <div v-if="!recentBranches.length" class="empty">没有最近分支</div>
        </template>
      </div>

      <div v-if="!searching" class="hint">输入分支名称进行搜索</div>

      <div v-if="searching" class="group">
        <button type="button" class="group-row" @click="toggleGroup('local')">
          <Icons name="chevron" :size="10" :class="{ folded: !isOpen('local') }" />
          <span>本地</span>
        </button>
        <template v-if="isOpen('local')">
          <button
            v-for="branch in localBranches"
            :key="`local:${branch.name}`"
            type="button"
            class="leaf"
            :class="{ current: branch.current, selected: branch.name === (highlighted || selected) }"
            :title="branch.name"
            @click="onLeafClick($event, branch)"
            @contextmenu="enableActions && openMenu($event, branch)"
          >
            <Icons :name="branch.current ? 'star' : 'circle'" :size="11" class="mark" />
            <span class="name">{{ branch.name }}</span>
            <span v-if="upstreamOf(branch)" class="track">{{ upstreamOf(branch) }}</span>
          </button>
          <div v-if="!localBranches.length" class="empty">没有匹配的本地分支</div>
        </template>
      </div>

      <div v-if="searching" class="group">
        <button type="button" class="group-row" @click="toggleGroup('remote')">
          <Icons name="chevron" :size="10" :class="{ folded: !isOpen('remote') }" />
          <span>远程</span>
        </button>
        <template v-if="isOpen('remote')">
          <button
            v-for="branch in remoteBranches"
            :key="`remote:${branch.name}`"
            type="button"
            class="leaf"
            :class="{ selected: branch.name === (highlighted || selected) }"
            :title="branch.name"
            @click="onLeafClick($event, branch)"
            @contextmenu="enableActions && openMenu($event, branch)"
          >
            <Icons name="circle" :size="11" class="mark" />
            <span class="name">{{ branch.name }}</span>
          </button>
          <div v-if="!remoteBranches.length" class="empty">没有匹配的远程分支</div>
        </template>
      </div>

      <div v-if="searching" class="group">
        <button type="button" class="group-row" @click="toggleGroup('tag')">
          <Icons name="chevron" :size="10" :class="{ folded: !isOpen('tag') }" />
          <span>标签</span>
        </button>
        <template v-if="isOpen('tag')">
          <button
            v-for="branch in tagBranches"
            :key="`tag:${branch.name}`"
            type="button"
            class="leaf"
            :class="{ selected: branch.name === (highlighted || selected) }"
            :title="branch.name"
            @click="onLeafClick($event, branch)"
            @contextmenu="enableActions && openMenu($event, branch)"
          >
            <Icons name="circle" :size="11" class="mark" />
            <span class="name">{{ branch.name }}</span>
          </button>
          <div v-if="!tagBranches.length" class="empty">没有匹配的标签</div>
        </template>
      </div>
    </div>

    <Teleport to="body">
      <div v-if="menu.open" class="ctx-mask" @mousedown="closeMenu">
        <div
          class="popup-menu ctx"
          :style="{ top: `${menu.y}px`, left: `${menu.x}px` }"
          @mousedown.stop
        >
          <button type="button" :disabled="Boolean(menu.branch?.current)" @click="run('checkout')">签出</button>
          <button type="button" @click="run('new-branch')">从 '{{ menu.branch?.name }}' 新建分支…</button>
          <button type="button" @click="run('diff')">显示与工作树的差异</button>
          <button type="button" @click="run('update')">更新</button>
          <button type="button" @click="run('push')">推送…</button>
          <button type="button" @click="run('track')">跟踪分支 '{{ trackTarget }}'</button>
          <button type="button" :disabled="Boolean(menu.branch?.remote || menu.branch?.kind === 'tag')" @click="run('rename')">重命名…</button>
        </div>
      </div>
    </Teleport>
  </div>
</template>

<style scoped>
.branch-filter {
  padding: 6px 0 2px;
  min-width: 320px;
}

.search {
  display: flex;
  align-items: center;
  gap: 6px;
  margin: 0 8px 6px;
  height: 26px;
  padding: 0 8px;
  border: 1px solid var(--border-soft);
  background: var(--bg-input);
  border-radius: 4px;
  color: var(--text-muted);
}

.search input {
  flex: 1;
  min-width: 0;
  border: 0;
  outline: none;
  background: transparent;
  color: var(--text);
}

.tree {
  max-height: 360px;
  overflow: auto;
}

.group-row,
.leaf {
  display: flex !important;
  align-items: center;
  gap: 6px;
  width: 100%;
  min-height: 24px;
  padding: 2px 10px;
  text-align: left;
  white-space: nowrap;
}

.group-row {
  color: var(--text-muted);
}

.group-row:hover,
.leaf:hover,
.leaf.selected,
.leaf.current {
  background: var(--bg-selected);
}

.leaf {
  padding-left: 22px;
}

.leaf.current {
  color: var(--link);
}

.mark {
  flex: none;
  color: #c9cdd3;
}

.leaf.current .mark {
  color: #e8c35a;
}

.name {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
}

.track {
  margin-left: auto;
  max-width: 42%;
  overflow: hidden;
  text-overflow: ellipsis;
  color: var(--text-dim);
  font-size: 11px;
}

.empty,
.hint {
  padding: 4px 10px 6px 22px;
  color: var(--text-dim);
  font-size: 12px;
}

.hint {
  padding: 8px 10px 10px;
}

.folded {
  transform: rotate(-90deg);
}

.ctx-mask {
  position: fixed;
  inset: 0;
  z-index: 1300;
}

.ctx {
  position: fixed;
  min-width: 260px;
  z-index: 1301;
}
</style>
