#!/bin/sh
# 用法：backup.sh once | loop
# pg_dump -Fc 到 $BACKUP_DIR/dog_shop-YYYYmmdd-HHMMSS.dump，刪掉超過 $BACKUP_KEEP_DAYS 天的（規格 §16：每日、保留 14 天）
# 連線資訊來自 PGHOST／PGUSER／PGPASSWORD／PGDATABASE（compose 的 backup 服務有設）
set -eu
DIR="${BACKUP_DIR:-/backups}"
KEEP="${BACKUP_KEEP_DAYS:-14}"

run_once() {
  mkdir -p "$DIR"
  f="$DIR/dog_shop-$(date +%Y%m%d-%H%M%S).dump"
  # 失敗要自己清掉 .tmp：下面的 find 只配 dog_shop-*.dump，殘留的 .dump.tmp 永遠不會被回收
  pg_dump -Fc -f "$f.tmp" || { rm -f "$f.tmp"; return 1; }
  mv "$f.tmp" "$f"
  find "$DIR" -name 'dog_shop-*.dump' -mtime +"$KEEP" -delete
  echo "backup ok: $f"
}

case "${1:-once}" in
  once) run_once ;;
  loop)
    while true; do
      run_once || echo "backup failed"
      sleep 86400
    done
    ;;
  *) echo "用法：backup.sh once | loop" >&2; exit 2 ;;
esac
