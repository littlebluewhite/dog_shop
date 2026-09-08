# dog_shop

自營寵物用品線上商店：後台可匯入商品、線上刷卡／ATM／超商代碼付款、綠界超商取貨與宅配、電子發票。取代原本用蝦皮開店的做法。

## 目錄

- `api/` — Rust（axum）後端。
- `web/` — SvelteKit 前端。
- `deploy/` — Dockerfile、docker compose、Caddyfile、備份／還原腳本。
- `docs/` — 部署手冊、開發筆記、規格與各階段實作計畫。

## 開發快速開始

```
docker compose -f deploy/docker-compose.dev.yml up -d db
cp .env.example .env
export PATH="$HOME/.cargo/bin:$PATH"
cargo run --manifest-path api/Cargo.toml
pnpm -C web dev
```

api 監聽 `:8080`，web dev 監聽 `:5173`（Vite 轉發 `/api`、`/uploads`）。建立第一個管理員帳號：`ADMIN_PASSWORD=<密碼> cargo run --manifest-path api/Cargo.toml -- create-admin <email>`。

## 測試

```
cargo test --manifest-path api/Cargo.toml   # 需要 DATABASE_URL，見 .env.example
pnpm -C web check
pnpm -C web test
pnpm -C web build
pnpm -C web test:e2e                        # 需要 api（:8080）與 web dev（:5173）都在跑
```

## 更多文件

- 部署步驟、環境檢查清單、備份還原、log 對帳：[`docs/deploy.md`](docs/deploy.md)
- 綠界 stage 手動走查：[`docs/dev/ecpay-stage.md`](docs/dev/ecpay-stage.md)
- 規格與各階段實作計畫：`docs/superpowers/`

## 本機部署煙霧測試

本機開發資料庫與正式環境 `deploy/docker-compose.yml` 共用 compose 專案名稱 `dog_shop`。本機驗證部署設定時，一定要另外指定 `-p dog_shop_smoke`（`-f deploy/docker-compose.yml -f deploy/docker-compose.smoke.yml`），`down -v` 也只對這個專案名稱下，避免誤刪開發資料庫。煙霧測試用的 `deploy/.env` 只在本機、不進 git，內容用 `ECPAY_ENV=stage`、`SITE_ADDRESS=http://localhost`、`PUBLIC_BASE_URL=http://localhost:8081`、`COOKIE_SECURE=false`。
