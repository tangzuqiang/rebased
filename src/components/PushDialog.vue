<script setup lang="ts">
import { onMounted, ref } from "vue";
import { getRemotes } from "../api";
import type { RepoInfo } from "../types";
import Modal from "./Modal.vue";

const props = defineProps<{
  repo: RepoInfo;
  busy: boolean;
  initialBranch?: string;
}>();

const emit = defineEmits<{
  close: [];
  push: [payload: { remote: string; branch: string }];
}>();

const remotes = ref<string[]>([]);
const remote = ref("");
const branch = ref(props.initialBranch || props.repo.branch);

onMounted(async () => {
  remotes.value = await getRemotes(props.repo.path).catch(() => []);
  remote.value = remotes.value[0] ?? "";
});
</script>

<template>
  <Modal title="推送" @close="emit('close')">
    <div class="push-dialog">
      <p class="muted">将当前分支推送到远程仓库。</p>
      <label>
        <span>远程</span>
        <select v-model="remote">
          <option v-if="!remotes.length" value="">暂无远程</option>
          <option v-for="item in remotes" :key="item" :value="item">{{ item }}</option>
        </select>
      </label>
      <label>
        <span>分支</span>
        <input v-model="branch" />
      </label>
      <p v-if="repo.ahead || repo.behind" class="muted">
        领先 {{ repo.ahead }} / 落后 {{ repo.behind }}
      </p>
    </div>
    <template #footer>
      <button type="button" class="secondary-btn" :disabled="busy" @click="emit('close')">取消</button>
      <button
        type="button"
        class="primary-btn"
        :class="{ 'is-loading': busy }"
        :disabled="busy || !remote || !branch"
        @click="emit('push', { remote, branch })"
      >
        <span v-if="busy" class="btn-spinner" />
        {{ busy ? "正在推送…" : "推送" }}
      </button>
    </template>
  </Modal>
</template>

<style scoped>
.push-dialog {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

label {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

select,
input {
  height: 30px;
  border: 1px solid var(--border-soft);
  background: var(--bg-input);
  border-radius: 4px;
  padding: 0 8px;
  outline: none;
}

select:focus,
input:focus {
  border-color: var(--accent);
}
</style>
