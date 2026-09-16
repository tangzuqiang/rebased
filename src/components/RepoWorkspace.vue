<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from "vue";
import {
  checkoutBranch,
  commitChanges,
  createBranchAt,
  fetchRemote,
  getBranches,
  getCommitFiles,
  getLog,
  getRecentBranches,
  getMergeState,
  getRepo,
  getStatus,
  mergeAbort,
  mergeBranch,
  mergeContinue,
  openRepo,
  pullRemote,
  pushTo,
  renameBranch,
  resolveConflict,
  setUpstream,
  unwatchRepo,
  updateBranch,
  watchRepo,
  workingTreeDiff,
  writeResolved,
} from "../api";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { buildGraph } from "../graph";
import { notify } from "../notify";
import { sameRepo } from "../path";
import type { BranchMenuAction } from "./BranchFilterList.vue";
import type { BranchInfo, FileDiff, GitCommit, GitFile, MergeState, RecentProject, RepoInfo } from "../types";
import CommitDialog from "./CommitDialog.vue";
import ConflictsDialog from "./ConflictsDialog.vue";
import ErrorDialog from "./ErrorDialog.vue";
import FilePreviewDrawer from "./FilePreviewDrawer.vue";
import FilesPanel from "./FilesPanel.vue";
import LogPanel from "./LogPanel.vue";
import MergeDialog from "./MergeDialog.vue";
import MergeEditorPage from "./MergeEditorPage.vue";
import PushDialog from "./PushDialog.vue";
import Modal from "./Modal.vue";
import StatusBar from "./StatusBar.vue";
import TerminalPanel from "./TerminalPanel.vue";
import Toolbar from "./Toolbar.vue";

const props = defineProps<{
  path: string;
  active: boolean;
  recent: RecentProject[];
  version?: string;
}>();

const emit = defineEmits<{
  open: [];
  openPath: [path: string];
  settings: [];
  failed: [path: string];
  meta: [payload: { path: string; name: string; branch: string; dirty: boolean; merging: boolean }];
}>();

const repo = ref<RepoInfo | null>(null);
const branches = ref<BranchInfo[]>([]);
const recentBranches = ref<string[]>([]);
const changes = ref<GitFile[]>([]);
const unversioned = ref<GitFile[]>([]);
const commits = ref<GitCommit[]>([]);
const commitFiles = ref<GitFile[]>([]);
const selectedHash = ref("");
const busy = ref(false);
const statusText = ref("就绪");
const errorText = ref("");
const showFiles = ref(false);
const showTerminal = ref(false);
const terminalReady = ref(false);
const termStartCwd = ref("");
const termSpawnSeq = ref(0);
const showCommit = ref(false);
const showPush = ref(false);
const showMerge = ref(false);
const showConflicts = ref(false);
const mergeEditPath = ref("");
const conflictState = ref<MergeState | null>(null);
const resolvePath = ref("");
const loaded = ref(false);
const logRefs = ref<string[]>([]);
const startAmend = ref(false);
const pushBranch = ref("");
const branchDialog = ref<"" | "new" | "rename" | "diff">("");
const actionBranch = ref<BranchInfo | null>(null);
const actionName = ref("");
const checkoutNewBranch = ref(true);
const branchDiff = ref<FileDiff | null>(null);
const branchError = ref("");
const logLoading = ref(false);
const actionError = ref("");
const actionErrorTitle = ref("操作失败");

const graph = computed(() => buildGraph(commits.value));
const repoPath = computed(() => repo.value?.path ?? props.path);
const fsDirty = ref(false);
const watchingPath = ref("");
const isDirty = computed(() => fsDirty.value || changes.value.length + unversioned.value.length > 0);

function emitMeta() {
  if (!repo.value) return;
  emit("meta", {
    path: repo.value.path,
    name: repo.value.name,
    branch: repo.value.branch,
    dirty: isDirty.value,
    merging: Boolean(conflictState.value?.inProgress),
  });
}

function markFsDirty() {
  if (fsDirty.value) return;
  fsDirty.value = true;
  emitMeta();
}

let watchTimer = 0;
let pendingWatch: "worktree" | "git" | "" = "";

function scheduleGitRefresh(kind: "worktree" | "git") {
  if (kind === "git") pendingWatch = "git";
  else if (!pendingWatch) pendingWatch = "worktree";
  window.clearTimeout(watchTimer);
  watchTimer = window.setTimeout(() => {
    void applyGitRefresh();
  }, 400);
}

async function applyGitRefresh() {
  const kind = pendingWatch;
  pendingWatch = "";
  if (!kind || !repo.value) return;
  if (busy.value) {
    pendingWatch = kind;
    return;
  }
  if (kind === "worktree") {
    await refreshStatus();
    return;
  }
  await refreshAll();
}

function onWindowResume() {
  if (document.visibilityState === "hidden") return;
  if (!loaded.value || busy.value) return;
  scheduleGitRefresh("git");
}

async function withBusy<T>(
  label: string,
  task: () => Promise<T>,
  options?: { errorPopup?: boolean; errorTitle?: string },
): Promise<T | undefined> {
  busy.value = true;
  statusText.value = label;
  errorText.value = "";
  await nextTick();
  await new Promise<void>((resolve) => requestAnimationFrame(() => requestAnimationFrame(() => resolve())));
  try {
    const result = await task();
    statusText.value = "就绪";
    return result;
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    errorText.value = message;
    statusText.value = "失败";
    if (options?.errorPopup) {
      actionErrorTitle.value = options.errorTitle || "操作失败";
      actionError.value = message;
    } else {
      notify(message, "error");
    }
    return undefined;
  } finally {
    busy.value = false;
  }
}

async function refreshCore() {
  if (!repo.value) return;
  const current = repoPath.value;
  const errors: string[] = [];
  const [statusResult, recentResult, repoResult] = await Promise.allSettled([
    getStatus(current),
    getRecentBranches(current, 4),
    getRepo(current),
  ]);

  if (statusResult.status === "fulfilled") {
    changes.value = statusResult.value.changes;
    unversioned.value = statusResult.value.unversioned;
    fsDirty.value = false;
  } else {
    errors.push(String(statusResult.reason));
  }

  if (recentResult.status === "fulfilled") {
    recentBranches.value = recentResult.value.slice(0, 4);
  }

  if (repoResult.status === "fulfilled" && current === repoPath.value) {
    repo.value = repoResult.value;
  }

  await syncMergeState(false);
  emitMeta();
  if (errors.length) {
    throw new Error(errors.join("；"));
  }
}

async function refreshLog() {
  if (!repo.value) return;
  const current = repoPath.value;
  logLoading.value = true;
  try {
    const logResult = await getLog(current, 200, logRefs.value);
    if (current !== repoPath.value) return;
    commits.value = logResult;
    if (!selectedHash.value && logResult[0]) {
      selectedHash.value = logResult[0].hash;
    }
    if (selectedHash.value) {
      commitFiles.value = await getCommitFiles(current, selectedHash.value).catch(() => []);
    }
  } catch (error) {
    if (!commits.value.length) {
      errorText.value = error instanceof Error ? error.message : String(error);
    }
  } finally {
    if (current === repoPath.value) {
      logLoading.value = false;
    }
  }
}

async function refreshBranches() {
  if (!repo.value) return;
  const current = repoPath.value;
  const list = await getBranches(current).catch(() => null);
  if (list && current === repoPath.value) {
    branches.value = list;
  }
}

function refreshBackground() {
  void refreshLog();
  void refreshBranches();
}

async function refreshAll() {
  await refreshCore();
  refreshBackground();
}

async function syncMergeState(openIfNeeded: boolean) {
  const merge = await getMergeState(repoPath.value).catch(() => null);
  if (merge?.inProgress) {
    conflictState.value = merge;
    if (openIfNeeded) {
      showConflicts.value = true;
    }
    emitMeta();
    return;
  }
  conflictState.value = null;
  showConflicts.value = false;
  resolvePath.value = "";
  mergeEditPath.value = "";
  emitMeta();
}

async function bootstrap(openConflicts = false) {
  const info = await withBusy("正在打开仓库…", async () => {
    const opened = await openRepo(props.path);
    repo.value = opened;
    await refreshAll();
    if (openConflicts) {
      await syncMergeState(true);
    }
    loaded.value = true;
    watchingPath.value = opened.path;
    void watchRepo(opened.path).catch(() => undefined);
    emitMeta();
    return opened;
  });
  if (!info) {
    emit("failed", props.path);
  }
}

async function refreshStatus() {
  if (!repo.value) return;
  const status = await getStatus(repoPath.value).catch(() => null);
  if (!status) return;
  changes.value = status.changes;
  unversioned.value = status.unversioned;
  fsDirty.value = false;
  emitMeta();
}

async function openCommit(amend = false) {
  startAmend.value = amend;
  await refreshStatus();
  showCommit.value = true;
}

async function handleRefsChange(refs: string[]) {
  logRefs.value = refs;
  if (!repo.value) return;
  commits.value = await getLog(repoPath.value, 200, refs).catch(() => commits.value);
}

async function handleLogChanged(message?: string) {
  await withBusy(message || "正在刷新…", async () => {
    repo.value = await openRepo(repoPath.value);
    selectedHash.value = "";
    await refreshAll();
    return true;
  });
}

function openAmend() {
  void openCommit(true);
}

async function handleSelect(hash: string) {
  selectedHash.value = hash;
  commitFiles.value = await getCommitFiles(repoPath.value, hash).catch(() => []);
}

async function handleCommit(payload: {
  message: string;
  amend: boolean;
  paths: string[];
  push: boolean;
}) {
  const ok = await withBusy(
    payload.push ? "正在提交并推送…" : "正在提交…",
    async () => {
      await commitChanges(repoPath.value, payload.message, payload.amend, payload.paths);
      if (payload.push) {
        await pushTo(repoPath.value, "", repo.value!.branch);
      }
      repo.value = await openRepo(repoPath.value);
      await refreshAll();
      showCommit.value = false;
      return true;
    },
    { errorPopup: true, errorTitle: payload.push ? "提交并推送失败" : "提交失败" },
  );
  if (ok) {
    notify(payload.push ? `已提交并推送到 ${repo.value?.branch || "远程"}` : "提交成功");
  }
}

async function handleCheckout(name: string) {
  const ok = await withBusy(
    `正在切换到 ${name}…`,
    async () => {
      await checkoutBranch(repoPath.value, name);
      repo.value = await openRepo(repoPath.value);
      selectedHash.value = "";
      await refreshAll();
      return true;
    },
    { errorPopup: true, errorTitle: "签出失败" },
  );
  if (ok) notify(`已切换到 ${repo.value?.branch || name}`);
}

async function handleFetch() {
  const ok = await withBusy("正在获取…", async () => {
    await fetchRemote(repoPath.value);
    repo.value = await openRepo(repoPath.value);
    await refreshAll();
    return true;
  });
  if (ok) notify("获取完成");
}

async function handlePull() {
  const ok = await withBusy("正在拉取…", async () => {
    await pullRemote(repoPath.value);
    repo.value = await openRepo(repoPath.value);
    await refreshAll();
    return true;
  });
  if (ok) notify("拉取完成");
}

function openNewBranchFromCurrent() {
  const name = repo.value?.branch || branches.value.find((item) => item.current)?.name || "";
  if (!name) return;
  actionBranch.value = branches.value.find((item) => item.name === name) ?? {
    name,
    current: true,
    remote: false,
    kind: "local",
  };
  actionName.value = "";
  checkoutNewBranch.value = true;
  branchDialog.value = "new";
}

function suggestedTrack(branch: BranchInfo) {
  if (branch.upstream) return branch.upstream;
  if (branch.remote) return branch.name;
  const short = branch.name;
  return branches.value.find((item) => item.remote && (item.name === `origin/${short}` || item.name.endsWith(`/${short}`)))?.name
    ?? `origin/${short}`;
}

function localNameFromRemote(name: string) {
  return name.includes("/") ? name.slice(name.lastIndexOf("/") + 1) : name;
}

async function handleBranchAction(payload: { action: BranchMenuAction; branch: BranchInfo }) {
  actionBranch.value = payload.branch;
  branchError.value = "";
  if (payload.action === "checkout") {
    await handleCheckout(payload.branch.name);
    return;
  }
  if (payload.action === "new-branch") {
    actionName.value = "";
    checkoutNewBranch.value = true;
    branchDialog.value = "new";
    return;
  }
  if (payload.action === "rename") {
    actionName.value = payload.branch.name;
    branchDialog.value = "rename";
    return;
  }
  if (payload.action === "diff") {
    branchDiff.value = null;
    branchDialog.value = "diff";
    branchDiff.value = await workingTreeDiff(repoPath.value, payload.branch.name).catch((error) => {
      branchError.value = error instanceof Error ? error.message : String(error);
      return null;
    });
    return;
  }
  if (payload.action === "push") {
    pushBranch.value = payload.branch.remote ? localNameFromRemote(payload.branch.name) : payload.branch.name;
    showPush.value = true;
    return;
  }
  if (payload.action === "update") {
    const message = await withBusy(`正在更新 ${payload.branch.name}…`, async () => {
      const result = await updateBranch(repoPath.value, payload.branch.name);
      repo.value = await openRepo(repoPath.value);
      await refreshAll();
      return result || "已更新";
    });
    if (message) {
      statusText.value = message;
      notify(message);
    }
    return;
  }
  if (payload.action === "track") {
    const message = await withBusy("正在设置跟踪分支…", async () => {
      let result = "";
      if (payload.branch.remote) {
        const local = localNameFromRemote(payload.branch.name);
        try {
          await createBranchAt(repoPath.value, local, payload.branch.name, true);
        } catch {
          await checkoutBranch(repoPath.value, local);
        }
        result = await setUpstream(repoPath.value, local, payload.branch.name);
      } else {
        result = await setUpstream(repoPath.value, payload.branch.name, suggestedTrack(payload.branch));
      }
      repo.value = await openRepo(repoPath.value);
      selectedHash.value = "";
      await refreshAll();
      return result;
    });
    if (message) {
      statusText.value = message;
      notify(message);
    }
  }
}

async function confirmNewBranch() {
  const source = actionBranch.value?.name;
  const name = actionName.value.trim();
  if (!source || !name) return;
  const ok = await withBusy("正在创建分支…", async () => {
    await createBranchAt(repoPath.value, name, source, checkoutNewBranch.value);
    branchDialog.value = "";
    repo.value = await openRepo(repoPath.value);
    selectedHash.value = "";
    await refreshAll();
    return true;
  });
  if (ok) notify(`已创建分支 ${name}`);
}

async function confirmRenameBranch() {
  const old = actionBranch.value?.name;
  const name = actionName.value.trim();
  if (!old || !name) return;
  const ok = await withBusy("正在重命名分支…", async () => {
    await renameBranch(repoPath.value, old, name);
    branchDialog.value = "";
    repo.value = await openRepo(repoPath.value);
    selectedHash.value = "";
    await refreshAll();
    return true;
  });
  if (ok) notify(`已重命名为 ${name}`);
}

async function handlePush(payload?: { remote: string; branch: string }) {
  const remote = payload?.remote || "";
  const branch = payload?.branch || repo.value?.branch || "";
  const ok = await withBusy(
    "正在推送…",
    async () => {
      if (payload) {
        await pushTo(repoPath.value, payload.remote, payload.branch);
      } else {
        await pushTo(repoPath.value, "", repo.value!.branch);
      }
      repo.value = await openRepo(repoPath.value);
      await refreshAll();
      showPush.value = false;
      return true;
    },
    { errorPopup: true, errorTitle: "推送失败" },
  );
  if (ok) {
    notify(remote && branch ? `已推送到 ${remote}/${branch}` : "推送成功");
  }
}

async function handleMerge(theirs: string) {
  const outcome = await withBusy(`正在合并 ${theirs}…`, async () => {
    const result = await mergeBranch(repoPath.value, theirs);
    repo.value = await openRepo(repoPath.value);
    selectedHash.value = "";
    await refreshAll();
    showMerge.value = false;
    if (result.status === "conflicts") {
      conflictState.value = result.state;
      showConflicts.value = true;
      emitMeta();
    }
    return result;
  }, { errorPopup: true, errorTitle: "合并失败" });
  if (outcome?.status === "conflicts") {
    showConflicts.value = true;
  } else if (outcome) {
    notify(`已合并 ${theirs}`);
  }
}

async function handleAcceptConflict(payload: { path?: string; paths?: string[]; side: "ours" | "theirs" }) {
  const paths = payload.paths ?? (payload.path ? [payload.path] : []);
  if (!paths.length) return;
  await withBusy(payload.side === "ours" ? "正在接受本地版本…" : "正在接受传入版本…", async () => {
    let state = conflictState.value;
    for (const path of paths) {
      state = await resolveConflict(repoPath.value, path, payload.side);
    }
    if (state) {
      conflictState.value = state;
    }
    if (paths.includes(resolvePath.value)) {
      resolvePath.value = "";
    }
    await refreshAll();
  });
}

async function handleSaveResolved(payload: { path: string; content: string } | string) {
  const path = typeof payload === "string" ? resolvePath.value : payload.path;
  const content = typeof payload === "string" ? payload : payload.content;
  if (!path) return;
  await withBusy("正在保存冲突解决结果…", async () => {
    conflictState.value = await writeResolved(repoPath.value, path, content);
    resolvePath.value = "";
    mergeEditPath.value = "";
    await refreshAll();
  });
}

async function handleContinueMerge() {
  const ok = await withBusy("正在完成合并…", async () => {
    await mergeContinue(repoPath.value);
    showConflicts.value = false;
    conflictState.value = null;
    resolvePath.value = "";
    mergeEditPath.value = "";
    repo.value = await openRepo(repoPath.value);
    selectedHash.value = "";
    await refreshAll();
    return true;
  }, { errorPopup: true, errorTitle: "完成合并失败" });
  if (ok) notify("合并已完成");
}

async function handleAbortMerge() {
  const ok = await withBusy("正在取消合并…", async () => {
    await mergeAbort(repoPath.value);
    showConflicts.value = false;
    conflictState.value = null;
    resolvePath.value = "";
    mergeEditPath.value = "";
    repo.value = await openRepo(repoPath.value);
    selectedHash.value = "";
    await refreshAll();
    return true;
  });
  if (ok) notify("已取消合并");
}

function openConflicts() {
  if (conflictState.value?.inProgress) {
    showConflicts.value = true;
  }
}

function toggleTerminal() {
  showTerminal.value = !showTerminal.value;
  if (showTerminal.value) {
    terminalReady.value = true;
  }
}

function openTerminalAt(cwd: string) {
  termStartCwd.value = cwd;
  if (!terminalReady.value) {
    terminalReady.value = true;
    showTerminal.value = true;
    return;
  }
  showTerminal.value = true;
  termSpawnSeq.value += 1;
}

function onGlobalKey(event: KeyboardEvent) {
  if (!props.active || !repo.value) return;
  if (event.altKey && !event.ctrlKey && (event.code === "F12" || event.key === "F12")) {
    event.preventDefault();
    event.stopPropagation();
    toggleTerminal();
  }
}

watch(busy, (value) => {
  if (!value && pendingWatch) {
    scheduleGitRefresh(pendingWatch);
  }
});

watch(
  () => props.active,
  (active) => {
    if (active && !loaded.value) {
      void bootstrap(true);
    } else if (active && loaded.value) {
      scheduleGitRefresh("git");
    }
  },
);

let stopWatch: UnlistenFn | undefined;

onMounted(() => {
  window.addEventListener("keydown", onGlobalKey, true);
  window.addEventListener("focus", onWindowResume);
  document.addEventListener("visibilitychange", onWindowResume);
  void listen<{ path: string; kind?: string }>("workdir-changed", (event) => {
    if (!sameRepo(event.payload.path, repoPath.value || props.path)) return;
    const kind = event.payload.kind === "git" ? "git" : "worktree";
    if (kind === "worktree") markFsDirty();
    if (busy.value) {
      pendingWatch = kind === "git" || pendingWatch === "git" ? "git" : "worktree";
      return;
    }
    scheduleGitRefresh(kind);
  }).then((unlisten) => {
    stopWatch = unlisten;
  });
  if (props.active) {
    void bootstrap(true);
  }
});

onUnmounted(() => {
  window.removeEventListener("keydown", onGlobalKey, true);
  window.removeEventListener("focus", onWindowResume);
  document.removeEventListener("visibilitychange", onWindowResume);
  window.clearTimeout(watchTimer);
  stopWatch?.();
  void unwatchRepo(watchingPath.value || props.path);
});
</script>

<template>
  <div class="workspace-root">
    <template v-if="repo">
      <Toolbar
        :repo="repo"
        :branches="branches"
        :busy="busy"
        :dirty="isDirty"
        :merging="Boolean(conflictState?.inProgress)"
        :recent="recent"
        :version="version"
        :files-open="showFiles"
        :terminal-open="showTerminal"
        :busy-text="statusText"
        :recent-branches="recentBranches"
        @files="showFiles = !showFiles"
        @terminal="toggleTerminal"
        @open="emit('open')"
        @open-path="emit('openPath', $event)"
        @fetch="handleFetch"
        @pull="handlePull"
        @commit="openCommit()"
        @push="showPush = true"
        @merge="showMerge = true"
        @new-branch="openNewBranchFromCurrent"
        @conflicts="openConflicts"
        @settings="emit('settings')"
        @branch-action="handleBranchAction"
      />
      <FilePreviewDrawer
        v-if="showFiles"
        :repo-path="repoPath"
        @close="showFiles = false"
        @changed="() => { markFsDirty(); scheduleGitRefresh('worktree'); }"
        @open-terminal="openTerminalAt"
      />
      <main v-else class="workspace">
        <LogPanel
          :repo="repo"
          :commits="commits"
          :graph="graph"
          :selected-hash="selectedHash"
          :branches="branches"
          :dirty="isDirty"
          :busy="busy"
          :log-loading="logLoading"
          @select="handleSelect"
          @refs-change="handleRefsChange"
          @changed="handleLogChanged"
          @amend="openAmend"
          @checkout="handleCheckout"
        />
        <FilesPanel :files="commitFiles" />
      </main>
      <TerminalPanel
        v-if="terminalReady"
        v-show="showTerminal"
        :repo-path="repoPath"
        :start-cwd="termStartCwd || repoPath"
        :spawn-seq="termSpawnSeq"
        :visible="showTerminal && active"
        @close="showTerminal = false"
      />
      <ErrorDialog
        v-if="actionError"
        :title="actionErrorTitle"
        :message="actionError"
        @close="actionError = ''"
      />
      <StatusBar :text="statusText" :error="errorText" />
      <div v-if="busy" class="git-busy-overlay">
        <div class="git-busy-card">
          <span class="btn-spinner" />
          <span>{{ statusText }}</span>
        </div>
      </div>
      <CommitDialog
        v-if="active && showCommit"
        :repo="repo"
        :changes="changes"
        :unversioned="unversioned"
        :busy="busy"
        :start-amend="startAmend"
        @close="showCommit = false; startAmend = false"
        @commit="handleCommit"
      />
      <PushDialog
        v-if="active && showPush"
        :repo="repo"
        :busy="busy"
        :initial-branch="pushBranch || repo.branch"
        @close="showPush = false; pushBranch = ''"
        @push="handlePush"
      />
      <Modal v-if="branchDialog === 'new'" title="新建分支" @close="branchDialog = ''">
        <p>从 <strong>{{ actionBranch?.name }}</strong> 创建新分支</p>
        <label>分支名 <input v-model="actionName" /></label>
        <label><input v-model="checkoutNewBranch" type="checkbox" /> 创建后切换</label>
        <template #footer>
          <button type="button" class="secondary-btn" @click="branchDialog = ''">取消</button>
          <button type="button" class="primary-btn" :disabled="busy || !actionName.trim()" @click="confirmNewBranch">
            <span v-if="busy" class="btn-spinner" />
            创建
          </button>
        </template>
      </Modal>
      <Modal v-if="branchDialog === 'rename'" title="重命名分支" @close="branchDialog = ''">
        <p>将 <strong>{{ actionBranch?.name }}</strong> 重命名为：</p>
        <label>新名称 <input v-model="actionName" /></label>
        <template #footer>
          <button type="button" class="secondary-btn" @click="branchDialog = ''">取消</button>
          <button type="button" class="primary-btn" :disabled="busy || !actionName.trim()" @click="confirmRenameBranch">
            <span v-if="busy" class="btn-spinner" />
            重命名
          </button>
        </template>
      </Modal>
      <Modal v-if="branchDialog === 'diff'" title="与工作树的差异" wide @close="branchDialog = ''">
        <p class="muted">工作树 → {{ actionBranch?.name }}</p>
        <p v-if="branchError" class="err">{{ branchError }}</p>
        <pre class="patch">{{ branchDiff?.binary ? "二进制差异" : (branchDiff?.patch || "没有差异") }}</pre>
        <template #footer>
          <button type="button" class="primary-btn" @click="branchDialog = ''">关闭</button>
        </template>
      </Modal>
      <MergeDialog
        v-if="active && showMerge"
        :repo="repo"
        :branches="branches"
        :recent="recentBranches"
        :busy="busy"
        @close="showMerge = false"
        @merge="handleMerge"
      />
      <ConflictsDialog
        v-if="active && showConflicts && conflictState && !mergeEditPath"
        :state="conflictState"
        :busy="busy"
        @close="showConflicts = false"
        @edit="mergeEditPath = $event"
        @accept="handleAcceptConflict"
        @continue="handleContinueMerge"
        @abort="handleAbortMerge"
      />
      <MergeEditorPage
        v-if="active && showConflicts && conflictState && mergeEditPath"
        :state="conflictState"
        :repo-path="repoPath"
        :path="mergeEditPath"
        :busy="busy"
        @back="mergeEditPath = ''"
        @save="handleSaveResolved"
        @abort="handleAbortMerge"
      />
    </template>
    <div v-else class="empty-hint">正在打开仓库…</div>
  </div>
</template>

<style scoped>
.workspace-root {
  position: relative;
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
}

.git-busy-overlay {
  position: absolute;
  inset: 36px 0 0;
  z-index: 80;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(43, 45, 48, 0.55);
  pointer-events: all;
}

.git-busy-card {
  display: inline-flex;
  align-items: center;
  gap: 10px;
  padding: 12px 16px;
  border-radius: 8px;
  background: var(--bg-elevated);
  border: 1px solid var(--border);
  color: var(--text);
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.35);
}

.workspace {
  position: relative;
  flex: 1;
  min-height: 0;
  display: flex;
}

label {
  display: flex;
  align-items: center;
  gap: 8px;
}

input[type="text"],
label input:not([type="checkbox"]) {
  height: 30px;
  border: 1px solid var(--border-soft);
  background: var(--bg-input);
  border-radius: 4px;
  padding: 0 8px;
}

.patch {
  margin: 0;
  max-height: 480px;
  overflow: auto;
  padding: 8px;
  background: var(--bg-deep);
  font-family: Consolas, "Courier New", monospace;
  font-size: 12px;
  white-space: pre-wrap;
}

.err {
  color: var(--danger);
}
</style>
