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
  top: 30%;
  left: 50%;
  transform: translateX(-50%);
  z-index: 2000;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 10px;
  pointer-events: none;
}

.toast {
  min-width: 280px;
  max-width: min(520px, calc(100vw - 32px));
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 13px 18px;
  border-radius: 8px;
  background: var(--bg-elevated);
  border: 1px solid var(--border-elevated);
  box-shadow: var(--shadow);
  color: var(--text);
  font-size: 15px;
  line-height: 1.5;
}

.toast.success .dot {
  background: var(--added);
}

.toast.error .dot {
  background: var(--danger);
}

.dot {
  width: 10px;
  height: 10px;
  border-radius: 50%;
  flex: none;
}
</style>
