<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from "vue";
import "highlight.js/styles/atom-one-dark.css";
import { absWorktreePath, copyWorktreeToClipboard, deleteWorktree, getFileBlame, listWorktree, openInOs, previewFile, renameWorktree, revealInOs, saveWorktreeFile } from "../api";
import { commentPrefix, findInTextarea, handleIdeEditorKey, writeTextarea } from "../editorKeys";
import { escapeHtml, highlightCode, languageFromPath } from "../highlight";
import { notify } from "../notify";
import type { BlameLine, FilePreview } from "../types";
import Icons from "./Icons.vue";
import Modal from "./Modal.vue";

interface TreeNode {
  name: string;
  path: string;
  dir: boolean;
  children?: TreeNode[];
}

const props = defineProps<{
  repoPath: string;
}>();

const emit = defineEmits<{
  close: [];
  changed: [];
  openTerminal: [cwd: string];
}>();

const query = ref("");
const loadingTree = ref(false);
const treeError = ref("");
const tree = ref<TreeNode[]>([]);
const expanded = ref(new Set<string>());
const selected = ref("");
const preview = ref<FilePreview | null>(null);
const draft = ref("");
const savedContent = ref("");
const loadingPreview = ref(false);
const saving = ref(false);
const previewError = ref("");
const menu = ref<{
  open: boolean;
  x: number;
  y: number;
  node: TreeNode | null;
  sub: boolean;
  subLeft: boolean;
}>({
  open: false,
  x: 0,
  y: 0,
  node: null,
  sub: false,
  subLeft: false,
});
const pathWindow = ref("");
const editorEl = ref<HTMLTextAreaElement | null>(null);
const highlightEl = ref<HTMLElement | null>(null);
const gutterEl = ref<HTMLElement | null>(null);
const findOpen = ref(false);
const findQuery = ref("");
const findHint = ref("");
const fileDialog = ref(false);
const fileDialogName = ref("");
const fileDialogNode = ref<TreeNode | null>(null);
const fileBusy = ref(false);
const showBlame = ref(true);
const blameLines = ref<BlameLine[]>([]);

const visible = computed(() => flatten(tree.value, query.value.trim().toLowerCase(), expanded.value));
const lineCount = computed(() => (draft.value ? draft.value.split("\n").length : 1));
const lineNos = computed(() => Array.from({ length: lineCount.value }, (_, index) => index + 1));
const gutterWide = computed(() => `${Math.max(String(lineCount.value).length, 2)}ch`);
const blameAnnos = computed(() => lineNos.value.map((n) => blameDateAuthor(blameLines.value[n - 1])));
const editable = computed(() => Boolean(preview.value && !preview.value.binary && !preview.value.truncated));
const dirty = computed(() => editable.value && draft.value !== savedContent.value);
const highlightedHtml = computed(() => renderHighlight(draft.value, selected.value || preview.value?.path || ""));

function blameDateAuthor(meta?: BlameLine) {
  if (!meta?.author) return "";
  const date = meta.timestamp
    ? new Date(meta.timestamp * 1000).toLocaleDateString("zh-CN")
    : "";
  return date ? `${date} ${meta.author}` : meta.author;
}

function formatBlame(meta?: BlameLine) {
  if (!meta?.author) return "";
  const time = meta.timestamp
    ? new Date(meta.timestamp * 1000).toLocaleString("zh-CN", { hour12: false })
    : "";
  return time ? `${meta.author} · ${time}` : meta.author;
}

async function loadBlame(path: string) {
  blameLines.value = await getFileBlame(props.repoPath, path).catch(() => []);
}

function renderHighlight(text: string, path: string) {
  if (!text) return "\n";
  const lang = languageFromPath(path);
  const html = lang ? highlightCode(text, path) : escapeHtml(text);
  return text.endsWith("\n") ? `${html}\n` : html;
}

function syncEditorView() {
  const editor = editorEl.value;
  if (!editor) return;
  if (highlightEl.value) {
    highlightEl.value.scrollTop = editor.scrollTop;
    highlightEl.value.scrollLeft = editor.scrollLeft;
  }
  if (gutterEl.value) {
    gutterEl.value.scrollTop = editor.scrollTop;
  }
}

function buildTree(paths: string[]) {
  const root: TreeNode[] = [];
  for (const raw of paths) {
    const isDirPath = raw.endsWith("/") || raw.endsWith("\\");
    const parts = raw.replace(/\\/g, "/").replace(/\/+$/, "").split("/").filter(Boolean);
    if (!parts.length) continue;
    let level = root;
    let acc = "";
    for (let i = 0; i < parts.length; i++) {
      const name = parts[i];
      acc = acc ? `${acc}/${name}` : name;
      const dir = isDirPath || i < parts.length - 1;
      let node = level.find((item) => item.name === name && item.dir === dir);
      if (!node) {
        node = { name, path: acc, dir, children: dir ? [] : undefined };
        level.push(node);
      } else if (dir) {
        node.children ??= [];
      }
      if (dir) level = node.children!;
    }
  }
  sortNodes(root);
  return root;
}

function sortNodes(nodes: TreeNode[]) {
  nodes.sort((a, b) => Number(b.dir) - Number(a.dir) || a.name.localeCompare(b.name, "zh-CN"));
  nodes.forEach((node) => node.children && sortNodes(node.children));
}

function flatten(nodes: TreeNode[], keyword: string, open: Set<string>, depth = 0) {
  const rows: Array<{ node: TreeNode; depth: number }> = [];
  for (const node of nodes) {
    const hit = !keyword || node.path.toLowerCase().includes(keyword);
    const childHit = Boolean(keyword && node.dir && hasMatch(node, keyword));
    if (!keyword || hit || childHit) {
      rows.push({ node, depth });
      if (node.dir && (keyword || open.has(node.path))) {
        rows.push(...flatten(node.children ?? [], keyword, open, depth + 1));
      }
    }
  }
  return rows;
}

function hasMatch(node: TreeNode, keyword: string): boolean {
  if (node.path.toLowerCase().includes(keyword)) return true;
  return (node.children ?? []).some((child) => hasMatch(child, keyword));
}

function toggleDir(path: string) {
  const next = new Set(expanded.value);
  if (next.has(path)) next.delete(path);
  else next.add(path);
  expanded.value = next;
}

function formatSize(size: number) {
  if (size < 1024) return `${size} B`;
  if (size < 1024 * 1024) return `${(size / 1024).toFixed(1)} KB`;
  return `${(size / (1024 * 1024)).toFixed(1)} MB`;
}

async function loadTree(keepExpanded = false) {
  loadingTree.value = true;
  treeError.value = "";
  const keep = keepExpanded ? new Set(expanded.value) : new Set<string>();
  try {
    const paths = await listWorktree(props.repoPath);
    tree.value = buildTree(paths);
    expanded.value = keep;
  } catch (error) {
    treeError.value = error instanceof Error ? error.message : String(error);
    tree.value = [];
  } finally {
    loadingTree.value = false;
  }
}

function applyPreview(result: FilePreview) {
  preview.value = result;
  draft.value = result.binary ? "" : result.content;
  savedContent.value = draft.value;
  void nextTick(() => {
    writeTextarea(editorEl.value, draft.value);
    syncEditorView();
  });
}

function onDraftInput(event: Event) {
  const target = event.target;
  if (target instanceof HTMLTextAreaElement) {
    draft.value = target.value;
  }
}

async function openFile(path: string) {
  if (path === selected.value && preview.value) return;
  if (dirty.value && !window.confirm("当前文件有未保存修改，确定丢弃吗？")) {
    return;
  }
  selected.value = path;
  loadingPreview.value = true;
  previewError.value = "";
  preview.value = null;
  draft.value = "";
  savedContent.value = "";
  blameLines.value = [];
  try {
    applyPreview(await previewFile(props.repoPath, path));
    void loadBlame(path);
  } catch (error) {
    previewError.value = error instanceof Error ? error.message : String(error);
  } finally {
    loadingPreview.value = false;
    await nextTick();
    writeTextarea(editorEl.value, draft.value);
    syncEditorView();
  }
}

async function saveFile() {
  if (!editable.value || !selected.value || saving.value) return;
  saving.value = true;
  previewError.value = "";
  try {
    applyPreview(await saveWorktreeFile(props.repoPath, selected.value, draft.value));
    void loadBlame(selected.value);
    notify(`已保存 ${selected.value}`);
    emit("changed");
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    previewError.value = message;
    notify(message, "error");
  } finally {
    saving.value = false;
  }
}

function absPath(node: TreeNode) {
  const base = props.repoPath.replace(/[\\/]+$/, "");
  return `${base}\\${node.path.replace(/\//g, "\\")}`;
}

function closeMenu() {
  menu.value.open = false;
  menu.value.sub = false;
}

function openMenu(event: MouseEvent, node: TreeNode) {
  event.preventDefault();
  event.stopPropagation();
  selected.value = node.path;
  const width = 228;
  const height = node.dir ? 168 : 196;
  const x = Math.min(event.clientX, window.innerWidth - width - 8);
  const y = Math.min(event.clientY, window.innerHeight - height - 8);
  menu.value = {
    open: true,
    x: Math.max(8, x),
    y: Math.max(8, y),
    node,
    sub: false,
    subLeft: event.clientX > window.innerWidth - 460,
  };
}

function parentDir(path: string) {
  const index = path.lastIndexOf("/");
  return index < 0 ? "" : path.slice(0, index);
}

function joinRel(dir: string, name: string) {
  const clean = name.replace(/\\/g, "/").replace(/^\/+|\/+$/g, "");
  return dir ? `${dir}/${clean}` : clean;
}

function underPath(child: string, parent: string) {
  return child === parent || child.startsWith(`${parent}/`);
}

function openRenameDialog() {
  const node = menu.value.node;
  closeMenu();
  if (!node) return;
  fileDialogNode.value = node;
  fileDialogName.value = node.name;
  fileDialog.value = true;
}

function closeFileDialog() {
  if (fileBusy.value) return;
  fileDialog.value = false;
  fileDialogNode.value = null;
}

async function copyFileResource() {
  const node = menu.value.node;
  closeMenu();
  if (!node) return;
  try {
    await copyWorktreeToClipboard(props.repoPath, node.path);
    notify("已复制文件，可在资源管理器或其他窗口粘贴");
  } catch (error) {
    notify(error instanceof Error ? error.message : String(error), "error");
  }
}

async function confirmFileDialog() {
  const node = fileDialogNode.value;
  const name = fileDialogName.value.trim();
  if (!node || !name || fileBusy.value) return;
  if (name.includes("..") || name.includes("/") || name.includes("\\")) {
    notify("名称不能包含路径分隔符", "error");
    return;
  }
  const dest = joinRel(parentDir(node.path), name);
  if (dest === node.path) {
    closeFileDialog();
    return;
  }
  fileBusy.value = true;
  try {
    const next = await renameWorktree(props.repoPath, node.path, dest);
    const dir = parentDir(next);
    if (dir) {
      const keep = new Set(expanded.value);
      keep.add(dir);
      expanded.value = keep;
    }
    if (selected.value === node.path || underPath(selected.value, node.path)) {
      selected.value = "";
      preview.value = null;
      draft.value = "";
      savedContent.value = "";
      if (!node.dir) {
        fileDialog.value = false;
        fileDialogNode.value = null;
        await loadTree(true);
        emit("changed");
        notify(`已重命名为 ${next}`);
        await openFile(next);
        return;
      }
    }
    fileDialog.value = false;
    fileDialogNode.value = null;
    await loadTree(true);
    emit("changed");
    notify(`已重命名为 ${next}`);
  } catch (error) {
    notify(error instanceof Error ? error.message : String(error), "error");
  } finally {
    fileBusy.value = false;
  }
}

async function deleteCurrent() {
  const node = menu.value.node;
  closeMenu();
  if (!node) return;
  const label = node.dir ? `目录「${node.name}」及其全部内容` : `「${node.name}」`;
  if (!window.confirm(`确定删除${label}？此操作不可恢复。`)) return;
  try {
    await deleteWorktree(props.repoPath, node.path);
    if (selected.value === node.path || underPath(selected.value, node.path)) {
      selected.value = "";
      preview.value = null;
      draft.value = "";
      savedContent.value = "";
    }
    await loadTree(true);
    emit("changed");
    notify(`已删除 ${node.path}`);
  } catch (error) {
    notify(error instanceof Error ? error.message : String(error), "error");
  }
}

function onRow(node: TreeNode) {
  closeMenu();
  if (node.dir) toggleDir(node.path);
  else void openFile(node.path);
}

async function copyPath() {
  const node = menu.value.node;
  closeMenu();
  if (!node) return;
  const text = await absWorktreePath(props.repoPath, node.path).catch(() => absPath(node));
  await navigator.clipboard.writeText(text).catch(() => undefined);
  notify("已复制路径");
}

async function revealInExplorer() {
  const node = menu.value.node;
  closeMenu();
  if (!node) return;
  try {
    await revealInOs(props.repoPath, node.path);
  } catch (error) {
    notify(error instanceof Error ? error.message : String(error), "error");
  }
}

async function openWithApp() {
  const node = menu.value.node;
  closeMenu();
  if (!node) return;
  try {
    await openInOs(props.repoPath, node.path);
  } catch (error) {
    notify(error instanceof Error ? error.message : String(error), "error");
  }
}

async function copyShownPath() {
  if (!pathWindow.value) return;
  await navigator.clipboard.writeText(pathWindow.value).catch(() => undefined);
  notify("已复制路径");
}

async function showPathWindow(node?: TreeNode | null) {
  const target = node ?? menu.value.node;
  closeMenu();
  if (!target) return;
  pathWindow.value = await absWorktreePath(props.repoPath, target.path).catch(() => absPath(target));
}

async function openInTerminal() {
  const node = menu.value.node;
  closeMenu();
  if (!node) return;
  const abs = await absWorktreePath(props.repoPath, node.path).catch(() => absPath(node));
  const normalized = abs.replace(/^\\\\\?\\/, "").replace(/\//g, "\\");
  const cwd = node.dir ? normalized : normalized.slice(0, Math.max(normalized.lastIndexOf("\\"), 0));
  emit("openTerminal", cwd || props.repoPath);
}

function runFind(reverse = false) {
  const el = editorEl.value;
  if (!el || !findQuery.value) return;
  findHint.value = findInTextarea(el, findQuery.value, reverse) ? "" : "未找到";
  syncEditorView();
}

function onEditorKey(event: KeyboardEvent) {
  if (event.key === "Escape" && findOpen.value) {
    event.preventDefault();
    findOpen.value = false;
    findHint.value = "";
    return;
  }
  if ((event.ctrlKey || event.metaKey) && event.key === "F3") {
    event.preventDefault();
    runFind(event.shiftKey);
    return;
  }
  handleIdeEditorKey(event, {
    onSave: () => void saveFile(),
    onFind: () => {
      findOpen.value = true;
      findHint.value = "";
    },
    commentPrefix: commentPrefix(selected.value),
  });
}

function onGlobalKey(event: KeyboardEvent) {
  if (event.ctrlKey && event.altKey && (event.code === "F12" || event.key === "F12")) {
    const name = selected.value;
    if (!name) return;
    const node = visible.value.find((item) => item.node.path === name)?.node;
    if (!node) return;
    event.preventDefault();
    event.stopPropagation();
    void showPathWindow(node);
  }
}

function requestClose() {
  if (dirty.value && !window.confirm("当前文件有未保存修改，确定关闭吗？")) {
    return;
  }
  emit("close");
}

onMounted(() => {
  window.addEventListener("keydown", onGlobalKey, true);
  void loadTree();
});

onUnmounted(() => {
  window.removeEventListener("keydown", onGlobalKey, true);
});

watch(editorEl, (el) => {
  if (el) {
    writeTextarea(el, draft.value);
    syncEditorView();
  }
});

watch(highlightedHtml, () => {
  void nextTick(syncEditorView);
});

watch(
  () => props.repoPath,
  () => {
    selected.value = "";
    preview.value = null;
    draft.value = "";
    savedContent.value = "";
    blameLines.value = [];
    void loadTree();
  },
);
</script>

<template>
  <div class="drawer">
    <section class="panel">
      <aside class="tree">
        <header>
          <strong>项目</strong>
          <button type="button" class="icon-btn" title="刷新" @click="loadTree()">
            <Icons name="refresh" :size="13" />
          </button>
        </header>
        <label class="search">
          <Icons name="search" :size="13" />
          <input v-model="query" placeholder="过滤文件名称" />
        </label>
        <div class="scroll list">
          <p v-if="loadingTree" class="empty-hint">正在加载文件树…</p>
          <p v-else-if="treeError" class="empty-hint">{{ treeError }}</p>
          <p v-else-if="!visible.length" class="empty-hint">没有匹配的文件</p>
          <button
            v-for="{ node, depth } in visible"
            :key="`${node.dir ? 'd' : 'f'}:${node.path}`"
            type="button"
            class="row"
            :class="{ selected: node.path === selected, dir: node.dir }"
            :style="{ paddingLeft: `${10 + depth * 14}px` }"
            :title="node.path"
            @click="onRow(node)"
            @contextmenu="openMenu($event, node)"
          >
            <span class="twist" :class="{ open: node.dir && (query || expanded.has(node.path)), hidden: !node.dir }">
              <Icons name="chevron" :size="10" />
            </span>
            <Icons :name="node.dir ? 'folder' : 'file'" :size="13" />
            <span>{{ node.name }}</span>
          </button>
        </div>
      </aside>

      <article class="preview">
        <header>
          <span class="path" :title="selected">
            {{ selected || "选择一个文件查看内容" }}
            <em v-if="dirty">*</em>
          </span>
          <label v-if="preview" class="blame-toggle">
            <input v-model="showBlame" type="checkbox" />
            Git 追溯
          </label>
          <span v-if="preview" class="muted">
            {{ languageFromPath(preview.path) || "文本" }}
            · {{ formatSize(preview.size) }}
            · {{ lineCount }} 行
          </span>
          <button
            type="button"
            class="save-btn"
            :disabled="!dirty || saving"
            @click="saveFile"
          >
            <span v-if="saving" class="btn-spinner" />
            {{ saving ? "保存中…" : "保存" }}
          </button>
          <button type="button" class="icon-btn" title="关闭" @click="requestClose">
            <Icons name="close" :size="12" />
          </button>
        </header>
        <div class="scroll body">
          <p v-if="loadingPreview" class="empty-hint">正在加载文件…</p>
          <p v-else-if="preview?.binary" class="empty-hint">二进制文件，无法预览或编辑</p>
          <p v-else-if="!selected" class="empty-hint">从左侧文件树选择一个文件</p>
          <template v-else>
            <p v-if="preview?.truncated" class="warn">文件较大，仅预览前 512 KB，不能在此编辑</p>
            <p v-if="previewError" class="warn">{{ previewError }}</p>
            <div v-if="findOpen" class="findbar">
              <input
                v-model="findQuery"
                placeholder="查找"
                autofocus
                @keydown.enter.prevent="runFind($event.shiftKey)"
                @keydown.escape.prevent="findOpen = false"
              />
              <button type="button" title="上一个" @click="runFind(true)">↑</button>
              <button type="button" title="下一个" @click="runFind(false)">↓</button>
              <span v-if="findHint" class="find-miss">{{ findHint }}</span>
              <button type="button" class="icon-btn" title="关闭" @click="findOpen = false">
                <Icons name="close" :size="12" />
              </button>
            </div>
            <div class="code-view">
              <div
                ref="gutterEl"
                class="gutter"
                :class="{ blame: showBlame }"
                :style="{ width: showBlame ? `calc(${gutterWide} + 128px)` : `calc(${gutterWide} + 18px)` }"
              >
                <div v-for="n in lineNos" :key="n" class="gutter-line" :title="formatBlame(blameLines[n - 1])">
                  <span class="no">{{ n }}</span>
                  <span v-if="showBlame" class="blame">{{ blameAnnos[n - 1] }}</span>
                </div>
              </div>
              <div class="code-main">
                <pre
                  ref="highlightEl"
                  class="code-hl hljs"
                  aria-hidden="true"
                  v-html="highlightedHtml"
                />
                <textarea
                  ref="editorEl"
                  class="editor"
                  spellcheck="false"
                  :readonly="!editable"
                  :placeholder="editable ? '在此编辑文件内容，Ctrl+S 保存' : ''"
                  @input="onDraftInput"
                  @keydown="onEditorKey"
                  @scroll="syncEditorView"
                />
              </div>
            </div>
          </template>
        </div>
      </article>
    </section>

    <Teleport to="body">
      <div v-if="menu.open" class="ctx-mask" @mousedown="closeMenu">
        <div class="popup-menu ctx" :style="{ top: `${menu.y}px`, left: `${menu.x}px` }" @mousedown.stop>
          <button v-if="menu.node && !menu.node.dir" type="button" @click="closeMenu(); openFile(menu.node.path)">
            在预览中打开
          </button>
          <button type="button" @click="copyFileResource">复制</button>
          <button type="button" @click="copyPath">复制路径</button>
          <button type="button" @click="openRenameDialog">重命名…</button>
          <button type="button" @click="deleteCurrent">删除</button>
          <div class="divider" />
          <div class="sub-wrap" @mouseenter="menu.sub = true">
            <button type="button" class="sub-btn" :class="{ open: menu.sub }" @click.stop="menu.sub = !menu.sub">
              <span>打开于</span>
              <span class="arrow">›</span>
            </button>
            <div v-show="menu.sub" class="submenu" :class="{ left: menu.subLeft }">
              <button type="button" @click="revealInExplorer">资源管理器</button>
              <button type="button" @click="openWithApp">在关联的应用程序中打开</button>
              <button type="button" class="with-key" @click="showPathWindow()">
                <span>路径窗口</span>
                <span class="key">Ctrl+Alt+F12</span>
              </button>
              <button type="button" @click="openInTerminal">终端</button>
            </div>
          </div>
        </div>
      </div>

      <Modal v-if="fileDialog" title="重命名" @close="closeFileDialog">
        <p>将 <strong>{{ fileDialogNode?.path }}</strong> 重命名为：</p>
        <label>新名称 <input v-model="fileDialogName" @keydown.enter.prevent="confirmFileDialog" /></label>
        <template #footer>
          <button type="button" class="secondary-btn" :disabled="fileBusy" @click="closeFileDialog">取消</button>
          <button type="button" class="primary-btn" :disabled="fileBusy || !fileDialogName.trim()" @click="confirmFileDialog">
            重命名
          </button>
        </template>
      </Modal>

      <div v-if="pathWindow" class="ctx-mask" @mousedown="pathWindow = ''">
        <div class="path-box" @mousedown.stop>
          <header>路径</header>
          <input :value="pathWindow" readonly @focus="($event.target as HTMLInputElement).select()" />
          <div class="path-actions">
            <button type="button" class="secondary-btn" @click="pathWindow = ''">关闭</button>
            <button type="button" class="primary-btn" @click="copyShownPath">复制</button>
          </div>
        </div>
      </div>
    </Teleport>
  </div>
</template>

<style scoped>
.drawer {
  flex: 1;
  min-height: 0;
  display: flex;
}

.panel {
  flex: 1;
  min-width: 0;
  min-height: 0;
  display: grid;
  grid-template-columns: minmax(220px, 280px) 1fr;
  background: var(--bg);
}

.tree,
.preview {
  min-width: 0;
  min-height: 0;
  display: flex;
  flex-direction: column;
}

.tree {
  border-right: 1px solid var(--border);
}

header {
  height: 34px;
  padding: 0 8px 0 12px;
  display: flex;
  align-items: center;
  gap: 8px;
  border-bottom: 1px solid var(--border);
}

.path {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.path em {
  color: var(--text-warn);
  font-style: normal;
}

.save-btn {
  height: 22px;
  padding: 0 8px;
  border-radius: 4px;
  background: var(--accent);
  color: #fff;
  display: inline-flex;
  align-items: center;
  gap: 6px;
}

.save-btn:disabled {
  opacity: 0.45;
}

.search {
  display: flex;
  align-items: center;
  gap: 6px;
  margin: 8px;
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

.list,
.body {
  flex: 1;
  min-height: 0;
}

.body {
  display: flex;
  flex-direction: column;
  background: var(--bg-editor);
}

.findbar {
  flex: none;
  display: flex;
  align-items: center;
  gap: 6px;
  min-height: 28px;
  padding: 0 8px;
  border-bottom: 1px solid var(--border);
  background: var(--bg-hover);
}

.findbar input {
  width: 220px;
  height: 22px;
  padding: 0 8px;
  border: 1px solid var(--border-soft);
  border-radius: 4px;
  background: var(--bg-input);
  color: var(--text);
}

.findbar button {
  height: 22px;
  min-width: 22px;
  padding: 0 6px;
  border-radius: 4px;
  color: var(--text-muted);
}

.findbar button:hover {
  background: var(--bg-selected);
  color: var(--text);
}

.find-miss {
  color: var(--danger);
}

.row {
  width: 100%;
  min-height: 26px;
  display: flex;
  align-items: center;
  gap: 6px;
  padding-right: 10px;
  text-align: left;
}

.twist {
  width: 10px;
  color: var(--text-dim);
  display: inline-flex;
  transform: rotate(-90deg);
}

.twist.open {
  transform: rotate(0deg);
}

.twist.hidden {
  visibility: hidden;
}

.row span {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.row:hover,
.row.selected {
  background: var(--bg-selected);
}

.warn {
  margin: 0;
  padding: 6px 12px;
  color: var(--text-warn);
  background: var(--bg-warn);
}

.code-view {
  flex: 1;
  min-height: 0;
  display: flex;
  background: var(--bg-editor);
}

.blame-toggle {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  color: var(--text-muted);
  white-space: nowrap;
}

.gutter {
  flex: none;
  overflow: hidden;
  padding: 12px 6px 20px 8px;
  color: var(--text-dim);
  background: var(--bg-deep);
  border-right: 1px solid var(--border);
  font-family: Consolas, "Courier New", monospace;
  font-size: 12px;
  line-height: 18px;
  user-select: none;
}

.gutter-line {
  display: flex;
  align-items: center;
  height: 18px;
  gap: 8px;
}

.gutter .no {
  flex: none;
  min-width: 2ch;
  text-align: right;
}

.gutter .blame {
  min-width: 0;
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 11px;
}

.code-main {
  position: relative;
  flex: 1;
  min-width: 0;
}

.code-hl,
.editor {
  position: absolute;
  inset: 0;
  margin: 0;
  padding: 12px 16px 20px;
  border: 0;
  font-family: Consolas, "Courier New", monospace;
  font-size: 12px;
  line-height: 18px;
  white-space: pre;
  tab-size: 4;
}

.code-hl {
  overflow: hidden;
  pointer-events: none;
  color: var(--text-editor);
  background: transparent;
}

.code-hl :deep(.hljs) {
  padding: 0;
  background: transparent;
  color: inherit;
}

.editor {
  z-index: 1;
  outline: none;
  resize: none;
  overflow: auto;
  color: transparent;
  caret-color: var(--text-editor);
  background: transparent;
}

.editor::selection {
  background: rgba(53, 116, 240, 0.38);
}

.editor[readonly] {
  caret-color: var(--text-editor-muted);
}

.ctx-mask {
  position: fixed;
  inset: 0;
  z-index: 1300;
}

.ctx {
  position: fixed;
  min-width: 220px;
  overflow: visible;
  z-index: 1301;
}

.sub-wrap {
  position: relative;
}

.sub-btn {
  display: flex !important;
  align-items: center;
  justify-content: space-between;
}

.sub-btn.open,
.sub-btn:hover {
  background: var(--bg-selected);
}

.arrow,
.key {
  color: var(--text-muted);
  font-size: 11px;
}

.with-key {
  display: flex !important;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
}

.submenu {
  position: absolute;
  top: -5px;
  left: calc(100% - 4px);
  min-width: 248px;
  z-index: 1302;
  background: var(--bg-elevated);
  border: 1px solid var(--border-elevated);
  border-radius: 6px;
  box-shadow: var(--shadow);
  padding: 4px 0;
}

.submenu.left {
  left: auto;
  right: calc(100% - 4px);
}

.path-box {
  position: fixed;
  left: 50%;
  top: 20%;
  transform: translateX(-50%);
  width: min(640px, calc(100vw - 32px));
  padding: 10px 12px 12px;
  background: var(--bg-elevated);
  border: 1px solid var(--border-elevated);
  border-radius: 6px;
  box-shadow: var(--shadow);
}

.path-box header {
  border: 0;
  padding: 0 0 8px;
  font-weight: 600;
}

.path-box input {
  width: 100%;
  height: 30px;
  border: 1px solid var(--border-soft);
  background: var(--bg-input);
  border-radius: 4px;
  padding: 0 8px;
  color: var(--text);
}

.path-actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  margin-top: 10px;
}
</style>
