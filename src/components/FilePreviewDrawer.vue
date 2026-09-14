<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import "highlight.js/styles/atom-one-dark.css";
import { listWorktree, previewFile, saveWorktreeFile } from "../api";
import { languageFromPath } from "../highlight";
import { notify } from "../notify";
import type { FilePreview } from "../types";
import Icons from "./Icons.vue";

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

const visible = computed(() => flatten(tree.value, query.value.trim().toLowerCase(), expanded.value));
const lineCount = computed(() => (draft.value ? draft.value.split("\n").length : 0));
const editable = computed(() => Boolean(preview.value && !preview.value.binary && !preview.value.truncated));
const dirty = computed(() => editable.value && draft.value !== savedContent.value);

function buildTree(paths: string[]) {
  const root: TreeNode[] = [];
  for (const path of paths) {
    const parts = path.split("/").filter(Boolean);
    let level = root;
    let acc = "";
    for (let i = 0; i < parts.length; i++) {
      const name = parts[i];
      acc = acc ? `${acc}/${name}` : name;
      const dir = i < parts.length - 1;
      let node = level.find((item) => item.name === name && item.dir === dir);
      if (!node) {
        node = { name, path: acc, dir, children: dir ? [] : undefined };
        level.push(node);
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

async function loadTree() {
  loadingTree.value = true;
  treeError.value = "";
  try {
    const paths = await listWorktree(props.repoPath);
    tree.value = buildTree(paths);
    expanded.value = new Set();
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
  try {
    applyPreview(await previewFile(props.repoPath, path));
  } catch (error) {
    previewError.value = error instanceof Error ? error.message : String(error);
  } finally {
    loadingPreview.value = false;
  }
}

async function saveFile() {
  if (!editable.value || !selected.value || saving.value) return;
  saving.value = true;
  previewError.value = "";
  try {
    applyPreview(await saveWorktreeFile(props.repoPath, selected.value, draft.value));
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

function onRow(node: TreeNode) {
  if (node.dir) toggleDir(node.path);
  else void openFile(node.path);
}

function onEditorKey(event: KeyboardEvent) {
  if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "s") {
    event.preventDefault();
    void saveFile();
  }
}

function requestClose() {
  if (dirty.value && !window.confirm("当前文件有未保存修改，确定关闭吗？")) {
    return;
  }
  emit("close");
}

onMounted(() => {
  void loadTree();
});

watch(
  () => props.repoPath,
  () => {
    selected.value = "";
    preview.value = null;
    draft.value = "";
    savedContent.value = "";
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
          <button type="button" class="icon-btn" title="刷新" @click="loadTree">
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
            <textarea
              v-model="draft"
              class="editor"
              spellcheck="false"
              :readonly="!editable"
              :placeholder="editable ? '在此编辑文件内容，Ctrl+S 保存' : ''"
              @keydown="onEditorKey"
            />
          </template>
        </div>
      </article>
    </section>
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
  background: #2b2d30;
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
  color: #c9a227;
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
  background: #282c34;
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
  color: #c9a227;
  background: #3a3420;
}

.editor {
  display: block;
  width: 100%;
  height: 100%;
  min-height: 0;
  margin: 0;
  padding: 12px 16px 20px;
  border: 0;
  outline: none;
  resize: none;
  color: #abb2bf;
  background: transparent;
  font-family: Consolas, "Courier New", monospace;
  font-size: 12px;
  line-height: 18px;
  white-space: pre;
  tab-size: 4;
}

.editor[readonly] {
  color: #9aa3ad;
}
</style>
