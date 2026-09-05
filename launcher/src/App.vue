<script setup lang="ts">
import { invoke } from "@tauri-apps/api/core";
import { ref } from "vue";
import HudBar from "./components/HudBar.vue";

const status = ref("待机");
const busy = ref(false);

async function onLaunch(): Promise<void> {
  busy.value = true;
  try {
    status.value = await invoke<string>("launch_game");
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
  } catch (err) {
    status.value = `失败: ${String(err)}`;
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <main class="shell">
    <h1>CapsuleRetro</h1>
    <p class="status">{{ status }}</p>
    <HudBar />
    <div class="actions">
      <button class="action" :disabled="busy" @click="onLaunch">启动 XEEN</button>
      <button class="action" :disabled="busy" @click="onStop">停止</button>
    </div>
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
h1 {
  font-size: 28px;
  letter-spacing: 2px;
}
.status {
  opacity: 0.8;
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
</style>
