<script setup lang="ts">
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { onMounted, onUnmounted, ref } from "vue";

/** HUD 快照（与后端 HudSnapshot 字段一一对应）。 */
interface HudSnapshot {
  hp: number[];
  /** 槽位占用（解雇/换人瞬间的空槽为 false）。 */
  occupied: boolean[];
  /** 各队员名册槽位（面容文件号 = 值 + 1——PL009.2） */
  face: number[];
  /** 各队员姓名原始字节（Big5，含控制码与 NUL 填充——PL009.2） */
  name: number[][];
  gold: number;
  gems: number;
  food: number;
  food_days: number;
}

/** 头像资产条目（与后端 FaceImage 对应）。 */
interface FaceImage {
  id: number;
  data_url: string;
}

const snapshot = ref<HudSnapshot | null>(null);
const stalled = ref(false);
/** 面容文件号 → PNG data URL（会话启动时经 get_faces 一次性取图）。 */
const faces = ref<Record<number, string>>({});
/** 各槽位当前角色绑定的名册槽位（面容 id，用于换人检测）。 */
const faceIds = ref<number[]>([-1, -1, -1, -1, -1, -1]);
/** 各槽位占用状态（解雇/换人瞬间的空槽不渲染头像与 HP）。 */
const occupied = ref<boolean[]>([false, false, false, false, false, false]);
/** 着色基线：各槽位会话内观测到的最大 HP（滚动抬升，换人重置）。 */
const baseline = ref<number[]>([1, 1, 1, 1, 1, 1]);
const unlisteners: Array<() => void> = [];
// 卸载竞态守卫：listen 是异步注册，晚 resolve 时直接释放，防止 unlisten 泄漏
let disposed = false;

void listen<HudSnapshot>("hud", (event) => {
  const snap = event.payload;
  occupied.value = [...snap.occupied];
  snap.face.forEach((slot, k) => {
    if (!snap.occupied[k] || faceIds.value[k] !== slot) {
      // 空槽或换人上場：基线重置为当前 HP（避免沿用前任的满血基线）
      faceIds.value[k] = slot;
      baseline.value[k] = Math.max(snap.hp[k], 1);
    } else if (snap.hp[k] > baseline.value[k]) {
      // 升级/治疗抬高生命上限：基线滚动抬升
      baseline.value[k] = snap.hp[k];
    }
  });
  snapshot.value = snap;
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
  faceIds.value = [-1, -1, -1, -1, -1, -1];
  occupied.value = [false, false, false, false, false, false];
  baseline.value = [1, 1, 1, 1, 1, 1];
  stalled.value = false;
}).then((unlisten) => {
  if (disposed) unlisten();
  else unlisteners.push(unlisten);
});

onMounted(async () => {
  try {
    const list = await invoke<FaceImage[]>("get_faces");
    const map: Record<number, string> = {};
    for (const f of list) map[f.id] = f.data_url;
    faces.value = map;
  } catch (err) {
    console.warn("头像资产加载失败（HUD 回落槽位号占位）:", err);
  }
});

onUnmounted(() => {
  disposed = true;
  for (const unlisten of unlisteners) unlisten();
});

/** 槽位对应头像 data URL（面容文件号 = 名册槽位 + 1；空槽或图缺失返回 undefined）。 */
function faceSrc(index: number): string | undefined {
  if (!occupied.value[index]) return undefined;
  return faces.value[faceIds.value[index] + 1];
}

/**
 * 三元联动判断（PL009.2 热更定案）：头像识别成功 = 槽位占用且面容图在册。
 * 头像 / HP / 姓名三处的显示与"—"占位统一由它驱动，识别不到则三者一起画"—"。
 */
function identified(index: number): boolean {
  return occupied.value[index] && faceSrc(index) != null;
}

/** Big5 解码器（WebView2 原生支持；不支持的环境姓名回落空串）。 */
let big5: TextDecoder | null = null;
try {
  big5 = new TextDecoder("big5");
} catch {
  big5 = null;
}

/** 解码姓名原始字节：剥 `%E` 控制码头部、去 NUL 填充、Big5 转文本（仅在 identified 时被模板调用）。 */
function nameOf(index: number): string {
  if (!snapshot.value) return "";
  let bytes = snapshot.value.name[index] ?? [];
  if (bytes.length >= 2 && bytes[0] === 0x25 && bytes[1] === 0x45) bytes = bytes.slice(2);
  const end = bytes.indexOf(0);
  if (end !== -1) bytes = bytes.slice(0, end);
  if (!bytes.length) return "";
  const text = (big5 ?? new TextDecoder("utf-8")).decode(Uint8Array.from(bytes)).trim();
  return text;
}

/**
 * HP 着色（PL009.2 定案：按当前 HP / 会话内基线 的比例取色）。
 * ≥60% 绿、20%–60% 黄、<20% 红、负值蓝；基线 ≤0 时交由回落色。
 */
function hpColor(hp: number, base: number): string {
  if (hp < 0) return "var(--hp-neg, #61afef)";
  const safe = Math.max(base, 1);
  const ratio = hp / safe;
  if (ratio >= 0.6) return "var(--hp-ok, #55d07c)";
  if (ratio >= 0.2) return "var(--hp-warn, #e5c07b)";
  return "var(--hp-danger, #ff7b72)";
}
</script>

<template>
  <section class="hud" :class="{ stalled }">
    <template v-if="snapshot">
      <span
        v-for="(hp, index) in snapshot.hp"
        :key="index"
        class="cell member"
        :class="{ 'slot-empty': !identified(index) }"
      >
        <span class="top">
          <img
            v-if="identified(index)"
            :src="faceSrc(index)"
            alt=""
            class="face"
            draggable="false"
          />
          <span v-else class="face face-fallback">—</span>
          <b v-if="identified(index)" class="hp" :style="{ color: hpColor(hp, baseline[index]) }">{{
            hp
          }}</b>
          <b v-else class="hp hp-empty">—</b>
        </span>
        <span class="name">{{ identified(index) ? nameOf(index) : "—" }}</span>
      </span>
      <span class="cell"
        >金币 <b>{{ snapshot.gold }}</b></span
      >
      <span class="cell"
        >宝石 <b>{{ snapshot.gems }}</b></span
      >
      <span class="cell"
        >食物 <b>{{ snapshot.food }}</b
        >（{{ snapshot.food_days }} 天）</span
      >
    </template>
    <span v-else class="cell muted">HUD 等待游戏会话…</span>
  </section>
</template>

<style scoped>
/* v3 黑金典藏版色板（token 继承自 App.vue .shell，回落值保独立可用） */
.hud {
  display: flex;
  gap: 16px;
  flex-wrap: wrap;
  align-items: center;
  font-size: 12px;
  color: var(--text-dim, #8f897a);
}
.cell {
  padding: 3px 10px;
  border: 1px solid var(--panel-line, #26241c);
  border-radius: 2px;
  background: rgba(7, 7, 8, 0.55);
}
.cell b {
  color: var(--gold, #d4af5a);
  font-weight: 600;
}
.member {
  display: inline-flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 2px;
}
.top {
  display: inline-flex;
  align-items: center;
  gap: 6px;
}
.name {
  max-width: 72px;
  min-height: 12px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 10px;
  line-height: 12px;
  color: var(--text-dim, #8f897a);
}
.slot-empty {
  opacity: 0.35;
}
.hp-empty {
  color: var(--text-dim, #8f897a);
}
.face {
  width: 24px;
  height: 28px;
  object-fit: cover;
  image-rendering: pixelated;
  border: 1px solid var(--gold-dim, #6a5a32);
  border-radius: 2px;
  background: rgba(7, 7, 8, 0.8);
}
.face-fallback {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  color: var(--text-dim, #8f897a);
  font-size: 11px;
}
.hp {
  font-size: 13px;
}
.muted {
  opacity: 0.5;
}
.hud.stalled .cell {
  opacity: 0.4;
}
</style>
