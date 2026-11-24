import {
  ref,
  toValue,
  watch,
  type MaybeRefOrGetter,
  type Ref,
} from "vue";

export interface EditorTab {
  path: string;
  name: string;
  content: string;
  savedContent: string;
  dirty: boolean;
}

export interface OpenEditorTab {
  path: string;
  name?: string;
  content: string;
}

export interface CloseEditorTabResult {
  closed: boolean;
  needsConfirmation: boolean;
  tab?: EditorTab;
}

export interface EditorTabsState {
  tabs: Ref<EditorTab[]>;
  activePath: Ref<string>;
  openTab(tab: OpenEditorTab): EditorTab | null;
  selectTab(path: string): boolean;
  updateContent(path: string, content: string): boolean;
  markSaved(path: string, content?: string): boolean;
  closeTab(path: string, discardDirty?: boolean): CloseEditorTabResult;
  restore(): void;
  clear(): void;
}

interface StoredEditorTabs {
  version: 1;
  activePath: string;
  tabs: EditorTab[];
}

const STORAGE_PREFIX = "rebased.editorTabs.";
const DEFAULT_MAX_TABS = 20;

export function useEditorTabs(
  repoPath: MaybeRefOrGetter<string>,
  options: { maxTabs?: number } = {},
): EditorTabsState {
  const tabs = ref<EditorTab[]>([]);
  const activePath = ref("");
  const maxTabs = Math.max(1, Math.trunc(options.maxTabs ?? DEFAULT_MAX_TABS));
  let currentRepo = toValue(repoPath);
  let restoring = false;

  function storageKey(repo = currentRepo) {
    return `${STORAGE_PREFIX}${encodeURIComponent(normalizeRepo(repo))}`;
  }

  function persist(repo = currentRepo) {
    if (!repo || restoring) return;
    const state: StoredEditorTabs = {
      version: 1,
      activePath: activePath.value,
      tabs: tabs.value.slice(0, maxTabs),
    };
    try {
      localStorage.setItem(storageKey(repo), JSON.stringify(state));
    } catch {
      // Storage can be disabled or full; editor state remains usable in memory.
    }
  }

  function restore() {
    restoring = true;
    try {
      const raw = currentRepo ? localStorage.getItem(storageKey()) : null;
      const stored = raw ? (JSON.parse(raw) as Partial<StoredEditorTabs>) : null;
      const restored = Array.isArray(stored?.tabs)
        ? stored.tabs
            .filter(isStoredTab)
            .slice(0, maxTabs)
            .map((tab) => ({ ...tab, dirty: tab.content !== tab.savedContent }))
        : [];
      tabs.value = restored;
      activePath.value =
        restored.some((tab) => tab.path === stored?.activePath)
          ? stored?.activePath ?? ""
          : restored[0]?.path ?? "";
    } catch {
      tabs.value = [];
      activePath.value = "";
    } finally {
      restoring = false;
    }
  }

  function openTab(input: OpenEditorTab) {
    const existing = tabs.value.find((tab) => tab.path === input.path);
    if (existing) {
      if (input.name) existing.name = input.name;
      activePath.value = existing.path;
      return existing;
    }
    if (tabs.value.length >= maxTabs) return null;
    const tab: EditorTab = {
      path: input.path,
      name: input.name ?? fileName(input.path),
      content: input.content,
      savedContent: input.content,
      dirty: false,
    };
    tabs.value.push(tab);
    activePath.value = tab.path;
    return tab;
  }

  function selectTab(path: string) {
    if (!tabs.value.some((tab) => tab.path === path)) return false;
    activePath.value = path;
    return true;
  }

  function updateContent(path: string, content: string) {
    const tab = tabs.value.find((item) => item.path === path);
    if (!tab) return false;
    tab.content = content;
    tab.dirty = content !== tab.savedContent;
    return true;
  }

  function markSaved(path: string, content?: string) {
    const tab = tabs.value.find((item) => item.path === path);
    if (!tab) return false;
    if (content !== undefined) tab.content = content;
    tab.savedContent = tab.content;
    tab.dirty = false;
    return true;
  }

  function closeTab(path: string, discardDirty = false): CloseEditorTabResult {
    const index = tabs.value.findIndex((tab) => tab.path === path);
    if (index < 0) return { closed: false, needsConfirmation: false };
    const tab = tabs.value[index];
    if (tab.dirty && !discardDirty) {
      return { closed: false, needsConfirmation: true, tab };
    }
    tabs.value.splice(index, 1);
    if (activePath.value === path) {
      activePath.value =
        tabs.value[Math.min(index, tabs.value.length - 1)]?.path ?? "";
    }
    return { closed: true, needsConfirmation: false, tab };
  }

  function clear() {
    tabs.value = [];
    activePath.value = "";
  }

  watch(
    () => toValue(repoPath),
    (repo, previous) => {
      if (previous) persist(previous);
      currentRepo = repo;
      restore();
    },
  );

  watch([tabs, activePath], () => persist(), { deep: true });

  restore();

  return {
    tabs,
    activePath,
    openTab,
    selectTab,
    updateContent,
    markSaved,
    closeTab,
    restore,
    clear,
  };
}

function normalizeRepo(path: string) {
  return path.replace(/\\/g, "/").replace(/\/+$/, "").toLowerCase();
}

function fileName(path: string) {
  const parts = path.replace(/\\/g, "/").split("/").filter(Boolean);
  return parts[parts.length - 1] ?? path;
}

function isStoredTab(value: unknown): value is EditorTab {
  if (!value || typeof value !== "object") return false;
  const tab = value as Partial<EditorTab>;
  return (
    typeof tab.path === "string" &&
    typeof tab.name === "string" &&
    typeof tab.content === "string" &&
    typeof tab.savedContent === "string"
  );
}
