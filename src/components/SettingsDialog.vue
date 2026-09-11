<script setup lang="ts">
import { onMounted, ref } from "vue";
import { getAiConfig, saveAiConfig, testAiConfig } from "../api";
import type { AiConfig } from "../types";
import Modal from "./Modal.vue";

const emit = defineEmits<{
  close: [];
}>();

type Section = "ai";

const section = ref<Section>("ai");
const config = ref<AiConfig>({
  baseUrl: "",
  apiKey: "",
  model: "",
});
const saving = ref(false);
const testing = ref(false);
const hint = ref("");
const hintKind = ref<"ok" | "err">("ok");

onMounted(async () => {
  config.value = await getAiConfig().catch(() => config.value);
});

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
        <button type="button" :class="{ active: section === 'ai' }" @click="section = 'ai'">
          AI 模型配置
        </button>
      </aside>
      <section v-if="section === 'ai'" class="panel">
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
          <input v-model="config.model" placeholder="例如 qwen-plus、gpt-4o-mini" />
        </label>
        <p class="muted">接口地址填到 /v1 即可。API Key 填密钥，不要填网址；服务不需要密钥则留空。</p>
        <p v-if="hint" class="hint" :class="hintKind">{{ hint }}</p>
      </section>
    </div>
    <template #footer>
      <button type="button" class="secondary-btn" @click="emit('close')">关闭</button>
      <button type="button" class="secondary-btn" :disabled="saving || testing" @click="test">测试连接</button>
      <button type="button" class="primary-btn" :disabled="saving" @click="save">保存</button>
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

.hint {
  margin: 0;
  color: #98c379;
}

.hint.err {
  color: var(--danger);
}
</style>
