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
    # 檔名白名單擋在停機之前：打錯名字要立刻失敗、不要先把店關掉。只接受 backup.sh 產生的
    # 精確形狀（dog_shop-YYYYmmdd-HHMMSS.dump），而且檔名只會當成一個參數傳給 pg_restore、
    # 不會再進任何 shell 字串。
    case "$name" in
      dog_shop-[0-9][0-9][0-9][0-9][0-9][0-9][0-9][0-9]-[0-9][0-9][0-9][0-9][0-9][0-9].dump) ;;
      *) echo "檔名不正確：只接受 dog_shop-….dump（先用 restore.sh list 看有哪些）" >&2; exit 2 ;;
    esac
    echo "停止 api 與 web…"
    docker compose stop api web
    # 還原是單一交易：失敗（檔案不存在、備份檔壞掉）就整個 rollback、資料維持還原前的內容，
    # 什麼都沒改；不管成功或失敗都要把店開回來，不能留在關閉狀態
    trap 'docker compose start api web' EXIT
    echo "還原 ${name}（--clean --if-exists：清掉現有資料與寫入在同一個交易裡）…"
    docker compose run --rm --entrypoint pg_restore backup --clean --if-exists --single-transaction --no-owner -d dog_shop "/backups/${name}"
    echo "重新啟動…"
    docker compose start api web
    echo "完成"
    ;;
esac
