<script setup lang="ts">
import { nextTick, onMounted, onUnmounted, ref, watch } from "vue";
import { createIdeEditor, type IdeEditor } from "../editor/ideEditor";

const props = withDefaults(
  defineProps<{
    path: string;
    content: string;
    readOnly?: boolean;
    line?: number;
    column?: number;
  }>(),
  {
    readOnly: false,
    line: 1,
    column: 1,
  },
);

const emit = defineEmits<{
  change: [content: string];
  save: [content: string];
}>();

const host = ref<HTMLElement | null>(null);
let editor: IdeEditor | null = null;

function revealPosition() {
  if (!editor || props.line < 1) return;
  editor.goTo(props.line, props.column);
}

onMounted(() => {
  if (!host.value) return;
  editor = createIdeEditor({
    parent: host.value,
    doc: props.content,
    path: props.path,
    readOnly: props.readOnly,
    onChange: (content) => emit("change", content),
    onSave: (content) => emit("save", content),
  });
  void nextTick(revealPosition);
});

onUnmounted(() => {
  editor?.destroy();
  editor = null;
});

watch(
  () => props.content,
  (content) => editor?.setDoc(content),
);

watch(
  () => props.path,
  (path) => editor?.setPath(path),
);

watch(
  () => props.readOnly,
  (readOnly) => editor?.setReadOnly(readOnly),
);

watch(
  () => [props.line, props.column] as const,
  revealPosition,
);

defineExpose({
  focus: () => editor?.focus(),
  goTo: (line: number, column = 1) => editor?.goTo(line, column),
  getContent: () => editor?.getDoc() ?? props.content,
});
</script>

<template>
  <div ref="host" class="ide-editor-pane" />
</template>

<style scoped>
.ide-editor-pane {
  width: 100%;
  height: 100%;
  min-width: 0;
  min-height: 0;
  overflow: hidden;
  background: var(--bg-deep);
}

.ide-editor-pane :deep(.cm-editor) {
  height: 100%;
  background: var(--bg-deep);
  color: var(--text-editor);
  font-size: 12px;
}

.ide-editor-pane :deep(.cm-editor.cm-focused) {
  outline: none;
}

.ide-editor-pane :deep(.cm-scroller) {
  font-family: "JetBrains Mono", Consolas, "Courier New", monospace;
  line-height: 18px;
}

.ide-editor-pane :deep(.cm-content) {
  caret-color: var(--text);
}

.ide-editor-pane :deep(.cm-cursor) {
  border-left-color: var(--text);
}

.ide-editor-pane :deep(.cm-gutters) {
  border: 0;
  border-right: 1px solid var(--border-soft);
  background: var(--bg-deep);
  color: var(--text-dim);
}

.ide-editor-pane :deep(.cm-lineNumbers .cm-gutterElement) {
  min-width: 32px;
  padding: 0 8px 0 4px;
}

.ide-editor-pane :deep(.cm-activeLine),
.ide-editor-pane :deep(.cm-activeLineGutter) {
  background: var(--bg-hover);
}

.ide-editor-pane :deep(.cm-selectionBackground),
.ide-editor-pane :deep(.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground) {
  background: rgba(53, 116, 240, 0.35);
}

.ide-editor-pane :deep(.cm-panels) {
  border-color: var(--border);
  background: var(--bg-hover);
  color: var(--text);
}

.ide-editor-pane :deep(.cm-textfield),
.ide-editor-pane :deep(.cm-button) {
  border: 1px solid var(--border-soft);
  border-radius: 4px;
  background: var(--bg-input);
  background-image: none;
  color: var(--text);
}

.ide-editor-pane :deep(.cm-searchMatch) {
  background: rgba(255, 200, 50, 0.3);
}

.ide-editor-pane :deep(.cm-searchMatch-selected) {
  background: rgba(255, 200, 50, 0.55);
}

.ide-editor-pane :deep(.tok-keyword),
.ide-editor-pane :deep(.tok-bool),
.ide-editor-pane :deep(.tok-atom) { color: var(--tok-keyword); }
.ide-editor-pane :deep(.tok-string),
.ide-editor-pane :deep(.tok-string2),
.ide-editor-pane :deep(.tok-inserted) { color: var(--tok-string); }
.ide-editor-pane :deep(.tok-number),
.ide-editor-pane :deep(.tok-literal) { color: var(--tok-number); }
.ide-editor-pane :deep(.tok-comment) { color: var(--tok-comment); font-style: italic; }
.ide-editor-pane :deep(.tok-typeName),
.ide-editor-pane :deep(.tok-className),
.ide-editor-pane :deep(.tok-namespace) { color: var(--tok-type); }
.ide-editor-pane :deep(.tok-propertyName) { color: var(--tok-property); }
.ide-editor-pane :deep(.tok-meta),
.ide-editor-pane :deep(.tok-macroName),
.ide-editor-pane :deep(.tok-annotation) { color: var(--tok-meta); }
.ide-editor-pane :deep(.tok-variableName.tok-definition),
.ide-editor-pane :deep(.tok-propertyName.tok-definition) { color: var(--tok-func); }
.ide-editor-pane :deep(.tok-invalid),
.ide-editor-pane :deep(.tok-deleted) { color: var(--danger); }
</style>
