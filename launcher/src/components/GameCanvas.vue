<script setup lang="ts">
import { invoke } from "@tauri-apps/api/core";
import { onUnmounted, ref, watch } from "vue";

// 帧流二进制协议（PL005 流式直推）：28 字节头（帧号 u64 + 宽/高/行距 u32×3 +
// 捕获时刻 u64，全部 LE）+ BGRX 像素；fork 的 chunked 长连接每帧即推，
// 网络读取与 rAF 绘制解耦——读线程只挑最新完整帧，绘制按合成器节奏走
const FRAME_HEADER_BYTES = 28;
const FRAME_BASE_URL = "http://127.0.0.1:18086"; // 端口锁死（webserver_port 定案值）
const MOUSE_BATCH_MS = 30; // 鼠标相对位移合帧间隔（Pointer Lock 事件高频）
const MAX_STREAM_FAILURES = 5; // 帧流连续失败上限（白名单口径：超限显式置错停循环）
const STATS_INTERVAL_MS = 15_000; // 性能采样上报周期
const SAMPLE_CAPACITY = 256; // 每类采样的环形容量

const props = defineProps<{ active: boolean }>();

const canvasRef = ref<HTMLCanvasElement | null>(null);
const pointerLocked = ref(false);
const frameError = ref<string | null>(null);

// PL005 测量：环形采样（注入 RTT / 帧龄 / 解码绘制耗时），定期落 launcher.log
const injectRtt: number[] = [];
const frameAges: number[] = [];
const parseDrawMs: number[] = [];

function pushSample(buf: number[], value: number): void {
  buf.push(value);
  if (buf.length > SAMPLE_CAPACITY) buf.shift();
}

/** 分布百分位（最近样本取整即可满足测量粒度）。 */
function percentile(buf: number[], p: number): number {
  if (buf.length === 0) return 0;
  const sorted = [...buf].sort((a, b) => a - b);
  const idx = Math.min(sorted.length - 1, Math.floor((p / 100) * sorted.length));
  return sorted[idx];
}

async function dumpStats(): Promise<void> {
  const kinds: Array<[string, number[]]> = [
    ["inject_rtt", injectRtt],
    ["frame_age", frameAges],
    ["parse_draw", parseDrawMs],
  ];
  for (const [kind, buf] of kinds) {
    if (buf.length === 0) continue;
    try {
      await invoke("log_perf_stats", {
        stats: {
          kind,
          count: buf.length,
          p50: percentile(buf, 50),
          p95: percentile(buf, 95),
          max: Math.max(...buf),
        },
      });
    } catch (err) {
      console.warn("性能采样上报失败:", err);
    }
  }
}
const statsTimer = window.setInterval(() => void dumpStats(), STATS_INTERVAL_MS);

let running = false;
let streamController: AbortController | null = null;
let rafHandle = 0;
let lastFrameNumber = -1;
let streamFailures = 0;
let latestFrame: ArrayBuffer | null = null;

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

/** 绘制最新帧（BGRX → RGBA + 帧龄/解码绘制耗时采样）。帧号未推进则跳过。 */
function drawLatest(): void {
  if (!canvasRef.value || !offCtx || !latestFrame) return;
  const startedAt = performance.now();
  const buffer = latestFrame;
  latestFrame = null;

  const view = new DataView(buffer);
  const number = Number(view.getBigUint64(0, true));
  const width = view.getUint32(8, true);
  const height = view.getUint32(12, true);
  const pitch = view.getUint32(16, true);
  const captureMs = Number(view.getBigUint64(20, true));
  // 帧协议校验：静默错值会表现为花屏且无诊断，宁可报错
  if (
    pitch < width * 4 ||
    buffer.byteLength < FRAME_HEADER_BYTES + (height - 1) * pitch + width * 4
  ) {
    throw new Error(`帧数据异常: ${buffer.byteLength}B（${width}x${height} pitch ${pitch}）`);
  }
  if (number === lastFrameNumber || width === 0 || height === 0) return;
  lastFrameNumber = number;

  // 尺寸未变时复用画布与 ImageData，消除每帧重分配
  if (offscreen.width !== width || offscreen.height !== height) {
    offscreen.width = width;
    offscreen.height = height;
    imageCache = offCtx.createImageData(width, height);
  }
  const image = imageCache;
  if (!image) return;
  const src = new Uint8Array(buffer, FRAME_HEADER_BYTES);
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

  pushSample(parseDrawMs, performance.now() - startedAt);
  // 帧龄 = 捕获时刻 → 绘制时刻（同机时钟，epoch 毫秒直减）
  const age = Date.now() - captureMs;
  if (age >= 0 && age < 1000) {
    pushSample(frameAges, age);
  }
}

/** rAF 绘制循环：按合成器节奏消费最新帧（与网络读取解耦）。 */
function rafTick(): void {
  if (!running) return;
  try {
    drawLatest();
  } catch (err) {
    frameError.value = `帧处理失败: ${String(err)}`;
    console.warn("帧处理失败:", err);
  }
  rafHandle = requestAnimationFrame(rafTick);
}

/** 流式消费循环：chunked 长连接每帧即推；中断按白名单口径重试，超限置错停循环。 */
async function streamLoop(): Promise<void> {
  while (running) {
    streamController = new AbortController();
    try {
      const resp = await fetch(`${FRAME_BASE_URL}/api/v2/frame/stream`, {
        signal: streamController.signal,
      });
      if (!resp.ok || !resp.body) {
        throw new Error(`流式端点响应异常: HTTP ${resp.status}`);
      }
      const reader = resp.body.getReader();
      let acc = new Uint8Array(0);
      streamFailures = 0;
      frameError.value = null;
      while (running) {
        const { done, value } = await reader.read();
        if (done) break;
        if (!value || value.length === 0) continue;
        const merged = new Uint8Array(acc.length + value.length);
        merged.set(acc);
        merged.set(value, acc.length);
        acc = merged;
        // 切分全部完整帧，仅保留最新一帧供 rAF 绘制（陈旧帧直接丢弃）
        while (acc.length >= FRAME_HEADER_BYTES) {
          const view = new DataView(acc.buffer, acc.byteOffset, acc.byteLength);
          const width = view.getUint32(8, true);
          const height = view.getUint32(12, true);
          const pitch = view.getUint32(16, true);
          if (pitch < width * 4) {
            throw new Error("流式帧头异常（行距校验失败，流错位）");
          }
          const total = FRAME_HEADER_BYTES + (height - 1) * pitch + width * 4;
          if (acc.length < total) break;
          latestFrame = acc.slice(0, total).buffer;
          acc = acc.slice(total);
        }
      }
      // 流结束：running 仍真说明是服务端异常关闭（正常结束由 active=false 中止）
      if (running) {
        throw new Error("帧流意外中断");
      }
    } catch (err) {
      if (!running) break; // 主动中止（会话结束 / 组件卸载）：正常终止
      streamFailures++;
      const message = String(err);
      frameError.value = `帧流中断（连续第 ${streamFailures} 次）: ${message}`;
      console.warn(`帧流中断（连续第 ${streamFailures} 次）:`, message);
      if (streamFailures >= MAX_STREAM_FAILURES) {
        frameError.value = `帧流连续 ${streamFailures} 次失败，已停止消费（末次: ${message}）`;
        running = false;
        break;
      }
      await new Promise((r) => setTimeout(r, 1000));
    }
  }
}

/** 转发按键事件到注入端点（附带注入 RTT 采样）。 */
async function forwardKey(event: KeyboardEvent, down: boolean): Promise<void> {
  const key = toKeyName(event.code);
  if (!key) return;
  event.preventDefault();
  const sentAt = performance.now();
  try {
    await invoke("inject_input", {
      payload: JSON.stringify({ kind: "key", key, down }),
    });
    pushSample(injectRtt, performance.now() - sentAt);
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
  const sentAt = performance.now();
  void invoke("inject_input", {
    payload: JSON.stringify({ kind: "click", button, down }),
  })
    .then(() => {
      pushSample(injectRtt, performance.now() - sentAt);
    })
    .catch((err) => console.warn("点击注入失败:", err));
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
      streamFailures = 0;
      frameError.value = null;
      lastFrameNumber = -1;
      latestFrame = null;
      void streamLoop();
      rafHandle = requestAnimationFrame(rafTick);
    } else {
      streamController?.abort();
      streamController = null;
      cancelAnimationFrame(rafHandle);
    }
  },
  { immediate: true },
);

onUnmounted(() => {
  running = false;
  streamController?.abort();
  window.clearInterval(statsTimer);
  cancelAnimationFrame(rafHandle);
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
