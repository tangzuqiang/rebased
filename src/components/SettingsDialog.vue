<script setup lang="ts">
import { onMounted, ref } from "vue";
import { appVersion, checkUpdate, getAiConfig, installUpdate, saveAiConfig, testAiConfig } from "../api";
import { currentTheme, saveTheme, type ThemeName } from "../theme";
import type { AiConfig, UpdateInfo } from "../types";
import Modal from "./Modal.vue";

const emit = defineEmits<{
  close: [];
}>();

type Section = "appearance" | "ai" | "update";

const section = ref<Section>("appearance");
const theme = ref<ThemeName>(currentTheme());
const config = ref<AiConfig>({
  baseUrl: "",
  apiKey: "",
  model: "",
});
const saving = ref(false);
const testing = ref(false);
const hint = ref("");
const hintKind = ref<"ok" | "err">("ok");
const version = ref("");
const updateInfo = ref<UpdateInfo | null>(null);
const checking = ref(false);
const installing = ref(false);
const updateHint = ref("");
const updateHintKind = ref<"ok" | "err">("ok");

onMounted(async () => {
  theme.value = currentTheme();
  config.value = await getAiConfig().catch(() => config.value);
  version.value = await appVersion().catch(() => "");
});

async function chooseTheme(next: ThemeName) {
  theme.value = next;
  await saveTheme(next).catch(() => undefined);
}

async function lookupUpdate() {
  checking.value = true;
  updateHint.value = "";
  try {
    updateInfo.value = await checkUpdate();
    updateHintKind.value = "ok";
    updateHint.value = updateInfo.value.available
      ? `发现新版本 ${updateInfo.value.latest}`
      : `已是最新版本 ${updateInfo.value.current}`;
  } catch (error) {
    updateHintKind.value = "err";
    updateHint.value = error instanceof Error ? error.message : String(error);
  } finally {
    checking.value = false;
  }
}

async function applyUpdate() {
  installing.value = true;
  updateHint.value = "正在下载安装包，下载完成后将打开安装向导…";
  updateHintKind.value = "ok";
  try {
    updateHint.value = await installUpdate();
  } catch (error) {
    updateHintKind.value = "err";
    updateHint.value = error instanceof Error ? error.message : String(error);
  } finally {
    installing.value = false;
  }
}

async function save() {
  saving.value = true;
  hint.value = "";
  try {
    config.value = await saveAiConfig({
      baseUrl: config.value.baseUrl.trim(),
      apiKey: config.value.apiKey.trim(),
      model: config.value.model.trim(),
    });
    hintKind.value = "ok";
    hint.value = "已保存。提交信息会调用该模型，并根据勾选文件的 diff 生成";
  } catch (error) {
    hintKind.value = "err";
    hint.value = error instanceof Error ? error.message : String(error);
  } finally {
    saving.value = false;
  }
}

async function test() {
  testing.value = true;
  hint.value = "";
  try {
    await save();
    if (hintKind.value === "err") return;
    hint.value = await testAiConfig();
    hintKind.value = "ok";
  } catch (error) {
    hintKind.value = "err";
    hint.value = error instanceof Error ? error.message : String(error);
  } finally {
    testing.value = false;
  }
}
</script>

<template>
  <Modal title="设置" @close="emit('close')">
    <div class="settings">
      <aside>
        <div class="nav-title">设置列表</div>
        <button type="button" :class="{ active: section === 'appearance' }" @click="section = 'appearance'">
          外观
        </button>
        <button type="button" :class="{ active: section === 'ai' }" @click="section = 'ai'">
          AI 模型配置
        </button>
        <button type="button" :class="{ active: section === 'update' }" @click="section = 'update'">
          关于与更新
        </button>
      </aside>
      <section v-if="section === 'appearance'" class="panel">
        <h3>主题</h3>
        <p class="muted">立即生效，并会记住本次选择。</p>
        <div class="theme-grid">
          <button type="button" class="theme-card" :class="{ active: theme === 'dark' }" @click="chooseTheme('dark')">
            <span class="swatch dark" />
            <span>黑色</span>
          </button>
          <button type="button" class="theme-card" :class="{ active: theme === 'light' }" @click="chooseTheme('light')">
            <span class="swatch light" />
            <span>白色</span>
          </button>
        </div>
      </section>
      <section v-else-if="section === 'ai'" class="panel">
        <h3>AI 模型配置</h3>
        <p class="muted">用于提交弹窗中的「AI 生成提交信息」。接口需兼容 OpenAI Chat Completions。</p>
        <label>
          <span>接口地址</span>
          <input v-model="config.baseUrl" placeholder="例如 http://127.0.0.1:11434/v1" />
        </label>
        <label>
          <span>API Key</span>
          <input v-model="config.apiKey" type="password" placeholder="可选，本地模型通常不需要" />
        </label>
        <label>
          <span>模型名称</span>
          <input v-model="config.model" placeholder="例如 qwen3:1.7b、gemma-4-31B-it" />
        </label>
        <p class="muted">Ollama 地址填 http://127.0.0.1:11434 或 …/v1 都可以。模型名必须和 `ollama list` 完全一致，不能简写（qw3 会 404）。本地模型 API Key 通常留空。</p>
        <p v-if="hint" class="hint" :class="hintKind">{{ hint }}</p>
      </section>
      <section v-else-if="section === 'update'" class="panel">
        <h3>关于与更新</h3>
        <p class="muted">当前版本 {{ version || "未知" }}。更新包下载完成后会直接打开安装向导。</p>
        <p v-if="updateInfo?.notes" class="muted">{{ updateInfo.notes }}</p>
        <p v-if="updateHint" class="hint" :class="updateHintKind">{{ updateHint }}</p>
        <div class="actions">
          <button type="button" class="secondary-btn" :disabled="checking || installing" @click="lookupUpdate">
            {{ checking ? "正在检查…" : "检查更新" }}
          </button>
          <button
            type="button"
            class="primary-btn"
            :class="{ 'is-loading': installing }"
            :disabled="installing || !updateInfo?.available"
            @click="applyUpdate"
          >
            <span v-if="installing" class="btn-spinner" />
            {{ installing ? "正在安装…" : "立即更新" }}
          </button>
        </div>
      </section>
    </div>
    <template #footer>
      <button type="button" class="secondary-btn" @click="emit('close')">关闭</button>
      <template v-if="section === 'ai'">
        <button type="button" class="secondary-btn" :disabled="saving || testing" @click="test">测试连接</button>
        <button type="button" class="primary-btn" :disabled="saving" @click="save">保存</button>
      </template>
    </template>
  </Modal>
</template>

<style scoped>
.settings {
  flex: 1;
  min-height: 360px;
  display: grid;
  grid-template-columns: 200px 1fr;
}

aside {
  border-right: 1px solid var(--border);
  padding: 8px;
}

.nav-title {
  padding: 6px 8px 10px;
  color: var(--text-dim);
}

aside button {
  width: 100%;
  text-align: left;
  padding: 7px 10px;
  border-radius: 4px;
}

aside button.active,
aside button:hover {
  background: var(--bg-selected);
}

.panel {
  padding: 8px 16px 0;
  display: flex;
  flex-direction: column;
  gap: 12px;
}

h3 {
  margin: 0;
  font-size: 14px;
}

label {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

input {
  height: 30px;
  border: 1px solid var(--border-soft);
  background: var(--bg-input);
  border-radius: 4px;
  padding: 0 8px;
  outline: none;
}

input:focus {
  border-color: var(--accent);
}

.theme-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 12px;
}

.theme-card {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
  padding: 12px 8px;
  border: 1px solid var(--border-soft);
  border-radius: 8px;
  background: var(--bg-input);
}

.theme-card.active {
  border-color: var(--accent);
  box-shadow: inset 0 0 0 1px var(--accent);
}

.swatch {
  width: 100%;
  height: 56px;
  border-radius: 6px;
  border: 1px solid var(--border-soft);
}

.swatch.dark {
  background: linear-gradient(180deg, #2b2d30 0%, #1e1f22 100%);
}

.swatch.light {
  background: linear-gradient(180deg, #ffffff 0%, #f2f3f5 100%);
}

.hint {
  margin: 0;
  color: var(--ok);
}

.hint.err {
  color: var(--danger);
}

.actions {
  display: flex;
  gap: 8px;
}
</style>
