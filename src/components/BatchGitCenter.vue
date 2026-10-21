<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import {
  commitChanges,
  fetchRemote,
  generateCommitMessage,
  getRepo,
  getStatus,
  syncPull,
  syncPush,
} from "../api";
import type { ProjectTab } from "../types";
import Icons from "./Icons.vue";

type Operation = "fetch" | "pull" | "commit" | "push";
type TaskState = "idle" | "running" | "success" | "error" | "warning";

interface BatchRow {
  path: string;
  name: string;
  branch: string;
  selected: boolean;
  dirty: boolean;
  changeCount: number;
  state: TaskState;
  message: string;
  stream: string[];
  conflicts: string[];
}

const props = defineProps<{ tabs: ProjectTab[] }>();
const emit = defineEmits<{
  close: [];
  updated: [tab: ProjectTab];
  resolve: [path: string];
}>();

const rows = ref<BatchRow[]>(props.tabs.map((tab) => ({
  path: tab.path,
  name: tab.name,
  branch: tab.branch,
  selected: true,
  dirty: tab.dirty,
  changeCount: 0,
  state: "idle",
  message: "等待检查",
  stream: [],
  conflicts: [],
})));
const running = ref(false);
const loading = ref(false);
const lastOperation = ref<Operation | "">("");

const selectedRows = computed(() => rows.value.filter((row) => row.selected));
const allSelected = computed(() => rows.value.length > 0 && rows.value.every((row) => row.selected));
const selectedCount = computed(() => selectedRows.value.length);
const successCount = computed(() => selectedRows.value.filter((row) => row.state === "success").length);
const issueCount = computed(() => selectedRows.value.filter((row) => row.state === "error" || row.state === "warning").length);

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

function toggleAll() {
  const selected = !allSelected.value;
  for (const row of rows.value) row.selected = selected;
}

function appendStream(row: BatchRow, message: string) {
  const time = new Date().toLocaleTimeString("zh-CN", { hour12: false });
  row.message = message;
  row.stream.push(`[${time}] ${message}`);
}

async function refreshRow(row: BatchRow, keepResult = false) {
  const [repo, status] = await Promise.all([getRepo(row.path), getStatus(row.path)]);
  row.name = repo.name;
  row.branch = repo.branch || (repo.detached ? "游离 HEAD" : "-");
  row.changeCount = status.changes.length + status.unversioned.length;
  row.dirty = row.changeCount > 0;
  if (!keepResult) {
    row.state = "idle";
    row.message = row.dirty ? `${row.changeCount} 个文件有变更` : "工作区干净";
  }
  emit("updated", {
    path: row.path,
    name: row.name,
    branch: repo.branch,
    dirty: row.dirty,
    merging: row.conflicts.length > 0 || (props.tabs.find((tab) => tab.path === row.path)?.merging ?? false),
  });
}

async function refreshAll() {
  if (running.value || loading.value) return;
  loading.value = true;
  await Promise.all(rows.value.map(async (row) => {
    row.state = "running";
    row.stream = [];
    appendStream(row, "正在检查仓库状态…");
    try {
      await refreshRow(row);
      appendStream(row, row.dirty ? `检查完成，发现 ${row.changeCount} 个变更` : "检查完成，工作区干净");
    } catch (error) {
      row.state = "error";
      row.message = errorMessage(error);
    }
  }));
  loading.value = false;
}

async function runRow(row: BatchRow, operation: Operation) {
  row.state = "running";
  row.stream = [];
  row.conflicts = [];
  appendStream(row, operation === "fetch" ? "开始获取远程引用…"
    : operation === "pull" ? "开始拉取远程更新…"
      : operation === "commit" ? "开始扫描待提交文件…" : "开始推送本地提交…");
  try {
    if (operation === "fetch") {
      const message = await fetchRemote(row.path);
      row.state = "success";
      appendStream(row, message.trim() || "获取完成");
    } else if (operation === "pull" || operation === "push") {
      const outcome = operation === "pull" ? await syncPull(row.path) : await syncPush(row.path);
      if (outcome.status === "conflicts") {
        row.state = "warning";
        row.conflicts = outcome.conflicts;
        appendStream(row, outcome.message);
        for (const path of outcome.conflicts.slice(0, 20)) appendStream(row, `冲突：${path}`);
      } else {
        row.state = "success";
        appendStream(row, outcome.message.trim() || (operation === "pull" ? "拉取完成" : "推送完成"));
      }
    } else if (operation === "commit") {
      const status = await getStatus(row.path);
      const files = [
        ...status.changes.map((file) => ({ path: file.path, untracked: false })),
        ...status.unversioned.map((file) => ({ path: file.path, untracked: true })),
      ];
      if (!files.length) {
        row.state = "success";
        appendStream(row, "没有可提交的变更");
      } else {
        appendStream(row, `发现 ${files.length} 个文件，正在整理差异…`);
        appendStream(row, "正在等待 AI 生成提交信息…");
        const generated = await generateCommitMessage(row.path, files);
        appendStream(row, `AI（${generated.model || "默认模型"}）已生成提交信息：\n${generated.message}`);
        appendStream(row, "正在写入 Git 提交…");
        await commitChanges(row.path, generated.message, false, files.map((file) => file.path));
        row.state = "success";
        appendStream(row, `提交完成，共 ${files.length} 个文件`);
      }
    }
    appendStream(row, "正在刷新仓库状态…");
    await refreshRow(row, true);
    appendStream(row, "状态刷新完成");
  } catch (error) {
    row.state = "error";
    appendStream(row, `失败：${errorMessage(error)}`);
    try {
      await refreshRow(row, true);
    } catch {
      // 保留原始操作错误。
    }
  }
}

async function run(operation: Operation) {
  if (running.value || !selectedRows.value.length) return;
  running.value = true;
  lastOperation.value = operation;
  const targets = [...selectedRows.value];
  await Promise.all(targets.map((row) => runRow(row, operation)));
  running.value = false;
}

onMounted(() => void refreshAll());
</script>

<template>
  <section class="batch-center">
    <header class="batch-header">
      <div>
        <h1>批量 Git 操作中心</h1>
        <p>选中多个项目后并发执行，单个项目失败不会中断其他任务</p>
      </div>
      <button type="button" class="icon-btn" title="关闭" :disabled="running" @click="emit('close')">
        <Icons name="close" :size="16" />
      </button>
    </header>

    <div class="batch-toolbar">
      <button type="button" class="secondary-btn" :disabled="running || loading || !selectedCount" @click="run('fetch')">
        <Icons name="fetch" /> 获取
      </button>
      <button type="button" class="secondary-btn" :disabled="running || loading || !selectedCount" @click="run('pull')">
        <Icons name="pull" /> 拉取
      </button>
      <button type="button" class="secondary-btn" :disabled="running || loading || !selectedCount" @click="run('push')">
        <Icons name="push" /> 推送
      </button>
      <span class="divider" />
      <button type="button" class="primary-btn" :disabled="running || loading || !selectedCount" @click="run('commit')">
        <Icons name="sparkle" /> AI 生成并提交
      </button>
      <button type="button" class="icon-btn refresh" title="重新检查" :disabled="running || loading" @click="refreshAll">
        <span v-if="loading" class="btn-spinner" />
        <Icons v-else name="refresh" />
      </button>
    </div>

    <div class="summary">
      <span>已选择 {{ selectedCount }} / {{ rows.length }} 个项目</span>
      <template v-if="lastOperation && !running">
        <span class="success">完成 {{ successCount }}</span>
        <span v-if="issueCount" class="issues">需处理 {{ issueCount }}</span>
      </template>
      <span v-if="running" class="running"><span class="btn-spinner" /> 正在并发执行…</span>
    </div>

    <div class="table-wrap">
      <table>
        <thead>
          <tr>
            <th class="check"><input type="checkbox" :checked="allSelected" :disabled="running" @change="toggleAll" /></th>
            <th>目录名</th>
            <th>Git 分支</th>
            <th>工作区</th>
            <th>状态 / 结果</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="row in rows" :key="row.path" :class="[`state-${row.state}`, { selected: row.selected }]">
            <td class="check"><input v-model="row.selected" type="checkbox" :disabled="running" /></td>
            <td class="project"><strong>{{ row.name }}</strong><small :title="row.path">{{ row.path }}</small></td>
            <td><span class="branch"><Icons name="branch" :size="12" />{{ row.branch || '-' }}</span></td>
            <td><span :class="row.dirty ? 'dirty' : 'clean'">{{ row.dirty ? `${row.changeCount} 个变更` : "干净" }}</span></td>
            <td class="result">
              <div class="result-head">
                <span v-if="row.state === 'running'" class="btn-spinner" />
                <Icons v-else-if="row.state === 'success'" name="check" :size="13" />
                <span v-else-if="row.state === 'error' || row.state === 'warning'" class="mark">!</span>
                <strong>{{ row.state === 'running' ? '执行中' : row.state === 'success' ? '已完成' : row.state === 'error' ? '失败' : row.state === 'warning' ? '需处理' : '就绪' }}</strong>
                <button
                  v-if="row.conflicts.length && !running"
                  type="button"
                  class="resolve-btn"
                  title="打开该项目处理合并冲突"
                  @click="emit('resolve', row.path)"
                >去处理 {{ row.conflicts.length }} 个冲突</button>
              </div>
              <div class="stream" :title="row.stream.join('\n')">
                <span v-for="(line, index) in row.stream" :key="index">{{ line }}</span>
                <span v-if="!row.stream.length">{{ row.message }}</span>
              </div>
            </td>
          </tr>
          <tr v-if="!rows.length"><td colspan="5" class="empty">当前没有已打开的项目</td></tr>
        </tbody>
      </table>
    </div>
  </section>
</template>

<style scoped>
.batch-center { position: fixed; inset: 32px 0 0; z-index: 80; display: flex; flex-direction: column; background: var(--bg); }
.batch-header { min-height: 76px; padding: 16px 22px; display: flex; align-items: flex-start; justify-content: space-between; border-bottom: 1px solid var(--border-soft); background: var(--bg-deep); }
h1 { margin: 0 0 5px; font-size: 18px; font-weight: 600; }
p { margin: 0; color: var(--text-muted); }
.batch-toolbar { min-height: 52px; padding: 10px 22px; display: flex; align-items: center; gap: 8px; border-bottom: 1px solid var(--border); }
.divider { width: 1px; height: 24px; margin: 0 5px; background: var(--border-soft); }
.refresh { margin-left: auto; }
.summary { height: 34px; padding: 0 22px; display: flex; align-items: center; gap: 18px; color: var(--text-muted); background: var(--bg-deep); border-bottom: 1px solid var(--border); }
.summary .success { color: var(--ok); }
.summary .issues { color: var(--danger); }
.running { display: inline-flex; align-items: center; gap: 6px; color: var(--link); }
.table-wrap { flex: 1; overflow: auto; }
table { width: 100%; border-collapse: collapse; table-layout: fixed; }
th { position: sticky; top: 0; z-index: 1; height: 34px; padding: 0 12px; text-align: left; color: var(--text-muted); background: var(--bg-elevated); border-bottom: 1px solid var(--border-soft); font-weight: 500; }
th.check, td.check { width: 46px; text-align: center; }
th:nth-child(2) { width: 31%; }
th:nth-child(3) { width: 17%; }
th:nth-child(4) { width: 13%; }
td { height: 58px; padding: 8px 12px; border-bottom: 1px solid var(--border); vertical-align: middle; }
tr.selected td { background: color-mix(in srgb, var(--bg-selected) 20%, transparent); }
tr:hover td { background: var(--bg-hover); }
.project strong, .project small { display: block; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.project small { margin-top: 4px; color: var(--text-dim); }
.branch, .result-head { display: flex; align-items: center; gap: 7px; }
.result { padding-top: 9px; padding-bottom: 9px; }
.result-head { margin-bottom: 5px; }
.result-head strong { font-size: 11px; font-weight: 600; }
.resolve-btn { margin-left: 6px; height: 20px; padding: 0 8px; border: 1px solid currentColor; border-radius: 4px; color: var(--text-warn); font-size: 11px; }
.resolve-btn:hover { background: color-mix(in srgb, var(--text-warn) 18%, transparent); }
.stream { max-height: 150px; overflow: auto; color: var(--text-muted); font-family: Consolas, "Courier New", monospace; font-size: 11px; line-height: 1.5; white-space: pre-wrap; }
.stream span { display: block; }
.dirty { color: var(--renamed); }
.clean, .state-success .result { color: var(--ok); }
.state-error .result { color: var(--danger); }
.state-warning .result { color: var(--text-warn); }
.mark { width: 14px; height: 14px; display: inline-flex; align-items: center; justify-content: center; border: 1px solid currentColor; border-radius: 50%; font-weight: 700; font-size: 10px; flex: none; }
.empty { height: 120px; text-align: center; color: var(--text-muted); }
</style>
