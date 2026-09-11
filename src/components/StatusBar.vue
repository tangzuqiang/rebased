<script setup lang="ts">
defineProps<{
  text: string;
  error: string;
}>();
</script>

<template>
  <footer class="status">
    <span :class="{ err: error }">{{ error || text }}</span>
  </footer>
</template>

<style scoped>
.status {
  height: 24px;
  padding: 0 10px;
  display: flex;
  align-items: center;
  border-top: 1px solid var(--border);
  background: #2b2d30;
  color: var(--text-muted);
}

.err {
  color: var(--danger);
}
</style>
