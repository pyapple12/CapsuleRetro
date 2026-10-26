<script setup lang="ts">
import DOMPurify from "dompurify";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getVersion } from "@tauri-apps/api/app";
import { marked } from "marked";
import { ref } from "vue";
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
  /** 封面 data URL（null = 未配置封面，回落占位）。 */
  cover: string | null;
  save: { version: string; hack: string | null; files: number } | null;
  patches: PatchView[];
}

/** launch 成功信息（后端 LaunchInfo）：seq 用于比对退出事件归属。 */
interface LaunchInfo {
  port: number;
  seq: number;
}

/** session-ended 事件载荷（后端 SessionEnded）。 */
interface SessionEndedEvent {
  pid: number;
  seq: number;
  saved: boolean;
}

const status = ref("待机");
const busy = ref(false);
const running = ref(false);
// 当前会话序号：session-ended 事件据此比对归属（FIX002.23）
let currentSeq = 0;
// 待机页数据：游戏名 / 封面 data URL / 补丁列表 / 当前选择 / save 状态
const gameName = ref("");
const coverUrl = ref<string | null>(null);
const patches = ref<PatchView[]>([]);
const selectedHack = ref<string | null>(null);
const saveHackLabel = ref("—");
const saveDetail = ref("");
// 产品版本（tauri.conf 注入；非 Tauri 环境下显示占位）
const appVersion = ref("…");
// 攻略视图开关与净化后的 HTML（marked 解析 + DOMPurify 消毒，防 XSS）
const showGuide = ref(false);
const guideHtml = ref("");

// 顶栏版本 chip：读 tauri.conf 注入的产品版本（getVersion API；失败留痕后
// 显示占位——容错白名单 FIX005.18，版本号为增强显示信息）
void getVersion()
  .then((v) => {
    appVersion.value = v;
  })
  .catch((err) => {
    console.warn("版本号读取失败:", err);
    appVersion.value = "—";
  });

// 后端退出监听线程在 DOSBox 自退/崩溃时广播此事件：UI 回待机态，不停留在假会话
void listen<SessionEndedEvent>("session-ended", (event) => {
  // 陈旧事件（旧会话的退出广播晚于新会话启动到达）忽略，防误回待机（FIX002.23）
  if (!running.value || event.payload.seq !== currentSeq) return;
  running.value = false;
  // 回写结果如实呈现，不再无条件宣称"save 已回写"（FIX002.30）
  status.value = event.payload.saved
    ? `游戏进程已退出（pid ${event.payload.pid}），存档已保存`
    : `游戏进程已退出（pid ${event.payload.pid}），存档回写失败，详见日志`;
  void loadLobby(); // 退出后刷新待机页 save 状态（FIX002.33）
});

/** 加载待机页数据（游戏信息 + 攻略 Markdown），失败显式呈现不静默。 */
async function loadLobby(): Promise<void> {
  try {
    const info = await invoke<GameInfo>("get_game_info");
    gameName.value = info.name;
    coverUrl.value = info.cover;
    // 窗口标题跟随游戏（产品名 · 游戏名）
    document.title = `CapsuleRetro · ${info.name}`;
    patches.value = info.patches;
    if (info.save == null) {
      saveHackLabel.value = "—";
      saveDetail.value = "尚无存档（首次运行将创建）";
    } else {
      saveHackLabel.value = info.save.hack == null ? "原版" : `补丁 ${info.save.hack}`;
      saveDetail.value = `格式 v${info.save.version} · ${info.save.files} 个数据文件`;
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
    status.value = await invoke<LaunchInfo>("launch_game", { hack: selectedHack.value }).then(
      (info) => {
        currentSeq = info.seq;
        return `游戏已启动（webserver 端口 ${info.port}）`;
      },
    );
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

// 金手指在途标记（FIX005.19）：读-改-写两次 HTTP 非原子，同字段并发请求
// 会同读后各写、丢失更新——在途期间同字段点击直接忽略
const cheatPending = new Set<string>();

/** 会话内数值修改（金手指）：写成功后经状态行反馈新值，HUD 轮询自动跟显。 */
async function onCheat(field: string, delta: number): Promise<void> {
  if (cheatPending.has(field)) return;
  cheatPending.add(field);
  try {
    const res = await invoke<{ field: string; value: number }>("cheat_adjust", {
      field,
      delta,
    });
    status.value = `${res.field} 已修改，现值 ${res.value}`;
  } catch (err) {
    status.value = `失败: ${String(err)}`;
  } finally {
    cheatPending.delete(field);
  }
}
</script>

<template>
  <main class="shell" :class="{ running }">
    <template v-if="running">
      <div class="s-top">
        <span class="brand2">CAPSULERETRO</span>
        <HudBar />
        <span class="spacer"></span>
        <button class="btn ghost small" :disabled="busy" @click="onCheat('gold', 10000)">
          金币 +10000
        </button>
        <button class="btn ghost small" :disabled="busy" @click="onCheat('gems', 10000)">
          宝石 +10000
        </button>
        <button class="btn ghost small" :disabled="busy" @click="onStop">停止游戏</button>
      </div>
      <GameCanvas :active="running" />
      <p class="status">{{ status }}</p>
    </template>
    <template v-else>
      <div class="topbar">
        <span class="brand">CAPSULERETRO <small>典藏系列</small></span>
        <span class="rule"></span>
        <span class="chip">V{{ appVersion }}</span>
      </div>
      <div class="stage">
        <div class="cart">
          <div class="cart-inner">
            <img v-if="coverUrl" :src="coverUrl" alt="封面" />
            <div v-else class="cart-placeholder"><span>NO COVER</span></div>
            <div class="plate">
              COLLECTOR'S CAPSULE
              <small>No.001 — {{ gameName || "…" }}</small>
            </div>
          </div>
        </div>
        <div class="info">
          <p class="kicker">CAPSULERETRO CLASSICS</p>
          <h1>
            {{ gameName || "…" }}
            <small>DOS 原版精确运行 · 关闭窗口自动保存</small>
          </h1>
          <div class="meta">
            <span>DOS</span>
            <span>{{ patches.length > 0 ? "原版 / 补丁双形态" : "原版形态" }}</span>
          </div>
          <p class="save">
            存档：<b>{{ saveHackLabel }}</b> · {{ saveDetail }}
          </p>
          <div class="row">
            <select v-model="selectedHack" class="hack-select" :disabled="busy">
              <option :value="null">原版</option>
              <option v-for="p in patches" :key="p.id" :value="p.id">{{ p.name }}</option>
            </select>
            <button class="btn" :disabled="busy" @click="onLaunch">启 动</button>
            <button class="btn ghost" :disabled="busy" @click="onStop">停止</button>
            <button class="btn ghost" :disabled="guideHtml === ''" @click="showGuide = true">
              攻略
            </button>
          </div>
          <p class="hint">启动后点击画面接管键鼠 · Esc 释放光标 · 关闭窗口自动保存</p>
          <p class="status">{{ status }}</p>
        </div>
      </div>
      <div v-if="showGuide" class="guide-overlay">
        <div class="guide-panel">
          <div class="guide-toolbar">
            <button class="btn ghost" @click="showGuide = false">关闭攻略</button>
          </div>
          <!-- 内容经 DOMPurify 消毒 + CSP 兜底，无脚本执行面 -->
          <article class="guide-body" v-html="guideHtml"></article>
        </div>
      </div>
    </template>
  </main>
</template>

<style scoped>
/* v3 黑金典藏版 token（设计定案 assets/design/v3-黑金典藏版.html；
   自定义属性沿 DOM 继承进子组件，HudBar/GameCanvas 同源取色） */
.shell {
  --bg: #070708;
  --panel: #101013;
  --panel-line: #26241c;
  --gold: #d4af5a;
  --gold-hi: #f0d894;
  --gold-dim: #8a743e;
  --text: #ece7db;
  --text-dim: #8f897a;

  display: flex;
  flex-direction: column;
  align-items: center;
  min-height: 100vh;
  color: var(--text);
  font-family: "Segoe UI", "Microsoft YaHei", sans-serif;
  background:
    radial-gradient(900px 420px at 50% -80px, rgba(212, 175, 90, 0.1), transparent 65%),
    linear-gradient(180deg, #0d0d10, var(--bg) 60%);
}
.shell:not(.running) {
  justify-content: center;
}
/* ===================== 待机页（v3 稿移植） ===================== */
/* 宽度与 .stage 同源（min(92vw, 1116px)）：flex 子项的 auto margin 会压过
   stretch 收缩包裹，显式同宽才能保证品牌条与卡带/信息列同列对齐 */
.topbar {
  display: flex;
  align-items: center;
  gap: 14px;
  align-self: center;
  width: min(92vw, 1116px);
  padding: 20px 0 18px;
  box-sizing: border-box;
}
.brand {
  font-size: 15px;
  letter-spacing: 5px;
  color: var(--gold);
  font-weight: 600;
}
.brand small {
  color: var(--text-dim);
  letter-spacing: 2px;
  font-weight: 400;
}
.rule {
  flex: 1;
  height: 1px;
  background: linear-gradient(90deg, var(--gold-dim), transparent);
}
.chip {
  font-size: 11px;
  color: var(--text-dim);
  border: 1px solid var(--panel-line);
  padding: 4px 12px;
  border-radius: 2px;
}
.stage {
  display: flex;
  gap: 34px;
  width: min(92vw, 1116px);
  margin-top: 8px;
}
.cart {
  flex: 0 0 250px;
  align-self: flex-start;
  border-radius: 4px;
  padding: 4px;
  background: linear-gradient(145deg, #3a3220, #191510);
  border: 1px solid var(--gold-dim);
  box-shadow:
    0 22px 60px rgba(0, 0, 0, 0.7),
    0 0 0 1px #000,
    inset 0 0 18px rgba(0, 0, 0, 0.6);
}
.cart-inner {
  border: 1px solid rgba(212, 175, 90, 0.4);
  padding: 10px 10px 12px;
  background: #0c0b09;
}
.cart img {
  width: 100%;
  display: block;
  filter: saturate(1.05) contrast(1.03);
}
.cart-placeholder {
  display: flex;
  align-items: center;
  justify-content: center;
  aspect-ratio: 2 / 3;
  border: 1px dashed var(--panel-line);
  background: repeating-linear-gradient(45deg, #0e0d0b 0 12px, #12100c 12px 24px);
}
.cart-placeholder span {
  font-size: 11px;
  letter-spacing: 4px;
  color: var(--gold-dim);
}
.plate {
  margin-top: 10px;
  text-align: center;
  font-size: 11px;
  letter-spacing: 4px;
  color: var(--gold);
  font-family: Georgia, serif;
}
.plate small {
  display: block;
  color: var(--text-dim);
  letter-spacing: 2px;
  margin-top: 4px;
  font-size: 9px;
}
.info {
  flex: 1;
  min-width: 0;
}
.kicker {
  font-size: 11px;
  letter-spacing: 6px;
  color: var(--gold-dim);
  margin: 4px 0 10px;
}
h1 {
  margin: 0;
  font-size: 42px;
  font-family: Georgia, "SimSun", serif;
  font-weight: 600;
  letter-spacing: 2px;
  line-height: 1.15;
  background: linear-gradient(180deg, var(--gold-hi), var(--gold) 55%, #93743a);
  -webkit-background-clip: text;
  background-clip: text;
  color: transparent;
}
h1 small {
  display: block;
  font-size: 14px;
  letter-spacing: 3px;
  font-family: "Segoe UI", "Microsoft YaHei", sans-serif;
  color: var(--text-dim);
  -webkit-text-fill-color: var(--text-dim);
  margin-top: 10px;
  font-weight: 400;
}
.meta {
  display: flex;
  gap: 10px;
  margin: 18px 0 4px;
}
.meta span {
  font-size: 11px;
  color: var(--gold);
  letter-spacing: 1px;
  padding: 4px 12px;
  border: 1px solid rgba(212, 175, 90, 0.35);
  border-radius: 2px;
  background: rgba(212, 175, 90, 0.05);
}
.save {
  margin: 16px 0 8px;
  font-size: 13px;
  color: var(--text-dim);
}
.save b {
  color: var(--gold);
}
.row {
  display: flex;
  align-items: center;
  gap: 12px;
  margin: 20px 0 16px;
}
.hack-select {
  padding: 10px 14px;
  font-size: 14px;
  color: var(--text);
  background: var(--panel);
  border: 1px solid var(--panel-line);
  border-radius: 2px;
  outline: none;
}
.btn {
  padding: 13px 46px;
  font-size: 16px;
  letter-spacing: 6px;
  white-space: nowrap;
  cursor: pointer;
  font-family: inherit;
  color: var(--gold-hi);
  background: linear-gradient(180deg, #1a1710, #0e0d0a);
  border: 1px solid var(--gold-dim);
  border-radius: 2px;
  box-shadow:
    inset 0 1px 0 rgba(240, 216, 148, 0.25),
    0 10px 30px rgba(0, 0, 0, 0.5);
}
.btn:hover:not(:disabled) {
  border-color: var(--gold);
  box-shadow:
    inset 0 1px 0 rgba(240, 216, 148, 0.35),
    0 0 24px rgba(212, 175, 90, 0.15);
}
.btn:disabled {
  opacity: 0.45;
  cursor: default;
}
.btn.ghost {
  color: var(--text-dim);
  font-size: 13px;
  letter-spacing: 2px;
  padding: 13px 24px;
}
.btn.small {
  padding: 5px 14px;
  font-size: 12px;
}
.hint {
  font-size: 12px;
  color: var(--text-dim);
  letter-spacing: 1px;
  margin: 0;
}
.status {
  font-size: 12px;
  color: var(--text-dim);
  opacity: 0.8;
  letter-spacing: 1px;
  margin: 6px 0 0;
}
/* ===================== 会话态顶栏（v3 稿 s-top） ===================== */
.shell.running {
  justify-content: flex-start;
  gap: 0;
  padding: 0 0 16px;
}
.s-top {
  display: flex;
  align-items: center;
  gap: 18px;
  align-self: stretch;
  max-width: 1400px;
  margin: 0 auto;
  padding: 10px 16px;
  border-bottom: 1px solid var(--panel-line);
  background: var(--panel);
}
.brand2 {
  font-size: 12px;
  letter-spacing: 4px;
  color: var(--gold);
}
.spacer {
  flex: 1;
}
/* ===================== 攻略 overlay（v3 色板） ===================== */
.guide-overlay {
  position: fixed;
  inset: 0;
  display: flex;
  justify-content: center;
  background: rgba(0, 0, 0, 0.72);
  z-index: 10;
}
.guide-panel {
  display: flex;
  flex-direction: column;
  width: min(92vw, 860px);
  margin: 24px 0;
  padding: 16px;
  background: var(--panel);
  border: 1px solid var(--gold-dim);
  border-radius: 2px;
  box-shadow: 0 24px 70px rgba(0, 0, 0, 0.8);
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
  color: var(--gold-hi);
}
.guide-body :deep(h2) {
  font-size: 18px;
  color: var(--gold);
  border-bottom: 1px solid var(--panel-line);
  padding-bottom: 4px;
}
.guide-body :deep(pre) {
  padding: 10px;
  background: #0c0b09;
  border: 1px solid var(--panel-line);
  border-radius: 2px;
  overflow-x: auto;
}
.guide-body :deep(table) {
  border-collapse: collapse;
}
.guide-body :deep(td),
.guide-body :deep(th) {
  padding: 4px 10px;
  border: 1px solid var(--panel-line);
}
</style>
