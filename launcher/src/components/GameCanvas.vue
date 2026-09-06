<script setup lang="ts">
import { invoke } from "@tauri-apps/api/core";
import { onUnmounted, ref, watch } from "vue";

// 帧流二进制协议：20 字节头（帧号 u64 + 宽/高/行距 u32，全部 LE）+ BGRX 像素
const HEADER_BYTES = 20;
const FRAME_INTERVAL_MS = 33; // 30fps 消费节奏（端点实测单帧 2.3ms）

const props = defineProps<{ active: boolean }>();

const canvasRef = ref<HTMLCanvasElement | null>(null);
const pointerLocked = ref(false);
const frameFps = ref(0);

let running = false;
let loopThread: Promise<void> | null = null;
let lastFrameNumber = -1;
let fpsCounter = 0;
let fpsWindowStart = 0;
// 鼠标相对位移累积（Pointer Lock 事件高频，30ms 合帧发送）
let pendingDx = 0;
let pendingDy = 0;
let lastMouseSent = 0;

const offscreen = document.createElement("canvas");
const offCtx = offscreen.getContext("2d");

/** event.code → 注入端点键名（覆盖 dosbox 常用键）。 */
function toKeyName(code: string): string | null {
  if (code.startsWith("Key")) return code.slice(3).toLowerCase();
  if (code.startsWith("Digit")) return code.slice(5);
  if (code.startsWith("Numpad")) return `numpad${code.slice(6)}`;
  const map: Record<string, string> = {
    ArrowUp: "up",
    ArrowDown: "down",
    ArrowLeft: "left",
    ArrowRight: "right",
    Enter: "return",
    NumpadEnter: "return",
    Escape: "escape",
    Space: "space",
    Tab: "tab",
    Backspace: "backspace",
    AltLeft: "leftalt",
    AltRight: "rightalt",
    ControlLeft: "leftctrl",
    ControlRight: "rightctrl",
    ShiftLeft: "leftshift",
    ShiftRight: "rightshift",
  };
  for (let f = 1; f <= 12; f++) {
    map[`F${f}`] = `f${f}`;
  }
  return map[code] ?? null;
}

/** 解码一帧并绘制到显示画布（BGRX → RGBA）。 */
function drawFrame(buffer: ArrayBuffer): void {
  if (!canvasRef.value || !offCtx) return;
  const view = new DataView(buffer);
  const number = view.getBigUint64(0, true);
  const width = view.getUint32(8, true);
  const height = view.getUint32(12, true);
  const pitch = view.getUint32(16, true);
  if (number === BigInt(lastFrameNumber) || width === 0 || height === 0) return;
  lastFrameNumber = Number(number);

  offscreen.width = width;
  offscreen.height = height;
  const image = offCtx.createImageData(width, height);
  const src = new Uint8Array(buffer, HEADER_BYTES);
  const dst = image.data;
  for (let y = 0; y < height; y++) {
    let s = y * pitch;
    let d = y * width * 4;
    for (let x = 0; x < width; x++) {
      // BGRX 内存序 → RGBA
      dst[d] = src[s + 2];
      dst[d + 1] = src[s + 1];
      dst[d + 2] = src[s];
      dst[d + 3] = 255;
      s += 4;
      d += 4;
    }
  }
  offCtx.putImageData(image, 0, 0);

  const ctx = canvasRef.value.getContext("2d");
  if (ctx) {
    ctx.imageSmoothingEnabled = false; // 像素风整数缩放
    ctx.drawImage(offscreen, 0, 0, canvasRef.value.width, canvasRef.value.height);
  }
}

/** 30fps 消费循环：取帧失败按瞬时抖动跳过本轮。 */
async function loop(): Promise<void> {
  while (running) {
    const cycleStart = performance.now();
    try {
      const buffer = (await invoke("get_frame")) as ArrayBuffer;
      drawFrame(buffer);
      fpsCounter++;
      const now = performance.now();
      if (now - fpsWindowStart >= 1000) {
        frameFps.value = Math.round((fpsCounter * 1000) / (now - fpsWindowStart));
        fpsCounter = 0;
        fpsWindowStart = now;
      }
    } catch {
      // 会话启动早期 503/无帧：静默跳过本轮
    }
    const spent = performance.now() - cycleStart;
    const wait = FRAME_INTERVAL_MS - spent;
    if (wait > 0) await new Promise((r) => setTimeout(r, wait));
  }
}

/** 转发按键事件到注入端点。 */
async function forwardKey(event: KeyboardEvent, down: boolean): Promise<void> {
  const key = toKeyName(event.code);
  if (!key) return;
  event.preventDefault();
  try {
    await invoke("inject_input", {
      payload: JSON.stringify({ kind: "key", key, down }),
    });
  } catch (err) {
    console.warn("按键注入失败:", err);
  }
}

function onMouseMove(event: MouseEvent): void {
  if (document.pointerLockElement !== canvasRef.value) return;
  pendingDx += event.movementX;
  pendingDy += event.movementY;
  const now = performance.now();
  if (now - lastMouseSent >= 30 && (pendingDx !== 0 || pendingDy !== 0)) {
    lastMouseSent = now;
    void invoke("inject_input", {
      payload: JSON.stringify({
        kind: "mouse",
        dx: Math.round(pendingDx),
        dy: Math.round(pendingDy),
      }),
    }).catch((err) => console.warn("鼠标注入失败:", err));
    pendingDx = 0;
    pendingDy = 0;
  }
}

function onMouseButton(event: MouseEvent, down: boolean): void {
  if (document.pointerLockElement !== canvasRef.value) return;
  const button = event.button === 2 ? "right" : event.button === 1 ? "middle" : "left";
  void invoke("inject_input", {
    payload: JSON.stringify({ kind: "click", button, down }),
  }).catch((err) => console.warn("点击注入失败:", err));
  event.preventDefault();
}

function onPointerLockChange(): void {
  pointerLocked.value = document.pointerLockElement === canvasRef.value;
}

function requestLock(): void {
  void canvasRef.value?.requestPointerLock();
}

// Pointer Lock 的鼠标移动在 document 上派发，必须 document 级监听
document.addEventListener("mousemove", onMouseMove);
document.addEventListener("pointerlockchange", onPointerLockChange);

watch(
  () => props.active,
  (active) => {
    running = active;
    if (active) {
      fpsWindowStart = performance.now();
      loopThread = loop();
    }
  },
  { immediate: true },
);

onUnmounted(() => {
  running = false;
  if (loopThread) void loopThread;
  document.removeEventListener("mousemove", onMouseMove);
  document.removeEventListener("pointerlockchange", onPointerLockChange);
});
</script>

<template>
  <div class="stage-wrap">
    <canvas
      ref="canvasRef"
      class="game-canvas"
      width="960"
      height="720"
      tabindex="0"
      :class="{ locked: pointerLocked }"
      @keydown="forwardKey($event, true)"
      @keyup="forwardKey($event, false)"
      @click="requestLock"
      @mousedown="onMouseButton($event, true)"
      @mouseup="onMouseButton($event, false)"
      @contextmenu.prevent
    />
    <p class="hint" :class="{ hidden: pointerLocked }">点击画布接管键鼠（Esc 释放光标）</p>
  </div>
</template>

<style scoped>
.stage-wrap {
  position: relative;
  display: flex;
  justify-content: center;
  align-items: center;
}
.game-canvas {
  width: min(90vw, calc(75vh * 4 / 3));
  aspect-ratio: 4 / 3;
  background: #000;
  border: 1px solid #3a3f4b;
  border-radius: 6px;
  outline: none;
  cursor: crosshair;
}
.game-canvas.locked {
  cursor: none;
  border-color: #4a8;
}
.hint {
  position: absolute;
  bottom: 8px;
  font-size: 12px;
  opacity: 0.6;
}
.hint.hidden {
  display: none;
}
</style>
