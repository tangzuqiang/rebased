<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref } from "vue";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  absWorktreePath,
  createWorktreeDir,
  createWorktreeFile,
  deleteWorktree,
  listDir,
  previewFile,
  revealInOs,
  renameWorktree,
  saveWorktreeFile,
  searchWorkspace,
} from "../api";
import { useEditorTabs } from "../composables/useEditorTabs";
import { notify } from "../notify";
import { sameRepo } from "../path";
import type { WorkspaceEntry, WorkspaceSearchHit } from "../types";
import EditorTabBar from "./EditorTabBar.vue";
import Icons from "./Icons.vue";
import IdeEditorPane from "./IdeEditorPane.vue";

interface TreeItem extends WorkspaceEntry {
  loaded: boolean;
  expanded: boolean;
  children: TreeItem[];
}

const props = defineProps<{ repoPath: string }>();
const emit = defineEmits<{
  changed: [];
  openTerminal: [cwd: string];
}>();

const mode = ref<"explorer" | "search">("explorer");
const roots = ref<TreeItem[]>([]);
const selectedEntry = ref<TreeItem | null>(null);
const treeFilter = ref("");
const loadingTree = ref(false);
const treeError = ref("");
const searchInput = ref<HTMLInputElement | null>(null);
const searchQuery = ref("");
const searchInclude = ref("");
const searchRegex = ref(false);
const searchCase = ref(false);
const searching = ref(false);
const searchError = ref("");
const searchHits = ref<WorkspaceSearchHit[]>([]);
const searchTruncated = ref(false);
const targetLine = ref(1);
const targetColumn = ref(1);
const readOnlyPaths = ref(new Set<string>());
const opening = ref("");
const saving = ref(false);
const menu = ref({ open: false, x: 0, y: 0, item: null as TreeItem | null });
const tabsState = useEditorTabs(() => props.repoPath);

const activeTab = computed(
  () => tabsState.tabs.value.find((tab) => tab.path === tabsState.activePath.value) ?? null,
);

const visibleTree = computed(() => {
  const query = treeFilter.value.trim().toLocaleLowerCase();
  const out: Array<{ item: TreeItem; depth: number }> = [];
  const visit = (items: TreeItem[], depth: number) => {
    for (const item of items) {
      const childMatch = query && containsMatch(item, query);
      if (!query || item.name.toLocaleLowerCase().includes(query) || childMatch) {
        out.push({ item, depth });
      }
      if (item.isDir && (item.expanded || (query && childMatch))) visit(item.children, depth + 1);
    }
  };
  visit(roots.value, 0);
  return out;
});

function asTree(entries: WorkspaceEntry[]): TreeItem[] {
  return entries.map((entry) => ({ ...entry, loaded: false, expanded: false, children: [] }));
}

function containsMatch(item: TreeItem, query: string): boolean {
  return item.name.toLocaleLowerCase().includes(query) || item.children.some((child) => containsMatch(child, query));
}

async function loadRoot(silent = false) {
  if (!silent) loadingTree.value = true;
  treeError.value = "";
  try {
    const previous = new Map(roots.value.map((item) => [item.path, item]));
    roots.value = asTree(await listDir(props.repoPath)).map((item) => {
      const old = previous.get(item.path);
      return old ? { ...item, loaded: old.loaded, expanded: old.expanded, children: old.children } : item;
    });
  } catch (error) {
    treeError.value = error instanceof Error ? error.message : String(error);
  } finally {
    loadingTree.value = false;
  }
}

async function loadChildren(item: TreeItem, force = false) {
  if (!item.isDir || (item.loaded && !force)) return;
  const previous = new Map(item.children.map((child) => [child.path, child]));
  const entries = asTree(await listDir(props.repoPath, item.path));
  item.children = entries.map((child) => {
    const old = previous.get(child.path);
    return old ? { ...child, loaded: old.loaded, expanded: old.expanded, children: old.children } : child;
  });
  item.loaded = true;
}

async function toggleItem(item: TreeItem) {
  selectedEntry.value = item;
  if (!item.isDir) {
    await openFile(item.path);
    return;
  }
  try {
    await loadChildren(item);
    item.expanded = !item.expanded;
  } catch (error) {
    notify(error instanceof Error ? error.message : String(error), "error");
  }
}

async function openFile(path: string, line = 1, column = 1) {
  const existing = tabsState.tabs.value.find((tab) => tab.path === path);
  targetLine.value = line;
  targetColumn.value = column;
  if (existing) {
    tabsState.selectTab(path);
    await retriggerPosition(line, column);
    return;
  }
  opening.value = path;
  try {
    const file = await previewFile(props.repoPath, path);
    if (file.binary) {
      notify(`${path} 是二进制文件，只能只读查看`, "error");
    } else if (file.truncated) {
      notify(`${path} 过大，仅显示前 512KB`, "error");
    }
    const next = new Set(readOnlyPaths.value);
    if (file.binary || file.truncated) next.add(path);
    else next.delete(path);
    readOnlyPaths.value = next;
    const opened = tabsState.openTab({ path, content: file.content });
    if (!opened) notify("已达到 20 个编辑标签上限，请先关闭部分文件", "error");
    await retriggerPosition(line, column);
  } catch (error) {
    notify(error instanceof Error ? error.message : String(error), "error");
  } finally {
    opening.value = "";
  }
}

async function retriggerPosition(line: number, column: number) {
  targetLine.value = 0;
  await nextTick();
  targetLine.value = line;
  targetColumn.value = column;
}

async function saveActive(content = activeTab.value?.content ?? "") {
  const tab = activeTab.value;
  if (!tab || readOnlyPaths.value.has(tab.path) || saving.value) return;
  saving.value = true;
  try {
    const result = await saveWorktreeFile(props.repoPath, tab.path, content);
    tabsState.updateContent(tab.path, result.content);
    tabsState.markSaved(tab.path, result.content);
    emit("changed");
    notify(`已保存 ${tab.path}`);
  } catch (error) {
    notify(error instanceof Error ? error.message : String(error), "error");
  } finally {
    saving.value = false;
  }
}

function closeTab(path: string) {
  const result = tabsState.closeTab(path);
  if (!result.needsConfirmation) return;
  if (window.confirm(`${result.tab?.name ?? path} 尚未保存，确定关闭吗？`)) {
    tabsState.closeTab(path, true);
  }
}

function parentPath(item: TreeItem | null) {
  if (!item) return "";
  if (item.isDir) return item.path;
  const parts = item.path.split("/");
  parts.pop();
  return parts.join("/");
}

function joinPath(parent: string, name: string) {
  return parent ? `${parent}/${name}` : name;
}

async function createEntry(kind: "file" | "dir") {
  const base = parentPath(selectedEntry.value);
  const name = window.prompt(kind === "file" ? "新文件名" : "新文件夹名");
  if (!name?.trim()) return;
  const path = joinPath(base, name.trim().replace(/\\/g, "/"));
  try {
    if (kind === "file") await createWorktreeFile(props.repoPath, path);
    else await createWorktreeDir(props.repoPath, path);
    await refreshParent(base);
    emit("changed");
    if (kind === "file") await openFile(path);
  } catch (error) {
    notify(error instanceof Error ? error.message : String(error), "error");
  }
}

async function renameEntry(item: TreeItem) {
  const name = window.prompt("新名称", item.name);
  if (!name?.trim() || name.trim() === item.name) return;
  const parent = parentPath(item);
  const destination = joinPath(parent, name.trim());
  const open = tabsState.tabs.value.find((tab) => tab.path === item.path);
  if (open?.dirty && !window.confirm(`${open.name} 尚未保存，重命名会丢弃修改，继续吗？`)) return;
  try {
    await renameWorktree(props.repoPath, item.path, destination);
    if (open) {
      tabsState.closeTab(item.path, true);
      await openFile(destination);
    }
    await refreshParent(parent);
    emit("changed");
  } catch (error) {
    notify(error instanceof Error ? error.message : String(error), "error");
  }
}

async function deleteEntry(item: TreeItem) {
  if (!window.confirm(`确定删除 ${item.path}${item.isDir ? " 及其全部内容" : ""}？`)) return;
  try {
    await deleteWorktree(props.repoPath, item.path);
    for (const tab of [...tabsState.tabs.value]) {
      if (tab.path === item.path || tab.path.startsWith(`${item.path}/`)) tabsState.closeTab(tab.path, true);
    }
    await refreshParent(parentPath(item));
    emit("changed");
  } catch (error) {
    notify(error instanceof Error ? error.message : String(error), "error");
  }
}

async function refreshParent(path: string) {
  if (!path) return loadRoot(true);
  const item = findTreeItem(roots.value, path);
  if (item) await loadChildren(item, true);
  else await loadRoot(true);
}

function findTreeItem(items: TreeItem[], path: string): TreeItem | null {
  for (const item of items) {
    if (item.path === path) return item;
    const found = findTreeItem(item.children, path);
    if (found) return found;
  }
  return null;
}

async function runSearch() {
  const query = searchQuery.value;
  if (!query) return;
  searching.value = true;
  searchError.value = "";
  try {
    const result = await searchWorkspace(props.repoPath, query, {
      regex: searchRegex.value,
      caseSensitive: searchCase.value,
      include: searchInclude.value.split(",").map((value) => value.trim()).filter(Boolean),
      maxResults: 500,
    });
    searchHits.value = result.hits;
    searchTruncated.value = result.truncated;
  } catch (error) {
    searchError.value = error instanceof Error ? error.message : String(error);
    searchHits.value = [];
  } finally {
    searching.value = false;
  }
}

function showSearch() {
  mode.value = "search";
  void nextTick(() => searchInput.value?.focus());
}

function showExplorer() {
  mode.value = "explorer";
}

async function copyEntryPath(item: TreeItem, full: boolean) {
  try {
    const path = full ? await absWorktreePath(props.repoPath, item.path) : item.path;
    await navigator.clipboard.writeText(path);
    notify(full ? "已复制完整路径" : "已复制路径");
  } catch (error) {
    notify(error instanceof Error ? error.message : String(error), "error");
  }
}

async function revealEntry(item: TreeItem) {
  try {
    await revealInOs(props.repoPath, item.path);
  } catch (error) {
    notify(error instanceof Error ? error.message : String(error), "error");
  }
}

function openMenu(event: MouseEvent, item: TreeItem) {
  event.preventDefault();
  selectedEntry.value = item;
  menu.value = {
    open: true,
    x: Math.min(event.clientX, window.innerWidth - 220),
    y: Math.min(event.clientY, window.innerHeight - 260),
    item,
  };
}

function closeMenu() {
  menu.value.open = false;
}

async function openTerminal(item: TreeItem | null) {
  const rel = item ? parentPath(item) : "";
  const path = rel ? await absWorktreePath(props.repoPath, rel) : props.repoPath;
  emit("openTerminal", path);
}

let stopWatch: UnlistenFn | undefined;
let refreshTimer = 0;

onMounted(() => {
  void loadRoot();
  void listen<{ path: string; kind?: string }>("workdir-changed", (event) => {
    if (!sameRepo(event.payload.path, props.repoPath) || event.payload.kind === "git") return;
    window.clearTimeout(refreshTimer);
    refreshTimer = window.setTimeout(() => void loadRoot(true), 250);
  }).then((unlisten) => {
    stopWatch = unlisten;
  });
});

onUnmounted(() => {
  stopWatch?.();
  window.clearTimeout(refreshTimer);
});

defineExpose({ showSearch, showExplorer });
</script>

<template>
  <section class="ide-workspace">
    <nav class="activity">
      <button class="activity-btn" :class="{ active: mode === 'explorer' }" title="资源管理器" @click="showExplorer">
        <Icons name="folder" :size="17" />
      </button>
      <button class="activity-btn" :class="{ active: mode === 'search' }" title="项目搜索" @click="showSearch">
        <Icons name="search" :size="17" />
      </button>
    </nav>

    <aside class="sidebar">
      <template v-if="mode === 'explorer'">
        <header class="side-head">
          <strong>资源管理器</strong>
          <span class="head-actions">
            <button title="新建文件" @click="createEntry('file')"><Icons name="file" :size="13" /><b>+</b></button>
            <button title="新建文件夹" @click="createEntry('dir')"><Icons name="folder" :size="13" /><b>+</b></button>
            <button title="刷新" @click="loadRoot()"><Icons name="refresh" :size="13" /></button>
          </span>
        </header>
        <label class="filter"><Icons name="search" :size="12" /><input v-model="treeFilter" placeholder="过滤文件" /></label>
        <div class="tree-scroll">
          <p v-if="loadingTree" class="hint">正在读取目录…</p>
          <p v-else-if="treeError" class="hint error">{{ treeError }}</p>
          <button
            v-for="{ item, depth } in visibleTree"
            :key="item.path"
            class="tree-row"
            :class="{ selected: selectedEntry?.path === item.path }"
            :style="{ paddingLeft: `${8 + depth * 13}px` }"
            :title="item.path"
            @click="toggleItem(item)"
            @contextmenu="openMenu($event, item)"
          >
            <span class="twist" :class="{ open: item.expanded, hidden: !item.isDir }"><Icons name="chevron" :size="9" /></span>
            <Icons :name="item.isDir ? 'folder' : 'file'" :size="13" />
            <span class="ellipsis">{{ item.name }}</span>
          </button>
        </div>
      </template>

      <template v-else>
        <header class="side-head"><strong>项目搜索</strong></header>
        <form class="search-form" @submit.prevent="runSearch">
          <input ref="searchInput" v-model="searchQuery" placeholder="搜索文本" />
          <input v-model="searchInclude" placeholder="包含：*.ts, src/*" />
          <div class="search-options">
            <label><input v-model="searchCase" type="checkbox" /> Aa</label>
            <label><input v-model="searchRegex" type="checkbox" /> 正则</label>
            <button type="submit" :disabled="searching || !searchQuery">{{ searching ? "搜索中…" : "搜索" }}</button>
          </div>
        </form>
        <p v-if="searchError" class="hint error">{{ searchError }}</p>
        <div class="search-results">
          <button
            v-for="(hit, index) in searchHits"
            :key="`${hit.path}:${hit.line}:${hit.column}:${index}`"
            class="search-hit"
            :title="hit.text"
            @click="openFile(hit.path, hit.line, hit.column)"
          >
            <strong>{{ hit.path }}</strong>
            <span>{{ hit.line }}:{{ hit.column }}　{{ hit.text.trim() }}</span>
          </button>
          <p v-if="!searching && searchQuery && !searchHits.length && !searchError" class="hint">没有匹配结果</p>
          <p v-if="searchTruncated" class="hint">结果过多，仅显示前 {{ searchHits.length }} 条</p>
        </div>
      </template>
    </aside>

    <main class="editor-area">
      <EditorTabBar
        :tabs="tabsState.tabs.value"
        :active-path="tabsState.activePath.value"
        @select="tabsState.selectTab"
        @close="closeTab"
      />
      <div v-if="opening" class="editor-empty">正在打开 {{ opening }}…</div>
      <IdeEditorPane
        v-else-if="activeTab"
        :key="activeTab.path"
        :path="activeTab.path"
        :content="activeTab.content"
        :read-only="readOnlyPaths.has(activeTab.path)"
        :line="targetLine"
        :column="targetColumn"
        @change="tabsState.updateContent(activeTab.path, $event)"
        @save="saveActive"
      />
      <div v-else class="editor-empty">
        <Icons name="file" :size="30" />
        <span>从资源管理器或搜索结果中打开文件</span>
      </div>
      <footer v-if="activeTab" class="editor-status">
        <span>{{ activeTab.path }}</span>
        <span>{{ activeTab.dirty ? "未保存" : "已保存" }}{{ saving ? " · 保存中…" : "" }}</span>
      </footer>
    </main>

    <Teleport to="body">
      <div v-if="menu.open" class="menu-mask" @mousedown="closeMenu">
        <div class="popup-menu context-menu" :style="{ left: `${menu.x}px`, top: `${menu.y}px` }" @mousedown.stop>
          <button v-if="menu.item?.isDir" @click="closeMenu(); createEntry('file')">新建文件…</button>
          <button v-if="menu.item?.isDir" @click="closeMenu(); createEntry('dir')">新建文件夹…</button>
          <button @click="closeMenu(); menu.item && renameEntry(menu.item)">重命名…</button>
          <button @click="closeMenu(); menu.item && copyEntryPath(menu.item, false)">复制路径</button>
          <button @click="closeMenu(); menu.item && copyEntryPath(menu.item, true)">复制完整路径</button>
          <button @click="closeMenu(); menu.item && revealEntry(menu.item)">在资源管理器中打开</button>
          <button @click="closeMenu(); openTerminal(menu.item)">在终端中打开</button>
          <div class="divider" />
          <button class="danger" @click="closeMenu(); menu.item && deleteEntry(menu.item)">删除…</button>
        </div>
      </div>
    </Teleport>
  </section>
</template>

<style scoped>
.ide-workspace { flex: 1; min-width: 420px; min-height: 0; display: flex; background: var(--bg); }
.activity { width: 38px; flex: none; display: flex; flex-direction: column; align-items: center; border-right: 1px solid var(--border); background: var(--bg-deep); }
.activity-btn { width: 38px; height: 38px; color: var(--text-muted); border-left: 2px solid transparent; }
.activity-btn:hover, .activity-btn.active { color: var(--text); background: var(--bg-hover); }
.activity-btn.active { border-left-color: var(--accent); }
.sidebar { width: 235px; min-width: 180px; flex: none; display: flex; flex-direction: column; border-right: 1px solid var(--border); overflow: hidden; }
.side-head { height: 32px; padding: 0 8px 0 10px; display: flex; align-items: center; justify-content: space-between; flex: none; border-bottom: 1px solid var(--border-soft); }
.side-head strong { font-size: 11px; text-transform: uppercase; }
.head-actions { display: flex; }
.head-actions button { width: 24px; height: 24px; position: relative; color: var(--text-muted); }
.head-actions button:hover { color: var(--text); background: var(--bg-hover); border-radius: 3px; }
.head-actions b { position: absolute; right: 1px; bottom: 0; font-size: 10px; }
.filter { margin: 6px; height: 24px; padding: 0 6px; display: flex; align-items: center; gap: 5px; border: 1px solid var(--border-soft); background: var(--bg-input); }
.filter input, .search-form input { min-width: 0; width: 100%; border: 0; outline: 0; background: transparent; }
.tree-scroll, .search-results { flex: 1; min-height: 0; overflow: auto; }
.tree-row { width: 100%; height: 23px; display: flex; align-items: center; gap: 4px; text-align: left; color: var(--text-muted); }
.tree-row:hover { background: var(--bg-hover); color: var(--text); }
.tree-row.selected { background: var(--bg-selected); color: var(--text); }
.twist { width: 10px; flex: none; display: inline-flex; transition: transform .1s; }
.twist.open { transform: rotate(180deg); }
.twist.hidden { visibility: hidden; }
.ellipsis { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.editor-area { flex: 1; min-width: 0; min-height: 0; display: flex; flex-direction: column; background: var(--bg-editor); }
.editor-empty { flex: 1; display: flex; align-items: center; justify-content: center; flex-direction: column; gap: 10px; color: var(--text-dim); }
.editor-status { height: 22px; padding: 0 8px; display: flex; justify-content: space-between; align-items: center; border-top: 1px solid var(--border); color: var(--text-muted); background: var(--bg); }
.search-form { padding: 8px; display: flex; flex-direction: column; gap: 6px; border-bottom: 1px solid var(--border); }
.search-form > input { height: 25px; padding: 0 7px; border: 1px solid var(--border-soft); background: var(--bg-input); }
.search-options { display: flex; align-items: center; gap: 8px; }
.search-options label { display: flex; align-items: center; gap: 3px; }
.search-options button { margin-left: auto; height: 24px; padding: 0 9px; border-radius: 3px; background: var(--accent); color: white; }
.search-hit { width: 100%; padding: 6px 8px; display: flex; flex-direction: column; gap: 3px; text-align: left; border-bottom: 1px solid var(--border-soft); }
.search-hit:hover { background: var(--bg-hover); }
.search-hit strong, .search-hit span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.search-hit strong { color: var(--text); font-weight: 500; }
.search-hit span { color: var(--text-muted); font-family: Consolas, monospace; }
.hint { margin: 0; padding: 12px 8px; color: var(--text-muted); overflow-wrap: anywhere; }
.hint.error, .danger { color: var(--danger) !important; }
.menu-mask { position: fixed; inset: 0; z-index: 1000; }
.context-menu { position: fixed; width: 210px; }
</style>
