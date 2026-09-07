#!/bin/bash
# DosboxCapsule 基线判定·采集（FB1，升级 rebase 后必跑，见同目录 PATCHES.md）。
# 用法：baseline-capture.sh <标签> <dosbox.exe 绝对路径>
#   标签建议：s1 = 官方发布版参照，s2 = rebase 后候选版
# 串行启动 XEEN（同 conf 同工作副本，标题屏不写档故安全），webserver 就绪并
# 稳定 12 秒后拍两份 1MB 全内存快照（间隔 2 秒），然后关闭进程。
# 快照为原始字节流，落 .temp/fb1-snap-<标签>{a,b}.bin，比对见 baseline-compare.py。
# URL/路径全部字面量。判定历史：v0.83.0 构建版曾因 5 秒稳定期不足被判"未稳定"，
# 12 秒为实测充足值，勿回调。
set -u
TAG="$1"
EXE="$2"
LOG="/e/GameMission/CapsuleRetro/.temp/fb1-${TAG}.log"
CONF='E:/GameMission/CapsuleRetro/.temp/xeen-test.conf'

# 启动前检查 conf 存在，避免 dosbox 无配置空跑污染日志与判定
if [ ! -f "$CONF" ]; then
  echo "[$TAG] CONF 不存在: $CONF"
  exit 1
fi

# 按 PID 终止本脚本启动的实例（kill 走 MSYS 进程句柄，等价 TerminateProcess）。
# 禁止 taskkill //IM 兜底——那会全机误杀并发运行的其他 dosbox 实例。
kill_dosbox() {
  kill "$PID" 2>/dev/null
  sleep 1
  if kill -0 "$PID" 2>/dev/null; then
    kill -9 "$PID" 2>/dev/null
    sleep 1
    if kill -0 "$PID" 2>/dev/null; then
      echo "[$TAG] 警告: dosbox (PID $PID) 按 PID 终止失败，需人工处理" >&2
      return 1
    fi
  fi
  return 0
}

"$EXE" --set webserver_enabled=on --set webserver_port=18086 -conf "$CONF" > "$LOG" 2>&1 &
PID=$!

READY=0
for i in $(seq 1 40); do
  sleep 1
  if curl -s --max-time 2 -o /dev/null http://127.0.0.1:18086/api/v1/cpu/state; then
    READY=1; break
  fi
done
if [ "$READY" != "1" ]; then
  echo "[$TAG] webserver 就绪超时"
  kill_dosbox
  exit 1
fi
sleep 12

curl -s --max-time 10 -o "/e/GameMission/CapsuleRetro/.temp/fb1-snap-${TAG}a.bin" http://127.0.0.1:18086/api/v1/memory/0x0/0x100000
sleep 2
curl -s --max-time 10 -o "/e/GameMission/CapsuleRetro/.temp/fb1-snap-${TAG}b.bin" http://127.0.0.1:18086/api/v1/memory/0x0/0x100000

kill_dosbox
wait "$PID" 2>/dev/null
A=$(wc -c < "/e/GameMission/CapsuleRetro/.temp/fb1-snap-${TAG}a.bin")
B=$(wc -c < "/e/GameMission/CapsuleRetro/.temp/fb1-snap-${TAG}b.bin")
echo "[$TAG] 快照 a=${A}B b=${B}B（应为 1048576）"
[ "$A" = "1048576" ] && [ "$B" = "1048576" ]
