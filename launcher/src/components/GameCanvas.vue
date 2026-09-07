<script setup lang="ts">
import { invoke } from "@tauri-apps/api/core";
import { onUnmounted, ref, watch } from "vue";

// 帧流二进制协议：20 字节头（帧号 u64 + 宽/高/行距 u32，全部 LE）+ BGRX 像素
const HEADER_BYTES = 20;
const FRAME_INTERVAL_MS = 33; // 30fps 消费节奏（端点实测单帧 2.3ms）
const MOUSE_BATCH_MS = 30; // 鼠标相对位移合帧间隔（Pointer Lock 事件高频）
// 帧轮询连续失败上限（容错白名单登记：单次失败跳过，超限显式置错并停循环）
const MAX_CONSECUTIVE_FAILURES = 5;

const props = defineProps<{ active: boolean }>();

const canvasRef = ref<HTMLCanvasElement | null>(null);
const pointerLocked = ref(false);
const frameError = ref<string | null>(null);

let running = false;
let loopThread: Promise<void> | null = null;
let lastFrameNumber = -1;
let consecutiveFailures = 0;
// 鼠标相对位移累积（Pointer Lock 事件高频，按 MOUSE_BATCH_MS 合帧发送）
let pendingDx = 0;
let pendingDy = 0;
let lastMouseSent = 0;
// 离屏画布按帧尺寸惰性重建；ImageData 与显示 ctx 均按尺寸缓存复用
const offscreen = document.createElement("canvas");
const offCtx = offscreen.getContext("2d");
let imageCache: ImageData | null = null;
let displayCtx: CanvasRenderingContext2D | null = null;

/** event.code → 注入端点键名（覆盖 dosbox 常用键）。 */
function toKeyName(code: string): string | null {
  if (code.startsWith("Key")) return code.slice(3).toLowerCase();
  if (code.startsWith("Digit")) return code.slice(5);
  // NumpadEnter 必须特判在前，否则被下方 Numpad 前缀分支吞掉成死条目
  if (code === "NumpadEnter") return "return";
  // fork 键名表无任何 numpad* 条目（input.cpp 全表核对）：小键盘数字退化为
  // 对应数字键名（numlock 打开时等价），其余小键盘键不受支持
  if (code.startsWith("Numpad")) {
    const digit = code.slice(6);
    return /^[0-9]$/.test(digit) ? digit : null;
  }
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

/** 解码一帧并绘制到显示画布（BGRX → RGBA）。长度与行距不符时抛错走失败路径。 */
function drawFrame(buffer: ArrayBuffer): void {
  if (!canvasRef.value || !offCtx) return;
  const view = new DataView(buffer);
  const number = view.getBigUint64(0, true);
  const width = view.getUint32(8, true);
  const height = view.getUint32(12, true);
  const pitch = view.getUint32(16, true);
  // 帧协议校验：静默错值会表现为花屏且无诊断，宁可报错
  if (pitch < width * 4 || buffer.byteLength < HEADER_BYTES + (height - 1) * pitch + width * 4) {
    throw new Error(`帧数据异常: ${buffer.byteLength}B（${width}x${height} pitch ${pitch}）`);
  }
  if (number === BigInt(lastFrameNumber) || width === 0 || height === 0) return;
  lastFrameNumber = Number(number);

  // 尺寸未变时复用画布与 ImageData，消除 30fps 下的每帧重分配
  if (offscreen.width !== width || offscreen.height !== height) {
    offscreen.width = width;
    offscreen.height = height;
    imageCache = offCtx.createImageData(width, height);
  }
  const image = imageCache;
  if (!image) return;
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

  if (!displayCtx) {
    displayCtx = canvasRef.value.getContext("2d");
  }
  if (displayCtx) {
    displayCtx.imageSmoothingEnabled = false; // 像素风整数缩放
    displayCtx.drawImage(offscreen, 0, 0, canvasRef.value.width, canvasRef.value.height);
  }
}

/** 30fps 消费循环：单次失败跳过本轮（瞬时抖动），连续超限显式置错停循环。 */
async function loop(): Promise<void> {
  while (running) {
    const cycleStart = performance.now();
    try {
      // 二进制 IPC 响应按 ArrayBuffer 消费；防御性兼容 Uint8Array 形态
      //（DataView 只认 ArrayBuffer，直接传视图会 TypeError）
      const raw: unknown = await invoke("get_frame");
      const bytes = raw instanceof ArrayBuffer ? raw : (raw as ArrayBuffer | ArrayLike<number>);
      const buffer = bytes instanceof ArrayBuffer ? bytes : new Uint8Array(bytes).buffer;
      drawFrame(buffer);
      consecutiveFailures = 0;
      frameError.value = null;
    } catch (err) {
      const message = String(err);
      if (message.includes("NO_SESSION")) {
        // 会话已由后端清槽（进程退出或被停止）：正常终止消费循环
        running = false;
        frameError.value = null;
        break;
      }
      consecutiveFailures++;
      // 立即显示最近一次失败原因（诊断窗口期：连续超限前就能看到根因）
      frameError.value = `取帧失败（连续第 ${consecutiveFailures} 次）: ${message}`;
      console.warn(`取帧失败（连续第 ${consecutiveFailures} 次）:`, message);
      if (consecutiveFailures >= MAX_CONSECUTIVE_FAILURES) {
        frameError.value = `帧流连续 ${consecutiveFailures} 次取帧失败，已停止消费（末次: ${message}）`;
        running = false;
        break;
      }
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
  if (now - lastMouseSent >= MOUSE_BATCH_MS && (pendingDx !== 0 || pendingDy !== 0)) {
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
      consecutiveFailures = 0;
      frameError.value = null;
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
    <p v-if="frameError" class="frame-error">{{ frameError }}</p>
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
.frame-error {
  position: absolute;
  top: 8px;
  padding: 4px 12px;
  font-size: 12px;
  color: #ff7b72;
  background: #1e222b;
  border: 1px solid #ff7b72;
  border-radius: 6px;
}
</style>
