<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { compareMerge } from "../api";
import type { BranchInfo, RepoInfo } from "../types";
import BranchFilterList from "./BranchFilterList.vue";
import Modal from "./Modal.vue";

const props = defineProps<{
  repo: RepoInfo;
  branches: BranchInfo[];
  recent?: string[];
  busy: boolean;
}>();

const emit = defineEmits<{
  close: [];
  merge: [theirs: string];
}>();

const theirs = ref("");
const canMerge = ref(false);
const sameHint = ref("");
const checking = ref(false);
const exclude = computed(() => [props.repo.branch]);
let compareSeq = 0;

function chooseTheirs(name: string) {
  theirs.value = name;
}

watch(theirs, async (name) => {
  const seq = ++compareSeq;
  canMerge.value = false;
  sameHint.value = "";
  if (!name) {
    checking.value = false;
    return;
  }
  checking.value = true;
  try {
    const result = await compareMerge(props.repo.path, name);
    if (seq !== compareSeq) return;
    canMerge.value = result.canMerge;
    sameHint.value = result.canMerge ? "" : result.message;
  } catch {
    if (seq !== compareSeq) return;
    canMerge.value = true;
    sameHint.value = "";
  } finally {
    if (seq === compareSeq) checking.value = false;
  }
});
</script>

<template>
  <Modal title="合并" tall @close="emit('close')">
    <div class="merge">
      <p class="muted">没有冲突时会直接完成合并；有冲突时再弹出冲突文件和三端对照。</p>
      <label>
        当前分支
        <strong>{{ repo.branch }}</strong>
      </label>
      <div class="picker">
        <span>合并入</span>
        <div class="picker-value">{{ theirs || "选择要合并的分支" }}</div>
      </div>
      <p v-if="sameHint" class="muted hint">{{ sameHint }}</p>
      <div class="picker-list">
        <BranchFilterList
          :branches="branches"
          :exclude="exclude"
          :selected="theirs"
          :recent="recent"
          :enable-actions="false"
          :default-open="['recent']"
          @select="chooseTheirs"
        />
      </div>
    </div>
    <template #footer>
      <button type="button" class="secondary-btn" @click="emit('close')">取消</button>
      <button
        type="button"
        class="primary-btn"
        :disabled="busy || checking || !theirs || !canMerge"
        :title="sameHint"
        @click="emit('merge', theirs)"
      >
        <span v-if="busy" class="btn-spinner" />
        合并
      </button>
    </template>
  </Modal>
</template>

<style scoped>
.merge {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  gap: 14px;
}

.picker {
  display: flex;
  align-items: center;
  gap: 8px;
}

.picker-value {
  flex: 1;
  min-height: 30px;
  padding: 0 10px;
  display: flex;
  align-items: center;
  border: 1px solid var(--border-soft);
  border-radius: 4px;
  background: var(--bg-input);
}

.picker-list {
  flex: 1;
  min-height: 200px;
  overflow: auto;
  border: 1px solid var(--border-soft);
  border-radius: 6px;
  background: var(--bg-input);
}

.picker-list :deep(.tree) {
  max-height: none;
}

.hint {
  margin: 0;
}
</style>
