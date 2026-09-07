<script setup lang="ts">
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ref } from "vue";
import GameCanvas from "./components/GameCanvas.vue";
import HudBar from "./components/HudBar.vue";

const status = ref("待机");
const busy = ref(false);
const running = ref(false);

// 后端退出监听线程在 DOSBox 自退/崩溃时广播此事件：UI 回待机态，不停留在假会话
void listen<number>("session-ended", (event) => {
  if (!running.value) return;
  running.value = false;
  status.value = `游戏进程已退出（pid ${event.payload}）`;
});

async function onLaunch(): Promise<void> {
  busy.value = true;
  try {
    status.value = await invoke<string>("launch_game");
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
      <p class="status">{{ status }}</p>
      <HudBar />
      <div class="actions">
        <button class="action" :disabled="busy" @click="onLaunch">启动 XEEN</button>
        <button class="action" :disabled="busy" @click="onStop">停止</button>
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
.status {
  opacity: 0.7;
  font-size: 13px;
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
.stop {
  padding: 8px 18px;
  font-size: 13px;
}
</style>
