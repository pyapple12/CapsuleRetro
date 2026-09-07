<script setup lang="ts">
import DOMPurify from "dompurify";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { marked } from "marked";
import { onMounted, ref } from "vue";
import GameCanvas from "./components/GameCanvas.vue";
import HudBar from "./components/HudBar.vue";

/** 补丁选项（后端 PatchView）。 */
interface PatchView {
  id: string;
  name: string;
}

/** 待机页聚合信息（后端 GameInfo）。 */
interface GameInfo {
  name: string;
  save: { version: string; hack: string | null; files: number } | null;
  patches: PatchView[];
}

const status = ref("待机");
const busy = ref(false);
const running = ref(false);
// 待机页数据：游戏名 / 补丁列表 / 当前选择 / save 状态
const gameName = ref("");
const patches = ref<PatchView[]>([]);
const selectedHack = ref<string | null>(null);
const saveStatus = ref("");
// 攻略视图开关与净化后的 HTML（marked 解析 + DOMPurify 消毒，防 XSS）
const showGuide = ref(false);
const guideHtml = ref("");

// 后端退出监听线程在 DOSBox 自退/崩溃时广播此事件：UI 回待机态，不停留在假会话
void listen<number>("session-ended", (event) => {
  if (!running.value) return;
  running.value = false;
  status.value = `游戏进程已退出（pid ${event.payload}），save 已回写`;
});

/** 加载待机页数据（游戏信息 + 攻略 Markdown），失败显式呈现不静默。 */
async function loadLobby(): Promise<void> {
  try {
    const info = await invoke<GameInfo>("get_game_info");
    gameName.value = info.name;
    patches.value = info.patches;
    if (info.save == null) {
      saveStatus.value = "尚无存档（首次运行将创建）";
    } else {
      const hack = info.save.hack == null ? "原版" : `补丁 ${info.save.hack}`;
      saveStatus.value = `存档：${hack} · 格式 v${info.save.version} · ${info.save.files} 个数据文件`;
      // 待选补丁默认跟随 save 里记录的上次选择
      selectedHack.value = info.save.hack;
    }
    const markdown = await invoke<string>("get_guide");
    guideHtml.value = DOMPurify.sanitize(await marked.parse(markdown, { async: true }));
  } catch (err) {
    status.value = `待机数据加载失败: ${String(err)}`;
  }
}

void loadLobby();

async function onLaunch(): Promise<void> {
  busy.value = true;
  try {
    status.value = await invoke<string>("launch_game", { hack: selectedHack.value });
    running.value = true;
  } catch (err) {
    status.value = `失败: ${String(err)}`;
  } finally {
    busy.value = false;
  }
}

async function onStop(): Promise<void> {
  busy.value = true;
  try {
    status.value = await invoke<string>("stop_game");
    running.value = false;
    void loadLobby(); // 回写后刷新 save 状态显示
  } catch (err) {
    status.value = `失败: ${String(err)}`;
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <main class="shell" :class="{ wide: running }">
    <template v-if="running">
      <div class="topline">
        <h1 class="brand">CapsuleRetro</h1>
        <HudBar />
        <button class="action stop" :disabled="busy" @click="onStop">停止游戏</button>
      </div>
      <GameCanvas :active="running" />
      <p class="status">{{ status }}</p>
    </template>
    <template v-else>
      <h1>CapsuleRetro</h1>
      <p class="subtitle">{{ gameName || "…" }}</p>
      <p class="status">{{ status }}</p>
      <HudBar />
      <label class="hack-row">
        版本
        <select v-model="selectedHack" class="hack-select" :disabled="busy">
          <option :value="null">原版</option>
          <option v-for="p in patches" :key="p.id" :value="p.id">{{ p.name }}</option>
        </select>
      </label>
      <p class="save-status">{{ saveStatus }}</p>
      <div class="actions">
        <button class="action" :disabled="busy" @click="onLaunch">
          启动 {{ gameName || "游戏" }}
        </button>
        <button class="action" :disabled="busy" @click="onStop">停止</button>
        <button class="action secondary" :disabled="guideHtml === ''" @click="showGuide = true">
          攻略
        </button>
      </div>
      <div v-if="showGuide" class="guide-overlay">
        <div class="guide-panel">
          <div class="guide-toolbar">
            <button class="action stop" @click="showGuide = false">关闭攻略</button>
          </div>
          <!-- 内容经 DOMPurify 消毒 + CSP 兜底，无脚本执行面 -->
          <article class="guide-body" v-html="guideHtml"></article>
        </div>
      </div>
    </template>
  </main>
</template>

<style scoped>
.shell {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  height: 100vh;
  background: #181a20;
  color: gainsboro;
  font-family: "Segoe UI", sans-serif;
  gap: 16px;
}
.shell.wide {
  justify-content: flex-start;
  padding-top: 12px;
}
.topline {
  display: flex;
  width: min(96vw, 1400px);
  align-items: center;
  justify-content: space-between;
  gap: 16px;
}
.brand {
  font-size: 18px;
  letter-spacing: 2px;
}
h1 {
  font-size: 28px;
  letter-spacing: 2px;
}
.subtitle {
  margin: 0;
  font-size: 16px;
  opacity: 0.85;
}
.status {
  opacity: 0.7;
  font-size: 13px;
}
.hack-row {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 14px;
}
.hack-select {
  padding: 6px 12px;
  font-size: 14px;
  border: 1px solid #3a3f4b;
  border-radius: 6px;
  background: #23262e;
  color: gainsboro;
}
.save-status {
  margin: 0;
  font-size: 12px;
  opacity: 0.6;
}
.actions {
  display: flex;
  gap: 12px;
}
.action {
  padding: 12px 36px;
  font-size: 16px;
  border: 1px solid #3a3f4b;
  border-radius: 8px;
  background: #23262e;
  color: gainsboro;
  cursor: pointer;
}
.action:hover:not(:disabled) {
  background: #2d313b;
}
.action:disabled {
  opacity: 0.5;
  cursor: default;
}
.action.secondary {
  padding: 12px 20px;
}
.stop {
  padding: 8px 18px;
  font-size: 13px;
}
.guide-overlay {
  position: fixed;
  inset: 0;
  display: flex;
  justify-content: center;
  background: rgba(0, 0, 0, 0.6);
}
.guide-panel {
  display: flex;
  flex-direction: column;
  width: min(92vw, 860px);
  margin: 24px 0;
  padding: 16px;
  background: #1e222b;
  border: 1px solid #3a3f4b;
  border-radius: 10px;
}
.guide-toolbar {
  display: flex;
  justify-content: flex-end;
  margin-bottom: 8px;
}
.guide-body {
  overflow-y: auto;
  font-size: 14px;
  line-height: 1.7;
}
.guide-body :deep(h1) {
  font-size: 22px;
}
.guide-body :deep(h2) {
  font-size: 18px;
  border-bottom: 1px solid #3a3f4b;
  padding-bottom: 4px;
}
.guide-body :deep(pre) {
  padding: 10px;
  background: #15171d;
  border-radius: 6px;
  overflow-x: auto;
}
.guide-body :deep(table) {
  border-collapse: collapse;
}
.guide-body :deep(td),
.guide-body :deep(th) {
  padding: 4px 10px;
  border: 1px solid #3a3f4b;
}
</style>
