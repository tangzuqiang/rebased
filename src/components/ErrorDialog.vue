<script setup lang="ts">
import { ref } from "vue";
import Icons from "./Icons.vue";
import Modal from "./Modal.vue";

const props = defineProps<{
  title?: string;
  message: string;
}>();

const emit = defineEmits<{
  close: [];
}>();

const copied = ref(false);

async function copy() {
  await navigator.clipboard.writeText(props.message).catch(() => undefined);
  copied.value = true;
  window.setTimeout(() => {
    copied.value = false;
  }, 1500);
}
</script>

<template>
  <Modal :title="title || '操作失败'" medium @close="emit('close')">
    <div class="error-box">
      <div class="toolbar">
        <button type="button" class="icon-btn" :title="copied ? '已复制' : '复制错误信息'" @click="copy">
          <Icons name="copy" :size="14" />
        </button>
        <span class="muted">{{ copied ? "已复制" : "复制错误信息" }}</span>
      </div>
      <pre>{{ message }}</pre>
    </div>
    <template #footer>
      <button type="button" class="secondary-btn" @click="copy">
        <Icons name="copy" :size="12" />
        {{ copied ? "已复制" : "复制" }}
      </button>
      <button type="button" class="primary-btn" @click="emit('close')">关闭</button>
    </template>
  </Modal>
</template>

<style scoped>
.error-box {
  display: flex;
  flex-direction: column;
  min-height: 280px;
  height: 100%;
}

.toolbar {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-bottom: 8px;
}

pre {
  flex: 1;
  min-height: 240px;
  margin: 0;
  padding: 12px;
  overflow: auto;
  white-space: pre-wrap;
  word-break: break-word;
  border: 1px solid var(--border-soft);
  border-radius: 6px;
  background: var(--bg-deep);
  color: var(--danger);
  font-family: Consolas, "Courier New", monospace;
  font-size: 12px;
  line-height: 18px;
}
</style>
