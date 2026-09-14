<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch, type CSSProperties } from "vue";

const props = defineProps<{
  open: boolean;
  anchor?: HTMLElement | null;
  x?: number;
  y?: number;
}>();

const emit = defineEmits<{
  close: [];
}>();

const menu = ref<HTMLElement | null>(null);
const style = ref<CSSProperties>({});

const positioned = computed<CSSProperties>(() => ({
  ...style.value,
  position: "fixed",
  zIndex: 1000,
}));

function updatePosition() {
  if (props.x != null && props.y != null) {
    const width = 320;
    const height = 420;
    let left = props.x;
    let top = props.y;
    if (left + width > window.innerWidth - 8) {
      left = Math.max(8, window.innerWidth - width - 8);
    }
    if (top + 240 > window.innerHeight - 8) {
      top = Math.max(8, window.innerHeight - Math.min(height, window.innerHeight - 16));
    }
    style.value = {
      top: `${top}px`,
      left: `${left}px`,
      minWidth: `${width}px`,
      maxHeight: `${Math.max(160, window.innerHeight - top - 8)}px`,
    };
    return;
  }
  const anchor = props.anchor;
  if (!anchor) return;
  const rect = anchor.getBoundingClientRect();
  const width = Math.max(rect.width, 320);
  let left = rect.left;
  if (left + width > window.innerWidth - 8) {
    left = Math.max(8, window.innerWidth - width - 8);
  }
  style.value = {
    top: `${rect.bottom + 4}px`,
    left: `${left}px`,
    minWidth: `${width}px`,
    maxHeight: `${Math.max(160, window.innerHeight - rect.bottom - 16)}px`,
  } satisfies CSSProperties;
}

function onDocMouseDown(event: MouseEvent) {
  const target = event.target as Node;
  if (props.anchor?.contains(target) || menu.value?.contains(target)) {
    return;
  }
  emit("close");
}

onMounted(() => {
  document.addEventListener("mousedown", onDocMouseDown);
  window.addEventListener("resize", updatePosition);
  window.addEventListener("scroll", updatePosition, true);
});

onUnmounted(() => {
  document.removeEventListener("mousedown", onDocMouseDown);
  window.removeEventListener("resize", updatePosition);
  window.removeEventListener("scroll", updatePosition, true);
});

watch(
  () => [props.open, props.x, props.y, props.anchor] as const,
  async ([open]) => {
    if (!open) return;
    await nextTick();
    updatePosition();
  },
);
</script>

<template>
  <Teleport to="body">
    <div v-if="open" ref="menu" class="popup-menu" :style="positioned" @mousedown.stop>
      <slot />
    </div>
  </Teleport>
</template>
