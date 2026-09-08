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
    # 檔名白名單擋在停機之前：打錯名字要立刻失敗、不要先把店關掉。也順便擋掉
    # 會被下面的 sh -c 字串拆開或當成指令執行的檔名（空白、分號）。
    case "$name" in
      dog_shop-*.dump) ;;
      *) echo "檔名不正確：只接受 dog_shop-….dump（先用 restore.sh list 看有哪些）" >&2; exit 2 ;;
    esac
    echo "停止 api 與 web…"
    docker compose stop api web
    # 還原失敗（檔名對但檔案不存在、備份檔壞掉）也要把店開回來，不能留在關閉狀態
    trap 'docker compose start api web' EXIT
    echo "還原 ${name}（--clean --if-exists：會先清掉現有資料）…"
    docker compose run --rm --entrypoint sh backup -c "pg_restore --clean --if-exists --no-owner -d dog_shop /backups/${name}"
    echo "重新啟動…"
    docker compose start api web
    echo "完成"
    ;;
esac
