<script setup lang="ts">
import { listen } from "@tauri-apps/api/event";
import { onUnmounted, ref } from "vue";

/** HUD 快照（与后端 HudSnapshot 字段一一对应）。 */
interface HudSnapshot {
  hp: number[];
  gold: number;
  gems: number;
  food: number;
  food_days: number;
}

const snapshot = ref<HudSnapshot | null>(null);
const stalled = ref(false);
const unlisteners: Array<() => void> = [];
// 卸载竞态守卫：listen 是异步注册，晚 resolve 时直接释放，防止 unlisten 泄漏
let disposed = false;

void listen<HudSnapshot>("hud", (event) => {
  snapshot.value = event.payload;
  stalled.value = false;
}).then((unlisten) => {
  if (disposed) unlisten();
  else unlisteners.push(unlisten);
});

void listen<string>("hud-error", (event) => {
  stalled.value = true;
  console.warn("HUD 停滞:", event.payload);
}).then((unlisten) => {
  if (disposed) unlisten();
  else unlisteners.push(unlisten);
});

void listen("hud-clear", () => {
  snapshot.value = null;
  stalled.value = false;
}).then((unlisten) => {
  if (disposed) unlisten();
  else unlisteners.push(unlisten);
});

onUnmounted(() => {
  disposed = true;
  for (const unlisten of unlisteners) unlisten();
});
</script>

<template>
  <section class="hud" :class="{ stalled }">
    <template v-if="snapshot">
      <span
        v-for="(hp, index) in snapshot.hp"
        :key="index"
        class="cell"
        :class="{ danger: hp <= 0 }"
      >
        HP{{ index + 1 }} {{ hp }}
      </span>
      <span class="cell">金币 {{ snapshot.gold }}</span>
      <span class="cell">宝石 {{ snapshot.gems }}</span>
      <span class="cell">食物 {{ snapshot.food }}（{{ snapshot.food_days }} 天）</span>
    </template>
    <span v-else class="cell muted">HUD 等待游戏会话…</span>
  </section>
</template>

<style scoped>
.hud {
  display: flex;
  gap: 10px;
  flex-wrap: wrap;
  font-size: 13px;
}
.cell {
  padding: 4px 10px;
  border: 1px solid #3a3f4b;
  border-radius: 6px;
  background: #1e222b;
}
.danger {
  color: #ff7b72;
}
.muted {
  opacity: 0.5;
}
.hud.stalled .cell {
  opacity: 0.4;
}
</style>
