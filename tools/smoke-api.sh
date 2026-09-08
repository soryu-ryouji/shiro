#!/usr/bin/env bash
# API 冒烟：全部 POST 端点（不含真实 LLM 调用；chat/messages 验证"未配置模型"的失败路径）。
# 用临时 HOME 隔离 config/history/db，不污染真实环境。用法：./tools/smoke-api.sh [端口]
# 需先构建 daemon（cargo build）。产物在 tools/.tmp/smoke-api/，退出时保留日志便于排查。
set -uo pipefail

PORT="${1:-27999}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TMP="$ROOT/tools/.tmp/smoke-api"
HOME_DIR="$TMP/home"
PROJ="$TMP/project"
BASE="http://127.0.0.1:$PORT"
AUTH="Authorization: Bearer smoke"
FAILED=0

rm -rf "$TMP"
mkdir -p "$HOME_DIR" "$PROJ/正文"
printf '# 测试文稿\n' > "$PROJ/正文/001.md"

(cd "$ROOT/shiro-daemon" && cargo build --quiet) || { echo "daemon 构建失败"; exit 1; }
HOME="$HOME_DIR" SHIRO_TOKEN=smoke "$ROOT/shiro-daemon/target/debug/shiro-daemon" --port "$PORT" \
  > "$TMP/daemon.log" 2>&1 &
DAEMON_PID=$!
trap 'kill "$DAEMON_PID" 2>/dev/null; wait "$DAEMON_PID" 2>/dev/null' EXIT

for _ in $(seq 1 60); do
  curl -sf "$BASE/health" >/dev/null 2>&1 && break
  sleep 0.5
done

# post <路径> <body>：输出 body，HTTP 码写入 $TMP/code
post() {
  curl -s -o "$TMP/body" -w '%{http_code}' -X POST \
    -H "$AUTH" -H 'Content-Type: application/json' -d "${2:-{\}}" "$BASE$1" > "$TMP/code"
}
# expect <说明> <期望码> [grep 关键字...]
expect() {
  local name="$1" want="$2"
  shift 2
  local code
  code="$(cat "$TMP/code")"
  if [ "$code" != "$want" ]; then
    echo "FAIL ${name}（HTTP ${code}，期望 ${want}）"; FAILED=1; return
  fi
  for kw in "$@"; do
    if ! grep -q "$kw" "$TMP/body"; then
      echo "FAIL ${name}（响应缺少 ${kw}）"; FAILED=1; return
    fi
  done
  echo "ok   ${name}"
}

# ---- 鉴权 ----
code=$(curl -s -o "$TMP/body" -w '%{http_code}' -X POST "$BASE/api/v1/app/startup")
[ "$code" = "401" ] && echo "ok   未带 token → 401" || { echo "FAIL 未带 token（HTTP $code）"; FAILED=1; }

# ---- app / projects ----
post /api/v1/app/startup; expect "app/startup" 200 '"ready"'
post /api/v1/projects/create "{\"path\":\"$PROJ\",\"name\":\"冒烟项目\"}"; expect "projects/create" 201 '"冒烟项目"'
post /api/v1/projects/list; expect "projects/list" 200 '冒烟项目'
post /api/v1/projects/tree "{\"path\":\"$PROJ\"}"; expect "projects/tree" 200 '001.md'
post /api/v1/projects/file/read "{\"path\":\"$PROJ\",\"file\":\"正文/001.md\"}"; expect "file/read" 200 '测试文稿'
post /api/v1/projects/file/write "{\"path\":\"$PROJ\",\"file\":\"正文/001.md\",\"content\":\"# 改写\\n\"}"; expect "file/write" 200 '改写'
post /api/v1/projects/file/create "{\"path\":\"$PROJ\",\"file\":\"正文/002.md\"}"; expect "file/create" 201
post /api/v1/projects/excerpts "{\"path\":\"$PROJ\",\"dir\":\"正文\"}"; expect "projects/excerpts" 200 '002.md'
post /api/v1/projects/dir/create "{\"path\":\"$PROJ\",\"dir\":\"大纲\"}"; expect "dir/create" 201
post /api/v1/projects/entry/rename "{\"path\":\"$PROJ\",\"rel\":\"正文/002.md\",\"new_name\":\"003.md\"}"; expect "entry/rename" 204
post /api/v1/projects/file/read "{\"path\":\"$PROJ\",\"file\":\"正文/003.md\"}"; expect "file/read（改名后）" 200
post /api/v1/projects/file/delete "{\"path\":\"$PROJ\",\"file\":\"正文/003.md\"}"; expect "file/delete" 204
post /api/v1/projects/dir/delete "{\"path\":\"$PROJ\",\"dir\":\"大纲\"}"; expect "dir/delete" 204

# ---- 目录监听（POST + SSE） ----
curl -sN --max-time 3 -X POST -H "$AUTH" -H 'Content-Type: application/json' \
  -d "{\"path\":\"$PROJ\"}" "$BASE/api/v1/projects/watch" > "$TMP/watch.out" &
WATCH_PID=$!
sleep 1
printf '变动\n' >> "$PROJ/正文/001.md"
sleep 1.5
wait "$WATCH_PID" 2>/dev/null
if grep -q '"changed"' "$TMP/watch.out"; then echo "ok   projects/watch（SSE 推送）"; else echo "FAIL projects/watch"; FAILED=1; fi

# ---- db ----
post /api/v1/db/characters/list; expect "db/characters/list" 200
post /api/v1/db/characters/create '{"name":"测试角色"}'; expect "db/characters/create" 201 '"测试角色"'
CID=$(sed -n 's/.*"id":"\([^"]*\)".*/\1/p' "$TMP/body")
post /api/v1/db/characters/get "{\"id\":\"$CID\"}"; expect "db/characters/get" 200 '"测试角色"'
post /api/v1/db/characters/save "{\"id\":\"$CID\",\"content\":\"---\\nshiro_asset: character\\nname: 测试角色\\n---\\n\\n正文\\n\"}"
expect "db/characters/save" 200 '"parsed":true'

# ---- model ----
post /api/v1/model/profiles/list; expect "model/profiles/list" 200
post /api/v1/model/profiles/save '{"key":"smoke","base_url":"http://127.0.0.1:1","model":"smoke-model","protocol":"openai","api_key":"sk-smoke"}'
expect "model/profiles/save" 200 '"smoke"'
post /api/v1/model/profiles/test '{"key":"smoke"}'; expect "model/profiles/test（不可达）" 200 '"ok":false'
post /api/v1/model/profiles/delete '{"key":"smoke"}'; expect "model/profiles/delete" 200
post /api/v1/model/profiles/delete '{"key":"nonexistent"}'; expect "model/profiles/delete（不存在）" 404

# ---- chat ----
post /api/v1/chat/sessions/create "{\"path\":\"$PROJ\"}"; expect "chat/sessions/create" 201 '"新会话"'
SID=$(sed -n 's/.*"id":"\([^"]*\)".*/\1/p' "$TMP/body")
post /api/v1/chat/sessions/list "{\"path\":\"$PROJ\"}"; expect "chat/sessions/list" 200 '"新会话"'
post /api/v1/chat/sessions/get "{\"path\":\"$PROJ\",\"id\":\"$SID\"}"; expect "chat/sessions/get" 200
post /api/v1/chat/sessions/messages "{\"path\":\"$PROJ\",\"id\":\"$SID\",\"content\":\"你好\"}"
expect "chat/sessions/messages（未配模型）" 400 '模型未配置'
post /api/v1/chat/sessions/apply "{\"path\":\"$PROJ\",\"id\":\"$SID\",\"proposal_id\":\"p1\"}"
expect "chat/sessions/apply（无提案）" 400 '提案不存在'
post /api/v1/chat/sessions/delete "{\"path\":\"$PROJ\",\"id\":\"$SID\"}"; expect "chat/sessions/delete" 204

# ---- 移除项目记录 ----
post /api/v1/projects/remove "{\"path\":\"$PROJ\"}"; expect "projects/remove" 204
post /api/v1/projects/list; expect "projects/list（已移除）" 200

echo
[ "$FAILED" = "0" ] && echo "全部通过（日志：tools/.tmp/smoke-api/）" || { echo "存在失败，见上方输出与 tools/.tmp/smoke-api/daemon.log"; exit 1; }
