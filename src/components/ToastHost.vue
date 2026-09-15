<script setup lang="ts">
import { useToasts } from "../notify";

const toasts = useToasts();
</script>

<template>
  <Teleport to="body">
    <div class="toasts" aria-live="polite">
      <div v-for="item in toasts" :key="item.id" class="toast" :class="item.kind">
        <span class="dot" />
        <span>{{ item.text }}</span>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.toasts {
  position: fixed;
  right: 16px;
  bottom: 28px;
  z-index: 2000;
  display: flex;
  flex-direction: column;
  gap: 8px;
  pointer-events: none;
}

.toast {
  min-width: 220px;
  max-width: min(420px, calc(100vw - 32px));
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 10px 14px;
  border-radius: 6px;
  background: var(--bg-elevated);
  border: 1px solid var(--border-elevated);
  box-shadow: var(--shadow);
  color: var(--text);
}

.toast.success .dot {
  background: var(--added);
}

.toast.error .dot {
  background: var(--danger);
}

.dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  flex: none;
}
</style>
