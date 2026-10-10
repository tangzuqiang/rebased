<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import {
  appVersion,
  checkUpdate,
  getRecentProjects,
  getSession,
  pickAndOpenRepo,
  recordRecentProject,
  removeRecentProject,
  saveSession,
} from "./api";
import { repoNameFromPath, sameRepo } from "./path";
import type { ProjectTab, RecentProject } from "./types";
import ProjectTabs from "./components/ProjectTabs.vue";
import RepoWorkspace from "./components/RepoWorkspace.vue";
import SettingsDialog from "./components/SettingsDialog.vue";
import ToastHost from "./components/ToastHost.vue";
import WelcomeView from "./components/WelcomeView.vue";
import { notify } from "./notify";
import { loadTheme } from "./theme";

const TABS_KEY = "rebased.openRepos";
const ACTIVE_KEY = "rebased.activeRepo";
const LAST_REPO_KEY = "rebased.lastRepo";

const tabs = ref<ProjectTab[]>([]);
const activePath = ref("");
const recent = ref<RecentProject[]>([]);
const showSettings = ref(false);
const version = ref("");
let persistTimer = 0;

const workspaceTabs = computed(() => [...tabs.value].sort((a, b) => a.path.localeCompare(b.path)));

function persist() {
  window.clearTimeout(persistTimer);
  persistTimer = window.setTimeout(() => {
    void saveSession({
      tabs: tabs.value.map((tab) => ({ path: tab.path, name: tab.name })),
      activePath: activePath.value,
    }).catch(() => undefined);
  }, 200);
}

async function remember(path: string, name = repoNameFromPath(path)) {
  recent.value = await recordRecentProject(path, name).catch(() => recent.value);
}

function addTab(path: string, name = repoNameFromPath(path)) {
  const existing = tabs.value.find((tab) => sameRepo(tab.path, path));
  if (existing) {
    activePath.value = existing.path;
    persist();
    void remember(existing.path, existing.name || name);
    return;
  }
  tabs.value.push({
    path,
    name,
    branch: "",
    dirty: false,
    merging: false,
  });
  activePath.value = path;
  persist();
  void remember(path, name);
}

async function handleOpen() {
  try {
    const opened = await pickAndOpenRepo();
    if (!opened) return;
    addTab(opened.path, opened.name);
  } catch (error) {
    notify(error instanceof Error ? error.message : String(error), "error", 6000);
  }
}

function handleOpenPath(path: string) {
  addTab(path);
}

function selectTab(path: string) {
  activePath.value = path;
  persist();
}

function reorderTabs(from: number, to: number) {
  if (from === to || from < 0 || to < 0 || from >= tabs.value.length || to >= tabs.value.length) return;
  const next = tabs.value.slice();
  const [moved] = next.splice(from, 1);
  if (!moved) return;
  next.splice(to, 0, moved);
  tabs.value = next;
  persist();
}

function closeTab(path: string) {
  const index = tabs.value.findIndex((tab) => sameRepo(tab.path, path));
  if (index < 0) return;
  const closingActive = sameRepo(tabs.value[index].path, activePath.value);
  tabs.value.splice(index, 1);
  if (closingActive) {
    const next = tabs.value[index] ?? tabs.value[index - 1];
    activePath.value = next?.path ?? "";
  }
  persist();
}

function reorderTab(from: number, to: number) {
  const [moved] = tabs.value.splice(from, 1);
  if (!moved) return;
  tabs.value.splice(to, 0, moved);
  persist();
}

function updateMeta(payload: ProjectTab) {
  const tab = tabs.value.find((item) => sameRepo(item.path, payload.path));
  if (!tab) return;
  const nameChanged = tab.name !== payload.name;
  tab.name = payload.name;
  tab.branch = payload.branch;
  tab.dirty = payload.dirty;
  tab.merging = payload.merging;
  if (nameChanged) {
    persist();
  }
}

async function handleRemoveRecent(path: string) {
  recent.value = await removeRecentProject(path).catch(() => recent.value);
}

function localSession(): { tabs: Array<{ path: string; name: string }>; activePath: string } {
  let saved: string[] = [];
  try {
    saved = JSON.parse(localStorage.getItem(TABS_KEY) || "[]");
  } catch {
    saved = [];
  }
  if (!saved.length) {
    const last = localStorage.getItem(LAST_REPO_KEY);
    if (last) saved = [last];
  }
  return {
    tabs: saved.filter(Boolean).map((path) => ({ path, name: repoNameFromPath(path) })),
    activePath: localStorage.getItem(ACTIVE_KEY) || saved[0] || "",
  };
}

function applySession(session: { tabs: Array<{ path: string; name: string }>; activePath: string }) {
  tabs.value = [];
  for (const item of session.tabs) {
    if (!item.path || tabs.value.some((tab) => sameRepo(tab.path, item.path))) continue;
    tabs.value.push({
      path: item.path,
      name: item.name || repoNameFromPath(item.path),
      branch: "",
      dirty: false,
      merging: false,
    });
  }
  const current = tabs.value.find((tab) => sameRepo(tab.path, session.activePath)) ?? tabs.value[0];
  activePath.value = current?.path ?? "";
}

onMounted(async () => {
  await loadTheme().catch(() => undefined);
  version.value = await appVersion().catch(() => "");
  recent.value = await getRecentProjects().catch(() => []);
  const session = await getSession().catch(() => ({ tabs: [], activePath: "" }));
  if (session.tabs.length) {
    applySession(session);
  } else {
    const legacy = localSession();
    applySession(legacy);
    if (legacy.tabs.length) {
      persist();
      for (const item of legacy.tabs) {
        await remember(item.path, item.name);
      }
      localStorage.removeItem(TABS_KEY);
      localStorage.removeItem(ACTIVE_KEY);
      localStorage.removeItem(LAST_REPO_KEY);
    }
  }
  void checkUpdate()
    .then((info) => {
      if (info.available) {
        notify(`发现新版本 ${info.latest}，可在设置中更新`);
      }
    })
    .catch(() => undefined);
});
</script>

<template>
  <div class="app">
    <ProjectTabs
      :tabs="tabs"
      :active-path="activePath"
      @select="selectTab"
      @close="closeTab"
      @add="handleOpen"
      @reorder="reorderTabs"
    />
    <WelcomeView
      v-if="!tabs.length"
      :recent="recent"
      :version="version"
      @open="handleOpen"
      @open-path="handleOpenPath"
      @remove-recent="handleRemoveRecent"
    />
    <RepoWorkspace
      v-for="tab in workspaceTabs"
      v-show="sameRepo(tab.path, activePath)"
      :key="tab.path"
      :path="tab.path"
      :active="sameRepo(tab.path, activePath)"
      :recent="recent"
      @open="handleOpen"
      @open-path="handleOpenPath"
      @settings="showSettings = true"
      @failed="closeTab"
      @meta="updateMeta"
    />
    <SettingsDialog v-if="showSettings" @close="showSettings = false" />
    <ToastHost />
  </div>
</template>

<style scoped>
.app {
  height: 100%;
  display: flex;
  flex-direction: column;
}
</style>
