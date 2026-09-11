<script setup lang="ts">
import { computed, ref } from "vue";
import type { BranchInfo } from "../types";
import Icons from "./Icons.vue";

export type BranchMenuAction =
  | "new-branch"
  | "diff"
  | "update"
  | "push"
  | "track"
  | "rename";

const props = withDefaults(
  defineProps<{
    branches: BranchInfo[];
    exclude?: string[];
    selected?: string;
    enableActions?: boolean;
  }>(),
  { enableActions: true },
);

const emit = defineEmits<{
  select: [name: string];
  action: [payload: { action: BranchMenuAction; branch: BranchInfo }];
}>();

const query = ref("");
const scope = ref<"all" | "local" | "remote">("all");
const menu = ref<{ open: boolean; x: number; y: number; branch: BranchInfo | null }>({
  open: false,
  x: 0,
  y: 0,
  branch: null,
});

const filtered = computed(() => {
  const keyword = query.value.trim().toLowerCase();
  const skip = new Set(props.exclude ?? []);
  return props.branches.filter((branch) => {
    if (skip.has(branch.name)) return false;
    if (branch.kind === "tag") return false;
    if (scope.value === "local" && branch.remote) return false;
    if (scope.value === "remote" && !branch.remote) return false;
    return !keyword || branch.name.toLowerCase().includes(keyword);
  });
});

const localBranches = computed(() => filtered.value.filter((branch) => !branch.remote));
const remoteBranches = computed(() => filtered.value.filter((branch) => branch.remote));

const trackTarget = computed(() => {
  const branch = menu.value.branch;
  if (!branch) return "";
  if (branch.upstream) return branch.upstream;
  if (branch.remote) return branch.name;
  const short = branch.name;
  const remote = props.branches.find((item) => item.remote && (item.name === `origin/${short}` || item.name.endsWith(`/${short}`)));
  return remote?.name ?? `origin/${short}`;
});

function openMenu(event: MouseEvent, branch: BranchInfo) {
  event.preventDefault();
  event.stopPropagation();
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
    scope.value = "all";
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
    <div class="scopes">
      <button type="button" :class="{ active: scope === 'all' }" @click="scope = 'all'">全部</button>
      <button type="button" :class="{ active: scope === 'local' }" @click="scope = 'local'">本地</button>
      <button type="button" :class="{ active: scope === 'remote' }" @click="scope = 'remote'">远程</button>
    </div>

    <div v-if="scope !== 'remote'" class="group">
      <div class="item dim">本地分支</div>
      <button
        v-for="branch in localBranches"
        :key="branch.name"
        type="button"
        :class="{ current: branch.current, selected: branch.name === selected }"
        :title="branch.name"
        @click="emit('select', branch.name)"
        @contextmenu="enableActions && openMenu($event, branch)"
      >
        {{ branch.name }}
      </button>
      <div v-if="!localBranches.length" class="item dim">没有匹配的本地分支</div>
    </div>

    <div v-if="scope !== 'local'" class="group">
      <div class="item dim">远程分支</div>
      <button
        v-for="branch in remoteBranches"
        :key="branch.name"
        type="button"
        :class="{ selected: branch.name === selected }"
        :title="branch.name"
        @click="emit('select', branch.name)"
        @contextmenu="enableActions && openMenu($event, branch)"
      >
        {{ branch.name }}
      </button>
      <div v-if="!remoteBranches.length" class="item dim">没有匹配的远程分支</div>
    </div>

    <Teleport to="body">
      <div v-if="menu.open" class="ctx-mask" @mousedown="closeMenu">
        <div
          class="popup-menu ctx"
          :style="{ top: `${menu.y}px`, left: `${menu.x}px` }"
          @mousedown.stop
        >
          <button type="button" @click="run('new-branch')">从 '{{ menu.branch?.name }}' 新建分支…</button>
          <button type="button" @click="run('diff')">显示与工作树的差异</button>
          <button type="button" @click="run('update')">更新</button>
          <button type="button" @click="run('push')">推送…</button>
          <button type="button" @click="run('track')">跟踪分支 '{{ trackTarget }}'</button>
          <button type="button" :disabled="Boolean(menu.branch?.remote)" @click="run('rename')">重命名…</button>
        </div>
      </div>
    </Teleport>
  </div>
</template>

<style scoped>
.branch-filter {
  padding: 6px 0 2px;
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

.scopes {
  display: flex;
  gap: 4px;
  margin: 0 8px 6px;
}

.scopes button {
  flex: 1;
  width: auto;
  height: 24px;
  padding: 0;
  border-radius: 4px;
  background: #3c3f41;
  color: var(--text-muted);
}

.scopes button.active {
  background: var(--bg-selected);
  color: var(--text);
}

.group button {
  width: 100%;
  text-align: left;
  padding: 5px 12px;
  min-height: 28px;
}

.group button:hover,
.group button.selected,
.group button.current {
  background: var(--bg-selected);
}

.group button.current {
  color: #7cb7ff;
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
