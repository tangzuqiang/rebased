<script setup lang="ts">
import { computed, ref } from "vue";
import { sameRepo } from "../path";
import type { BranchInfo, RecentProject, RepoInfo } from "../types";
import BranchFilterList, { type BranchMenuAction } from "./BranchFilterList.vue";
import Icons from "./Icons.vue";
import PopupMenu from "./PopupMenu.vue";

const props = defineProps<{
  repo: RepoInfo;
  branches: BranchInfo[];
  busy: boolean;
  dirty: boolean;
  merging?: boolean;
  recent?: RecentProject[];
  filesOpen?: boolean;
  terminalOpen?: boolean;
  busyText?: string;
  recentBranches?: string[];
  version?: string;
}>();

const emit = defineEmits<{
  open: [];
  openPath: [path: string];
  fetch: [];
  pull: [];
  commit: [];
  push: [];
  merge: [];
  conflicts: [];
  settings: [];
  files: [];
  terminal: [];
  newBranch: [];
  branchAction: [payload: { action: BranchMenuAction; branch: BranchInfo }];
}>();

const repoOpen = ref(false);
const branchOpen = ref(false);
const repoAnchor = ref<HTMLElement | null>(null);
const branchAnchor = ref<HTMLElement | null>(null);

const branchFilter = ref<InstanceType<typeof BranchFilterList> | null>(null);
const otherRecent = computed(
  () => props.recent?.filter((item) => !sameRepo(item.path, props.repo.path)) ?? [],
);

function toggleRepo() {
  branchOpen.value = false;
  repoOpen.value = !repoOpen.value;
}

function toggleBranch() {
  repoOpen.value = false;
  branchOpen.value = !branchOpen.value;
  if (!branchOpen.value) {
    branchFilter.value?.reset();
  }
}

function run(action: "commit" | "push" | "merge" | "fetch" | "pull" | "new-branch") {
  branchOpen.value = false;
  if (action === "commit") emit("commit");
  else if (action === "push") emit("push");
  else if (action === "merge") emit("merge");
  else if (action === "fetch") emit("fetch");
  else if (action === "new-branch") emit("newBranch");
  else emit("pull");
}
</script>

<template>
  <header class="toolbar">
    <div class="left">
      <button
        type="button"
        class="icon-btn"
        :class="{ active: filesOpen }"
        title="项目"
        @click="emit('files')"
      >
        <Icons name="folder" />
      </button>
      <button
        type="button"
        class="icon-btn"
        :class="{ active: terminalOpen }"
        title="终端 (Alt+F12)"
        @click="emit('terminal')"
      >
        <Icons name="terminal" />
      </button>
      <img src="/rebased.svg" width="18" height="18" alt="" />
      <span v-if="version" class="app-ver" :title="`Rebased ${version}`">{{ version }}</span>
      <button ref="repoAnchor" type="button" class="combo" @click="toggleRepo">
        <span>{{ repo.name }}</span>
        <Icons name="chevron" :size="12" />
      </button>
      <PopupMenu :open="repoOpen" :anchor="repoAnchor" @close="repoOpen = false">
        <button type="button" @click="repoOpen = false; emit('open')">打开仓库…</button>
        <button type="button" @click="repoOpen = false; emit('settings')">设置…</button>
        <div class="item dim" :title="repo.path">{{ repo.path }}</div>
        <template v-if="otherRecent.length">
          <div class="divider" />
          <div class="item dim">最近项目</div>
          <button
            v-for="item in otherRecent"
            :key="item.path"
            type="button"
            :title="item.path"
            @click="repoOpen = false; emit('openPath', item.path)"
          >
            {{ item.name }}
          </button>
        </template>
      </PopupMenu>

      <span class="sep" />
      <span v-if="dirty" class="dirty" title="工作区有未提交变更">*</span>

      <button ref="branchAnchor" type="button" class="combo" :class="{ busy }" @click="toggleBranch">
        <span v-if="busy" class="btn-spinner" />
        <Icons v-else name="branch" :size="12" />
        <span>{{ busy ? (busyText || "Git 操作中…") : repo.branch }}</span>
        <Icons name="chevron" :size="12" />
      </button>
      <PopupMenu :open="branchOpen" :anchor="branchAnchor" @close="branchOpen = false">
        <div class="item dim">Git 操作</div>
        <button type="button" :disabled="busy" @click="run('commit')">提交…</button>
        <button type="button" :disabled="busy" @click="run('push')">推送…</button>
        <button type="button" :disabled="busy" @click="run('pull')">
          <span v-if="busy && busyText?.includes('拉取')" class="btn-spinner" />
          拉取
        </button>
        <button type="button" :disabled="busy" @click="run('fetch')">
          <span v-if="busy && busyText?.includes('获取')" class="btn-spinner" />
          获取
        </button>
        <button type="button" :disabled="busy" @click="run('merge')">合并…</button>
        <button type="button" :disabled="busy" @click="run('new-branch')">新建分支…</button>
        <button v-if="merging" type="button" @click="branchOpen = false; emit('conflicts')">解决冲突…</button>
        <div class="divider" />
        <BranchFilterList
          ref="branchFilter"
          :branches="branches"
          :selected="repo.branch"
          :recent="recentBranches"
          @action="branchOpen = false; emit('branchAction', $event)"
        />
      </PopupMenu>

      <button
        v-if="merging"
        type="button"
        class="conflict-btn"
        title="当前合并存在未解决冲突"
        @click="emit('conflicts')"
      >
        冲突…
      </button>

      <span v-if="repo.ahead || repo.behind" class="sync muted">
        <template v-if="repo.ahead">↑{{ repo.ahead }}</template>
        <template v-if="repo.behind"> ↓{{ repo.behind }}</template>
      </span>
    </div>

    <div class="right">
      <button class="icon-btn" title="搜索">
        <Icons name="search" />
      </button>
      <button class="icon-btn" title="设置" @click="emit('settings')">
        <Icons name="gear" />
      </button>
    </div>
  </header>
</template>

<style scoped>
.toolbar {
  height: 36px;
  padding: 0 10px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  border-bottom: 1px solid var(--border);
  background: #2b2d30;
  overflow: visible;
}

.left,
.right {
  display: flex;
  align-items: center;
  gap: 6px;
}

.combo {
  position: relative;
  display: inline-flex;
  align-items: center;
  gap: 4px;
  height: 24px;
  padding: 0 6px;
  border-radius: 4px;
  color: var(--text);
}

.combo:hover {
  background: var(--bg-hover);
}

.combo.busy {
  color: #7cb7ff;
  cursor: wait;
}

.sep {
  width: 1px;
  height: 16px;
  background: var(--border-soft);
  margin: 0 2px;
}

.app-ver {
  color: var(--text-muted);
  font-size: 11px;
  line-height: 1;
}

.dirty {
  color: #c9a227;
  font-weight: 700;
}

.conflict-btn {
  height: 22px;
  padding: 0 8px;
  border-radius: 4px;
  color: #ffd6d6;
  background: #5a2a2a;
}

.sync {
  font-size: 11px;
}

.current {
  color: #7cb7ff;
}
</style>
