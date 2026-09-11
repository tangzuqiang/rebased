<script setup lang="ts">
import { FitAddon } from "@xterm/addon-fit";
import { Terminal } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { nextTick, onMounted, onUnmounted, ref, watch } from "vue";
import { closeTerminal, resizeTerminal, startTerminal, writeTerminal } from "../api";

const props = defineProps<{
  id: string;
  cwd: string;
  active: boolean;
}>();

const emit = defineEmits<{
  error: [message: string];
}>();

const host = ref<HTMLElement | null>(null);
let term: Terminal | null = null;
let fit: FitAddon | null = null;
let unlistenData: UnlistenFn | undefined;
let unlistenExit: UnlistenFn | undefined;

function currentSize() {
  if (!term) return { cols: 80, rows: 24 };
  return {
    cols: term.cols >= 20 ? term.cols : 80,
    rows: term.rows >= 8 ? term.rows : 24,
  };
}

function applySize() {
  if (!term || !fit || !props.active) return;
  const width = host.value?.clientWidth ?? 0;
  const height = host.value?.clientHeight ?? 0;
  if (width < 40 || height < 24) return;
  fit.fit();
  const { cols, rows } = currentSize();
  void resizeTerminal(props.id, cols, rows).catch(() => undefined);
}

async function ensureSession() {
  if (!term) return;
  try {
    await nextTick();
    await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
    if (host.value && host.value.clientWidth >= 40) {
      fit?.fit();
    }
    const { cols, rows } = currentSize();
    await startTerminal(props.id, props.cwd, cols, rows);
    applySize();
    if (props.active) term.focus();
    emit("error", "");
  } catch (error) {
    emit("error", error instanceof Error ? error.message : String(error));
  }
}

onMounted(async () => {
  window.addEventListener("resize", applySize);
  term = new Terminal({
    cols: 80,
    rows: 24,
    fontFamily: 'Consolas, "Courier New", "Microsoft YaHei Mono", monospace',
    fontSize: 13,
    cursorBlink: true,
    scrollback: 4000,
    windowsPty: { backend: "conpty", buildNumber: 22621 },
    theme: {
      background: "#1e1f22",
      foreground: "#dfe1e5",
      cursor: "#dfe1e5",
      selectionBackground: "#2e436e",
    },
  });
  fit = new FitAddon();
  term.loadAddon(fit);
  if (host.value) {
    term.open(host.value);
  }
  term.onData((data) => {
    void writeTerminal(props.id, data).catch((error) => {
      emit("error", error instanceof Error ? error.message : String(error));
    });
  });

  unlistenData = await listen<{ id: string; data: string }>("term-data", (event) => {
    if (event.payload.id === props.id) {
      term?.write(event.payload.data);
    }
  });
  unlistenExit = await listen<string>("term-exit", (event) => {
    if (event.payload === props.id) {
      term?.writeln("\r\n[进程已退出]");
    }
  });

  await nextTick();
  applySize();
  await ensureSession();
});

onUnmounted(() => {
  window.removeEventListener("resize", applySize);
  unlistenData?.();
  unlistenExit?.();
  term?.dispose();
  term = null;
  void closeTerminal(props.id).catch(() => undefined);
});

watch(
  () => props.active,
  async (active) => {
    if (!active) return;
    await nextTick();
    applySize();
    term?.focus();
  },
);

defineExpose({ applySize, focus: () => term?.focus() });
</script>

<template>
  <div ref="host" class="host" />
</template>

<style scoped>
.host {
  flex: 1;
  min-height: 0;
  min-width: 0;
  padding: 4px 8px 8px;
}

.host :deep(.xterm) {
  height: 100%;
}

.host :deep(.xterm-viewport) {
  overflow-y: auto;
}
</style>
