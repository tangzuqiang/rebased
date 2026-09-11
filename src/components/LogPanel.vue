<script setup lang="ts">
import { computed, ref } from "vue";
import { save } from "@tauri-apps/plugin-dialog";
import {
  checkoutRevision,
  cherryPick,
  commitDiff,
  createBranchAt,
  createTagAt,
  dropCommit,
  fixupCommit,
  formatPatch,
  pushUpto,
  rebasePlan,
  resetTo,
  revertCommit,
  squashCommit,
  undoCommit,
  writeTextFile,
} from "../api";
import { formatLogDate } from "../path";
import type { BranchInfo, FileDiff, GitCommit, GraphRow, RepoInfo } from "../types";
import BranchTree from "./BranchTree.vue";
import Icons from "./Icons.vue";
import Modal from "./Modal.vue";
import PopupMenu from "./PopupMenu.vue";
import UserFilter from "./UserFilter.vue";

const props = defineProps<{
  repo: RepoInfo;
  commits: GitCommit[];
  graph: GraphRow[];
  selectedHash: string;
  branches: BranchInfo[];
  dirty: boolean;
  busy: boolean;
}>();

const emit = defineEmits<{
  select: [hash: string];
  refsChange: [refs: string[]];
  changed: [message?: string];
  amend: [];
  checkout: [name: string];
}>();

const query = ref("");
const userFilter = ref<string[]>([]);
const dateFilter = ref<"all" | "today" | "week">("all");

const authors = computed(() => props.commits.map((commit) => commit.author).filter(Boolean));
const menu = ref({ open: false, x: 0, y: 0, hash: "" });
const dialog = ref<
  | ""
  | "reset"
  | "branch"
  | "tag"
  | "compare"
  | "rebase"
  | "confirm-revert"
  | "confirm-drop"
  | "confirm-undo"
  | "confirm-checkout"
>("");
const resetMode = ref<"soft" | "mixed" | "hard">("mixed");
const refName = ref("");
const checkoutAfterCreate = ref(true);
const comparePatch = ref<FileDiff | null>(null);
const rebaseItems = ref<Array<{ action: string; hash: string; message: string }>>([]);
const acting = ref(false);
const actionError = ref("");

const selected = computed(() => props.commits.find((item) => item.hash === (menu.value.hash || props.selectedHash)));
const isHead = computed(() => Boolean(selected.value && selected.value.hash === props.repo.head));
const commitRefs = computed(() => (selected.value ? cleanRefs(selected.value.refs) : []));

const filtered = computed(() => {
  const q = query.value.trim().toLowerCase();
  const now = Date.now() / 1000;
  return props.commits
    .map((commit, index) => ({ commit, graph: props.graph[index] }))
    .filter(({ commit }) => {
      if (q && !commit.message.toLowerCase().includes(q) && !commit.hash.toLowerCase().includes(q)) {
        return false;
      }
      if (userFilter.value.length && !userFilter.value.some((name) => commit.author.toLowerCase() === name.toLowerCase() || commit.author.toLowerCase().includes(name.toLowerCase()))) {
        return false;
      }
      if (dateFilter.value === "today" && now - commit.timestamp > 86_400) return false;
      if (dateFilter.value === "week" && now - commit.timestamp > 7 * 86_400) return false;
      return true;
    });
});

function laneWidth(row?: GraphRow) {
  const count = Math.max(row?.lanes.length ?? 1, 1);
  return Math.min(count, 8) * 14 + 8;
}

function cleanRefs(refs: string[]) {
  return refs
    .flatMap((item) => item.split(","))
    .map((item) => item.trim().replace(/^HEAD\s*->\s*/, "").replace(/^tag:\s*/, ""))
    .filter((item) => item && item !== "HEAD");
}

function openMenu(event: MouseEvent, hash: string) {
  event.preventDefault();
  emit("select", hash);
  menu.value = { open: true, x: event.clientX, y: event.clientY, hash };
}

function closeMenu() {
  menu.value.open = false;
}

function closeDialog() {
  dialog.value = "";
  actionError.value = "";
}

async function copyHash() {
  const hash = selected.value?.hash;
  if (!hash) return;
  await navigator.clipboard.writeText(hash).catch(() => undefined);
  closeMenu();
}

async function runAction(label: string, task: () => Promise<string | void>) {
  acting.value = true;
  actionError.value = "";
  try {
    const message = await task();
    closeMenu();
    closeDialog();
    emit("changed", typeof message === "string" ? message : label);
  } catch (error) {
    actionError.value = error instanceof Error ? error.message : String(error);
  } finally {
    acting.value = false;
  }
}

function createPatch() {
  const commit = selected.value;
  if (!commit) return;
  closeMenu();
  void runAction("已创建补丁", async () => {
    const patch = await formatPatch(props.repo.path, commit.hash);
    const dest = await save({
      defaultPath: `${commit.shortHash}.patch`,
      filters: [{ name: "Patch", extensions: ["patch"] }],
    });
    if (!dest) return "已取消";
    await writeTextFile(dest, patch);
    return "已保存补丁";
  });
}

function doCherryPick() {
  const hash = selected.value?.hash;
  if (!hash) return;
  void runAction("正在拣选…", () => cherryPick(props.repo.path, hash));
}

function confirmCheckout() {
  const hash = selected.value?.hash;
  if (!hash) return;
  void runAction("已检出", () => checkoutRevision(props.repo.path, hash));
}

function doCheckoutRev() {
  const hash = selected.value?.hash;
  if (!hash) return;
  dialog.value = "confirm-checkout";
  closeMenu();
}

function doCompare() {
  const commit = selected.value;
  if (!commit) return;
  closeMenu();
  dialog.value = "compare";
  comparePatch.value = null;
  void commitDiff(props.repo.path, commit.hash, "HEAD").then((diff) => {
    comparePatch.value = diff;
  }).catch((error) => {
    actionError.value = error instanceof Error ? error.message : String(error);
  });
}

function doReset() {
  closeMenu();
  dialog.value = "reset";
}

function confirmReset() {
  const hash = selected.value?.hash;
  if (!hash) return;
  void runAction("已重置", () => resetTo(props.repo.path, hash, resetMode.value));
}

function doRevert() {
  closeMenu();
  dialog.value = "confirm-revert";
}

function confirmRevert() {
  const hash = selected.value?.hash;
  if (!hash) return;
  void runAction("已还原", () => revertCommit(props.repo.path, hash));
}

function doUndo() {
  closeMenu();
  dialog.value = "confirm-undo";
}

function confirmUndo() {
  void runAction("已撤销提交", () => undoCommit(props.repo.path));
}

function doAmend() {
  closeMenu();
  emit("amend");
}

function doFixup() {
  const hash = selected.value?.hash;
  if (!hash) return;
  void runAction("已 Fixup", () => fixupCommit(props.repo.path, hash));
}

function doSquash() {
  const hash = selected.value?.hash;
  if (!hash) return;
  void runAction("已压缩", () => squashCommit(props.repo.path, hash));
}

function doDrop() {
  closeMenu();
  dialog.value = "confirm-drop";
}

function confirmDrop() {
  const hash = selected.value?.hash;
  if (!hash) return;
  void runAction("已删除提交", () => dropCommit(props.repo.path, hash));
}

function doRebase() {
  const commit = selected.value;
  if (!commit) return;
  const index = props.commits.findIndex((item) => item.hash === commit.hash);
  const slice = props.commits.slice(0, index + 1).slice().reverse();
  rebaseItems.value = slice.map((item) => ({
    action: "pick",
    hash: item.hash,
    message: item.message,
  }));
  closeMenu();
  dialog.value = "rebase";
}

function confirmRebase() {
  const first = rebaseItems.value[0];
  const commit = props.commits.find((item) => item.hash === first?.hash);
  const upstream = commit?.parents[0];
  if (!upstream) {
    actionError.value = "无法确定变基起点";
    return;
  }
  void runAction("变基完成", () =>
    rebasePlan(
      props.repo.path,
      upstream,
      rebaseItems.value.map((item) => ({ action: item.action, hash: item.hash })),
    ),
  );
}

function doPushUpto() {
  const hash = selected.value?.hash;
  if (!hash) return;
  void runAction("已推送", () => pushUpto(props.repo.path, "", props.repo.branch, hash));
}

function doNewBranch() {
  refName.value = "";
  checkoutAfterCreate.value = true;
  closeMenu();
  dialog.value = "branch";
}

function confirmBranch() {
  const hash = selected.value?.hash;
  if (!hash || !refName.value.trim()) return;
  void runAction("已创建分支", async () => {
    const message = await createBranchAt(props.repo.path, refName.value.trim(), hash, checkoutAfterCreate.value);
    if (checkoutAfterCreate.value) {
      emit("checkout", refName.value.trim());
    }
    return message;
  });
}

function doNewTag() {
  refName.value = "";
  closeMenu();
  dialog.value = "tag";
}

function confirmTag() {
  const hash = selected.value?.hash;
  if (!hash || !refName.value.trim()) return;
  void runAction("已创建标签", () => createTagAt(props.repo.path, refName.value.trim(), hash));
}

function goChild() {
  const hash = selected.value?.hash;
  if (!hash) return;
  const child = props.commits.find((item) => item.parents.includes(hash));
  closeMenu();
  if (child) emit("select", child.hash);
}

function goParent() {
  const parent = selected.value?.parents[0];
  closeMenu();
  if (parent) emit("select", parent);
}

function checkoutRef(name: string) {
  closeMenu();
  emit("checkout", name);
}
</script>

<template>
  <section class="log">
    <div class="log-body">
      <BranchTree
        :branches="branches"
        :current-branch="repo.branch"
        @change="emit('refsChange', $event)"
      />
      <div class="main">
        <div class="filters">
          <label class="chip grow">
            <Icons name="search" :size="13" />
            <input v-model="query" placeholder="文本或哈希" />
          </label>
          <UserFilter
            :authors="authors"
            :me="repo.userName || ''"
            @change="userFilter = $event"
          />
          <label class="chip">
            日期
            <select v-model="dateFilter">
              <option value="all">全部</option>
              <option value="today">今天</option>
              <option value="week">最近 7 天</option>
            </select>
          </label>
        </div>
        <div class="table-head">
          <span class="col-msg">提交说明</span>
          <span class="col-ref">分支</span>
          <span class="col-user">用户</span>
          <span class="col-date">日期</span>
        </div>
        <div class="scroll rows">
          <button
            v-for="{ commit, graph } in filtered"
            :key="commit.hash"
            type="button"
            class="row"
            :class="{ selected: commit.hash === selectedHash, merge: commit.parents.length > 1 }"
            :title="commit.parents.length > 1 ? '合并提交' : commit.message"
            @click="emit('select', commit.hash)"
            @contextmenu="openMenu($event, commit.hash)"
          >
            <span class="col-msg">
              <svg
                class="graph"
                :width="laneWidth(graph)"
                height="22"
                :viewBox="`0 0 ${laneWidth(graph)} 22`"
              >
                <template v-for="(lane, index) in graph?.lanes ?? []" :key="index">
                  <line
                    v-if="lane.symbol !== 'empty'"
                    :x1="10 + index * 14"
                    y1="0"
                    :x2="10 + index * 14"
                    y2="22"
                    :stroke="lane.color"
                    stroke-width="1.4"
                  />
                  <circle
                    v-if="lane.symbol === 'dot'"
                    :cx="10 + index * 14"
                    cy="11"
                    r="3.3"
                    :fill="lane.color"
                  />
                </template>
              </svg>
              <span class="msg">{{ commit.message }}</span>
              <span v-if="commit.parents.length > 1" class="merge-tag">合并</span>
            </span>
            <span class="col-ref">
              <span v-if="commit.hash === repo.head" class="ref head">HEAD</span>
              <span v-for="name in cleanRefs(commit.refs)" :key="name" class="ref">{{ name }}</span>
            </span>
            <span class="col-user muted">{{ commit.author }}</span>
            <span class="col-date muted">{{ formatLogDate(commit.timestamp) }}</span>
          </button>
          <div v-if="!filtered.length" class="empty-hint">没有匹配的提交</div>
        </div>
      </div>
    </div>

    <PopupMenu :open="menu.open" :x="menu.x" :y="menu.y" @close="closeMenu">
      <button type="button" @click="copyHash">复制修订号</button>
      <button type="button" :disabled="busy || acting" @click="createPatch">创建补丁…</button>
      <button type="button" :disabled="busy || acting || isHead" @click="doCherryPick">拣选</button>
      <button type="button" :disabled="busy || acting || isHead" @click="doCheckoutRev">检出修订</button>
      <button type="button" :disabled="busy || acting || isHead" @click="doCompare">与本地比较</button>
      <button type="button" :disabled="busy || acting" @click="doReset">将当前分支重置到此处…</button>
      <button type="button" :disabled="busy || acting" @click="doRevert">还原提交</button>
      <button type="button" :disabled="busy || acting || !isHead" @click="doUndo">撤销提交</button>
      <button type="button" :disabled="busy || acting || !isHead" @click="doAmend">编辑提交信息…</button>
      <button type="button" :disabled="busy || acting || !dirty" @click="doFixup">Fixup…</button>
      <button type="button" :disabled="busy || acting || isHead" @click="doSquash">压缩…</button>
      <button type="button" :disabled="busy || acting || isHead" @click="doDrop">删除提交</button>
      <button type="button" :disabled="busy || acting" @click="doRebase">从此处交互变基…</button>
      <button type="button" :disabled="busy || acting || repo.detached" @click="doPushUpto">推送到此前的所有提交…</button>
      <div v-if="commitRefs.length" class="divider" />
      <button v-for="name in commitRefs" :key="name" type="button" @click="checkoutRef(name)">
        分支 {{ name }}
      </button>
      <button type="button" :disabled="busy || acting" @click="doNewBranch">新建分支…</button>
      <button type="button" :disabled="busy || acting" @click="doNewTag">新建标签…</button>
      <button type="button" @click="goChild">转到子提交</button>
      <button type="button" @click="goParent">转到父提交</button>
    </PopupMenu>

    <Modal v-if="dialog === 'reset'" title="将当前分支重置到此处" @close="closeDialog">
      <p>将 <strong>{{ repo.branch }}</strong> 重置到 {{ selected?.shortHash }}：{{ selected?.message }}</p>
      <label><input v-model="resetMode" type="radio" value="soft" /> 软重置（保留暂存和工作区）</label>
      <label><input v-model="resetMode" type="radio" value="mixed" /> 混合重置（保留工作区，取消暂存）</label>
      <label><input v-model="resetMode" type="radio" value="hard" /> 硬重置（丢弃未提交变更）</label>
      <p v-if="actionError" class="err">{{ actionError }}</p>
      <template #footer>
        <button type="button" class="secondary-btn" @click="closeDialog">取消</button>
        <button type="button" class="primary-btn" :disabled="acting" @click="confirmReset">重置</button>
      </template>
    </Modal>

    <Modal v-if="dialog === 'branch'" title="新建分支" @close="closeDialog">
      <label>分支名 <input v-model="refName" /></label>
      <label><input v-model="checkoutAfterCreate" type="checkbox" /> 创建后切换</label>
      <p v-if="actionError" class="err">{{ actionError }}</p>
      <template #footer>
        <button type="button" class="secondary-btn" @click="closeDialog">取消</button>
        <button type="button" class="primary-btn" :disabled="acting || !refName.trim()" @click="confirmBranch">创建</button>
      </template>
    </Modal>

    <Modal v-if="dialog === 'tag'" title="新建标签" @close="closeDialog">
      <label>标签名 <input v-model="refName" /></label>
      <p v-if="actionError" class="err">{{ actionError }}</p>
      <template #footer>
        <button type="button" class="secondary-btn" @click="closeDialog">取消</button>
        <button type="button" class="primary-btn" :disabled="acting || !refName.trim()" @click="confirmTag">创建</button>
      </template>
    </Modal>

    <Modal v-if="dialog === 'compare'" title="与本地比较" wide @close="closeDialog">
      <p class="muted">{{ selected?.shortHash }} → HEAD</p>
      <pre class="patch">{{ comparePatch?.binary ? "二进制差异" : comparePatch?.patch || "正在加载…" }}</pre>
      <p v-if="actionError" class="err">{{ actionError }}</p>
      <template #footer>
        <button type="button" class="primary-btn" @click="closeDialog">关闭</button>
      </template>
    </Modal>

    <Modal v-if="dialog === 'rebase'" title="从此处交互变基" @close="closeDialog">
      <p>从该提交到 HEAD 的提交将按下列动作重放。</p>
      <div class="rebase-list">
        <label v-for="item in rebaseItems" :key="item.hash">
          <select v-model="item.action">
            <option value="pick">pick</option>
            <option value="squash">squash</option>
            <option value="fixup">fixup</option>
            <option value="drop">drop</option>
          </select>
          <span>{{ item.hash.slice(0, 7) }} {{ item.message }}</span>
        </label>
      </div>
      <p v-if="actionError" class="err">{{ actionError }}</p>
      <template #footer>
        <button type="button" class="secondary-btn" @click="closeDialog">取消</button>
        <button type="button" class="primary-btn" :disabled="acting" @click="confirmRebase">开始变基</button>
      </template>
    </Modal>

    <Modal v-if="dialog === 'confirm-revert'" title="还原提交" @close="closeDialog">
      <p>将生成一次新提交，用来撤销 {{ selected?.shortHash }}。</p>
      <p v-if="actionError" class="err">{{ actionError }}</p>
      <template #footer>
        <button type="button" class="secondary-btn" @click="closeDialog">取消</button>
        <button type="button" class="primary-btn" :disabled="acting" @click="confirmRevert">还原</button>
      </template>
    </Modal>

    <Modal v-if="dialog === 'confirm-drop'" title="删除提交" @close="closeDialog">
      <p>将从当前分支历史中删除 {{ selected?.shortHash }}，之后的提交会变基重放。</p>
      <p v-if="actionError" class="err">{{ actionError }}</p>
      <template #footer>
        <button type="button" class="secondary-btn" @click="closeDialog">取消</button>
        <button type="button" class="primary-btn" :disabled="acting" @click="confirmDrop">删除</button>
      </template>
    </Modal>

    <Modal v-if="dialog === 'confirm-undo'" title="撤销提交" @close="closeDialog">
      <p>撤销最近一次提交，文件变更会留在暂存区。</p>
      <p v-if="actionError" class="err">{{ actionError }}</p>
      <template #footer>
        <button type="button" class="secondary-btn" @click="closeDialog">取消</button>
        <button type="button" class="primary-btn" :disabled="acting" @click="confirmUndo">撤销</button>
      </template>
    </Modal>

    <Modal v-if="dialog === 'confirm-checkout'" title="检出修订" @close="closeDialog">
      <p>将进入游离 HEAD，工作区切换到 {{ selected?.shortHash }}。</p>
      <p v-if="actionError" class="err">{{ actionError }}</p>
      <template #footer>
        <button type="button" class="secondary-btn" @click="closeDialog">取消</button>
        <button
          type="button"
          class="primary-btn"
          :disabled="acting"
          @click="confirmCheckout"
        >
          检出
        </button>
      </template>
    </Modal>
  </section>
</template>

<style scoped>
.log {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  background: #2b2d30;
}

.log-body {
  flex: 1;
  min-height: 0;
  display: flex;
}

.main {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
}

.filters {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 6px 8px;
  border-bottom: 1px solid var(--border);
}

.grow {
  flex: 1;
  min-width: 90px;
}

.chip select {
  background: transparent;
  border: 0;
  outline: none;
  color: inherit;
}

.table-head,
.row {
  display: grid;
  grid-template-columns: minmax(0, 1.6fr) minmax(120px, 0.8fr) 110px 110px;
  align-items: center;
}

.table-head {
  height: 24px;
  padding: 0 8px;
  color: var(--text-dim);
  border-bottom: 1px solid var(--border);
}

.rows {
  flex: 1;
}

.row {
  width: 100%;
  height: 22px;
  padding: 0 8px;
  text-align: left;
  color: var(--text);
}

.row:hover {
  background: #323437;
}

.row.selected {
  background: var(--bg-selected);
}

.row.merge .msg {
  color: #d2a8ff;
  font-weight: 600;
}

.row.merge.selected .msg {
  color: #efd4ff;
}

.merge-tag {
  flex: none;
  height: 16px;
  padding: 0 5px;
  border-radius: 3px;
  background: #5b3d7a;
  color: #efd4ff;
  font-size: 11px;
  line-height: 16px;
}

.col-msg {
  display: flex;
  align-items: center;
  min-width: 0;
  gap: 4px;
}

.graph {
  flex: none;
}

.msg,
.col-ref,
.col-user,
.col-date {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.col-ref {
  display: flex;
  gap: 4px;
}

.ref {
  display: inline-block;
  padding: 0 5px;
  border-radius: 3px;
  background: #38507a;
  color: #c7d7f0;
  font-size: 10px;
}

.ref.head {
  background: #3d5a2c;
  color: #c8e6a0;
}

.err {
  color: var(--danger);
}

.patch {
  margin: 0;
  max-height: 520px;
  overflow: auto;
  padding: 8px;
  background: #1e1f22;
  font-family: Consolas, "Courier New", monospace;
  font-size: 12px;
  white-space: pre-wrap;
}

.rebase-list,
.rebase-list label {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.rebase-list label {
  flex-direction: row;
  align-items: center;
}

.rebase-list span {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

label {
  display: flex;
  align-items: center;
  gap: 8px;
}
</style>
