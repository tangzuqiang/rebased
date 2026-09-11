<script setup lang="ts">
import { ref } from "vue";

const props = defineProps<{
  title: string;
  wide?: boolean;
  medium?: boolean;
  allowFullscreen?: boolean;
  noFooter?: boolean;
  flush?: boolean;
}>();

const emit = defineEmits<{
  close: [];
}>();

const fullscreen = ref(false);
</script>

<template>
  <Teleport to="body">
    <div class="modal-mask" :class="{ fill: fullscreen }" @mousedown.self="emit('close')">
      <div class="modal-box" :class="{ wide, medium, fullscreen, flush }">
        <header class="modal-head">
          <h2>{{ title }}</h2>
          <div class="head-actions">
            <button
              v-if="allowFullscreen"
              type="button"
              class="icon-btn"
              :title="fullscreen ? '退出全屏' : '全屏'"
              @click="fullscreen = !fullscreen"
            >
              {{ fullscreen ? "❐" : "⛶" }}
            </button>
            <button type="button" class="icon-btn" title="关闭" @click="emit('close')">✕</button>
          </div>
        </header>
        <div class="modal-body">
          <slot />
        </div>
        <footer v-if="!noFooter" class="modal-foot">
          <slot name="footer" />
        </footer>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.head-actions {
  display: flex;
  align-items: center;
  gap: 4px;
}
</style>
