#!/bin/sh
# 用法：
#   ./restore.sh list                 列出 backups volume 裡的備份
#   ./restore.sh <dog_shop-….dump>    還原（會先停 api／web，還原完再啟動）
set -eu
cd "$(dirname "$0")"
case "${1:-}" in
  list)
    docker compose run --rm --entrypoint sh backup -c 'ls -1 /backups'
    ;;
  "")
    echo "用法：restore.sh list | restore.sh <檔名>" >&2; exit 2
    ;;
  *)
    name="$1"
    echo "停止 api 與 web…"
    docker compose stop api web
    echo "還原 ${name}（--clean --if-exists：會先清掉現有資料）…"
    docker compose run --rm --entrypoint sh backup -c "pg_restore --clean --if-exists --no-owner -d dog_shop /backups/$name"
    echo "重新啟動…"
    docker compose start api web
    echo "完成"
    ;;
esac
