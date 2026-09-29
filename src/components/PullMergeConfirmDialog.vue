<script setup lang="ts">
import type { RepoInfo } from "../types";
import Modal from "./Modal.vue";

defineProps<{
  repo: RepoInfo;
  upstream: string;
  busy: boolean;
}>();

const emit = defineEmits<{
  close: [];
  confirm: [];
}>();
</script>

<template>
  <Modal title="合并远程更新" medium @close="emit('close')">
    <p class="lead">
      本地与远程均有新提交，无法直接快进拉取。
    </p>
    <p class="muted">
      是否将 <strong>{{ upstream }}</strong> 合并到当前分支 <strong>{{ repo.branch }}</strong>？
      取消不会修改仓库。
    </p>
    <p class="muted hint">若产生冲突，将打开冲突处理界面。</p>
    <template #footer>
      <button type="button" class="secondary-btn" :disabled="busy" @click="emit('close')">取消</button>
      <button type="button" class="primary-btn" :disabled="busy" @click="emit('confirm')">
        <span v-if="busy" class="btn-spinner" />
        确认合并
      </button>
    </template>
  </Modal>
</template>

<style scoped>
.lead {
  margin: 0 0 10px;
}

.hint {
  margin: 12px 0 0;
}
</style>
