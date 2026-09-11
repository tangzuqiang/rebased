<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch } from "vue";
import { getAppSetting, setAppSetting } from "../api";
import Icons from "./Icons.vue";
import PopupMenu from "./PopupMenu.vue";

const props = defineProps<{
  authors: string[];
  me: string;
}>();

const emit = defineEmits<{
  change: [users: string[]];
}>();

const RECENT_KEY = "log.recentUsers";

const open = ref(false);
const draft = ref("");
const selected = ref<string[]>([]);
const recent = ref<string[]>([]);
const anchor = ref<HTMLElement | null>(null);
const input = ref<HTMLInputElement | null>(null);

const keyword = computed(() => draft.value.trim().toLowerCase());

const authorList = computed(() => {
  const seen = new Set<string>();
  const list: string[] = [];
  for (const name of props.authors) {
    const value = name.trim();
    if (!value || seen.has(value.toLowerCase())) continue;
    seen.add(value.toLowerCase());
    list.push(value);
  }
  return list.sort((a, b) => a.localeCompare(b, "zh-CN"));
});

const visibleAuthors = computed(() => {
  if (!keyword.value) return authorList.value;
  return authorList.value.filter((name) => name.toLowerCase().includes(keyword.value));
});

const visibleRecent = computed(() => {
  const names = recent.value.filter((name) => authorList.value.includes(name) || selected.value.includes(name));
  if (!keyword.value) return names;
  return names.filter((name) => name.toLowerCase().includes(keyword.value));
});

const displayText = computed(() => {
  if (!selected.value.length) return "";
  return selected.value.join(", ");
});

function has(name: string) {
  return selected.value.some((item) => item.toLowerCase() === name.trim().toLowerCase());
}

function apply(next: string[]) {
  const unique: string[] = [];
  const seen = new Set<string>();
  for (const name of next) {
    const value = name.trim();
    if (!value) continue;
    const key = value.toLowerCase();
    if (seen.has(key)) continue;
    seen.add(key);
    unique.push(value);
  }
  selected.value = unique;
  emit("change", unique);
  remember(unique);
}

function toggle(name: string) {
  if (!name.trim()) return;
  apply(has(name) ? selected.value.filter((item) => item.toLowerCase() !== name.toLowerCase()) : [...selected.value, name]);
}

function clear() {
  apply([]);
  draft.value = "";
}

function selectMe() {
  if (!props.me.trim()) return;
  toggle(props.me.trim());
}

function selectRecent() {
  if (!recent.value.length) return;
  apply([...recent.value]);
}

function addDraft() {
  const value = draft.value.trim();
  if (!value) return;
  apply([...selected.value, value]);
  draft.value = "";
}

function onKeydown(event: KeyboardEvent) {
  if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) {
    event.preventDefault();
    addDraft();
  }
  if (event.key === "Escape") {
    open.value = false;
  }
}

function remember(names: string[]) {
  if (!names.length) return;
  const next = [...names, ...recent.value.filter((item) => !names.some((name) => name.toLowerCase() === item.toLowerCase()))];
  recent.value = next.slice(0, 12);
  void setAppSetting(RECENT_KEY, JSON.stringify(recent.value)).catch(() => undefined);
}

async function toggleOpen() {
  open.value = !open.value;
  if (open.value) {
    await nextTick();
    input.value?.focus();
  }
}

watch(open, async (value) => {
  if (!value) return;
  await nextTick();
  input.value?.focus();
});

onMounted(async () => {
  const raw = await getAppSetting(RECENT_KEY).catch(() => null);
  if (!raw) return;
  try {
    const parsed = JSON.parse(raw) as string[];
    if (Array.isArray(parsed)) {
      recent.value = parsed.filter((item) => typeof item === "string" && item.trim()).slice(0, 12);
    }
  } catch {
    recent.value = [];
  }
});
</script>

<template>
  <div class="user-filter">
    <button ref="anchor" type="button" class="chip" :class="{ open, filled: selected.length }" @click="toggleOpen">
      <span class="label">用户</span>
      <span v-if="displayText" class="value" :title="displayText">{{ displayText }}</span>
      <Icons name="chevron" :size="10" />
    </button>
    <PopupMenu :open="open" :anchor="anchor" @close="open = false">
      <div class="panel" @keydown="onKeydown">
        <div class="input-row">
          <input
            ref="input"
            v-model="draft"
            placeholder="选择一个或多个用户或输入新的值，并使用 Ctrl+Enter 完成"
          />
        </div>
        <button type="button" :class="{ current: !selected.length }" @click="clear">选择...</button>
        <button type="button" :disabled="!me.trim()" :class="{ current: me.trim() && has(me) }" @click="selectMe">我</button>
        <button type="button" :disabled="!recent.length" :class="{ current: recent.length > 0 && recent.every((name) => has(name)) }" @click="selectRecent">
          最近
        </button>
        <template v-if="visibleRecent.length">
          <div class="divider" />
          <div class="item dim">最近使用</div>
          <button
            v-for="name in visibleRecent"
            :key="`r-${name}`"
            type="button"
            :class="{ current: has(name) }"
            @click="toggle(name)"
          >
            {{ name }}
          </button>
        </template>
        <div class="divider" />
        <button
          v-for="name in visibleAuthors"
          :key="name"
          type="button"
          :class="{ current: has(name) }"
          @click="toggle(name)"
        >
          {{ name }}
        </button>
        <div v-if="!visibleAuthors.length" class="item dim">没有匹配的用户</div>
      </div>
    </PopupMenu>
  </div>
</template>

<style scoped>
.user-filter {
  min-width: 140px;
  flex: 1;
}

.chip {
  width: 100%;
  justify-content: flex-start;
}

.chip.open,
.chip.filled {
  color: var(--text);
}

.label {
  color: var(--text-muted);
}

.value {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--text);
  text-align: left;
}

.panel {
  min-width: 320px;
}

.input-row {
  padding: 6px 8px 4px;
}

.input-row input {
  width: 100%;
  height: 26px;
  border: 1px solid var(--border-soft);
  background: var(--bg-input);
  border-radius: 4px;
  padding: 0 8px;
  outline: none;
}

.input-row input:focus {
  border-color: var(--accent);
}

.input-row input::placeholder {
  color: var(--text-dim);
  font-size: 11px;
}
</style>
