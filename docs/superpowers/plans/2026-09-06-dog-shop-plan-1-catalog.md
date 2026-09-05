# dog_shop 實作計畫 1／5：骨架、後台登入、商品目錄

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 做出可以跑起來的第一片：Rust API（設定、錯誤格式、資料庫、登入、分類、商品、圖片上傳）＋ SvelteKit 網站（後台登入、分類與商品管理、公開的首頁／商品列表／商品頁／購物車）＋ 開發環境與 CI。做完後老闆能登入後台上架商品，買家能瀏覽商品並加進購物車。

**Architecture:** 前端 SvelteKit（SSR，adapter-node）在伺服器端用內網位址呼叫 Rust axum API 並轉送 cookie；瀏覽器端用同源 `/api/...`（開發時由 Vite proxy 轉到 :8080）。資料在 PostgreSQL 17（Docker）。Session 存 DB、cookie 只放 session id。圖片由 API 重新編碼後存本機目錄並由 API 靜態提供。

**Tech Stack:** Rust stable（1.98）、axum 0.8.9、sqlx 0.9.0（Postgres、純 SQL migration）、tokio、tower-http 0.7、tower_governor 0.8、argon2 0.6、image 0.25；SvelteKit 2.70 + Svelte 5（runes）、Tailwind CSS 4、Vite 8、vitest、pnpm 10、Node 24；PostgreSQL 17；Docker Compose。

**Spec:** `docs/superpowers/specs/2026-09-06-dog-shop-mvp-design.md`（本計畫只做規格 §18 的第 1、2 步，加上第 3 步裡「後台登入」的部分；其他見下方分工表）

## Global Constraints

- 前端一律用最新版 SvelteKit 2 / Svelte 5 runes（`$state`、`$derived`、`$props`、`$effect`），不用 legacy `export let` / store 寫法（規格 §1.1）。
- 後端 Rust 1.98 stable、axum 0.8、sqlx 0.9、tokio；edition 2024（規格 §1.1）。
- 資料庫 PostgreSQL 17；主鍵 UUID v7（app 端 `Uuid::now_v7()` 產生）；金額 `integer` 新台幣；時間 `timestamptz` 存 UTC（規格 §3）。
- 錯誤回應格式固定 `{ "error": { "code", "message", "details" } }`；本計畫用到的 code：`VALIDATION`、`UNAUTHORIZED`、`FORBIDDEN`、`NOT_FOUND`、`RATE_LIMITED`、`INTERNAL`（規格 §10）。`VALIDATION` 的 `details` 有兩種：欄位層級驗證是 `{ "fields": { "<欄位>": "<訊息>" } }`；整個 JSON body／查詢字串／網址參數解析失敗（沒有可歸屬的欄位）是 `{ "detail": "<解析錯誤文字>" }`。前端 `ApiError.field()` 在後者會回 undefined，顯示 `message` 即可。
- 密碼 argon2id、最少 8 碼；cookie `sid`：`HttpOnly`、`Secure`（開發可關）、`SameSite=Lax`、`Path=/`、30 天；所有變更請求要 `X-Requested-With: fetch` 並比對 `Origin`；`/api/auth/*` 每 IP 每分鐘 10 次（規格 §11）。
- 上傳只收 jpeg/png/webp/gif、≤ 10 MB，伺服器重新解碼再輸出（主圖最長邊 1600、縮圖 400），存 `uploads/yyyy/mm/{uuid}`，回 `Cache-Control: public, max-age=31536000, immutable`（規格 §11；輸出格式見「與規格不同之處」）。
- 商品：最多兩層規格名稱、沒規格也有一列預設規格、圖片最多 9 張、刪除＝`archived`、slug 預設 8 碼隨機小寫英數字、描述純文字（規格 §3）。
- 公開商品列表參數 `q`、`category`、`sort ∈ {newest, price_asc, price_desc}`、`page`、`per_page`（預設 24、最大 60），只回 `active`（規格 §6.1、§10）。
- 樣式 Tailwind CSS 4，自寫元件，不引入元件庫；語言繁體中文（規格 §6.2）。
- 所有 commit 只在本機分支 `worktree-mvp-design`。**不要 `git push`、不要開 PR**（使用者的 CLAUDE.md 第 5 條）。
- 秘密（HashKey、密碼）不進 log、不進 git（規格 §11）。

---

## 五份計畫的分工（規格涵蓋表）

規格的每一段都要有人負責。這張表說明每一項在哪一份計畫。這份是計畫 1。

| 規格章節／需求 | 計畫 |
|---|---|
| §2 架構、§2.1 開發環境（db compose、`cargo run`、`pnpm dev`、Vite proxy） | **1**（cloudflared 在 3） |
| §2.2 Repo 佈局：`api/`、`web/`、`deploy/docker-compose.dev.yml`、`.env.example` | **1**（`deploy/docker-compose.yml`、`Caddyfile`、`backup.sh` 在 5） |
| §3 資料表：users、sessions、categories、products、product_variants、product_images、settings | **1** |
| §3 資料表：password_resets、addresses、orders、order_items、payments、shipments、invoices、jobs、cvs_store_selections | 2（orders/order_items/addresses/password_resets/settings 運費欄位）、3（payments/invoices/jobs）、4（shipments/cvs_store_selections） |
| §4 訂單狀態機、§5 庫存規則 | 2（下單扣庫存、取消歸還）、3（付款、過期）、4（出貨、完成、退回） |
| §6.1 買家頁：`/`、`/products`、`/products/[slug]`、`/cart`（不含伺服器驗證）、`/login`、sitemap、robots | **1** |
| §6.1 買家頁：`/cart` 伺服器驗證、`/checkout`、`/orders/[id]`、`/register`、`/forgot-password`、`/reset/[token]`、`/account/*` | 2（付款輪詢與重付在 3） |
| §6.1 後台：`/admin`（先只有入口）、`/admin/products*`、`/admin/categories` | **1** |
| §6.1 後台：`/admin/settings`、`/admin/orders*`、`/admin/import`、`/admin` 儀表板數字 | 2（settings）、4（orders、儀表板）、5（import） |
| §6.2 `hooks.server.ts`、`lib/api.ts`、購物車 store、Tailwind、台灣地址 JSON | **1**（地址 JSON 在 2） |
| §7 結帳流程 | 2（表單、下單、運費）、3（金流）、4（超商門市） |
| §8 綠界金流／物流／發票 | 3（8.1、8.2、8.4）、4（8.3） |
| §9 背景工作 jobs、排程 | 3 |
| §10 API：health、settings/public、auth login/logout/me、categories、products、admin products/uploads/categories | **1** |
| §10 API：其餘會員、訂單、結帳、綠界、後台訂單、import、dashboard | 2、3、4、5 |
| §11 認證與安全：argon2id、session cookie、CSRF、速率限制、上傳重編碼、request id、log 不含秘密 | **1**（忘記密碼 token、訪客 token 在 2） |
| §12 Email | 3 |
| §13 Excel 匯入 | 5 |
| §14 錯誤處理原則（ApiError、前端 `+error.svelte`、toast） | **1**（綠界回呼規則在 3、4） |
| §15 測試：Rust 單元與 `sqlx::test` 整合、vitest、GitHub Actions CI | **1**（Playwright 主流程在 2；綠界 stage 手動驗收在 3、4） |
| §16 部署：Dockerfile、compose、Caddy、備份 | 5 |
| §17 使用者要準備的東西 | 使用者；5 會列出上線檢查表 |

每份計畫做完、程式碼進 git 之後，才寫下一份，這樣後面的計畫會對著真的程式碼寫。

## 與規格不同之處（已決定，執行時照這裡做）

1. **圖片輸出 JPEG，不是 WebP**（規格 §11 說轉 WebP）。原因：`image` 0.25 的 WebP 編碼器只支援無損（lossless），商品照片會比 JPEG 大好幾倍；要有損 WebP 得綁 libwebp（C 函式庫）增加建置複雜度。決定：解碼後輸出 JPEG（主圖品質 82、縮圖 80、去透明層），檔名 `.jpg`。安全目的（重新解碼、不留原檔）不變。
2. **`users.email` 用 `text` + `unique index on lower(email)`，不用 `citext`**（規格 §3 寫 citext）。行為相同（不分大小寫唯一），少一個 extension、少一個 sqlx 型別對應風險。程式在存與查之前一律 `trim().to_lowercase()`。
3. **上傳圖片先回路徑，不先寫 `product_images`**。上傳 API 回 `{ path, thumb_path, width, height }`；商品建立／更新時把整組 `images[]` 送進來由伺服器寫入 `product_images`。這樣新商品也能先上圖再存。規格 §3 的資料表不變。
4. **規格的 `product_variants.image_id`** 由前端用 `image_path` 指定、伺服器對回 `image_id`（因為新商品的圖片還沒有 id）。
5. **「已有訂單的規格只能停用不能刪」**（規格 §10）在本計畫還沒有 `order_items` 表，所以更新商品時沒出現在 payload 的規格會直接刪除。**計畫 2** 建 `order_items` 時要把 `domain/products.rs` 裡 `write_images_and_variants` 的刪除改成：先查 `order_items` 有沒有引用，有就 `is_active=false`、沒有才刪。這是計畫 2 的必做項。
6. **`INTERNAL` 錯誤的 request id** 放在回應 header `x-request-id`（`tower-http` request-id 層），不放進 JSON body。
7. **速率限制的 key** 用 `SmartIpKeyExtractor`（先看 `X-Forwarded-For`／`X-Real-IP`，沒有才用連線 IP），因為正式環境前面永遠是 Caddy。測試請求一律帶 `X-Forwarded-For: 127.0.0.1`。

## 環境事實（每個任務開始前都要知道）

- 這台機器：macOS、zsh、Node 24.2、pnpm 10.32、Docker（OrbStack）、Homebrew。Task 1 已裝好 Rust 1.98.1（rustup，在 `~/.cargo`）。**這個 sandbox 的新 shell 找不到 `cargo`，而且拒絕 `source "$HOME/.cargo/env"`**：每個含 cargo 的指令一律寫成 `export PATH="$HOME/.cargo/bin:$PATH" && cd api && cargo ...`（已驗證可用）。下面任務裡的 `cd api && cargo ...` 都要這樣加前綴。
- 工作目錄是 git worktree：`/Users/wilson08/IdeaProjects/dog_shop/.claude/worktrees/mvp-design`，分支 `worktree-mvp-design`。所有指令都從這裡跑（下面寫的相對路徑都以它為根）。
- 這個環境的 shell 會拒絕「複雜」的 git 指令（`-C`、放在迴圈或 heredoc 裡）。commit 步驟一律寫成兩行純指令：`git add <檔案...>` 然後 `git commit -m "..."`。每次 Bash 呼叫都是新的 shell，環境變數不會留到下一次。
- 本機有 Homebrew 的 PostgreSQL 14 **客戶端**（`psql`）。**開發資料庫在 `localhost:5435`**（Task 1 發現 5432、5433、5434 都被別的專案的容器占用）。本計畫所有指令已改成 5435；只有 Task 17 的 GitHub Actions 仍是 5432，因為那是 CI 容器裡的埠。
- 開發資料庫連線字串（下面所有測試指令都直接寫出來）：`postgres://dog_shop:dog_shop@localhost:5435/dog_shop`。`#[sqlx::test]` 從**行程環境變數** `DATABASE_URL` 讀連線（不會讀 `.env`），所以測試指令一律寫成 `DATABASE_URL=postgres://dog_shop:dog_shop@localhost:5435/dog_shop cargo test`。它會為每個測試建獨立的臨時資料庫並跑 `migrations/`（Docker 的 postgres 使用者是 superuser，有權限）。
- `cargo run`（開發啟動）用 `dotenvy` 從目前目錄往上找 `.env`，所以根目錄的 `.env`（Task 1 從 `.env.example` 複製）會被 `cd api && cargo run` 讀到。
- sqlx 0.9 的 feature：`FromRow` derive 在 `derive`；`#[sqlx::test]` 需要 `migrate`；runtime 用 `runtime-tokio`；TLS 用 `tls-rustls-ring`。
- axum 0.8 路徑參數寫法是 `/api/products/{slug}`（大括號），不是 `:slug`。
- `sv create` 要完全非互動：`--template minimal --types ts --no-add-ons --no-dir-check --no-download-check --install pnpm`。Tailwind 與 adapter-node 手動加（Task 11 有完整步驟）。
- Rust 每次 commit 前跑 `cargo fmt --all` 與 `cargo clippy --all-targets -- -D warnings`（CI 會用同樣的指令）。前端每次 commit 前跑 `pnpm check`。
- **長時間指令要背景跑。** Bash 工具預設 2 分鐘、最多 10 分鐘就會被砍。第一次 `cargo build`、第一次 `cargo test`（9 個整合測試檔各自編譯）、`pnpm dlx sv@latest create`、`pnpm install` 都可能超過。這些指令用 `run_in_background: true` 執行，再用 Monitor 等它結束並讀輸出；不要在前景跑然後被時間砍掉當成失敗。`cargo run`、`pnpm dev` 這種伺服器也一律背景跑。
- **「手動走一遍」步驟的處理。** 執行任務的人沒有瀏覽器。能用 `curl` 檢查的就用 curl 做（例如頁面 HTML 是否含某字串、robots/sitemap 內容）；要點按的步驟跳過，但**一定要在任務回報裡逐條列出哪些手動步驟沒做**，最後彙整給使用者驗收。跳過手動步驟不算任務失敗；自動測試（cargo test、pnpm test、pnpm check、pnpm build）沒過才算。

## 檔案結構（本計畫會建立的所有檔案）

```
.gitignore
.env.example
.github/workflows/ci.yml
deploy/docker-compose.dev.yml
api/
  Cargo.toml                     套件與 lib/bin 設定
  rust-toolchain.toml            stable + rustfmt + clippy
  build.rs                       migrations/ 變動時重新編譯
  migrations/0001_init.sql       users、sessions、categories、products、product_variants、product_images、settings
  src/
    main.rs                      啟動、migration、create-admin 子指令、serve
    lib.rs                       模組宣告（測試從這裡 use）
    app.rs                       組 Router：路由、靜態檔、中介層
    config.rs                    環境變數 → Config
    error.rs                     ApiError、FieldErrors、JSON 錯誤格式
    extract.rs                   AppJson / AppQuery / AppPath（錯誤轉成我們的格式）
    state.rs                     AppState { db, config }
    db.rs                        連線池、MIGRATOR
    cli.rs                       create-admin
    auth/mod.rs
    auth/password.rs             argon2id hash / verify
    auth/cookie.rs               讀 Cookie header、組 Set-Cookie 字串
    auth/session.rs              sessions 表存取
    auth/extract.rs              CurrentUser / AuthUser / AdminUser 擷取器
    auth/csrf.rs                 X-Requested-With + Origin 中介層
    domain/mod.rs
    domain/settings.rs           settings 表讀取
    domain/users.rs              User、UserPublic、users 表存取
    domain/categories.rs         Category、slug 規則、CRUD
    domain/products.rs           商品／規格／圖片的輸入驗證、寫入、後台查詢、公開查詢
    routes/mod.rs
    routes/health.rs             GET /api/health
    routes/settings.rs           GET /api/settings/public
    routes/auth.rs               login / logout / me（含速率限制）
    routes/categories.rs         公開列表 + 後台 CRUD
    routes/admin_products.rs     後台商品 CRUD
    routes/products.rs           公開商品列表與商品頁
    routes/uploads.rs            POST /api/admin/uploads
    storage/mod.rs               圖片重編碼與存檔
  tests/
    common/mod.rs                測試用 app、請求、登入
    health.rs  settings.rs  create_admin.rs  auth.rs  csrf.rs
    categories.rs  admin_products.rs  uploads.rs  products_public.rs
web/
  package.json  svelte.config.js  vite.config.ts  tsconfig.json（sv create 產生）
  src/
    app.html  app.css  app.d.ts  hooks.server.ts
    lib/types.ts                 API 回應型別
    lib/api.ts                   瀏覽器端 fetch 包裝、ApiError、parseResponse
    lib/api.test.ts
    lib/server/api.ts            伺服器端 fetch（內網位址 + 轉送 cookie）
    lib/format.ts                金額、日期
    lib/toast.svelte.ts          toast 狀態
    lib/cart.svelte.ts           購物車狀態（localStorage）
    lib/cart.test.ts
    lib/components/Toasts.svelte
    lib/components/Pagination.svelte
    lib/components/ProductCard.svelte
    lib/components/admin/ProductForm.svelte
    routes/+layout.server.ts  +layout.svelte  +error.svelte
    routes/+page.server.ts  +page.svelte                    首頁
    routes/products/+page.server.ts  +page.svelte           列表
    routes/products/[slug]/+page.server.ts  +page.svelte    商品頁
    routes/cart/+page.svelte
    routes/login/+page.server.ts  +page.svelte
    routes/robots.txt/+server.ts
    routes/sitemap.xml/+server.ts
    routes/admin/+layout.server.ts  +layout.svelte  +page.svelte
    routes/admin/categories/+page.server.ts  +page.svelte
    routes/admin/products/+page.server.ts  +page.svelte
    routes/admin/products/new/+page.server.ts  +page.svelte
    routes/admin/products/[id]/+page.server.ts  +page.svelte
```

## 介面總表（跨任務共用的名字，各任務的 Interfaces 區塊會再說一次）

後端 JSON 形狀：

- 使用者 `UserPublic`：`{ id, email, name, phone: string|null, role: "customer"|"admin" }`；`GET /api/auth/me` 與登入回 `{ "user": UserPublic }`。
- 分類 `Category`：`{ id, slug, name, sort_order }`。
- 後台商品 `AdminProduct`：`{ id, slug, name, description, category_id, status, option1_name, option2_name, external_ref, sort_order, created_at, updated_at, variants: VariantRow[], images: ImageRow[] }`；`VariantRow = { id, product_id, option1_value, option2_value, sku, price, compare_at_price, stock, is_active, image_id, sort_order }`；`ImageRow = { id, product_id, path, thumb_path, alt, sort_order }`。
- 後台商品輸入 `ProductInput`：`{ name, slug?, description?, category_id?, status, option1_name?, option2_name?, sort_order?, variants: [{ id?, option1_value?, option2_value?, sku?, price, compare_at_price?, stock, is_active?, image_path? }], images: [{ path, thumb_path, alt? }] }`。
- 分頁 `Page<T>`：`{ items: T[], total, page, per_page }`。
- 後台商品列表項 `AdminListItem`：`{ id, slug, name, status, category_name, price_min, price_max, stock_total, image_thumb, updated_at }`。
- 公開商品列表項 `PublicListItem`：`{ slug, name, price_min, price_max, image_thumb, in_stock }`。
- 公開商品頁 `PublicProduct`：`{ id, slug, name, description, category: { slug, name }|null, option1_name, option2_name, images: [{ path, thumb_path, alt }], variants: [{ id, option1_value, option2_value, price, compare_at_price, stock, image_path }] }`。
- 上傳回應 `StoredImage`：`{ path, thumb_path, width, height }`，路徑形如 `/uploads/2026/09/<uuid>.jpg`。
- 錯誤：`{ "error": { "code", "message", "details" } }`；驗證錯誤 `details = { "fields": { "<欄位>": "<訊息>" } }`。

前端共用：`api<T>(path, init?)`（瀏覽器）、`serverApi<T>(event, path, init?)`（伺服器）、`ApiError { status, code, message, details, field(name) }`、`toast.show(message)`、`cart.add / setQty / remove / clear / count / subtotal / lines / loaded / load()`。

---

### Task 1: Rust 工具鏈、repo 基礎檔、開發用資料庫

**Files:**
- Create: `.gitignore`
- Create: `.env.example`
- Create: `.env`（從 `.env.example` 複製；**不進 git**）
- Create: `deploy/docker-compose.dev.yml`

**Interfaces:**
- Produces: 可用的 `cargo`；跑在 `localhost:5435` 的 PostgreSQL 17，帳密／資料庫都是 `dog_shop`；根目錄 `.env`（後面 `cargo run`、`pnpm dev` 都讀它）。

- [ ] **Step 1: 安裝 Rust（rustup 官方安裝器，非互動）**

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile default
```

- [ ] **Step 2: 確認 cargo 可用**

Run: `source "$HOME/.cargo/env" && cargo --version && rustc --version && cargo fmt --version && cargo clippy --version`
Expected: 四行版本，cargo 與 rustc 是 1.98 或更新。若之後任何 Bash 呼叫找不到 cargo，先 `source "$HOME/.cargo/env"`。

- [ ] **Step 3: 寫 `.gitignore`**

```gitignore
# Rust
api/target/

# Node / SvelteKit
web/node_modules/
web/.svelte-kit/
web/build/
web/test-results/

# 環境變數與上傳檔
.env
.env.*
!.env.example
uploads/
api/uploads/

# 編輯器與系統
.idea/
.vscode/
.DS_Store
```

- [ ] **Step 4: 寫 `.env.example`（規格 §16 的變數全列；綠界 stage 憑證是公開測試資料）**

```dotenv
# ── 資料庫（開發：deploy/docker-compose.dev.yml 的 db）──
DATABASE_URL=postgres://dog_shop:dog_shop@localhost:5435/dog_shop

# ── 網址 ──
# 對外網址。開發時是 Vite 的網址；要收綠界回呼時改成 cloudflared 給的 https 網址
PUBLIC_BASE_URL=http://localhost:5173
# web 在 SSR 時打 api 的內網位址（Docker 裡是 http://api:8080）
API_INTERNAL_URL=http://localhost:8080

# ── Cookie：開發用 http 要設 false；正式一定要 true ──
COOKIE_SECURE=false

# ── 圖片存放目錄（相對於 api/ 的執行目錄，或絕對路徑）──
UPLOAD_DIR=./uploads

# ── Log ──
RUST_LOG=info,tower_http=info

# ── 綠界（計畫 3、4 才會用到；stage 是公開測試帳號）──
ECPAY_ENV=stage
ECPAY_AIO_MERCHANT_ID=3002607
ECPAY_AIO_HASH_KEY=pwFHCqoQZGmho4w6
ECPAY_AIO_HASH_IV=EkRm7iFT261dpevs
ECPAY_LOGISTICS_MERCHANT_ID=
ECPAY_LOGISTICS_HASH_KEY=
ECPAY_LOGISTICS_HASH_IV=
ECPAY_INVOICE_MERCHANT_ID=2000132
ECPAY_INVOICE_HASH_KEY=
ECPAY_INVOICE_HASH_IV=

# ── SMTP（計畫 3）──
SMTP_HOST=
SMTP_PORT=587
SMTP_USER=
SMTP_PASS=
SMTP_FROM=
```

- [ ] **Step 5: 複製成 `.env`**

Run: `cp .env.example .env`

- [ ] **Step 6: 檢查 5432 埠有沒有被占用**

Run: `lsof -nP -iTCP:5432 -sTCP:LISTEN || echo "5432 可用"`
Expected: 印出 `5432 可用`。若印出某個行程在聽 5432：在下一步的 compose 檔把 `"5432:5432"` 改成 `"5433:5432"`，並把 `.env` 與 `.env.example` 的 `DATABASE_URL` 埠號改成 5433，之後本計畫所有指令裡的 5432 都用 5433。

- [ ] **Step 7: 寫 `deploy/docker-compose.dev.yml`**

```yaml
name: dog_shop

services:
  db:
    image: postgres:17
    environment:
      POSTGRES_USER: dog_shop
      POSTGRES_PASSWORD: dog_shop
      POSTGRES_DB: dog_shop
    ports:
      - "5435:5432"
    volumes:
      - pgdata_dev:/var/lib/postgresql/data
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U dog_shop -d dog_shop"]
      interval: 5s
      timeout: 5s
      retries: 12

volumes:
  pgdata_dev: {}
```

- [ ] **Step 8: 啟動資料庫並確認可連**

Run: `docker compose -f deploy/docker-compose.dev.yml up -d db`
Then run: `docker compose -f deploy/docker-compose.dev.yml ps`
Expected: `db` 的 STATUS 出現 `healthy`（剛啟動可能是 `health: starting`，過幾秒再跑一次 ps）。
Then run: `psql postgres://dog_shop:dog_shop@localhost:5435/dog_shop -c 'select version();'`
Expected: 一行 `PostgreSQL 17.x ...`。

- [ ] **Step 9: Commit**

```bash
git add .gitignore .env.example deploy/docker-compose.dev.yml
git commit -m "chore: repo 基礎檔、開發資料庫 compose、環境變數範例"
```

---

### Task 2: api crate 骨架（Config、錯誤格式、health）

**Files:**
- Create: `api/Cargo.toml`、`api/rust-toolchain.toml`
- Create: `api/src/main.rs`、`api/src/lib.rs`、`api/src/app.rs`、`api/src/config.rs`、`api/src/error.rs`、`api/src/extract.rs`、`api/src/state.rs`
- Create: `api/src/routes/mod.rs`、`api/src/routes/health.rs`
- Test: `api/tests/common/mod.rs`、`api/tests/health.rs`

**Interfaces:**
- Produces:
  - `Config { database_url: String, public_base_url: String, cookie_secure: bool, upload_dir: PathBuf }`、`Config::from_env() -> anyhow::Result<Config>`、`Config::public_origin(&self) -> &str`
  - `AppState { db: PgPool, config: Arc<Config> }`（Clone）
  - `ApiError` 列舉：`Validation { message, details }`、`Unauthorized(&'static str)`、`Forbidden(&'static str)`、`NotFound`、`RateLimited`、`Internal(anyhow::Error)`；`ApiError::field(field, message)`；`ApiResult<T>`；`FieldErrors { new(), add(field, msg), is_empty(), into_error(), into_result() }`；`From<sqlx::Error>`、`From<anyhow::Error>`、`From<std::io::Error>`、`From<tokio::task::JoinError>`
  - `AppJson<T>`、`AppQuery<T>`、`AppPath<T>` 擷取器
  - `app::router(state: AppState) -> Router`
  - 測試工具 `common::state(pool)`、`common::app(pool)`、`common::req(method, uri, cookie, body)`、`common::send(&app, request) -> (StatusCode, Value, HeaderMap)`、`common::TEST_ORIGIN`

- [ ] **Step 1: 建 crate**

Run: `cargo new api --name dog-shop-api --vcs none`
Expected: 產生 `api/Cargo.toml` 與 `api/src/main.rs`。

- [ ] **Step 2: 覆寫 `api/Cargo.toml`（一次把本計畫會用到的相依都放進來）**

```toml
[package]
name = "dog-shop-api"
version = "0.1.0"
edition = "2024"
rust-version = "1.98"
publish = false

[lib]
name = "dog_shop_api"
path = "src/lib.rs"

[[bin]]
name = "api"
path = "src/main.rs"

[dependencies]
anyhow = "1"
argon2 = "0.6"
axum = { version = "0.8", features = ["macros", "multipart"] }
chrono = { version = "0.4", features = ["serde"] }
dotenvy = "0.15"
image = { version = "0.25", default-features = false, features = ["gif", "jpeg", "png", "webp"] }
rpassword = "7"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
sqlx = { version = "0.9", features = [
  "chrono",
  "derive",
  "json",
  "macros",
  "migrate",
  "postgres",
  "runtime-tokio",
  "tls-rustls-ring",
  "uuid",
] }
thiserror = "2"
tokio = { version = "1", features = ["full"] }
tower = { version = "0.5", features = ["util"] }
tower-http = { version = "0.7", features = ["fs", "request-id", "set-header", "trace"] }
tower_governor = "0.8"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
uuid = { version = "1", features = ["serde", "v4", "v7"] }

[dev-dependencies]
http-body-util = "0.1"
```

- [ ] **Step 3: 寫 `api/rust-toolchain.toml`**

```toml
[toolchain]
channel = "stable"
components = ["rustfmt", "clippy"]
```

- [ ] **Step 4: 寫 `api/src/config.rs`（含單元測試）**

```rust
use std::path::PathBuf;

use anyhow::Context;

/// 從環境變數讀進來的設定。測試會直接建構這個 struct，所以欄位都是 pub。
#[derive(Clone, Debug)]
pub struct Config {
    pub database_url: String,
    /// 對外網址，例如 http://localhost:5173 或 https://shop.example.com（結尾不帶 /）
    pub public_base_url: String,
    /// cookie 是否加 Secure。開發環境（http）設 COOKIE_SECURE=false
    pub cookie_secure: bool,
    /// 圖片存放目錄
    pub upload_dir: PathBuf,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let database_url = std::env::var("DATABASE_URL").context("缺少環境變數 DATABASE_URL")?;
        let public_base_url = std::env::var("PUBLIC_BASE_URL")
            .unwrap_or_else(|_| "http://localhost:5173".to_string())
            .trim_end_matches('/')
            .to_string();
        let cookie_secure = match std::env::var("COOKIE_SECURE") {
            Ok(value) => !matches!(value.trim().to_ascii_lowercase().as_str(), "false" | "0" | "no"),
            Err(_) => true,
        };
        let upload_dir =
            PathBuf::from(std::env::var("UPLOAD_DIR").unwrap_or_else(|_| "./uploads".to_string()));
        Ok(Self { database_url, public_base_url, cookie_secure, upload_dir })
    }

    /// 只留 scheme://host[:port]，用來和瀏覽器送來的 Origin header 比對
    pub fn public_origin(&self) -> &str {
        let url = &self.public_base_url;
        let Some(scheme_end) = url.find("://") else { return url };
        let rest = &url[scheme_end + 3..];
        match rest.find('/') {
            Some(i) => &url[..scheme_end + 3 + i],
            None => url,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(base: &str) -> Config {
        Config {
            database_url: String::new(),
            public_base_url: base.to_string(),
            cookie_secure: false,
            upload_dir: PathBuf::from("/tmp"),
        }
    }

    #[test]
    fn public_origin_strips_path() {
        assert_eq!(cfg("https://shop.example.com/some/path").public_origin(), "https://shop.example.com");
        assert_eq!(cfg("http://localhost:5173").public_origin(), "http://localhost:5173");
    }
}
```

- [ ] **Step 5: 寫 `api/src/error.rs`（含單元測試）**

```rust
use std::collections::BTreeMap;

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::{Value, json};

/// 所有 handler 的錯誤型別。回應格式固定為
/// `{ "error": { "code", "message", "details" } }`（規格 §10）。
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("{message}")]
    Validation { message: String, details: Value },
    #[error("{0}")]
    Unauthorized(&'static str),
    #[error("{0}")]
    Forbidden(&'static str),
    #[error("找不到資料")]
    NotFound,
    #[error("請求太頻繁，請稍後再試")]
    RateLimited,
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

pub type ApiResult<T> = Result<T, ApiError>;

impl ApiError {
    /// 單一欄位的驗證錯誤
    pub fn field(field: &str, message: &str) -> Self {
        let mut errors = FieldErrors::new();
        errors.add(field, message);
        errors.into_error()
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::Validation { .. } => "VALIDATION",
            Self::Unauthorized(_) => "UNAUTHORIZED",
            Self::Forbidden(_) => "FORBIDDEN",
            Self::NotFound => "NOT_FOUND",
            Self::RateLimited => "RATE_LIMITED",
            Self::Internal(_) => "INTERNAL",
        }
    }

    pub fn status(&self) -> StatusCode {
        match self {
            Self::Validation { .. } => StatusCode::BAD_REQUEST,
            Self::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            Self::Forbidden(_) => StatusCode::FORBIDDEN,
            Self::NotFound => StatusCode::NOT_FOUND,
            Self::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status();
        let code = self.code();
        let (message, details) = match &self {
            Self::Validation { message, details } => (message.clone(), details.clone()),
            Self::Internal(err) => {
                // 細節只進 log；request id 在回應 header x-request-id
                tracing::error!(error = ?err, "internal error");
                ("伺服器發生錯誤，請稍後再試".to_string(), Value::Null)
            }
            other => (other.to_string(), Value::Null),
        };
        let body = json!({ "error": { "code": code, "message": message, "details": details } });
        (status, Json(body)).into_response()
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(err: sqlx::Error) -> Self {
        match err {
            sqlx::Error::RowNotFound => Self::NotFound,
            other => Self::Internal(other.into()),
        }
    }
}

impl From<std::io::Error> for ApiError {
    fn from(err: std::io::Error) -> Self {
        Self::Internal(err.into())
    }
}

impl From<tokio::task::JoinError> for ApiError {
    fn from(err: tokio::task::JoinError) -> Self {
        Self::Internal(err.into())
    }
}

/// 收集欄位錯誤，最後變成一個 VALIDATION 錯誤：
/// `details = { "fields": { "name": "必填" } }`。同一欄位只留第一個訊息。
#[derive(Debug, Default)]
pub struct FieldErrors(BTreeMap<String, String>);

impl FieldErrors {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, field: &str, message: &str) {
        self.0.entry(field.to_string()).or_insert_with(|| message.to_string());
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn into_error(self) -> ApiError {
        ApiError::Validation { message: "輸入資料有誤".to_string(), details: json!({ "fields": self.0 }) }
    }

    /// 沒錯誤回 Ok(())，有錯誤回 Err(VALIDATION)
    pub fn into_result(self) -> Result<(), ApiError> {
        if self.is_empty() { Ok(()) } else { Err(self.into_error()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn body_json(response: Response) -> Value {
        let bytes = axum::body::to_bytes(response.into_body(), 1 << 20).await.unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn not_found_envelope() {
        let response = ApiError::NotFound.into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let v = body_json(response).await;
        assert_eq!(v["error"]["code"], "NOT_FOUND");
        assert_eq!(v["error"]["message"], "找不到資料");
        assert!(v["error"]["details"].is_null());
    }

    #[tokio::test]
    async fn field_errors_envelope() {
        let mut errors = FieldErrors::new();
        errors.add("name", "必填");
        errors.add("name", "第二個訊息會被忽略");
        let response = errors.into_error().into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let v = body_json(response).await;
        assert_eq!(v["error"]["code"], "VALIDATION");
        assert_eq!(v["error"]["details"]["fields"]["name"], "必填");
    }

    #[test]
    fn empty_field_errors_is_ok() {
        assert!(FieldErrors::new().into_result().is_ok());
    }
}
```

- [ ] **Step 6: 寫 `api/src/extract.rs`**

```rust
//! 把 axum 內建擷取器的錯誤轉成我們的 JSON 錯誤格式。
//! handler 一律用 AppJson / AppQuery / AppPath，不要直接用 axum::Json 等。
use axum::extract::rejection::{JsonRejection, PathRejection, QueryRejection};
use axum::extract::{FromRequest, FromRequestParts};
use serde_json::json;

use crate::error::ApiError;

#[derive(FromRequest)]
#[from_request(via(axum::Json), rejection(ApiError))]
pub struct AppJson<T>(pub T);

#[derive(FromRequestParts)]
#[from_request(via(axum::extract::Query), rejection(ApiError))]
pub struct AppQuery<T>(pub T);

#[derive(FromRequestParts)]
#[from_request(via(axum::extract::Path), rejection(ApiError))]
pub struct AppPath<T>(pub T);

impl From<JsonRejection> for ApiError {
    fn from(rejection: JsonRejection) -> Self {
        ApiError::Validation {
            message: "JSON 格式錯誤".to_string(),
            details: json!({ "detail": rejection.body_text() }),
        }
    }
}

impl From<QueryRejection> for ApiError {
    fn from(rejection: QueryRejection) -> Self {
        ApiError::Validation {
            message: "查詢參數格式錯誤".to_string(),
            details: json!({ "detail": rejection.body_text() }),
        }
    }
}

impl From<PathRejection> for ApiError {
    fn from(rejection: PathRejection) -> Self {
        ApiError::Validation {
            message: "網址參數格式錯誤".to_string(),
            details: json!({ "detail": rejection.body_text() }),
        }
    }
}
```

- [ ] **Step 7: 寫 `api/src/state.rs`**

```rust
use std::sync::Arc;

use sqlx::PgPool;

use crate::config::Config;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub config: Arc<Config>,
}
```

- [ ] **Step 8: 寫 `api/src/routes/health.rs` 與 `api/src/routes/mod.rs`**

`api/src/routes/health.rs`:

```rust
use axum::{Json, Router, routing::get};
use serde_json::{Value, json};

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/api/health", get(health))
}

async fn health() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}
```

`api/src/routes/mod.rs`:

```rust
pub mod health;
```

- [ ] **Step 9: 寫 `api/src/app.rs`**

```rust
use axum::{Router, extract::DefaultBodyLimit};
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::trace::TraceLayer;

use crate::{routes, state::AppState};

/// 上傳上限 10 MB，多留一點給 multipart 邊界與 JSON
pub const BODY_LIMIT_BYTES: usize = 10 * 1024 * 1024 + 64 * 1024;

/// 組出整個 API。layer 的順序：後加的在外層，所以 request id 最外、trace 其次。
pub fn router(state: AppState) -> Router {
    Router::new()
        .merge(routes::health::router())
        .layer(TraceLayer::new_for_http())
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
        .layer(DefaultBodyLimit::max(BODY_LIMIT_BYTES))
        .with_state(state)
}
```

- [ ] **Step 10: 寫 `api/src/lib.rs`**

```rust
pub mod app;
pub mod config;
pub mod error;
pub mod extract;
pub mod routes;
pub mod state;
```

- [ ] **Step 11: 覆寫 `api/src/main.rs`**

```rust
use std::{net::SocketAddr, sync::Arc};

use dog_shop_api::{app, config::Config, state::AppState};
use sqlx::postgres::PgPoolOptions;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,tower_http=info")),
        )
        .init();

    let config = Arc::new(Config::from_env()?);
    let db = PgPoolOptions::new().max_connections(10).connect(&config.database_url).await?;

    let state = AppState { db, config: config.clone() };
    let app = app::router(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await?;
    tracing::info!("api listening on http://0.0.0.0:8080");
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    tracing::info!("shutting down");
}
```

- [ ] **Step 12: 第一次編譯（會下載並編譯所有相依，可能要 3～8 分鐘）**

Run: `cd api && cargo build`
Expected: 最後一行 `Finished \`dev\` profile ...`，沒有 error。

- [ ] **Step 13: 寫測試工具 `api/tests/common/mod.rs`**

```rust
#![allow(dead_code)]

use std::sync::Arc;

use axum::{
    Router,
    body::Body,
    http::{HeaderMap, Request, StatusCode, header},
};
use dog_shop_api::{app, config::Config, state::AppState};
use http_body_util::BodyExt;
use serde_json::Value;
use sqlx::PgPool;
use tower::ServiceExt;

pub const TEST_ORIGIN: &str = "http://localhost:5173";

/// 每個測試一個獨立的上傳目錄，避免互相干擾
pub fn state(pool: PgPool) -> AppState {
    let upload_dir = std::env::temp_dir().join(format!("dog_shop_test_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&upload_dir).unwrap();
    AppState {
        db: pool,
        config: Arc::new(Config {
            database_url: String::new(),
            public_base_url: TEST_ORIGIN.to_string(),
            cookie_secure: false,
            upload_dir,
        }),
    }
}

pub fn app(pool: PgPool) -> Router {
    app::router(state(pool))
}

/// 建一個像瀏覽器 fetch 送出的請求：帶 Origin、X-Requested-With、X-Forwarded-For（速率限制用）
pub fn req(method: &str, uri: &str, cookie: Option<&str>, body: Option<Value>) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::ACCEPT, "application/json")
        .header(header::ORIGIN, TEST_ORIGIN)
        .header("x-requested-with", "fetch")
        .header("x-forwarded-for", "127.0.0.1");
    if let Some(cookie) = cookie {
        builder = builder.header(header::COOKIE, cookie);
    }
    match body {
        Some(json) => builder
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(json.to_string()))
            .unwrap(),
        None => builder.body(Body::empty()).unwrap(),
    }
}

/// 送出請求，回 (狀態碼, JSON body（空 body 是 Null；不是 JSON 就包成字串）, 回應 headers)
pub async fn send(app: &Router, request: Request<Body>) -> (StatusCode, Value, HeaderMap) {
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let json = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into_owned()))
    };
    (status, json, headers)
}
```

- [ ] **Step 14: 寫 `api/tests/health.rs`**

```rust
mod common;

use axum::http::StatusCode;
use sqlx::postgres::PgPoolOptions;

#[tokio::test]
async fn health_returns_ok_with_request_id() {
    // health 不碰資料庫，用 lazy 連線就好（不會真的連）
    let pool = PgPoolOptions::new()
        .connect_lazy("postgres://dog_shop:dog_shop@localhost:5435/dog_shop")
        .unwrap();
    let app = common::app(pool);
    let (status, body, headers) = common::send(&app, common::req("GET", "/api/health", None, None)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ok");
    assert!(headers.contains_key("x-request-id"));
}
```

- [ ] **Step 15: 跑測試**

Run: `cd api && cargo test`
Expected: `config::tests::public_origin_strips_path`、`error::tests::*`（3 個）、`health_returns_ok_with_request_id` 全部 `ok`。

- [ ] **Step 16: 格式與 lint**

Run: `cd api && cargo fmt --all && cargo clippy --all-targets -- -D warnings`
Expected: clippy 沒有 warning／error。有的話照訊息修到乾淨。

- [ ] **Step 17: Commit**

```bash
git add api/Cargo.toml api/Cargo.lock api/rust-toolchain.toml api/src api/tests
git commit -m "feat(api): axum 骨架、Config、ApiError JSON 格式、health"
```


---

### Task 3: 第一版 migration、DB 連線、公開設定 API

**Files:**
- Create: `api/build.rs`、`api/migrations/0001_init.sql`、`api/src/db.rs`
- Create: `api/src/domain/mod.rs`、`api/src/domain/settings.rs`、`api/src/routes/settings.rs`
- Modify: `api/src/main.rs`（用 `db::connect`、跑 migration）、`api/src/lib.rs`、`api/src/app.rs`、`api/src/routes/mod.rs`
- Test: `api/tests/settings.rs`

**Interfaces:**
- Consumes: Task 2 的 `AppState`、`ApiResult`、`common::*`。
- Produces:
  - `db::connect(database_url: &str) -> anyhow::Result<PgPool>`、`db::migrate(pool: &PgPool) -> anyhow::Result<()>`、`db::MIGRATOR`
  - `settings::get(db: &PgPool, key: &str) -> Result<Option<serde_json::Value>, sqlx::Error>`、`settings::SHOP_KEY = "shop"`
  - `GET /api/settings/public` → settings 表 `shop` 的 JSON：`{ name, description, contact_email, contact_phone }`
  - 資料表：`users`、`sessions`、`categories`、`products`、`product_images`、`product_variants`、`settings`（欄位見 SQL）

- [ ] **Step 1: 寫 `api/build.rs`**

```rust
fn main() {
    // migrations/ 有變動時重新編譯，讓 sqlx::migrate! 內嵌的內容跟著更新
    println!("cargo:rerun-if-changed=migrations");
}
```

- [ ] **Step 2: 寫 `api/migrations/0001_init.sql`**

```sql
-- 使用者（老闆也是 user，role = admin）
CREATE TABLE users (
    id            uuid PRIMARY KEY,
    email         text NOT NULL,
    password_hash text NOT NULL,
    name          text NOT NULL DEFAULT '',
    phone         text,
    role          text NOT NULL DEFAULT 'customer' CHECK (role IN ('customer', 'admin')),
    created_at    timestamptz NOT NULL DEFAULT now(),
    updated_at    timestamptz NOT NULL DEFAULT now()
);
-- email 不分大小寫唯一（程式存入前一律轉小寫）
CREATE UNIQUE INDEX users_email_lower_idx ON users (lower(email));

-- 登入 session；cookie sid 存這裡的 id
CREATE TABLE sessions (
    id         uuid PRIMARY KEY,
    user_id    uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    expires_at timestamptz NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX sessions_expires_at_idx ON sessions (expires_at);
CREATE INDEX sessions_user_id_idx ON sessions (user_id);

-- 單層分類
CREATE TABLE categories (
    id         uuid PRIMARY KEY,
    slug       text NOT NULL UNIQUE,
    name       text NOT NULL,
    sort_order integer NOT NULL DEFAULT 0
);

-- 商品主檔（刪除 = status archived）
CREATE TABLE products (
    id           uuid PRIMARY KEY,
    slug         text NOT NULL UNIQUE,
    name         text NOT NULL,
    description  text NOT NULL DEFAULT '',
    category_id  uuid REFERENCES categories(id) ON DELETE SET NULL,
    status       text NOT NULL DEFAULT 'draft' CHECK (status IN ('draft', 'active', 'archived')),
    option1_name text,
    option2_name text,
    external_ref text UNIQUE,
    sort_order   integer NOT NULL DEFAULT 0,
    created_at   timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX products_category_status_idx ON products (category_id, status);
CREATE INDEX products_status_created_idx ON products (status, created_at DESC);

-- 商品圖片（最多 9 張由程式限制）
CREATE TABLE product_images (
    id         uuid PRIMARY KEY,
    product_id uuid NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    path       text NOT NULL,
    thumb_path text NOT NULL,
    alt        text NOT NULL DEFAULT '',
    sort_order integer NOT NULL DEFAULT 0
);
CREATE INDEX product_images_product_idx ON product_images (product_id, sort_order);

-- 規格：每個規格一列；沒規格的商品也有一列「預設」（option 值為 NULL）
CREATE TABLE product_variants (
    id               uuid PRIMARY KEY,
    product_id       uuid NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    option1_value    text,
    option2_value    text,
    sku              text,
    price            integer NOT NULL CHECK (price >= 0),
    compare_at_price integer CHECK (compare_at_price >= 0),
    stock            integer NOT NULL DEFAULT 0 CHECK (stock >= 0),
    is_active        boolean NOT NULL DEFAULT true,
    image_id         uuid REFERENCES product_images(id) ON DELETE SET NULL,
    sort_order       integer NOT NULL DEFAULT 0
);
CREATE INDEX product_variants_product_idx ON product_variants (product_id);

-- 商店設定（key/value）。計畫 2 會加運費等 key
CREATE TABLE settings (
    key        text PRIMARY KEY,
    value      jsonb NOT NULL,
    updated_at timestamptz NOT NULL DEFAULT now()
);
INSERT INTO settings (key, value) VALUES (
    'shop',
    '{"name": "dog_shop", "description": "", "contact_email": "", "contact_phone": ""}'
);
```

- [ ] **Step 3: 寫 `api/src/db.rs`**

```rust
use sqlx::{PgPool, migrate::Migrator, postgres::PgPoolOptions};

/// 內嵌 ./migrations 底下的 SQL；啟動時執行（規格 §16）
pub static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

pub async fn connect(database_url: &str) -> anyhow::Result<PgPool> {
    let pool = PgPoolOptions::new().max_connections(10).connect(database_url).await?;
    Ok(pool)
}

pub async fn migrate(pool: &PgPool) -> anyhow::Result<()> {
    MIGRATOR.run(pool).await?;
    Ok(())
}
```

- [ ] **Step 4: 寫 `api/src/domain/settings.rs` 與 `api/src/domain/mod.rs`**

`api/src/domain/settings.rs`:

```rust
use serde_json::Value;
use sqlx::PgPool;

pub const SHOP_KEY: &str = "shop";

pub async fn get(db: &PgPool, key: &str) -> Result<Option<Value>, sqlx::Error> {
    sqlx::query_scalar::<_, Value>("SELECT value FROM settings WHERE key = $1")
        .bind(key)
        .fetch_optional(db)
        .await
}
```

`api/src/domain/mod.rs`:

```rust
pub mod settings;
```

- [ ] **Step 5: 寫 `api/src/routes/settings.rs`，並在 `api/src/routes/mod.rs` 加 `pub mod settings;`**

```rust
use axum::{Json, Router, extract::State, routing::get};
use serde_json::{Value, json};

use crate::{domain::settings, error::ApiResult, state::AppState};

pub fn router() -> Router<AppState> {
    Router::new().route("/api/settings/public", get(public_settings))
}

/// 公開的商店設定：目前只有 shop 這把 key（名稱、聯絡方式）。計畫 2 會加運費。
async fn public_settings(State(state): State<AppState>) -> ApiResult<Json<Value>> {
    let shop = settings::get(&state.db, settings::SHOP_KEY).await?.unwrap_or_else(|| json!({}));
    Ok(Json(shop))
}
```

- [ ] **Step 6: 接上 lib.rs、app.rs、main.rs**

`api/src/lib.rs` 加兩行（保持字母順序）：

```rust
pub mod db;
pub mod domain;
```

`api/src/app.rs`：在 `.merge(routes::health::router())` 下面加

```rust
        .merge(routes::settings::router())
```

`api/src/main.rs`：把 `use sqlx::postgres::PgPoolOptions;` 換成 `use dog_shop_api::db;`，`use dog_shop_api::{app, config::Config, state::AppState};` 保留；把建立連線的那一行換成

```rust
    let db = db::connect(&config.database_url).await?;
    db::migrate(&db).await?;
```

- [ ] **Step 7: 寫 `api/tests/settings.rs`**

```rust
mod common;

use axum::http::StatusCode;
use sqlx::PgPool;

#[sqlx::test(migrations = "./migrations")]
async fn public_settings_returns_shop(pool: PgPool) {
    let app = common::app(pool);
    let (status, body, _) = common::send(&app, common::req("GET", "/api/settings/public", None, None)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["name"], "dog_shop");
    assert_eq!(body["contact_email"], "");
}

#[sqlx::test(migrations = "./migrations")]
async fn migration_creates_catalog_tables(pool: PgPool) {
    for table in ["users", "sessions", "categories", "products", "product_variants", "product_images"] {
        let count: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM {table}")).fetch_one(&pool).await.unwrap();
        assert_eq!(count, 0, "{table} 應該是空的");
    }
}
```

- [ ] **Step 8: 跑測試（資料庫要先起來，見 Task 1 Step 8）**

Run: `cd api && DATABASE_URL=postgres://dog_shop:dog_shop@localhost:5435/dog_shop cargo test`
Expected: 全部 `ok`，包含 `public_settings_returns_shop` 與 `migration_creates_catalog_tables`。

- [ ] **Step 9: 手動確認 `cargo run` 會跑 migration**

Run（背景執行）: `cd api && cargo run`，等 log 出現 `api listening` 之後再跑下一行，看完把它停掉。
Run: `psql postgres://dog_shop:dog_shop@localhost:5435/dog_shop -c '\dt'`
Expected: 看到 `_sqlx_migrations`、`users`、`sessions`、`categories`、`products`、`product_images`、`product_variants`、`settings`。

- [ ] **Step 10: 格式、lint、commit**

Run: `cd api && cargo fmt --all && cargo clippy --all-targets -- -D warnings`

```bash
git add api/build.rs api/migrations api/src api/tests
git commit -m "feat(api): 第一版 migration、連線池、公開設定 API"
```

---

### Task 4: 密碼雜湊與 create-admin 指令

**Files:**
- Create: `api/src/auth/mod.rs`、`api/src/auth/password.rs`、`api/src/domain/users.rs`、`api/src/cli.rs`
- Modify: `api/src/lib.rs`、`api/src/domain/mod.rs`、`api/src/main.rs`
- Test: `api/tests/create_admin.rs`

**Interfaces:**
- Consumes: Task 3 的 `db`、`users` 表。
- Produces:
  - `auth::password::{hash_password(&str) -> anyhow::Result<String>, verify_password(&str, &str) -> bool, hash_password_async(String) -> anyhow::Result<String>, verify_password_async(String, String) -> anyhow::Result<bool>, MIN_PASSWORD_CHARS = 8}`
  - `domain::users::{User { id, email, password_hash, name, phone, role, created_at, updated_at }, UserPublic, User::public(&self) -> UserPublic, User::is_admin(&self) -> bool, normalize_email(&str) -> String, is_valid_email(&str) -> bool, find_by_email(db, email) -> Result<Option<User>, sqlx::Error>, find_by_id(db, id), create(db, email, password_hash, name, role) -> Result<User, sqlx::Error>, set_password_and_role(db, id, hash, role), ROLE_CUSTOMER, ROLE_ADMIN}`
  - `cli::create_admin_with_password(db, email, password) -> anyhow::Result<String>`、`cli::create_admin_interactive(db, email)`
  - 指令：`cargo run -- create-admin <email>`

- [ ] **Step 1: 寫 `api/src/auth/password.rs`（含單元測試）**

```rust
use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};

pub const MIN_PASSWORD_CHARS: usize = 8;

/// argon2id（crate 預設參數）。回傳 PHC 字串（$argon2id$v=19$...）
pub fn hash_password(password: &str) -> anyhow::Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!("hash password: {e}"))?;
    Ok(hash.to_string())
}

/// 對不上、或雜湊字串格式壞掉，都回 false
pub fn verify_password(password: &str, password_hash: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(password_hash) else { return false };
    Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok()
}

/// argon2 很吃 CPU（幾十毫秒），在 async 裡要丟到 blocking thread
pub async fn hash_password_async(password: String) -> anyhow::Result<String> {
    tokio::task::spawn_blocking(move || hash_password(&password)).await?
}

pub async fn verify_password_async(password: String, password_hash: String) -> anyhow::Result<bool> {
    Ok(tokio::task::spawn_blocking(move || verify_password(&password, &password_hash)).await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_and_verify_roundtrip() {
        let hash = hash_password("correct horse").unwrap();
        assert!(hash.starts_with("$argon2id$"));
        assert!(verify_password("correct horse", &hash));
        assert!(!verify_password("wrong", &hash));
    }

    #[test]
    fn two_hashes_differ_because_of_salt() {
        assert_ne!(hash_password("same").unwrap(), hash_password("same").unwrap());
    }

    #[test]
    fn garbage_hash_is_false() {
        assert!(!verify_password("x", "not-a-hash"));
    }
}
```

- [ ] **Step 2: 寫 `api/src/auth/mod.rs`**

```rust
pub mod password;
```

- [ ] **Step 3: 寫 `api/src/domain/users.rs`，並在 `api/src/domain/mod.rs` 加 `pub mod users;`**

```rust
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

pub const ROLE_CUSTOMER: &str = "customer";
pub const ROLE_ADMIN: &str = "admin";

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    pub password_hash: String,
    pub name: String,
    pub phone: Option<String>,
    pub role: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 給前端看的使用者資料（沒有 password_hash）
#[derive(Debug, Clone, Serialize)]
pub struct UserPublic {
    pub id: Uuid,
    pub email: String,
    pub name: String,
    pub phone: Option<String>,
    pub role: String,
}

impl User {
    pub fn is_admin(&self) -> bool {
        self.role == ROLE_ADMIN
    }

    pub fn public(&self) -> UserPublic {
        UserPublic {
            id: self.id,
            email: self.email.clone(),
            name: self.name.clone(),
            phone: self.phone.clone(),
            role: self.role.clone(),
        }
    }
}

/// Email 一律去頭尾空白、轉小寫後再存、再查
pub fn normalize_email(email: &str) -> String {
    email.trim().to_lowercase()
}

/// 很寬鬆的格式檢查：有一個 @，兩邊都有東西，domain 裡有 .，沒有空白
pub fn is_valid_email(email: &str) -> bool {
    let Some((local, domain)) = email.split_once('@') else { return false };
    !local.is_empty()
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && !email.contains(char::is_whitespace)
        && !domain.contains('@')
}

pub async fn find_by_email(db: &PgPool, email: &str) -> Result<Option<User>, sqlx::Error> {
    sqlx::query_as::<_, User>("SELECT * FROM users WHERE lower(email) = $1")
        .bind(normalize_email(email))
        .fetch_optional(db)
        .await
}

pub async fn find_by_id(db: &PgPool, id: Uuid) -> Result<Option<User>, sqlx::Error> {
    sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = $1").bind(id).fetch_optional(db).await
}

pub async fn create(
    db: &PgPool,
    email: &str,
    password_hash: &str,
    name: &str,
    role: &str,
) -> Result<User, sqlx::Error> {
    sqlx::query_as::<_, User>(
        "INSERT INTO users (id, email, password_hash, name, role) VALUES ($1, $2, $3, $4, $5) RETURNING *",
    )
    .bind(Uuid::now_v7())
    .bind(normalize_email(email))
    .bind(password_hash)
    .bind(name)
    .bind(role)
    .fetch_one(db)
    .await
}

pub async fn set_password_and_role(
    db: &PgPool,
    id: Uuid,
    password_hash: &str,
    role: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE users SET password_hash = $2, role = $3, updated_at = now() WHERE id = $1")
        .bind(id)
        .bind(password_hash)
        .bind(role)
        .execute(db)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn email_rules() {
        assert!(is_valid_email("a@b.co"));
        assert!(!is_valid_email("a@b"));
        assert!(!is_valid_email("@b.co"));
        assert!(!is_valid_email("a b@c.co"));
        assert_eq!(normalize_email("  Boss@Example.COM "), "boss@example.com");
    }
}
```

- [ ] **Step 4: 寫 `api/src/cli.rs`**

```rust
use anyhow::Context;
use sqlx::PgPool;

use crate::auth::password::{MIN_PASSWORD_CHARS, hash_password};
use crate::domain::users::{self, ROLE_ADMIN, is_valid_email, normalize_email};

/// `api create-admin <email>`：互動輸入密碼兩次（規格 §11）。
/// 沒有終端機可以互動時（腳本、CI）可以改用環境變數 ADMIN_PASSWORD。
pub async fn create_admin_interactive(db: &PgPool, email: &str) -> anyhow::Result<()> {
    let password = match std::env::var("ADMIN_PASSWORD") {
        Ok(from_env) => from_env,
        Err(_) => {
            let password =
                rpassword::prompt_password(format!("{email} 的密碼（至少 {MIN_PASSWORD_CHARS} 碼）: "))?;
            let confirm = rpassword::prompt_password("再輸入一次: ")?;
            anyhow::ensure!(password == confirm, "兩次輸入的密碼不一樣");
            password
        }
    };
    let outcome = create_admin_with_password(db, email, &password).await?;
    println!("{outcome}");
    Ok(())
}

/// 建立 admin；email 已存在就把那個帳號升成 admin 並換密碼。回傳給人看的訊息。測試直接呼叫這個。
pub async fn create_admin_with_password(db: &PgPool, email: &str, password: &str) -> anyhow::Result<String> {
    let email = normalize_email(email);
    anyhow::ensure!(is_valid_email(&email), "Email 格式不正確：{email}");
    anyhow::ensure!(password.chars().count() >= MIN_PASSWORD_CHARS, "密碼至少 {MIN_PASSWORD_CHARS} 碼");
    let hash = hash_password(password)?;
    match users::find_by_email(db, &email).await.context("查詢使用者")? {
        Some(user) => {
            users::set_password_and_role(db, user.id, &hash, ROLE_ADMIN).await.context("更新使用者")?;
            Ok(format!("已把 {email} 設為管理員並更新密碼"))
        }
        None => {
            users::create(db, &email, &hash, "管理員", ROLE_ADMIN).await.context("建立使用者")?;
            Ok(format!("已建立管理員 {email}"))
        }
    }
}
```

- [ ] **Step 5: `api/src/lib.rs` 加 `pub mod auth;` 與 `pub mod cli;`（字母順序：app、auth、cli、config、db、…）**

- [ ] **Step 6: `api/src/main.rs` 加子指令處理**

在 `db::migrate(&db).await?;` 之後、建立 `state` 之前插入：

```rust
    // 子指令：cargo run -- create-admin <email>
    let mut args = std::env::args().skip(1);
    if let Some(command) = args.next() {
        return match command.as_str() {
            "create-admin" => {
                let email = args.next().context("用法：cargo run -- create-admin <email>")?;
                cli::create_admin_interactive(&db, &email).await
            }
            other => anyhow::bail!("未知指令：{other}（可用：create-admin <email>）"),
        };
    }
```

並把 `use dog_shop_api::db;` 改成 `use dog_shop_api::{cli, db};`，在檔案最上面加 `use anyhow::Context;`。

- [ ] **Step 7: 寫 `api/tests/create_admin.rs`**

```rust
mod common;

use dog_shop_api::{auth::password::verify_password, cli::create_admin_with_password, domain::users};
use sqlx::PgPool;

#[sqlx::test(migrations = "./migrations")]
async fn creates_then_promotes_same_email(pool: PgPool) {
    let msg = create_admin_with_password(&pool, " Boss@Example.com ", "password123").await.unwrap();
    assert!(msg.contains("已建立"), "{msg}");
    let user = users::find_by_email(&pool, "boss@example.com").await.unwrap().unwrap();
    assert_eq!(user.role, "admin");
    assert_eq!(user.email, "boss@example.com");
    assert!(user.is_admin());

    // 同一個 email 再跑一次：不會多一個人，密碼被換掉
    let msg = create_admin_with_password(&pool, "boss@example.com", "newpassword9").await.unwrap();
    assert!(msg.contains("設為管理員"), "{msg}");
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM users").fetch_one(&pool).await.unwrap();
    assert_eq!(count, 1);
    let user = users::find_by_email(&pool, "boss@example.com").await.unwrap().unwrap();
    assert!(verify_password("newpassword9", &user.password_hash));
    assert!(!verify_password("password123", &user.password_hash));
}

#[sqlx::test(migrations = "./migrations")]
async fn rejects_short_password_and_bad_email(pool: PgPool) {
    let err = create_admin_with_password(&pool, "a@b.co", "short").await.unwrap_err();
    assert!(err.to_string().contains("至少"), "{err}");
    let err = create_admin_with_password(&pool, "not-an-email", "password123").await.unwrap_err();
    assert!(err.to_string().contains("Email"), "{err}");
}
```

- [ ] **Step 8: 跑測試**

Run: `cd api && DATABASE_URL=postgres://dog_shop:dog_shop@localhost:5435/dog_shop cargo test`
Expected: 全部 `ok`，新增 `auth::password::tests::*`（3 個）、`domain::users::tests::email_rules`、`creates_then_promotes_same_email`、`rejects_short_password_and_bad_email`。

- [ ] **Step 9: 手動建一個開發用 admin（之後前端登入會用）**

Run: `cd api && ADMIN_PASSWORD=admin12345 cargo run -- create-admin admin@example.com`
Expected: 印出 `已建立管理員 admin@example.com`。（有終端機時不帶 `ADMIN_PASSWORD` 會改成互動輸入兩次。）

- [ ] **Step 10: 格式、lint、commit**

Run: `cd api && cargo fmt --all && cargo clippy --all-targets -- -D warnings`

```bash
git add api/src api/tests
git commit -m "feat(api): argon2id 密碼雜湊、users 存取、create-admin 指令"
```

---

### Task 5: Session、登入／登出／me、使用者擷取器、速率限制

**Files:**
- Create: `api/src/auth/cookie.rs`、`api/src/auth/session.rs`、`api/src/auth/extract.rs`、`api/src/routes/auth.rs`
- Modify: `api/src/auth/mod.rs`、`api/src/routes/mod.rs`、`api/src/app.rs`
- Modify: `api/tests/common/mod.rs`（加登入工具）
- Test: `api/tests/auth.rs`

**Interfaces:**
- Consumes: Task 4 的 `users`、`password`；Task 2 的 `AppJson`、`ApiError`。
- Produces:
  - `auth::cookie::{SESSION_COOKIE = "sid", get_cookie(&HeaderMap, name) -> Option<String>, session_cookie(sid: &str, secure: bool) -> String, clear_session_cookie(secure: bool) -> String}`
  - `auth::session::{create(db, user_id) -> Result<Uuid, sqlx::Error>, find_user(db, sid) -> Result<Option<User>, sqlx::Error>, delete(db, sid), delete_all_for_user(db, user_id), SESSION_DAYS = 30}`
  - `auth::extract::{CurrentUser(pub Option<User>), AuthUser(pub User), AdminUser(pub User)}`：handler 參數直接放，`AuthUser` 沒登入回 401，`AdminUser` 不是 admin 回 403
  - `POST /api/auth/login { email, password }` → 200 `{ user }` + `Set-Cookie: sid=...`；錯 → 401 `UNAUTHORIZED`
  - `POST /api/auth/logout` → 204 + 清 cookie
  - `GET /api/auth/me` → 200 `{ user }` 或 401
  - `/api/auth/*` 超過每分鐘 10 次 → 429 `RATE_LIMITED`
  - 測試工具：`common::{ADMIN_EMAIL, ADMIN_PASSWORD, create_admin(&pool), login(&app, email, password) -> String /* "sid=..." */, admin_cookie(&app, &pool) -> String, customer_cookie(&app, &pool) -> String}`

- [ ] **Step 1: 寫 `api/src/auth/cookie.rs`（含單元測試）**

```rust
use axum::http::{HeaderMap, header::COOKIE};

pub const SESSION_COOKIE: &str = "sid";
const SESSION_MAX_AGE_SECS: i64 = 30 * 24 * 60 * 60;

/// 從 Cookie header 取一個值（可能有多個 Cookie header、每個裡面用 ; 分隔）
pub fn get_cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get_all(COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|line| line.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(key, _)| *key == name)
        .map(|(_, value)| value.trim().to_string())
}

/// Set-Cookie 的值：HttpOnly、SameSite=Lax、Path=/、30 天；Secure 依設定（規格 §11）
pub fn session_cookie(sid: &str, secure: bool) -> String {
    format!(
        "{SESSION_COOKIE}={sid}; Path=/; HttpOnly; SameSite=Lax; Max-Age={SESSION_MAX_AGE_SECS}{}",
        if secure { "; Secure" } else { "" }
    )
}

/// 清掉 cookie（Max-Age=0）
pub fn clear_session_cookie(secure: bool) -> String {
    format!(
        "{SESSION_COOKIE}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0{}",
        if secure { "; Secure" } else { "" }
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn reads_named_cookie() {
        let mut headers = HeaderMap::new();
        headers.insert(COOKIE, HeaderValue::from_static("a=1; sid=abc-123; b=2"));
        assert_eq!(get_cookie(&headers, "sid").as_deref(), Some("abc-123"));
        assert_eq!(get_cookie(&headers, "zzz"), None);
    }

    #[test]
    fn cookie_attributes() {
        let cookie = session_cookie("x", true);
        for part in ["sid=x", "Path=/", "HttpOnly", "SameSite=Lax", "Max-Age=2592000", "Secure"] {
            assert!(cookie.contains(part), "{cookie} 缺 {part}");
        }
        assert!(!session_cookie("x", false).contains("Secure"));
        assert!(clear_session_cookie(false).contains("Max-Age=0"));
    }
}
```

- [ ] **Step 2: 寫 `api/src/auth/session.rs`**

```rust
use chrono::{Duration, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::users::User;

pub const SESSION_DAYS: i64 = 30;

/// 建一個 session，回 session id。用隨機的 UUID v4，不用可預測的 v7。
pub async fn create(db: &PgPool, user_id: Uuid) -> Result<Uuid, sqlx::Error> {
    let sid = Uuid::new_v4();
    sqlx::query("INSERT INTO sessions (id, user_id, expires_at) VALUES ($1, $2, $3)")
        .bind(sid)
        .bind(user_id)
        .bind(Utc::now() + Duration::days(SESSION_DAYS))
        .execute(db)
        .await?;
    Ok(sid)
}

/// 用 session id 找還沒過期的使用者
pub async fn find_user(db: &PgPool, sid: Uuid) -> Result<Option<User>, sqlx::Error> {
    sqlx::query_as::<_, User>(
        "SELECT u.* FROM sessions s JOIN users u ON u.id = s.user_id WHERE s.id = $1 AND s.expires_at > now()",
    )
    .bind(sid)
    .fetch_optional(db)
    .await
}

pub async fn delete(db: &PgPool, sid: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM sessions WHERE id = $1").bind(sid).execute(db).await?;
    Ok(())
}

/// 使用者所有 session 全部登出（重設密碼時用；計畫 2）
pub async fn delete_all_for_user(db: &PgPool, user_id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM sessions WHERE user_id = $1").bind(user_id).execute(db).await?;
    Ok(())
}
```

- [ ] **Step 3: 寫 `api/src/auth/extract.rs`**

```rust
use axum::{extract::FromRequestParts, http::request::Parts};
use uuid::Uuid;

use crate::{
    auth::{cookie, session},
    domain::users::User,
    error::ApiError,
    state::AppState,
};

/// 有登入就是 Some(user)，沒登入是 None；只在資料庫壞掉時才會失敗
pub struct CurrentUser(pub Option<User>);

/// 一定要登入，否則 401
pub struct AuthUser(pub User);

/// 一定要是 admin：沒登入 401、不是 admin 403
pub struct AdminUser(pub User);

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let Some(raw) = cookie::get_cookie(&parts.headers, cookie::SESSION_COOKIE) else {
            return Ok(Self(None));
        };
        let Ok(sid) = Uuid::parse_str(&raw) else { return Ok(Self(None)) };
        let user = session::find_user(&state.db, sid).await?;
        Ok(Self(user))
    }
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        match CurrentUser::from_request_parts(parts, state).await?.0 {
            Some(user) => Ok(Self(user)),
            None => Err(ApiError::Unauthorized("請先登入")),
        }
    }
}

impl FromRequestParts<AppState> for AdminUser {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let AuthUser(user) = AuthUser::from_request_parts(parts, state).await?;
        if user.is_admin() { Ok(Self(user)) } else { Err(ApiError::Forbidden("需要管理員權限")) }
    }
}
```

- [ ] **Step 4: `api/src/auth/mod.rs` 改成**

```rust
pub mod cookie;
pub mod extract;
pub mod password;
pub mod session;
```

- [ ] **Step 5: 寫 `api/src/routes/auth.rs`，並在 `api/src/routes/mod.rs` 加 `pub mod auth;`**

```rust
use std::{sync::Arc, time::Duration};

use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode, header::SET_COOKIE},
    response::{AppendHeaders, IntoResponse},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};
use tower_governor::{
    GovernorError, GovernorLayer, governor::GovernorConfigBuilder, key_extractor::SmartIpKeyExtractor,
};
use uuid::Uuid;

use crate::{
    auth::{
        cookie::{self, SESSION_COOKIE},
        extract::AuthUser,
        password::verify_password_async,
        session,
    },
    domain::users,
    error::{ApiError, ApiResult},
    extract::AppJson,
    state::AppState,
};

/// /api/auth/* 每個 IP 每分鐘 10 次（規格 §11）：burst 10，每 6 秒補 1 個。
/// key 用 SmartIpKeyExtractor：先看 X-Forwarded-For / X-Real-IP（正式環境前面是 Caddy），沒有才用連線 IP。
pub fn router() -> Router<AppState> {
    let governor_conf = Arc::new(
        GovernorConfigBuilder::default()
            .key_extractor(SmartIpKeyExtractor)
            .per_second(6)
            .burst_size(10)
            .error_handler(|err| match err {
                GovernorError::TooManyRequests { .. } => ApiError::RateLimited.into_response(),
                other => ApiError::Internal(anyhow::anyhow!("rate limiter: {other:?}")).into_response(),
            })
            .finish()
            .expect("governor config"),
    );
    // 定期清掉沒在用的 IP 記錄，不然記憶體只會長
    let limiter = governor_conf.limiter().clone();
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_secs(60));
            limiter.retain_recent();
        }
    });

    Router::new()
        .route("/api/auth/login", post(login))
        .route("/api/auth/logout", post(logout))
        .route("/api/auth/me", get(me))
        .layer(GovernorLayer::new(governor_conf))
}

#[derive(Deserialize)]
pub struct LoginBody {
    pub email: String,
    pub password: String,
}

async fn login(
    State(state): State<AppState>,
    AppJson(body): AppJson<LoginBody>,
) -> ApiResult<impl IntoResponse> {
    // 帳號不存在與密碼錯誤回同一個錯，不透露帳號存不存在
    let Some(user) = users::find_by_email(&state.db, &body.email).await? else {
        return Err(ApiError::Unauthorized("Email 或密碼錯誤"));
    };
    if !verify_password_async(body.password, user.password_hash.clone()).await? {
        return Err(ApiError::Unauthorized("Email 或密碼錯誤"));
    }
    let sid = session::create(&state.db, user.id).await?;
    let cookie = cookie::session_cookie(&sid.to_string(), state.config.cookie_secure);
    Ok((StatusCode::OK, AppendHeaders([(SET_COOKIE, cookie)]), Json(json!({ "user": user.public() }))))
}

async fn logout(State(state): State<AppState>, headers: HeaderMap) -> ApiResult<impl IntoResponse> {
    if let Some(sid) = cookie::get_cookie(&headers, SESSION_COOKIE).and_then(|raw| Uuid::parse_str(&raw).ok()) {
        session::delete(&state.db, sid).await?;
    }
    let cookie = cookie::clear_session_cookie(state.config.cookie_secure);
    Ok((StatusCode::NO_CONTENT, AppendHeaders([(SET_COOKIE, cookie)])))
}

async fn me(AuthUser(user): AuthUser) -> Json<Value> {
    Json(json!({ "user": user.public() }))
}
```

- [ ] **Step 6: `api/src/app.rs` 在 `.merge(routes::settings::router())` 下面加**

```rust
        .merge(routes::auth::router())
```

- [ ] **Step 7: 在 `api/tests/common/mod.rs` 尾端加登入工具（並在最上面的 use 加 `use serde_json::json;`）**

```rust
pub const ADMIN_EMAIL: &str = "admin@test.local";
pub const ADMIN_PASSWORD: &str = "password123";

pub async fn create_admin(pool: &PgPool) {
    dog_shop_api::cli::create_admin_with_password(pool, ADMIN_EMAIL, ADMIN_PASSWORD).await.unwrap();
}

/// 登入並回傳 "sid=<uuid>"，之後直接放進 Cookie header
pub async fn login(app: &Router, email: &str, password: &str) -> String {
    let (status, body, headers) = send(
        app,
        req("POST", "/api/auth/login", None, Some(json!({ "email": email, "password": password }))),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "login failed: {body}");
    let set_cookie = headers.get(header::SET_COOKIE).expect("set-cookie").to_str().unwrap();
    set_cookie.split(';').next().unwrap().to_string()
}

/// 建 admin 並登入，回 cookie
pub async fn admin_cookie(app: &Router, pool: &PgPool) -> String {
    create_admin(pool).await;
    login(app, ADMIN_EMAIL, ADMIN_PASSWORD).await
}

/// 建一個一般會員並登入（還沒有註冊 API，直接寫 DB）
pub async fn customer_cookie(app: &Router, pool: &PgPool) -> String {
    let hash = dog_shop_api::auth::password::hash_password("password123").unwrap();
    dog_shop_api::domain::users::create(pool, "user@test.local", &hash, "小明", "customer").await.unwrap();
    login(app, "user@test.local", "password123").await
}
```

- [ ] **Step 8: 寫 `api/tests/auth.rs`**

```rust
mod common;

use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

#[sqlx::test(migrations = "./migrations")]
async fn login_me_logout(pool: PgPool) {
    let app = common::app(pool.clone());
    common::create_admin(&pool).await;

    // 沒登入 → 401
    let (status, body, _) = common::send(&app, common::req("GET", "/api/auth/me", None, None)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"]["code"], "UNAUTHORIZED");

    // 登入：Email 大小寫、空白都可以
    let (status, body, headers) = common::send(
        &app,
        common::req(
            "POST",
            "/api/auth/login",
            None,
            Some(json!({ "email": " Admin@Test.local ", "password": common::ADMIN_PASSWORD })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["user"]["email"], common::ADMIN_EMAIL);
    assert_eq!(body["user"]["role"], "admin");
    assert!(body["user"].get("password_hash").is_none());
    let set_cookie = headers.get("set-cookie").unwrap().to_str().unwrap();
    assert!(set_cookie.starts_with("sid="));
    assert!(set_cookie.contains("HttpOnly") && set_cookie.contains("SameSite=Lax"));
    assert!(!set_cookie.contains("Secure"), "測試設定 cookie_secure=false");
    let cookie = set_cookie.split(';').next().unwrap().to_string();

    // me
    let (status, body, _) = common::send(&app, common::req("GET", "/api/auth/me", Some(&cookie), None)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["user"]["email"], common::ADMIN_EMAIL);

    // logout → 204 並清 cookie；之後 me 又是 401
    let (status, _, headers) = common::send(&app, common::req("POST", "/api/auth/logout", Some(&cookie), None)).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(headers.get("set-cookie").unwrap().to_str().unwrap().contains("Max-Age=0"));
    let (status, _, _) = common::send(&app, common::req("GET", "/api/auth/me", Some(&cookie), None)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "./migrations")]
async fn wrong_password_and_unknown_email_are_401(pool: PgPool) {
    let app = common::app(pool.clone());
    common::create_admin(&pool).await;
    let (status, body, _) = common::send(
        &app,
        common::req("POST", "/api/auth/login", None, Some(json!({ "email": common::ADMIN_EMAIL, "password": "nope-nope" }))),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"]["code"], "UNAUTHORIZED");
    let (status, _, _) = common::send(
        &app,
        common::req("POST", "/api/auth/login", None, Some(json!({ "email": "ghost@test.local", "password": "nope-nope" }))),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "./migrations")]
async fn missing_field_is_validation_error(pool: PgPool) {
    let app = common::app(pool);
    let (status, body, _) =
        common::send(&app, common::req("POST", "/api/auth/login", None, Some(json!({ "email": "x" })))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "VALIDATION");
}

#[sqlx::test(migrations = "./migrations")]
async fn auth_routes_are_rate_limited(pool: PgPool) {
    let app = common::app(pool);
    // burst 10：前 10 次都會被處理（401），第 11 次 429
    for _ in 0..10 {
        let (status, _, _) = common::send(
            &app,
            common::req("POST", "/api/auth/login", None, Some(json!({ "email": "a@b.co", "password": "x" }))),
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }
    let (status, body, _) = common::send(
        &app,
        common::req("POST", "/api/auth/login", None, Some(json!({ "email": "a@b.co", "password": "x" }))),
    )
    .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(body["error"]["code"], "RATE_LIMITED");
}
```

- [ ] **Step 9: 跑測試**

Run: `cd api && DATABASE_URL=postgres://dog_shop:dog_shop@localhost:5435/dog_shop cargo test`
Expected: 全部 `ok`，新增 `auth::cookie::tests::*`（2 個）與 `tests/auth.rs` 的 4 個。
若 `GovernorLayer::new(governor_conf)` 編譯錯誤說型別不對：把 `Arc::new(...)` 拿掉、直接傳 `GovernorConfigBuilder...finish().expect(...)` 的值，並在 `.limiter().clone()` 之後才移進 `new`。
若 `error_handler` 抱怨回傳的 Response body 型別不合：把兩個 `.into_response()` 改成 `.into_response().map(axum::body::Body::new)`。

- [ ] **Step 10: 格式、lint、commit**

Run: `cd api && cargo fmt --all && cargo clippy --all-targets -- -D warnings`

```bash
git add api/src api/tests
git commit -m "feat(api): session cookie、登入登出 me、使用者擷取器、auth 速率限制"
```

---

### Task 6: CSRF 中介層（X-Requested-With + Origin）

**Files:**
- Create: `api/src/auth/csrf.rs`
- Modify: `api/src/auth/mod.rs`、`api/src/app.rs`
- Test: `api/tests/csrf.rs`

**Interfaces:**
- Consumes: `Config::public_origin()`、`ApiError::Forbidden`。
- Produces: `auth::csrf::require_same_origin` 中介層：POST/PUT/PATCH/DELETE 且路徑不在 `/api/ecpay/` 底下時，缺 `X-Requested-With: fetch` 或 `Origin` 不等於 `PUBLIC_BASE_URL` 的 origin → 403 `FORBIDDEN`。GET 不受影響。

- [ ] **Step 1: 寫 `api/src/auth/csrf.rs`**

```rust
use axum::{
    extract::{Request, State},
    http::{Method, header::ORIGIN},
    middleware::Next,
    response::{IntoResponse, Response},
};

use crate::{error::ApiError, state::AppState};

/// 綠界伺服器回呼與門市回傳走這些前綴，帶不了我們的 header（路由本身在計畫 3、4 才會加）
const EXEMPT_PREFIXES: &[&str] = &["/api/ecpay/"];

/// 會改資料的請求（POST/PUT/PATCH/DELETE）必須（規格 §11）：
/// 1. 帶 `X-Requested-With: fetch`（跨站的 <form> 送不出自訂 header）
/// 2. 如果有 Origin header，必須等於 PUBLIC_BASE_URL 的 origin
pub async fn require_same_origin(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let mutating = matches!(*request.method(), Method::POST | Method::PUT | Method::PATCH | Method::DELETE);
    let exempt = EXEMPT_PREFIXES.iter().any(|prefix| request.uri().path().starts_with(prefix));
    if mutating && !exempt {
        let has_marker =
            request.headers().get("x-requested-with").and_then(|v| v.to_str().ok()) == Some("fetch");
        if !has_marker {
            return ApiError::Forbidden("缺少 X-Requested-With header").into_response();
        }
        if let Some(origin) = request.headers().get(ORIGIN).and_then(|v| v.to_str().ok())
            && origin.trim_end_matches('/') != state.config.public_origin()
        {
            return ApiError::Forbidden("Origin 不符").into_response();
        }
    }
    next.run(request).await
}
```

- [ ] **Step 2: `api/src/auth/mod.rs` 加 `pub mod csrf;`（放在 cookie 後面）**

- [ ] **Step 3: `api/src/app.rs`：在所有 `.merge(...)` 之後、`.layer(TraceLayer::new_for_http())` 之前加**

```rust
        .layer(axum::middleware::from_fn_with_state(state.clone(), auth::csrf::require_same_origin))
```

並把 `use crate::{routes, state::AppState};` 改成 `use crate::{auth, routes, state::AppState};`。

- [ ] **Step 4: 寫 `api/tests/csrf.rs`**

```rust
mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use serde_json::json;
use sqlx::PgPool;

fn login_body() -> Body {
    Body::from(json!({ "email": "a@b.co", "password": "x" }).to_string())
}

#[sqlx::test(migrations = "./migrations")]
async fn post_without_marker_is_forbidden(pool: PgPool) {
    let app = common::app(pool);
    let request = Request::builder()
        .method("POST")
        .uri("/api/auth/login")
        .header(header::CONTENT_TYPE, "application/json")
        .header("x-forwarded-for", "127.0.0.1")
        .body(login_body())
        .unwrap();
    let (status, body, _) = common::send(&app, request).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"]["code"], "FORBIDDEN");
}

#[sqlx::test(migrations = "./migrations")]
async fn post_from_other_origin_is_forbidden(pool: PgPool) {
    let app = common::app(pool);
    let request = Request::builder()
        .method("POST")
        .uri("/api/auth/login")
        .header(header::CONTENT_TYPE, "application/json")
        .header("x-requested-with", "fetch")
        .header(header::ORIGIN, "https://evil.example")
        .header("x-forwarded-for", "127.0.0.1")
        .body(login_body())
        .unwrap();
    let (status, _, _) = common::send(&app, request).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = "./migrations")]
async fn post_with_marker_and_same_origin_passes_through(pool: PgPool) {
    let app = common::app(pool);
    // common::req 會帶正確的 header；沒有這個帳號所以是 401，而不是 403
    let (status, _, _) = common::send(
        &app,
        common::req("POST", "/api/auth/login", None, Some(json!({ "email": "a@b.co", "password": "x" }))),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "./migrations")]
async fn get_without_marker_is_fine(pool: PgPool) {
    let app = common::app(pool);
    let request = Request::builder().method("GET").uri("/api/health").body(Body::empty()).unwrap();
    let (status, _, _) = common::send(&app, request).await;
    assert_eq!(status, StatusCode::OK);
}
```

- [ ] **Step 5: 跑測試**

Run: `cd api && DATABASE_URL=postgres://dog_shop:dog_shop@localhost:5435/dog_shop cargo test`
Expected: 全部 `ok`，新增 `tests/csrf.rs` 的 4 個；既有的 auth 測試仍通過（`common::req` 已帶正確 header）。

- [ ] **Step 6: 格式、lint、commit**

Run: `cd api && cargo fmt --all && cargo clippy --all-targets -- -D warnings`

```bash
git add api/src api/tests
git commit -m "feat(api): CSRF 中介層（X-Requested-With 與 Origin 比對）"
```


---

### Task 7: 分類 API（公開列表 + 後台 CRUD）

**Files:**
- Create: `api/src/domain/categories.rs`、`api/src/routes/categories.rs`
- Modify: `api/src/domain/mod.rs`、`api/src/routes/mod.rs`、`api/src/app.rs`
- Test: `api/tests/categories.rs`

**Interfaces:**
- Consumes: `AdminUser`、`AppJson`、`AppPath`、`FieldErrors`、`common::admin_cookie / customer_cookie`。
- Produces:
  - `domain::categories::{Category { id, slug, name, sort_order }, is_valid_slug(&str) -> bool, random_slug() -> String, resolve_slug(Option<&str>) -> Result<String, ApiError>, list(db) -> Result<Vec<Category>, sqlx::Error>, create(db, slug: Option<&str>, name: &str, sort_order: i32) -> Result<Category, ApiError>, update(db, id, slug, name, sort_order) -> Result<Category, ApiError>, delete(db, id) -> Result<bool, sqlx::Error>}`（`is_valid_slug`、`random_slug` 之後商品也用）
  - `GET /api/categories` → `Category[]`（依 sort_order、name 排）
  - `GET /api/admin/categories`、`POST /api/admin/categories { name, slug?, sort_order? }` → 201、`PUT /api/admin/categories/{id}`、`DELETE /api/admin/categories/{id}` → 204；都要 admin

- [ ] **Step 1: 寫 `api/src/domain/categories.rs`（含單元測試）**

```rust
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::{ApiError, FieldErrors};

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Category {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub sort_order: i32,
}

/// slug 規則：小寫英數字與 -，1～60 字，頭尾不是 -
pub fn is_valid_slug(slug: &str) -> bool {
    (1..=60).contains(&slug.len())
        && slug.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !slug.starts_with('-')
        && !slug.ends_with('-')
}

/// 8 碼隨機小寫英數字（規格 §3：取 UUID v4 的前 8 個 hex 字元）
pub fn random_slug() -> String {
    Uuid::new_v4().simple().to_string()[..8].to_string()
}

/// 使用者有給 slug 就檢查格式，沒給（None 或空白）就隨機
pub fn resolve_slug(input: Option<&str>) -> Result<String, ApiError> {
    match input.map(str::trim).filter(|s| !s.is_empty()) {
        Some(slug) if is_valid_slug(slug) => Ok(slug.to_string()),
        Some(_) => Err(ApiError::field("slug", "網址代稱只能用小寫英文、數字和 -（1～60 字）")),
        None => Ok(random_slug()),
    }
}

fn validate_name(name: &str) -> Result<(), ApiError> {
    let mut errors = FieldErrors::new();
    let len = name.trim().chars().count();
    if len == 0 || len > 50 {
        errors.add("name", "必填，最多 50 字");
    }
    errors.into_result()
}

/// slug 撞到 unique index → 欄位錯誤；其他錯誤照原樣往上丟
fn map_slug_conflict<T>(result: Result<T, sqlx::Error>) -> Result<T, ApiError> {
    match result {
        Ok(value) => Ok(value),
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
            Err(ApiError::field("slug", "這個網址代稱已經有人用了"))
        }
        Err(e) => Err(e.into()),
    }
}

pub async fn list(db: &PgPool) -> Result<Vec<Category>, sqlx::Error> {
    sqlx::query_as::<_, Category>("SELECT id, slug, name, sort_order FROM categories ORDER BY sort_order, name")
        .fetch_all(db)
        .await
}

pub async fn create(db: &PgPool, slug: Option<&str>, name: &str, sort_order: i32) -> Result<Category, ApiError> {
    validate_name(name)?;
    let slug = resolve_slug(slug)?;
    let result = sqlx::query_as::<_, Category>(
        "INSERT INTO categories (id, slug, name, sort_order) VALUES ($1, $2, $3, $4) RETURNING id, slug, name, sort_order",
    )
    .bind(Uuid::now_v7())
    .bind(&slug)
    .bind(name.trim())
    .bind(sort_order)
    .fetch_one(db)
    .await;
    map_slug_conflict(result)
}

pub async fn update(
    db: &PgPool,
    id: Uuid,
    slug: Option<&str>,
    name: &str,
    sort_order: i32,
) -> Result<Category, ApiError> {
    validate_name(name)?;
    let slug = resolve_slug(slug)?;
    let result = sqlx::query_as::<_, Category>(
        "UPDATE categories SET slug = $2, name = $3, sort_order = $4 WHERE id = $1 RETURNING id, slug, name, sort_order",
    )
    .bind(id)
    .bind(&slug)
    .bind(name.trim())
    .bind(sort_order)
    .fetch_optional(db)
    .await;
    map_slug_conflict(result)?.ok_or(ApiError::NotFound)
}

/// 刪分類；商品的 category_id 會因 FK ON DELETE SET NULL 變成空。回 false 表示沒這個 id。
pub async fn delete(db: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM categories WHERE id = $1").bind(id).execute(db).await?;
    Ok(result.rows_affected() > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_rules() {
        assert!(is_valid_slug("dog-food"));
        assert!(is_valid_slug("a1"));
        assert!(!is_valid_slug(""));
        assert!(!is_valid_slug("Dog"));
        assert!(!is_valid_slug("-a"));
        assert!(!is_valid_slug("a b"));
        assert!(!is_valid_slug(&"x".repeat(61)));
    }

    #[test]
    fn random_slug_is_8_lowercase_alnum() {
        let slug = random_slug();
        assert_eq!(slug.len(), 8);
        assert!(is_valid_slug(&slug));
        assert_ne!(random_slug(), random_slug());
    }

    #[test]
    fn resolve_slug_cases() {
        assert_eq!(resolve_slug(Some(" food ")).unwrap(), "food");
        assert_eq!(resolve_slug(Some("   ")).unwrap().len(), 8);
        assert_eq!(resolve_slug(None).unwrap().len(), 8);
        assert!(resolve_slug(Some("Bad Slug")).is_err());
    }
}
```

- [ ] **Step 2: `api/src/domain/mod.rs` 加 `pub mod categories;`（放最前面，保持字母順序）**

- [ ] **Step 3: 寫 `api/src/routes/categories.rs`，並在 `api/src/routes/mod.rs` 加 `pub mod categories;`**

```rust
use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    routing::{get, put},
};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    auth::extract::AdminUser,
    domain::categories::{self, Category},
    error::{ApiError, ApiResult},
    extract::{AppJson, AppPath},
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/categories", get(list_public))
        .route("/api/admin/categories", get(list_admin).post(create))
        .route("/api/admin/categories/{id}", put(update).delete(remove))
}

#[derive(Deserialize)]
pub struct CategoryBody {
    pub name: String,
    pub slug: Option<String>,
    pub sort_order: Option<i32>,
}

async fn list_public(State(state): State<AppState>) -> ApiResult<Json<Vec<Category>>> {
    Ok(Json(categories::list(&state.db).await?))
}

async fn list_admin(_admin: AdminUser, State(state): State<AppState>) -> ApiResult<Json<Vec<Category>>> {
    Ok(Json(categories::list(&state.db).await?))
}

async fn create(
    _admin: AdminUser,
    State(state): State<AppState>,
    AppJson(body): AppJson<CategoryBody>,
) -> ApiResult<(StatusCode, Json<Category>)> {
    let category =
        categories::create(&state.db, body.slug.as_deref(), &body.name, body.sort_order.unwrap_or(0)).await?;
    Ok((StatusCode::CREATED, Json(category)))
}

async fn update(
    _admin: AdminUser,
    State(state): State<AppState>,
    AppPath(id): AppPath<Uuid>,
    AppJson(body): AppJson<CategoryBody>,
) -> ApiResult<Json<Category>> {
    let category =
        categories::update(&state.db, id, body.slug.as_deref(), &body.name, body.sort_order.unwrap_or(0)).await?;
    Ok(Json(category))
}

async fn remove(
    _admin: AdminUser,
    State(state): State<AppState>,
    AppPath(id): AppPath<Uuid>,
) -> ApiResult<StatusCode> {
    if categories::delete(&state.db, id).await? { Ok(StatusCode::NO_CONTENT) } else { Err(ApiError::NotFound) }
}
```

- [ ] **Step 4: `api/src/app.rs` 在 `.merge(routes::auth::router())` 下面加**

```rust
        .merge(routes::categories::router())
```

- [ ] **Step 5: 寫 `api/tests/categories.rs`**

```rust
mod common;

use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

#[sqlx::test(migrations = "./migrations")]
async fn admin_crud_and_public_list(pool: PgPool) {
    let app = common::app(pool.clone());
    let cookie = common::admin_cookie(&app, &pool).await;

    // 建立（slug 自動）
    let (status, body, _) = common::send(
        &app,
        common::req("POST", "/api/admin/categories", Some(&cookie), Some(json!({ "name": "飼料" }))),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["name"], "飼料");
    assert_eq!(body["slug"].as_str().unwrap().len(), 8);
    let id = body["id"].as_str().unwrap().to_string();

    // 建立（指定 slug 與排序）
    let (status, body, _) = common::send(
        &app,
        common::req(
            "POST",
            "/api/admin/categories",
            Some(&cookie),
            Some(json!({ "name": "零食", "slug": "snacks", "sort_order": 1 })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["slug"], "snacks");

    // slug 重複 → VALIDATION，欄位 slug
    let (status, body, _) = common::send(
        &app,
        common::req("POST", "/api/admin/categories", Some(&cookie), Some(json!({ "name": "別的", "slug": "snacks" }))),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "VALIDATION");
    assert!(body["error"]["details"]["fields"]["slug"].is_string());

    // 更新
    let (status, body, _) = common::send(
        &app,
        common::req(
            "PUT",
            &format!("/api/admin/categories/{id}"),
            Some(&cookie),
            Some(json!({ "name": "狗飼料", "slug": "food", "sort_order": 0 })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["name"], "狗飼料");
    assert_eq!(body["slug"], "food");

    // 公開列表（不用登入），依 sort_order 排
    let (status, body, _) = common::send(&app, common::req("GET", "/api/categories", None, None)).await;
    assert_eq!(status, StatusCode::OK);
    let items = body.as_array().unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["slug"], "food");
    assert_eq!(items[1]["slug"], "snacks");

    // 刪除；再刪一次是 404
    let (status, _, _) =
        common::send(&app, common::req("DELETE", &format!("/api/admin/categories/{id}"), Some(&cookie), None)).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _, _) =
        common::send(&app, common::req("DELETE", &format!("/api/admin/categories/{id}"), Some(&cookie), None)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "./migrations")]
async fn customer_and_guest_cannot_manage_categories(pool: PgPool) {
    let app = common::app(pool.clone());
    let cookie = common::customer_cookie(&app, &pool).await;
    let (status, body, _) = common::send(
        &app,
        common::req("POST", "/api/admin/categories", Some(&cookie), Some(json!({ "name": "x" }))),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(body["error"]["code"], "FORBIDDEN");
    let (status, _, _) =
        common::send(&app, common::req("POST", "/api/admin/categories", None, Some(json!({ "name": "x" })))).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "./migrations")]
async fn name_is_required(pool: PgPool) {
    let app = common::app(pool.clone());
    let cookie = common::admin_cookie(&app, &pool).await;
    let (status, body, _) = common::send(
        &app,
        common::req("POST", "/api/admin/categories", Some(&cookie), Some(json!({ "name": "   " }))),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["details"]["fields"]["name"], "必填，最多 50 字");
}

#[sqlx::test(migrations = "./migrations")]
async fn bad_uuid_in_path_is_validation_error(pool: PgPool) {
    let app = common::app(pool.clone());
    let cookie = common::admin_cookie(&app, &pool).await;
    let (status, body, _) =
        common::send(&app, common::req("DELETE", "/api/admin/categories/not-a-uuid", Some(&cookie), None)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "VALIDATION");
}
```

- [ ] **Step 6: 跑測試**

Run: `cd api && DATABASE_URL=postgres://dog_shop:dog_shop@localhost:5435/dog_shop cargo test`
Expected: 全部 `ok`，新增 `domain::categories::tests::*`（3 個）與 `tests/categories.rs` 的 4 個。

- [ ] **Step 7: 格式、lint、commit**

Run: `cd api && cargo fmt --all && cargo clippy --all-targets -- -D warnings`

```bash
git add api/src api/tests
git commit -m "feat(api): 分類公開列表與後台 CRUD"
```

---

### Task 8: 後台商品 API（建立、更新、查詢、封存、列表）

**Files:**
- Create: `api/src/domain/products.rs`、`api/src/routes/admin_products.rs`
- Modify: `api/src/domain/mod.rs`、`api/src/routes/mod.rs`、`api/src/app.rs`
- Test: `api/tests/admin_products.rs`

**Interfaces:**
- Consumes: `categories::{is_valid_slug, random_slug}`、`FieldErrors`、`AdminUser`、`AppJson/AppQuery/AppPath`。
- Produces（Task 10、13 會用）:
  - 型別：`ProductRow`、`VariantRow`、`ImageRow`、`AdminProduct { product(flatten), variants, images }`、`ProductInput`、`VariantInput`、`ImageInput`、`Page<T> { items, total, page, per_page }`、`AdminListItem`
  - 函式：`validate(&ProductInput) -> Result<(), ApiError>`、`create(db, ProductInput) -> Result<AdminProduct, ApiError>`、`update(db, id, ProductInput) -> Result<AdminProduct, ApiError>`、`get_admin(db, id) -> Result<Option<AdminProduct>, ApiError>`、`archive(db, id) -> Result<bool, ApiError>`、`list_admin(db, q: Option<&str>, status: Option<&str>, page: i64, per_page: i64) -> Result<Page<AdminListItem>, ApiError>`、`clamp_paging(page: Option<i64>, per_page: Option<i64>, default_per_page: i64, max_per_page: i64) -> (i64, i64)`、`clean(&Option<String>) -> Option<String>`、常數 `STATUS_DRAFT/STATUS_ACTIVE/STATUS_ARCHIVED`
  - 端點：`GET /api/admin/products?q&status&page&per_page`（預設 20、最大 100；沒給 status 時不含 archived）、`POST /api/admin/products` → 201 `AdminProduct`、`GET /api/admin/products/{id}`、`PUT /api/admin/products/{id}`、`DELETE /api/admin/products/{id}` → 204（status 變 archived）

- [ ] **Step 1: 寫 `api/src/domain/products.rs`（後台部分；公開查詢在 Task 10 加到同一檔）**

```rust
use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::domain::categories::{is_valid_slug, random_slug};
use crate::error::{ApiError, FieldErrors};

pub const STATUS_DRAFT: &str = "draft";
pub const STATUS_ACTIVE: &str = "active";
pub const STATUS_ARCHIVED: &str = "archived";
pub const MAX_IMAGES: usize = 9;
pub const MAX_VARIANTS: usize = 100;

// ───── 資料列 ─────

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ProductRow {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub description: String,
    pub category_id: Option<Uuid>,
    pub status: String,
    pub option1_name: Option<String>,
    pub option2_name: Option<String>,
    pub external_ref: Option<String>,
    pub sort_order: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct VariantRow {
    pub id: Uuid,
    pub product_id: Uuid,
    pub option1_value: Option<String>,
    pub option2_value: Option<String>,
    pub sku: Option<String>,
    pub price: i32,
    pub compare_at_price: Option<i32>,
    pub stock: i32,
    pub is_active: bool,
    pub image_id: Option<Uuid>,
    pub sort_order: i32,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ImageRow {
    pub id: Uuid,
    pub product_id: Uuid,
    pub path: String,
    pub thumb_path: String,
    pub alt: String,
    pub sort_order: i32,
}

#[derive(Debug, Serialize)]
pub struct AdminProduct {
    #[serde(flatten)]
    pub product: ProductRow,
    pub variants: Vec<VariantRow>,
    pub images: Vec<ImageRow>,
}

#[derive(Debug, Serialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub total: i64,
    pub page: i64,
    pub per_page: i64,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct AdminListItem {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub status: String,
    pub category_name: Option<String>,
    pub price_min: Option<i32>,
    pub price_max: Option<i32>,
    pub stock_total: i64,
    pub image_thumb: Option<String>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip)]
    pub total: i64,
}

// ───── 輸入 ─────

#[derive(Debug, Deserialize)]
pub struct ProductInput {
    pub name: String,
    pub slug: Option<String>,
    pub description: Option<String>,
    pub category_id: Option<Uuid>,
    pub status: String,
    pub option1_name: Option<String>,
    pub option2_name: Option<String>,
    pub sort_order: Option<i32>,
    #[serde(default)]
    pub variants: Vec<VariantInput>,
    #[serde(default)]
    pub images: Vec<ImageInput>,
}

#[derive(Debug, Deserialize)]
pub struct VariantInput {
    /// 有 id 且屬於這個商品 → 更新；否則新增
    pub id: Option<Uuid>,
    pub option1_value: Option<String>,
    pub option2_value: Option<String>,
    pub sku: Option<String>,
    pub price: i32,
    pub compare_at_price: Option<i32>,
    pub stock: i32,
    pub is_active: Option<bool>,
    /// 對應 images[].path；伺服器換成 image_id
    pub image_path: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ImageInput {
    pub path: String,
    pub thumb_path: String,
    pub alt: Option<String>,
}

/// 去頭尾空白；空字串當作沒填
pub fn clean(value: &Option<String>) -> Option<String> {
    value.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string)
}

/// page 至少 1；per_page 夾在 1..=max
pub fn clamp_paging(page: Option<i64>, per_page: Option<i64>, default_per_page: i64, max_per_page: i64) -> (i64, i64) {
    (page.unwrap_or(1).max(1), per_page.unwrap_or(default_per_page).clamp(1, max_per_page))
}

pub fn validate(input: &ProductInput) -> Result<(), ApiError> {
    let mut errors = FieldErrors::new();
    let name_len = input.name.trim().chars().count();
    if name_len == 0 || name_len > 120 {
        errors.add("name", "必填，最多 120 字");
    }
    if !matches!(input.status.as_str(), STATUS_DRAFT | STATUS_ACTIVE | STATUS_ARCHIVED) {
        errors.add("status", "狀態只能是 draft、active 或 archived");
    }
    if let Some(slug) = clean(&input.slug)
        && !is_valid_slug(&slug)
    {
        errors.add("slug", "網址代稱只能用小寫英文、數字和 -（1～60 字）");
    }
    if input.variants.is_empty() {
        errors.add("variants", "至少要有一個規格");
    }
    if input.variants.len() > MAX_VARIANTS {
        errors.add("variants", "規格最多 100 個");
    }
    if input.images.len() > MAX_IMAGES {
        errors.add("images", "圖片最多 9 張");
    }
    let has_opt1 = clean(&input.option1_name).is_some();
    let has_opt2 = clean(&input.option2_name).is_some();
    if has_opt2 && !has_opt1 {
        errors.add("option2_name", "要先有規格 1 才能有規格 2");
    }
    if !has_opt1 && input.variants.len() > 1 {
        errors.add("variants", "沒有規格名稱時只能有一個規格");
    }
    let mut seen: HashSet<(String, String)> = HashSet::new();
    for (i, v) in input.variants.iter().enumerate() {
        if v.price < 0 {
            errors.add(&format!("variants.{i}.price"), "價格不能是負數");
        }
        if v.stock < 0 {
            errors.add(&format!("variants.{i}.stock"), "庫存不能是負數");
        }
        if v.compare_at_price.is_some_and(|p| p < 0) {
            errors.add(&format!("variants.{i}.compare_at_price"), "原價不能是負數");
        }
        let v1 = clean(&v.option1_value);
        let v2 = clean(&v.option2_value);
        if has_opt1 && v1.is_none() {
            errors.add(&format!("variants.{i}.option1_value"), "必填");
        }
        if has_opt2 && v2.is_none() {
            errors.add(&format!("variants.{i}.option2_value"), "必填");
        }
        if !seen.insert((v1.unwrap_or_default(), v2.unwrap_or_default())) {
            errors.add(&format!("variants.{i}"), "規格重複");
        }
        if let Some(path) = v.image_path.as_deref()
            && !input.images.iter().any(|img| img.path == path)
        {
            errors.add(&format!("variants.{i}.image_path"), "圖片不在這個商品的圖片清單裡");
        }
    }
    for (i, img) in input.images.iter().enumerate() {
        if !img.path.starts_with("/uploads/") || !img.thumb_path.starts_with("/uploads/") {
            errors.add(&format!("images.{i}"), "圖片路徑不正確");
        }
    }
    errors.into_result()
}

/// 資料庫錯誤 → 我們的錯誤：slug 撞 unique、category 不存在（FK）；其他往上丟
fn map_product_db_error(err: sqlx::Error) -> ApiError {
    if let sqlx::Error::Database(e) = &err {
        if e.is_unique_violation() {
            return ApiError::field("slug", "這個網址代稱已經有人用了");
        }
        if e.is_foreign_key_violation() {
            return ApiError::field("category_id", "分類不存在");
        }
    }
    err.into()
}

pub async fn create(db: &PgPool, input: ProductInput) -> Result<AdminProduct, ApiError> {
    validate(&input)?;
    let id = Uuid::now_v7();
    let slug = clean(&input.slug).unwrap_or_else(random_slug);
    let mut tx = db.begin().await?;
    sqlx::query(
        "INSERT INTO products (id, slug, name, description, category_id, status, option1_name, option2_name, sort_order)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(id)
    .bind(&slug)
    .bind(input.name.trim())
    .bind(input.description.as_deref().unwrap_or(""))
    .bind(input.category_id)
    .bind(&input.status)
    .bind(clean(&input.option1_name))
    .bind(clean(&input.option2_name))
    .bind(input.sort_order.unwrap_or(0))
    .execute(&mut *tx)
    .await
    .map_err(map_product_db_error)?;
    write_images_and_variants(&mut tx, id, &input).await?;
    tx.commit().await?;
    get_admin(db, id).await?.ok_or(ApiError::NotFound)
}

pub async fn update(db: &PgPool, id: Uuid, input: ProductInput) -> Result<AdminProduct, ApiError> {
    validate(&input)?;
    let slug = clean(&input.slug).unwrap_or_else(random_slug);
    let mut tx = db.begin().await?;
    let result = sqlx::query(
        "UPDATE products SET slug = $2, name = $3, description = $4, category_id = $5, status = $6,
                option1_name = $7, option2_name = $8, sort_order = $9, updated_at = now()
         WHERE id = $1",
    )
    .bind(id)
    .bind(&slug)
    .bind(input.name.trim())
    .bind(input.description.as_deref().unwrap_or(""))
    .bind(input.category_id)
    .bind(&input.status)
    .bind(clean(&input.option1_name))
    .bind(clean(&input.option2_name))
    .bind(input.sort_order.unwrap_or(0))
    .execute(&mut *tx)
    .await
    .map_err(map_product_db_error)?;
    if result.rows_affected() == 0 {
        return Err(ApiError::NotFound);
    }
    write_images_and_variants(&mut tx, id, &input).await?;
    tx.commit().await?;
    get_admin(db, id).await?.ok_or(ApiError::NotFound)
}

/// 圖片整組重建；規格有 id 的更新、沒有的新增、沒出現的刪除。
/// 計畫 2 建了 order_items 之後，這裡的刪除要改成：有訂單引用就 is_active=false，沒有才刪。
async fn write_images_and_variants(
    tx: &mut Transaction<'_, Postgres>,
    product_id: Uuid,
    input: &ProductInput,
) -> Result<(), ApiError> {
    // 圖片：先清掉再依順序寫入（variants.image_id 會因 FK ON DELETE SET NULL 先變空，下面再補回）
    sqlx::query("DELETE FROM product_images WHERE product_id = $1").bind(product_id).execute(&mut **tx).await?;
    let mut image_id_by_path: HashMap<&str, Uuid> = HashMap::new();
    for (i, img) in input.images.iter().enumerate() {
        let image_id = Uuid::now_v7();
        sqlx::query(
            "INSERT INTO product_images (id, product_id, path, thumb_path, alt, sort_order) VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(image_id)
        .bind(product_id)
        .bind(&img.path)
        .bind(&img.thumb_path)
        .bind(img.alt.as_deref().unwrap_or(""))
        .bind(i as i32)
        .execute(&mut **tx)
        .await?;
        image_id_by_path.insert(img.path.as_str(), image_id);
    }

    // 規格
    let has_opt1 = clean(&input.option1_name).is_some();
    let has_opt2 = clean(&input.option2_name).is_some();
    let existing: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM product_variants WHERE product_id = $1")
        .bind(product_id)
        .fetch_all(&mut **tx)
        .await?;
    let mut kept: Vec<Uuid> = Vec::with_capacity(input.variants.len());
    for (i, v) in input.variants.iter().enumerate() {
        let image_id = v.image_path.as_deref().and_then(|p| image_id_by_path.get(p)).copied();
        let option1_value = if has_opt1 { clean(&v.option1_value) } else { None };
        let option2_value = if has_opt2 { clean(&v.option2_value) } else { None };
        let sku = clean(&v.sku);
        let is_active = v.is_active.unwrap_or(true);
        let variant_id = match v.id.filter(|id| existing.contains(id)) {
            Some(id) => {
                sqlx::query(
                    "UPDATE product_variants SET option1_value = $2, option2_value = $3, sku = $4, price = $5,
                            compare_at_price = $6, stock = $7, is_active = $8, image_id = $9, sort_order = $10
                     WHERE id = $1",
                )
                .bind(id)
                .bind(&option1_value)
                .bind(&option2_value)
                .bind(&sku)
                .bind(v.price)
                .bind(v.compare_at_price)
                .bind(v.stock)
                .bind(is_active)
                .bind(image_id)
                .bind(i as i32)
                .execute(&mut **tx)
                .await?;
                id
            }
            None => {
                let id = Uuid::now_v7();
                sqlx::query(
                    "INSERT INTO product_variants (id, product_id, option1_value, option2_value, sku, price,
                            compare_at_price, stock, is_active, image_id, sort_order)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
                )
                .bind(id)
                .bind(product_id)
                .bind(&option1_value)
                .bind(&option2_value)
                .bind(&sku)
                .bind(v.price)
                .bind(v.compare_at_price)
                .bind(v.stock)
                .bind(is_active)
                .bind(image_id)
                .bind(i as i32)
                .execute(&mut **tx)
                .await?;
                id
            }
        };
        kept.push(variant_id);
    }
    for old in existing.iter().filter(|id| !kept.contains(id)) {
        sqlx::query("DELETE FROM product_variants WHERE id = $1").bind(old).execute(&mut **tx).await?;
    }
    Ok(())
}

pub async fn get_admin(db: &PgPool, id: Uuid) -> Result<Option<AdminProduct>, ApiError> {
    let Some(product) =
        sqlx::query_as::<_, ProductRow>("SELECT * FROM products WHERE id = $1").bind(id).fetch_optional(db).await?
    else {
        return Ok(None);
    };
    let variants = sqlx::query_as::<_, VariantRow>(
        "SELECT * FROM product_variants WHERE product_id = $1 ORDER BY sort_order, id",
    )
    .bind(id)
    .fetch_all(db)
    .await?;
    let images =
        sqlx::query_as::<_, ImageRow>("SELECT * FROM product_images WHERE product_id = $1 ORDER BY sort_order, id")
            .bind(id)
            .fetch_all(db)
            .await?;
    Ok(Some(AdminProduct { product, variants, images }))
}

/// 刪除 = 封存（規格 §3）。回 false 表示沒這個商品。
pub async fn archive(db: &PgPool, id: Uuid) -> Result<bool, ApiError> {
    let result = sqlx::query("UPDATE products SET status = 'archived', updated_at = now() WHERE id = $1")
        .bind(id)
        .execute(db)
        .await?;
    Ok(result.rows_affected() > 0)
}

/// 後台列表。沒給 status 時不含 archived；q 比對名稱（不分大小寫、部分相符）。
pub async fn list_admin(
    db: &PgPool,
    q: Option<&str>,
    status: Option<&str>,
    page: i64,
    per_page: i64,
) -> Result<Page<AdminListItem>, ApiError> {
    let items = sqlx::query_as::<_, AdminListItem>(
        "SELECT p.id, p.slug, p.name, p.status, c.name AS category_name,
                v.price_min, v.price_max, v.stock_total,
                (SELECT i.thumb_path FROM product_images i WHERE i.product_id = p.id ORDER BY i.sort_order, i.id LIMIT 1) AS image_thumb,
                p.updated_at,
                COUNT(*) OVER () AS total
         FROM products p
         LEFT JOIN categories c ON c.id = p.category_id
         LEFT JOIN LATERAL (
            SELECT MIN(pv.price) AS price_min, MAX(pv.price) AS price_max, COALESCE(SUM(pv.stock), 0)::bigint AS stock_total
            FROM product_variants pv WHERE pv.product_id = p.id
         ) v ON true
         WHERE ($1::text IS NULL OR p.name ILIKE '%' || $1 || '%')
           AND ($2::text IS NULL OR p.status = $2)
           AND ($2::text IS NOT NULL OR p.status <> 'archived')
         ORDER BY p.updated_at DESC, p.id DESC
         LIMIT $3 OFFSET $4",
    )
    .bind(q)
    .bind(status)
    .bind(per_page)
    .bind((page - 1) * per_page)
    .fetch_all(db)
    .await?;
    let total = items.first().map(|i| i.total).unwrap_or(0);
    Ok(Page { items, total, page, per_page })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_input() -> ProductInput {
        ProductInput {
            name: "狗糧".to_string(),
            slug: None,
            description: None,
            category_id: None,
            status: STATUS_DRAFT.to_string(),
            option1_name: None,
            option2_name: None,
            sort_order: None,
            variants: vec![VariantInput {
                id: None,
                option1_value: None,
                option2_value: None,
                sku: None,
                price: 100,
                compare_at_price: None,
                stock: 1,
                is_active: None,
                image_path: None,
            }],
            images: vec![],
        }
    }

    fn fields(err: ApiError) -> serde_json::Value {
        match err {
            ApiError::Validation { details, .. } => details["fields"].clone(),
            other => panic!("expected validation error, got {other:?}"),
        }
    }

    #[test]
    fn minimal_product_is_valid() {
        assert!(validate(&base_input()).is_ok());
    }

    #[test]
    fn needs_name_status_and_a_variant() {
        let mut input = base_input();
        input.name = "  ".to_string();
        input.status = "gone".to_string();
        input.variants.clear();
        let f = fields(validate(&input).unwrap_err());
        assert!(f["name"].is_string());
        assert!(f["status"].is_string());
        assert!(f["variants"].is_string());
    }

    #[test]
    fn options_must_be_consistent() {
        let mut input = base_input();
        input.option2_name = Some("尺寸".to_string());
        let f = fields(validate(&input).unwrap_err());
        assert!(f["option2_name"].is_string());

        let mut input = base_input();
        input.option1_name = Some("口味".to_string());
        input.variants.push(VariantInput { option1_value: Some("雞".to_string()), ..base_input().variants.remove(0) });
        let f = fields(validate(&input).unwrap_err());
        assert_eq!(f["variants.0.option1_value"], "必填");
    }

    #[test]
    fn duplicate_variants_and_negative_numbers() {
        let mut input = base_input();
        input.option1_name = Some("口味".to_string());
        let first = VariantInput { option1_value: Some("雞".to_string()), price: -1, stock: -2, ..base_input().variants.remove(0) };
        let second = VariantInput { option1_value: Some("雞".to_string()), ..base_input().variants.remove(0) };
        input.variants = vec![first, second];
        let f = fields(validate(&input).unwrap_err());
        assert!(f["variants.0.price"].is_string());
        assert!(f["variants.0.stock"].is_string());
        assert_eq!(f["variants.1"], "規格重複");
    }

    #[test]
    fn image_rules() {
        let mut input = base_input();
        input.images = (0..10)
            .map(|i| ImageInput { path: format!("/uploads/a{i}.jpg"), thumb_path: format!("/uploads/a{i}_t.jpg"), alt: None })
            .collect();
        input.variants[0].image_path = Some("/uploads/nope.jpg".to_string());
        let f = fields(validate(&input).unwrap_err());
        assert_eq!(f["images"], "圖片最多 9 張");
        assert!(f["variants.0.image_path"].is_string());

        let mut input = base_input();
        input.images = vec![ImageInput { path: "http://evil/x.jpg".to_string(), thumb_path: "/uploads/t.jpg".to_string(), alt: None }];
        let f = fields(validate(&input).unwrap_err());
        assert_eq!(f["images.0"], "圖片路徑不正確");
    }

    #[test]
    fn paging_clamps() {
        assert_eq!(clamp_paging(None, None, 24, 60), (1, 24));
        assert_eq!(clamp_paging(Some(0), Some(999), 24, 60), (1, 60));
        assert_eq!(clamp_paging(Some(3), Some(0), 24, 60), (3, 1));
    }
}
```

- [ ] **Step 2: `api/src/domain/mod.rs` 加 `pub mod products;`（字母順序：categories、products、settings、users）**

- [ ] **Step 3: 寫 `api/src/routes/admin_products.rs`，並在 `api/src/routes/mod.rs` 加 `pub mod admin_products;`**

```rust
use axum::{Json, Router, extract::State, http::StatusCode, routing::get};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    auth::extract::AdminUser,
    domain::products::{self, AdminListItem, AdminProduct, Page, ProductInput},
    error::{ApiError, ApiResult},
    extract::{AppJson, AppPath, AppQuery},
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/admin/products", get(list).post(create))
        .route("/api/admin/products/{id}", get(get_one).put(update).delete(archive))
}

#[derive(Deserialize)]
pub struct AdminListQuery {
    pub q: Option<String>,
    pub status: Option<String>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

async fn list(
    _admin: AdminUser,
    State(state): State<AppState>,
    AppQuery(query): AppQuery<AdminListQuery>,
) -> ApiResult<Json<Page<AdminListItem>>> {
    let status = products::clean(&query.status);
    if let Some(s) = status.as_deref()
        && !matches!(s, products::STATUS_DRAFT | products::STATUS_ACTIVE | products::STATUS_ARCHIVED)
    {
        return Err(ApiError::field("status", "狀態只能是 draft、active 或 archived"));
    }
    let (page, per_page) = products::clamp_paging(query.page, query.per_page, 20, 100);
    let q = products::clean(&query.q);
    let result = products::list_admin(&state.db, q.as_deref(), status.as_deref(), page, per_page).await?;
    Ok(Json(result))
}

async fn create(
    _admin: AdminUser,
    State(state): State<AppState>,
    AppJson(input): AppJson<ProductInput>,
) -> ApiResult<(StatusCode, Json<AdminProduct>)> {
    let product = products::create(&state.db, input).await?;
    Ok((StatusCode::CREATED, Json(product)))
}

async fn get_one(
    _admin: AdminUser,
    State(state): State<AppState>,
    AppPath(id): AppPath<Uuid>,
) -> ApiResult<Json<AdminProduct>> {
    products::get_admin(&state.db, id).await?.map(Json).ok_or(ApiError::NotFound)
}

async fn update(
    _admin: AdminUser,
    State(state): State<AppState>,
    AppPath(id): AppPath<Uuid>,
    AppJson(input): AppJson<ProductInput>,
) -> ApiResult<Json<AdminProduct>> {
    Ok(Json(products::update(&state.db, id, input).await?))
}

async fn archive(
    _admin: AdminUser,
    State(state): State<AppState>,
    AppPath(id): AppPath<Uuid>,
) -> ApiResult<StatusCode> {
    if products::archive(&state.db, id).await? { Ok(StatusCode::NO_CONTENT) } else { Err(ApiError::NotFound) }
}
```

- [ ] **Step 4: `api/src/app.rs` 在 `.merge(routes::categories::router())` 下面加**

```rust
        .merge(routes::admin_products::router())
```

- [ ] **Step 5: 寫 `api/tests/admin_products.rs`**

```rust
mod common;

use axum::http::StatusCode;
use serde_json::{Value, json};
use sqlx::PgPool;

/// 兩層規格（口味 × 尺寸）、兩張圖的商品
fn sample_product(status: &str) -> Value {
    json!({
        "name": "雞肉狗糧",
        "description": "第一行\n第二行",
        "status": status,
        "option1_name": "口味",
        "option2_name": "尺寸",
        "images": [
            { "path": "/uploads/2026/09/a.jpg", "thumb_path": "/uploads/2026/09/a_thumb.jpg", "alt": "正面" },
            { "path": "/uploads/2026/09/b.jpg", "thumb_path": "/uploads/2026/09/b_thumb.jpg" }
        ],
        "variants": [
            { "option1_value": "雞肉", "option2_value": "S", "price": 300, "stock": 5, "sku": "CK-S", "image_path": "/uploads/2026/09/a.jpg" },
            { "option1_value": "雞肉", "option2_value": "L", "price": 500, "compare_at_price": 600, "stock": 0 },
            { "option1_value": "牛肉", "option2_value": "S", "price": 320, "stock": 2 },
            { "option1_value": "牛肉", "option2_value": "L", "price": 520, "stock": 1, "is_active": false }
        ]
    })
}

#[sqlx::test(migrations = "./migrations")]
async fn create_get_update_archive(pool: PgPool) {
    let app = common::app(pool.clone());
    let cookie = common::admin_cookie(&app, &pool).await;

    // 建立
    let (status, body, _) =
        common::send(&app, common::req("POST", "/api/admin/products", Some(&cookie), Some(sample_product("draft")))).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["name"], "雞肉狗糧");
    assert_eq!(body["status"], "draft");
    assert_eq!(body["slug"].as_str().unwrap().len(), 8);
    assert_eq!(body["variants"].as_array().unwrap().len(), 4);
    assert_eq!(body["images"].as_array().unwrap().len(), 2);
    assert_eq!(body["images"][0]["alt"], "正面");
    assert_eq!(body["images"][1]["sort_order"], 1);
    // 第一個規格的 image_id 指到第一張圖
    assert_eq!(body["variants"][0]["image_id"], body["images"][0]["id"]);
    assert!(body["variants"][1]["image_id"].is_null());
    assert_eq!(body["variants"][3]["is_active"], false);
    let id = body["id"].as_str().unwrap().to_string();
    let keep_variant_id = body["variants"][0]["id"].as_str().unwrap().to_string();

    // 取單筆
    let (status, body, _) =
        common::send(&app, common::req("GET", &format!("/api/admin/products/{id}"), Some(&cookie), None)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["id"], id);

    // 更新：改名、換 slug、上架、第一個規格改價、丟掉其他三個、新增一個、圖片只留第二張
    let update = json!({
        "name": "雞肉狗糧（新包裝）",
        "slug": "chicken-food",
        "status": "active",
        "option1_name": "口味",
        "option2_name": "尺寸",
        "images": [ { "path": "/uploads/2026/09/b.jpg", "thumb_path": "/uploads/2026/09/b_thumb.jpg" } ],
        "variants": [
            { "id": keep_variant_id, "option1_value": "雞肉", "option2_value": "S", "price": 310, "stock": 9, "image_path": "/uploads/2026/09/b.jpg" },
            { "option1_value": "鮭魚", "option2_value": "S", "price": 350, "stock": 3 }
        ]
    });
    let (status, body, _) =
        common::send(&app, common::req("PUT", &format!("/api/admin/products/{id}"), Some(&cookie), Some(update))).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["slug"], "chicken-food");
    assert_eq!(body["status"], "active");
    let variants = body["variants"].as_array().unwrap();
    assert_eq!(variants.len(), 2);
    assert_eq!(variants[0]["id"], keep_variant_id, "有 id 的規格要保留同一個 id");
    assert_eq!(variants[0]["price"], 310);
    assert_eq!(variants[0]["image_id"], body["images"][0]["id"]);
    assert_eq!(variants[1]["option1_value"], "鮭魚");
    assert_eq!(body["images"].as_array().unwrap().len(), 1);

    // 資料庫裡真的只剩 2 個規格
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM product_variants").fetch_one(&pool).await.unwrap();
    assert_eq!(count, 2);

    // 封存；再查 status 是 archived；封存不存在的 id 是 404
    let (status, _, _) =
        common::send(&app, common::req("DELETE", &format!("/api/admin/products/{id}"), Some(&cookie), None)).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, body, _) =
        common::send(&app, common::req("GET", &format!("/api/admin/products/{id}"), Some(&cookie), None)).await;
    assert_eq!(body["status"], "archived");
    let (status, _, _) = common::send(
        &app,
        common::req("DELETE", "/api/admin/products/018f0000-0000-7000-8000-000000000000", Some(&cookie), None),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "./migrations")]
async fn validation_and_conflicts(pool: PgPool) {
    let app = common::app(pool.clone());
    let cookie = common::admin_cookie(&app, &pool).await;

    // 沒有規格、名稱空白
    let (status, body, _) = common::send(
        &app,
        common::req("POST", "/api/admin/products", Some(&cookie), Some(json!({ "name": " ", "status": "draft", "variants": [] }))),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "VALIDATION");
    assert!(body["error"]["details"]["fields"]["name"].is_string());
    assert!(body["error"]["details"]["fields"]["variants"].is_string());

    // slug 重複
    let mut first = sample_product("draft");
    first["slug"] = json!("same-slug");
    let (status, _, _) = common::send(&app, common::req("POST", "/api/admin/products", Some(&cookie), Some(first.clone()))).await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, body, _) = common::send(&app, common::req("POST", "/api/admin/products", Some(&cookie), Some(first))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"]["details"]["fields"]["slug"].is_string());

    // 分類不存在
    let mut bad_cat = sample_product("draft");
    bad_cat["category_id"] = json!("018f0000-0000-7000-8000-000000000000");
    let (status, body, _) = common::send(&app, common::req("POST", "/api/admin/products", Some(&cookie), Some(bad_cat))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["error"]["details"]["fields"]["category_id"], "分類不存在");

    // 商品建好後分類不會有殘留：資料庫只有 1 個商品
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM products").fetch_one(&pool).await.unwrap();
    assert_eq!(count, 1);
}

#[sqlx::test(migrations = "./migrations")]
async fn list_filters_and_paging(pool: PgPool) {
    let app = common::app(pool.clone());
    let cookie = common::admin_cookie(&app, &pool).await;

    // 分類
    let (_, cat, _) = common::send(
        &app,
        common::req("POST", "/api/admin/categories", Some(&cookie), Some(json!({ "name": "飼料", "slug": "food" }))),
    )
    .await;

    for (name, status) in [("A 草稿", "draft"), ("B 上架", "active"), ("C 上架", "active"), ("D 封存", "archived")] {
        let mut p = sample_product(status);
        p["name"] = json!(name);
        p["category_id"] = cat["id"].clone();
        let (s, b, _) = common::send(&app, common::req("POST", "/api/admin/products", Some(&cookie), Some(p))).await;
        assert_eq!(s, StatusCode::CREATED, "{b}");
    }

    // 預設不含 archived
    let (status, body, _) = common::send(&app, common::req("GET", "/api/admin/products", Some(&cookie), None)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["total"], 3);
    assert_eq!(body["items"].as_array().unwrap().len(), 3);
    let item = &body["items"][0];
    assert_eq!(item["category_name"], "飼料");
    assert_eq!(item["price_min"], 300);
    assert_eq!(item["price_max"], 520);
    assert_eq!(item["stock_total"], 8);
    assert_eq!(item["image_thumb"], "/uploads/2026/09/a_thumb.jpg");

    // status=archived 只有 1
    let (_, body, _) = common::send(&app, common::req("GET", "/api/admin/products?status=archived", Some(&cookie), None)).await;
    assert_eq!(body["total"], 1);
    assert_eq!(body["items"][0]["name"], "D 封存");

    // q 不分大小寫
    let (_, body, _) = common::send(&app, common::req("GET", "/api/admin/products?q=%E4%B8%8A%E6%9E%B6", Some(&cookie), None)).await;
    assert_eq!(body["total"], 2);

    // 分頁
    let (_, body, _) = common::send(&app, common::req("GET", "/api/admin/products?per_page=2&page=2", Some(&cookie), None)).await;
    assert_eq!(body["total"], 3);
    assert_eq!(body["items"].as_array().unwrap().len(), 1);
    assert_eq!(body["page"], 2);
    assert_eq!(body["per_page"], 2);

    // 錯的 status
    let (status, _, _) = common::send(&app, common::req("GET", "/api/admin/products?status=weird", Some(&cookie), None)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "./migrations")]
async fn customer_is_forbidden(pool: PgPool) {
    let app = common::app(pool.clone());
    let cookie = common::customer_cookie(&app, &pool).await;
    let (status, _, _) =
        common::send(&app, common::req("POST", "/api/admin/products", Some(&cookie), Some(sample_product("draft")))).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _, _) = common::send(&app, common::req("GET", "/api/admin/products", Some(&cookie), None)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}
```

- [ ] **Step 6: 跑測試**

Run: `cd api && DATABASE_URL=postgres://dog_shop:dog_shop@localhost:5435/dog_shop cargo test`
Expected: 全部 `ok`，新增 `domain::products::tests::*`（6 個）與 `tests/admin_products.rs` 的 4 個。

- [ ] **Step 7: 格式、lint、commit**

Run: `cd api && cargo fmt --all && cargo clippy --all-targets -- -D warnings`

```bash
git add api/src api/tests
git commit -m "feat(api): 後台商品 CRUD（規格、圖片、封存、列表）"
```


---

### Task 9: 圖片上傳與靜態檔

**Files:**
- Create: `api/src/storage/mod.rs`、`api/src/routes/uploads.rs`
- Modify: `api/src/lib.rs`、`api/src/routes/mod.rs`、`api/src/app.rs`
- Test: `api/tests/uploads.rs`

**Interfaces:**
- Consumes: `AdminUser`、`Config.upload_dir`、`ApiError`。
- Produces:
  - `storage::{StoredImage { path, thumb_path, width, height }, StorageError { Decode, Io }, Encoded { main, thumb, width, height }, encode(&[u8]) -> Result<Encoded, image::ImageError>, save(upload_dir: &Path, bytes: Vec<u8>) -> Result<StoredImage, StorageError>, MAX_UPLOAD_BYTES, ALLOWED_MIME, MAIN_MAX_EDGE = 1600, THUMB_MAX_EDGE = 400}`
  - `POST /api/admin/uploads`（multipart 欄位名 `file`）→ 201 `StoredImage`；錯的 MIME、太大、解不開 → 400 `VALIDATION` 欄位 `file`
  - `GET /uploads/<yyyy>/<mm>/<uuid>.jpg` 由 API 直接提供，帶 `Cache-Control: public, max-age=31536000, immutable`

- [ ] **Step 1: 寫 `api/src/storage/mod.rs`（含單元測試）**

```rust
//! 圖片：重新解碼 → 縮圖 → JPEG → 存檔。不保留原檔（規格 §11）。
//! 輸出用 JPEG 而不是 WebP：image crate 的 WebP 只支援無損，商品照會太大（見計畫「與規格不同之處」1）。
use std::{
    io::Cursor,
    path::{Path, PathBuf},
};

use chrono::{Datelike, Utc};
use image::{DynamicImage, ImageError, codecs::jpeg::JpegEncoder, imageops::FilterType};
use serde::Serialize;
use uuid::Uuid;

pub const MAX_UPLOAD_BYTES: usize = 10 * 1024 * 1024;
pub const MAIN_MAX_EDGE: u32 = 1600;
pub const THUMB_MAX_EDGE: u32 = 400;
pub const ALLOWED_MIME: &[&str] = &["image/jpeg", "image/png", "image/webp", "image/gif"];

#[derive(Debug, Serialize)]
pub struct StoredImage {
    /// 網址路徑，例如 /uploads/2026/09/0192a1b2-....jpg
    pub path: String,
    pub thumb_path: String,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("圖片無法讀取")]
    Decode(#[from] ImageError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub struct Encoded {
    pub main: Vec<u8>,
    pub thumb: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// 解碼 → 主圖最長邊 1600（小圖不放大）→ 縮圖最長邊 400 → 兩張都轉 JPEG（去透明層）
pub fn encode(bytes: &[u8]) -> Result<Encoded, ImageError> {
    let decoded = image::load_from_memory(bytes)?;
    let main = if decoded.width().max(decoded.height()) > MAIN_MAX_EDGE {
        decoded.resize(MAIN_MAX_EDGE, MAIN_MAX_EDGE, FilterType::Lanczos3)
    } else {
        decoded
    };
    let thumb = main.resize(THUMB_MAX_EDGE, THUMB_MAX_EDGE, FilterType::Triangle);
    Ok(Encoded { width: main.width(), height: main.height(), main: to_jpeg(&main, 82)?, thumb: to_jpeg(&thumb, 80)? })
}

fn to_jpeg(img: &DynamicImage, quality: u8) -> Result<Vec<u8>, ImageError> {
    let mut buffer = Cursor::new(Vec::new());
    let encoder = JpegEncoder::new_with_quality(&mut buffer, quality);
    DynamicImage::ImageRgb8(img.to_rgb8()).write_with_encoder(encoder)?;
    Ok(buffer.into_inner())
}

/// 存到 {upload_dir}/yyyy/mm/{uuid}.jpg 與 {uuid}_thumb.jpg；回網址路徑
pub async fn save(upload_dir: &Path, bytes: Vec<u8>) -> Result<StoredImage, StorageError> {
    // 解碼與縮圖很吃 CPU，丟到 blocking thread
    let encoded = tokio::task::spawn_blocking(move || encode(&bytes)).await.map_err(std::io::Error::other)??;
    let now = Utc::now();
    let rel_dir = format!("{:04}/{:02}", now.year(), now.month());
    let id = Uuid::now_v7();
    let dir: PathBuf = upload_dir.join(&rel_dir);
    tokio::fs::create_dir_all(&dir).await?;
    tokio::fs::write(dir.join(format!("{id}.jpg")), &encoded.main).await?;
    tokio::fs::write(dir.join(format!("{id}_thumb.jpg")), &encoded.thumb).await?;
    Ok(StoredImage {
        path: format!("/uploads/{rel_dir}/{id}.jpg"),
        thumb_path: format!("/uploads/{rel_dir}/{id}_thumb.jpg"),
        width: encoded.width,
        height: encoded.height,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(width: u32, height: u32) -> Vec<u8> {
        let img = image::RgbaImage::from_pixel(width, height, image::Rgba([200, 30, 30, 128]));
        let mut buffer = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(img).write_to(&mut buffer, image::ImageFormat::Png).unwrap();
        buffer.into_inner()
    }

    #[test]
    fn big_image_is_resized_and_becomes_jpeg() {
        let encoded = encode(&png(3200, 1600)).unwrap();
        assert_eq!((encoded.width, encoded.height), (1600, 800));
        assert_eq!(&encoded.main[..2], &[0xFF, 0xD8], "JPEG 開頭是 FF D8");
        let thumb = image::load_from_memory(&encoded.thumb).unwrap();
        assert_eq!((thumb.width(), thumb.height()), (400, 200));
    }

    #[test]
    fn small_image_is_not_upscaled() {
        let encoded = encode(&png(300, 200)).unwrap();
        assert_eq!((encoded.width, encoded.height), (300, 200));
    }

    #[test]
    fn garbage_is_decode_error() {
        assert!(encode(b"not an image").is_err());
    }

    #[tokio::test]
    async fn save_writes_two_files_under_year_month() {
        let dir = std::env::temp_dir().join(format!("dog_shop_storage_{}", Uuid::new_v4()));
        let stored = save(&dir, png(50, 40)).await.unwrap();
        assert!(stored.path.starts_with("/uploads/"));
        assert!(stored.thumb_path.ends_with("_thumb.jpg"));
        let rel = stored.path.trim_start_matches("/uploads/");
        assert!(dir.join(rel).is_file());
        assert!(dir.join(stored.thumb_path.trim_start_matches("/uploads/")).is_file());
        let _ = std::fs::remove_dir_all(dir);
    }
}
```

- [ ] **Step 2: 寫 `api/src/routes/uploads.rs`，並在 `api/src/routes/mod.rs` 加 `pub mod uploads;`**

```rust
use axum::{
    Json, Router,
    extract::{Multipart, State},
    http::StatusCode,
    routing::post,
};
use serde_json::json;

use crate::{
    auth::extract::AdminUser,
    error::{ApiError, ApiResult},
    state::AppState,
    storage::{self, ALLOWED_MIME, MAX_UPLOAD_BYTES, StorageError, StoredImage},
};

pub fn router() -> Router<AppState> {
    Router::new().route("/api/admin/uploads", post(upload))
}

/// multipart，欄位名 `file`。只收 jpeg/png/webp/gif、≤ 10 MB（規格 §10、§11）
async fn upload(
    _admin: AdminUser,
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> ApiResult<(StatusCode, Json<StoredImage>)> {
    while let Some(field) = multipart.next_field().await.map_err(|e| ApiError::Validation {
        message: "上傳內容格式錯誤".to_string(),
        details: json!({ "detail": e.body_text() }),
    })? {
        if field.name() != Some("file") {
            continue;
        }
        let content_type = field.content_type().unwrap_or("").to_string();
        if !ALLOWED_MIME.contains(&content_type.as_str()) {
            return Err(ApiError::field("file", "只接受 JPEG、PNG、WebP、GIF"));
        }
        let bytes = field.bytes().await.map_err(|e| ApiError::Validation {
            message: "讀取上傳檔失敗".to_string(),
            details: json!({ "detail": e.body_text() }),
        })?;
        if bytes.len() > MAX_UPLOAD_BYTES {
            return Err(ApiError::field("file", "檔案不能超過 10 MB"));
        }
        let stored = storage::save(&state.config.upload_dir, bytes.to_vec()).await.map_err(|e| match e {
            StorageError::Decode(_) => ApiError::field("file", "圖片無法讀取"),
            StorageError::Io(io) => ApiError::Internal(io.into()),
        })?;
        return Ok((StatusCode::CREATED, Json(stored)));
    }
    Err(ApiError::field("file", "缺少 file 欄位"))
}
```

- [ ] **Step 3: `api/src/lib.rs` 加 `pub mod storage;`（放最後）**

- [ ] **Step 4: `api/src/app.rs` 加靜態檔服務**

把 use 區塊改成：

```rust
use axum::{
    Router,
    extract::DefaultBodyLimit,
    http::{HeaderValue, header::CACHE_CONTROL},
};
use tower::ServiceBuilder;
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::services::ServeDir;
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::trace::TraceLayer;

use crate::{auth, routes, state::AppState};
```

在 `pub fn router(state: AppState) -> Router {` 的第一行加：

```rust
    // 上傳的圖片直接由 api 提供；檔名含 uuid 所以可以長期快取（規格 §11）
    let uploads = ServiceBuilder::new()
        .layer(SetResponseHeaderLayer::overriding(
            CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=31536000, immutable"),
        ))
        .service(ServeDir::new(&state.config.upload_dir));
```

在 `.merge(routes::admin_products::router())` 下面加：

```rust
        .merge(routes::uploads::router())
        .nest_service("/uploads", uploads)
```

- [ ] **Step 5: 寫 `api/tests/uploads.rs`**

```rust
mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use sqlx::PgPool;

fn png(width: u32, height: u32) -> Vec<u8> {
    let img = image::RgbaImage::from_pixel(width, height, image::Rgba([10, 120, 200, 255]));
    let mut buffer = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(img).write_to(&mut buffer, image::ImageFormat::Png).unwrap();
    buffer.into_inner()
}

/// 手工組 multipart（沒有 reqwest）
fn multipart(cookie: &str, filename: &str, content_type: &str, data: &[u8]) -> Request<Body> {
    let boundary = "XxDogShopBoundaryxX";
    let mut body = Vec::new();
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\nContent-Type: {content_type}\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(data);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    Request::builder()
        .method("POST")
        .uri("/api/admin/uploads")
        .header(header::CONTENT_TYPE, format!("multipart/form-data; boundary={boundary}"))
        .header(header::ORIGIN, common::TEST_ORIGIN)
        .header("x-requested-with", "fetch")
        .header("x-forwarded-for", "127.0.0.1")
        .header(header::COOKIE, cookie)
        .body(Body::from(body))
        .unwrap()
}

#[sqlx::test(migrations = "./migrations")]
async fn upload_then_serve(pool: PgPool) {
    let app = common::app(pool.clone());
    let cookie = common::admin_cookie(&app, &pool).await;

    let (status, body, _) = common::send(&app, multipart(&cookie, "a.png", "image/png", &png(2000, 1000))).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["width"], 1600);
    assert_eq!(body["height"], 800);
    let path = body["path"].as_str().unwrap();
    let thumb = body["thumb_path"].as_str().unwrap();
    assert!(path.starts_with("/uploads/") && path.ends_with(".jpg"), "{path}");
    assert!(thumb.ends_with("_thumb.jpg"));

    // 靜態檔服務：兩張都拿得到，是 JPEG，帶長快取
    for p in [path, thumb] {
        let request = Request::builder().method("GET").uri(p).body(Body::empty()).unwrap();
        let response = tower::ServiceExt::oneshot(app.clone(), request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK, "{p}");
        assert_eq!(response.headers()[header::CONTENT_TYPE], "image/jpeg");
        assert_eq!(response.headers()[header::CACHE_CONTROL], "public, max-age=31536000, immutable");
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn rejects_wrong_type_and_garbage(pool: PgPool) {
    let app = common::app(pool.clone());
    let cookie = common::admin_cookie(&app, &pool).await;

    let (status, body, _) = common::send(&app, multipart(&cookie, "a.txt", "text/plain", b"hello")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["details"]["fields"]["file"], "只接受 JPEG、PNG、WebP、GIF");

    let (status, body, _) = common::send(&app, multipart(&cookie, "a.png", "image/png", b"not really a png")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["details"]["fields"]["file"], "圖片無法讀取");
}

#[sqlx::test(migrations = "./migrations")]
async fn customer_cannot_upload(pool: PgPool) {
    let app = common::app(pool.clone());
    let cookie = common::customer_cookie(&app, &pool).await;
    let (status, _, _) = common::send(&app, multipart(&cookie, "a.png", "image/png", &png(10, 10))).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}
```

- [ ] **Step 6: 跑測試**

Run: `cd api && DATABASE_URL=postgres://dog_shop:dog_shop@localhost:5435/dog_shop cargo test`
Expected: 全部 `ok`，新增 `storage::tests::*`（4 個）與 `tests/uploads.rs` 的 3 個。

- [ ] **Step 7: 格式、lint、commit**

Run: `cd api && cargo fmt --all && cargo clippy --all-targets -- -D warnings`

```bash
git add api/src api/tests
git commit -m "feat(api): 圖片上傳重編碼為 JPEG、靜態檔服務"
```

---

### Task 10: 公開商品 API（列表、商品頁）

**Files:**
- Modify: `api/src/domain/products.rs`（加公開查詢）
- Create: `api/src/routes/products.rs`
- Modify: `api/src/routes/mod.rs`、`api/src/app.rs`
- Test: `api/tests/products_public.rs`

**Interfaces:**
- Consumes: Task 8 的型別與 `clamp_paging`、`clean`；後台 API（測試用它建資料）。
- Produces:
  - `domain::products::{PublicListItem { slug, name, price_min, price_max, image_thumb, in_stock }, PublicProduct { id, slug, name, description, category: Option<CategoryRef>, option1_name, option2_name, images: Vec<PublicImage>, variants: Vec<PublicVariant> }, CategoryRef { slug, name }, PublicImage { path, thumb_path, alt }, PublicVariant { id, option1_value, option2_value, price, compare_at_price, stock, image_path }, SORT_OPTIONS, list_public(db, q, category_slug, sort, page, per_page) -> Result<Page<PublicListItem>, ApiError>, get_public(db, slug) -> Result<Option<PublicProduct>, ApiError>}`
  - `GET /api/products?q&category&sort&page&per_page`（sort 預設 newest；per_page 預設 24、最大 60；只回 active 且至少有一個啟用規格的商品）
  - `GET /api/products/{slug}` → `PublicProduct`（只回 active；規格只回 is_active）；沒有 → 404

- [ ] **Step 1: 在 `api/src/domain/products.rs` 的 `#[cfg(test)] mod tests` **之前**加入公開查詢**

```rust
// ───── 公開（買家）查詢 ─────

pub const SORT_OPTIONS: &[&str] = &["newest", "price_asc", "price_desc"];

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct PublicListItem {
    pub slug: String,
    pub name: String,
    pub price_min: i32,
    pub price_max: i32,
    pub image_thumb: Option<String>,
    pub in_stock: bool,
    #[serde(skip)]
    pub total: i64,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct CategoryRef {
    pub slug: String,
    pub name: String,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct PublicImage {
    pub path: String,
    pub thumb_path: String,
    pub alt: String,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct PublicVariant {
    pub id: Uuid,
    pub option1_value: Option<String>,
    pub option2_value: Option<String>,
    pub price: i32,
    pub compare_at_price: Option<i32>,
    pub stock: i32,
    pub image_path: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PublicProduct {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub description: String,
    pub category: Option<CategoryRef>,
    pub option1_name: Option<String>,
    pub option2_name: Option<String>,
    pub images: Vec<PublicImage>,
    pub variants: Vec<PublicVariant>,
}

/// 買家列表：只有 active 且至少一個啟用規格的商品。價格範圍與庫存只算啟用的規格。
/// sort 只接受 SORT_OPTIONS 裡的值（route 先驗證過），這裡用白名單組 ORDER BY。
pub async fn list_public(
    db: &PgPool,
    q: Option<&str>,
    category_slug: Option<&str>,
    sort: &str,
    page: i64,
    per_page: i64,
) -> Result<Page<PublicListItem>, ApiError> {
    let order = match sort {
        "price_asc" => "v.price_min ASC, p.created_at DESC",
        "price_desc" => "v.price_max DESC, p.created_at DESC",
        _ => "p.created_at DESC",
    };
    let sql = format!(
        "SELECT p.slug, p.name, v.price_min, v.price_max, (v.stock_total > 0) AS in_stock,
                (SELECT i.thumb_path FROM product_images i WHERE i.product_id = p.id ORDER BY i.sort_order, i.id LIMIT 1) AS image_thumb,
                COUNT(*) OVER () AS total
         FROM products p
         JOIN LATERAL (
            SELECT MIN(pv.price) AS price_min, MAX(pv.price) AS price_max, COALESCE(SUM(pv.stock), 0)::bigint AS stock_total
            FROM product_variants pv WHERE pv.product_id = p.id AND pv.is_active
         ) v ON true
         WHERE p.status = 'active'
           AND v.price_min IS NOT NULL
           AND ($1::text IS NULL OR p.name ILIKE '%' || $1 || '%')
           AND ($2::text IS NULL OR p.category_id = (SELECT c.id FROM categories c WHERE c.slug = $2))
         ORDER BY {order}, p.id DESC
         LIMIT $3 OFFSET $4"
    );
    let items = sqlx::query_as::<_, PublicListItem>(&sql)
        .bind(q)
        .bind(category_slug)
        .bind(per_page)
        .bind((page - 1) * per_page)
        .fetch_all(db)
        .await?;
    let total = items.first().map(|i| i.total).unwrap_or(0);
    Ok(Page { items, total, page, per_page })
}

/// 買家商品頁：只回 active 商品、啟用的規格；規格的 image_path 由 image_id 對出來
pub async fn get_public(db: &PgPool, slug: &str) -> Result<Option<PublicProduct>, ApiError> {
    let Some(p) = sqlx::query_as::<_, ProductRow>("SELECT * FROM products WHERE slug = $1 AND status = 'active'")
        .bind(slug)
        .fetch_optional(db)
        .await?
    else {
        return Ok(None);
    };
    let category = match p.category_id {
        Some(category_id) => {
            sqlx::query_as::<_, CategoryRef>("SELECT slug, name FROM categories WHERE id = $1")
                .bind(category_id)
                .fetch_optional(db)
                .await?
        }
        None => None,
    };
    let images = sqlx::query_as::<_, PublicImage>(
        "SELECT path, thumb_path, alt FROM product_images WHERE product_id = $1 ORDER BY sort_order, id",
    )
    .bind(p.id)
    .fetch_all(db)
    .await?;
    let variants = sqlx::query_as::<_, PublicVariant>(
        "SELECT v.id, v.option1_value, v.option2_value, v.price, v.compare_at_price, v.stock, i.path AS image_path
         FROM product_variants v
         LEFT JOIN product_images i ON i.id = v.image_id
         WHERE v.product_id = $1 AND v.is_active
         ORDER BY v.sort_order, v.id",
    )
    .bind(p.id)
    .fetch_all(db)
    .await?;
    Ok(Some(PublicProduct {
        id: p.id,
        slug: p.slug,
        name: p.name,
        description: p.description,
        category,
        option1_name: p.option1_name,
        option2_name: p.option2_name,
        images,
        variants,
    }))
}
```

- [ ] **Step 2: 寫 `api/src/routes/products.rs`，並在 `api/src/routes/mod.rs` 加 `pub mod products;`**

```rust
use axum::{Json, Router, extract::State, routing::get};
use serde::Deserialize;

use crate::{
    domain::products::{self, Page, PublicListItem, PublicProduct, SORT_OPTIONS},
    error::{ApiError, ApiResult},
    extract::{AppPath, AppQuery},
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new().route("/api/products", get(list)).route("/api/products/{slug}", get(detail))
}

#[derive(Deserialize)]
pub struct PublicListQuery {
    pub q: Option<String>,
    pub category: Option<String>,
    pub sort: Option<String>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

async fn list(
    State(state): State<AppState>,
    AppQuery(query): AppQuery<PublicListQuery>,
) -> ApiResult<Json<Page<PublicListItem>>> {
    let sort = products::clean(&query.sort).unwrap_or_else(|| "newest".to_string());
    if !SORT_OPTIONS.contains(&sort.as_str()) {
        return Err(ApiError::field("sort", "排序只能是 newest、price_asc 或 price_desc"));
    }
    let (page, per_page) = products::clamp_paging(query.page, query.per_page, 24, 60);
    let q = products::clean(&query.q);
    let category = products::clean(&query.category);
    let result = products::list_public(&state.db, q.as_deref(), category.as_deref(), &sort, page, per_page).await?;
    Ok(Json(result))
}

async fn detail(State(state): State<AppState>, AppPath(slug): AppPath<String>) -> ApiResult<Json<PublicProduct>> {
    products::get_public(&state.db, &slug).await?.map(Json).ok_or(ApiError::NotFound)
}
```

- [ ] **Step 3: `api/src/app.rs` 在 `.merge(routes::admin_products::router())` 下面加**

```rust
        .merge(routes::products::router())
```

- [ ] **Step 4: 寫 `api/tests/products_public.rs`**

```rust
mod common;

use axum::http::StatusCode;
use serde_json::{Value, json};
use sqlx::PgPool;

/// 用後台 API 建商品，回 slug
async fn seed(app: &axum::Router, cookie: &str, product: Value) -> String {
    let (status, body, _) = common::send(app, common::req("POST", "/api/admin/products", Some(cookie), Some(product))).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    body["slug"].as_str().unwrap().to_string()
}

fn one_variant(name: &str, slug: &str, status: &str, price: i32, stock: i32, category_id: Option<&Value>) -> Value {
    json!({
        "name": name, "slug": slug, "status": status, "category_id": category_id,
        "images": [ { "path": format!("/uploads/2026/09/{slug}.jpg"), "thumb_path": format!("/uploads/2026/09/{slug}_thumb.jpg") } ],
        "variants": [ { "price": price, "stock": stock } ]
    })
}

async fn setup(pool: &PgPool) -> (axum::Router, String) {
    let app = common::app(pool.clone());
    let cookie = common::admin_cookie(&app, pool).await;
    let (_, cat, _) = common::send(
        &app,
        common::req("POST", "/api/admin/categories", Some(&cookie), Some(json!({ "name": "飼料", "slug": "food" }))),
    )
    .await;

    // 兩層規格、價格 100～300、庫存全 0、有一個停用的規格（價 999，不該算進範圍）
    seed(&app, &cookie, json!({
        "name": "綜合狗糧", "slug": "mix", "status": "active", "category_id": cat["id"],
        "option1_name": "口味",
        "images": [ { "path": "/uploads/2026/09/mix.jpg", "thumb_path": "/uploads/2026/09/mix_thumb.jpg", "alt": "mix" } ],
        "variants": [
            { "option1_value": "雞", "price": 100, "stock": 0, "image_path": "/uploads/2026/09/mix.jpg" },
            { "option1_value": "牛", "price": 300, "compare_at_price": 350, "stock": 0 },
            { "option1_value": "停用", "price": 999, "stock": 9, "is_active": false }
        ]
    })).await;
    seed(&app, &cookie, one_variant("潔牙骨", "bone", "active", 200, 5, None)).await;
    seed(&app, &cookie, one_variant("草稿商品", "draft-item", "draft", 50, 5, Some(&cat["id"]))).await;
    seed(&app, &cookie, one_variant("封存商品", "archived-item", "archived", 50, 5, None)).await;
    (app, cookie)
}

#[sqlx::test(migrations = "./migrations")]
async fn list_only_active_with_ranges_and_sorting(pool: PgPool) {
    let (app, _) = setup(&pool).await;

    // 預設 newest：bone（後建）在前、mix 在後；草稿與封存不出現
    let (status, body, _) = common::send(&app, common::req("GET", "/api/products", None, None)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["total"], 2);
    assert_eq!(body["per_page"], 24);
    let items = body["items"].as_array().unwrap();
    assert_eq!(items[0]["slug"], "bone");
    assert_eq!(items[1]["slug"], "mix");
    let mix = &items[1];
    assert_eq!(mix["price_min"], 100);
    assert_eq!(mix["price_max"], 300, "停用規格的 999 不算");
    assert_eq!(mix["in_stock"], false);
    assert_eq!(mix["image_thumb"], "/uploads/2026/09/mix_thumb.jpg");
    assert_eq!(items[0]["in_stock"], true);

    // price_asc：mix(100) 在前；price_desc：mix(300) 在前
    let (_, body, _) = common::send(&app, common::req("GET", "/api/products?sort=price_asc", None, None)).await;
    assert_eq!(body["items"][0]["slug"], "mix");
    let (_, body, _) = common::send(&app, common::req("GET", "/api/products?sort=price_desc", None, None)).await;
    assert_eq!(body["items"][0]["slug"], "mix");

    // 錯的 sort
    let (status, _, _) = common::send(&app, common::req("GET", "/api/products?sort=random", None, None)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "./migrations")]
async fn list_filters_and_paging(pool: PgPool) {
    let (app, _) = setup(&pool).await;

    // 分類 food 只有 mix（草稿不算）
    let (_, body, _) = common::send(&app, common::req("GET", "/api/products?category=food", None, None)).await;
    assert_eq!(body["total"], 1);
    assert_eq!(body["items"][0]["slug"], "mix");

    // 不存在的分類 → 空
    let (_, body, _) = common::send(&app, common::req("GET", "/api/products?category=nope", None, None)).await;
    assert_eq!(body["total"], 0);

    // 關鍵字（潔牙）
    let (_, body, _) = common::send(&app, common::req("GET", "/api/products?q=%E6%BD%94%E7%89%99", None, None)).await;
    assert_eq!(body["total"], 1);
    assert_eq!(body["items"][0]["slug"], "bone");

    // per_page 上限 60；分頁
    let (_, body, _) = common::send(&app, common::req("GET", "/api/products?per_page=500", None, None)).await;
    assert_eq!(body["per_page"], 60);
    let (_, body, _) = common::send(&app, common::req("GET", "/api/products?per_page=1&page=2", None, None)).await;
    assert_eq!(body["total"], 2);
    assert_eq!(body["items"].as_array().unwrap().len(), 1);
    assert_eq!(body["items"][0]["slug"], "mix");
}

#[sqlx::test(migrations = "./migrations")]
async fn detail_returns_active_variants_only(pool: PgPool) {
    let (app, _) = setup(&pool).await;

    let (status, body, _) = common::send(&app, common::req("GET", "/api/products/mix", None, None)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["name"], "綜合狗糧");
    assert_eq!(body["category"]["slug"], "food");
    assert_eq!(body["option1_name"], "口味");
    assert!(body["option2_name"].is_null());
    assert_eq!(body["images"][0]["alt"], "mix");
    let variants = body["variants"].as_array().unwrap();
    assert_eq!(variants.len(), 2, "停用的規格不回");
    assert_eq!(variants[0]["option1_value"], "雞");
    assert_eq!(variants[0]["image_path"], "/uploads/2026/09/mix.jpg");
    assert!(variants[1]["image_path"].is_null());
    assert_eq!(variants[1]["compare_at_price"], 350);
    assert!(body.get("external_ref").is_none(), "公開頁不吐內部欄位");

    // 草稿、封存、不存在 → 404
    for slug in ["draft-item", "archived-item", "nothing-here"] {
        let (status, body, _) = common::send(&app, common::req("GET", &format!("/api/products/{slug}"), None, None)).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{slug}");
        assert_eq!(body["error"]["code"], "NOT_FOUND");
    }
}
```

- [ ] **Step 5: 跑測試**

Run: `cd api && DATABASE_URL=postgres://dog_shop:dog_shop@localhost:5435/dog_shop cargo test`
Expected: 全部 `ok`，新增 `tests/products_public.rs` 的 3 個。

- [ ] **Step 6: 格式、lint、commit**

Run: `cd api && cargo fmt --all && cargo clippy --all-targets -- -D warnings`

```bash
git add api/src api/tests
git commit -m "feat(api): 公開商品列表與商品頁"
```


---

### Task 11: web 骨架（SvelteKit、Tailwind、api 包裝、hooks、版面）

**Files:**
- Create（由 `sv create` 產生後再覆寫／新增）: `web/svelte.config.js`、`web/vite.config.ts`、`web/src/app.html`、`web/src/app.css`、`web/src/app.d.ts`
- Create: `web/src/lib/types.ts`、`web/src/lib/api.ts`、`web/src/lib/api.test.ts`、`web/src/lib/server/api.ts`、`web/src/lib/format.ts`、`web/src/lib/toast.svelte.ts`、`web/src/lib/components/Toasts.svelte`
- Create: `web/src/hooks.server.ts`、`web/src/routes/+layout.server.ts`、`web/src/routes/+layout.svelte`、`web/src/routes/+error.svelte`、`web/src/routes/+page.svelte`（暫時版，Task 15 換掉）
- Modify: `web/package.json`（加 `test` script）

**Interfaces:**
- Consumes: API `GET /api/auth/me`、`GET /api/settings/public`。
- Produces:
  - `$lib/types`：`User`、`ShopSettings`、`Category`、`Page<T>`、`ProductStatus`、`ProductImage`、`AdminVariant`、`AdminProduct`、`AdminProductListItem`、`ProductListItem`、`PublicVariant`、`ProductDetail`
  - `$lib/api`：`class ApiError extends Error { status, code, message, details, field(name): string | undefined }`、`parseResponse<T>(res: Response): Promise<T>`、`api<T>(path: string, init?: RequestInit): Promise<T>`（瀏覽器；非 GET 自動帶 `X-Requested-With: fetch`；字串 body 自動帶 JSON content-type；FormData 不動）
  - `$lib/server/api`：`serverApi<T>(event: { request: Request }, path: string, init?: RequestInit): Promise<T>`（用 `API_INTERNAL_URL`、轉送 cookie）、re-export `ApiError`
  - `$lib/format`：`twd(n)`、`priceRange(min, max)`、`formatDate(iso)`
  - `$lib/toast.svelte`：`toast.show(message, ms?)`、`toast.items`
  - `App.Locals.user: User | null`；`+layout.server.ts` 回 `{ user, shop }`
  - Vite dev proxy：`/api`、`/uploads` → `http://localhost:8080`；`.env` 從 repo 根目錄讀（`envDir: '..'`）

- [ ] **Step 1: 建 SvelteKit 專案（非互動）**

Run: `pnpm dlx sv@latest create web --template minimal --types ts --no-add-ons --no-dir-check --no-download-check --install pnpm`
Expected: 產生 `web/`，最後印出 `Project created`／下一步提示，沒有停下來問問題。

- [ ] **Step 2: 加 Tailwind、adapter-node、vitest；移除 adapter-auto**

Run: `cd web && pnpm add -D @sveltejs/adapter-node tailwindcss @tailwindcss/vite vitest && pnpm remove @sveltejs/adapter-auto`
Expected: `package.json` 的 devDependencies 有這四個、沒有 `@sveltejs/adapter-auto`。

Run: `cd web && npm pkg set scripts.test="vitest run --passWithNoTests"`
Expected: `package.json` 的 scripts 多一個 `test`。

- [ ] **Step 3: 覆寫 `web/svelte.config.js`**

```js
import adapter from '@sveltejs/adapter-node';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

/** @type {import('@sveltejs/kit').Config} */
const config = {
	preprocess: vitePreprocess(),
	kit: {
		adapter: adapter()
	}
};

export default config;
```

- [ ] **Step 4: 覆寫 `web/vite.config.ts`**

```ts
/// <reference types="vitest/config" />
import { sveltekit } from '@sveltejs/kit/vite';
import tailwindcss from '@tailwindcss/vite';
import { defineConfig } from 'vite';

export default defineConfig({
	plugins: [tailwindcss(), sveltekit()],
	// .env 放在 repo 根目錄，api 與 web 共用
	envDir: '..',
	server: {
		proxy: {
			'/api': 'http://localhost:8080',
			'/uploads': 'http://localhost:8080'
		}
	},
	test: {
		include: ['src/**/*.test.ts'],
		environment: 'node'
	}
});
```

- [ ] **Step 5: 覆寫 `web/src/app.html`、新增 `web/src/app.css`、覆寫 `web/src/app.d.ts`**

`web/src/app.html`:

```html
<!doctype html>
<html lang="zh-Hant">
	<head>
		<meta charset="utf-8" />
		<meta name="viewport" content="width=device-width, initial-scale=1" />
		%sveltekit.head%
	</head>
	<body data-sveltekit-preload-data="hover" class="bg-gray-50 text-gray-900">
		<div style="display: contents">%sveltekit.body%</div>
	</body>
</html>
```

`web/src/app.css`:

```css
@import 'tailwindcss';

@layer base {
	html {
		font-family: system-ui, -apple-system, 'Noto Sans TC', 'PingFang TC', 'Microsoft JhengHei', sans-serif;
	}
}
```

`web/src/app.d.ts`:

```ts
import type { User } from '$lib/types';

declare global {
	namespace App {
		interface Locals {
			user: User | null;
		}
		// interface Error {}
		// interface PageData {}
		// interface PageState {}
		// interface Platform {}
	}
}

export {};
```

- [ ] **Step 6: 寫 `web/src/lib/types.ts`**

```ts
export type User = {
	id: string;
	email: string;
	name: string;
	phone: string | null;
	role: 'customer' | 'admin';
};

export type ShopSettings = {
	name: string;
	description: string;
	contact_email: string;
	contact_phone: string;
};

export type Category = { id: string; slug: string; name: string; sort_order: number };

export type Page<T> = { items: T[]; total: number; page: number; per_page: number };

export type ProductStatus = 'draft' | 'active' | 'archived';

export type ProductImage = {
	id: string;
	product_id: string;
	path: string;
	thumb_path: string;
	alt: string;
	sort_order: number;
};

export type AdminVariant = {
	id: string;
	product_id: string;
	option1_value: string | null;
	option2_value: string | null;
	sku: string | null;
	price: number;
	compare_at_price: number | null;
	stock: number;
	is_active: boolean;
	image_id: string | null;
	sort_order: number;
};

export type AdminProduct = {
	id: string;
	slug: string;
	name: string;
	description: string;
	category_id: string | null;
	status: ProductStatus;
	option1_name: string | null;
	option2_name: string | null;
	external_ref: string | null;
	sort_order: number;
	created_at: string;
	updated_at: string;
	variants: AdminVariant[];
	images: ProductImage[];
};

export type AdminProductListItem = {
	id: string;
	slug: string;
	name: string;
	status: ProductStatus;
	category_name: string | null;
	price_min: number | null;
	price_max: number | null;
	stock_total: number;
	image_thumb: string | null;
	updated_at: string;
};

export type ProductListItem = {
	slug: string;
	name: string;
	price_min: number;
	price_max: number;
	image_thumb: string | null;
	in_stock: boolean;
};

export type PublicVariant = {
	id: string;
	option1_value: string | null;
	option2_value: string | null;
	price: number;
	compare_at_price: number | null;
	stock: number;
	image_path: string | null;
};

export type ProductDetail = {
	id: string;
	slug: string;
	name: string;
	description: string;
	category: { slug: string; name: string } | null;
	option1_name: string | null;
	option2_name: string | null;
	images: { path: string; thumb_path: string; alt: string }[];
	variants: PublicVariant[];
};
```

- [ ] **Step 7: 寫 `web/src/lib/api.ts`**

```ts
/** API 回的錯誤：對應後端 { error: { code, message, details } } */
export class ApiError extends Error {
	constructor(
		public status: number,
		public code: string,
		message: string,
		public details: unknown = null
	) {
		super(message);
		this.name = 'ApiError';
	}

	/** 驗證錯誤時取某個欄位的訊息 */
	field(name: string): string | undefined {
		const fields = (this.details as { fields?: Record<string, string> } | null)?.fields;
		return fields?.[name];
	}
}

/** 把 fetch 的 Response 變成資料或丟 ApiError。204 回 undefined。 */
export async function parseResponse<T>(res: Response): Promise<T> {
	if (res.status === 204) return undefined as T;
	const text = await res.text();
	let data: unknown = null;
	try {
		data = text ? JSON.parse(text) : null;
	} catch {
		data = null;
	}
	if (!res.ok) {
		const err = (data as { error?: { code?: string; message?: string; details?: unknown } } | null)?.error;
		throw new ApiError(
			res.status,
			err?.code ?? `HTTP_${res.status}`,
			err?.message ?? `請求失敗（${res.status}）`,
			err?.details ?? null
		);
	}
	return data as T;
}

/**
 * 瀏覽器端呼叫 API（同源 /api/...，開發時由 Vite proxy 轉到 :8080）。
 * 非 GET 自動帶 X-Requested-With（後端 CSRF 檢查要求）；字串 body 當 JSON；FormData 交給瀏覽器自己設 content-type。
 */
export async function api<T>(path: string, init: RequestInit = {}): Promise<T> {
	const method = (init.method ?? 'GET').toUpperCase();
	const headers = new Headers(init.headers);
	headers.set('accept', 'application/json');
	if (method !== 'GET') headers.set('x-requested-with', 'fetch');
	if (typeof init.body === 'string' && !headers.has('content-type')) {
		headers.set('content-type', 'application/json');
	}
	const res = await fetch(path, { ...init, method, headers, credentials: 'same-origin' });
	return parseResponse<T>(res);
}
```

- [ ] **Step 8: 寫 `web/src/lib/api.test.ts`**

```ts
import { describe, expect, it } from 'vitest';
import { ApiError, parseResponse } from './api';

describe('parseResponse', () => {
	it('returns parsed JSON on success', async () => {
		const res = new Response(JSON.stringify({ ok: 1 }), { status: 200 });
		await expect(parseResponse<{ ok: number }>(res)).resolves.toEqual({ ok: 1 });
	});

	it('returns undefined on 204', async () => {
		const res = new Response(null, { status: 204 });
		await expect(parseResponse(res)).resolves.toBeUndefined();
	});

	it('throws ApiError with code, message and field errors from the envelope', async () => {
		const body = {
			error: { code: 'VALIDATION', message: '輸入資料有誤', details: { fields: { name: '必填' } } }
		};
		const res = new Response(JSON.stringify(body), { status: 400 });
		const err = await parseResponse(res).catch((e: unknown) => e);
		expect(err).toBeInstanceOf(ApiError);
		const apiErr = err as ApiError;
		expect(apiErr.status).toBe(400);
		expect(apiErr.code).toBe('VALIDATION');
		expect(apiErr.message).toBe('輸入資料有誤');
		expect(apiErr.field('name')).toBe('必填');
		expect(apiErr.field('other')).toBeUndefined();
	});

	it('handles non-JSON error bodies', async () => {
		const res = new Response('Bad Gateway', { status: 502 });
		const err = await parseResponse(res).catch((e: unknown) => e);
		expect(err).toBeInstanceOf(ApiError);
		expect((err as ApiError).code).toBe('HTTP_502');
	});
});
```

- [ ] **Step 9: 寫 `web/src/lib/server/api.ts`（只有伺服器端能 import）**

```ts
import { env } from '$env/dynamic/private';
import { parseResponse } from '$lib/api';

export { ApiError } from '$lib/api';

/**
 * 伺服器端（load、hooks、+server.ts）呼叫 API：用內網位址，並把瀏覽器的 cookie 轉送過去。
 * 只需要 event.request，所以 RequestEvent / ServerLoadEvent 都可以直接傳。
 */
export async function serverApi<T>(
	event: { request: Request },
	path: string,
	init: RequestInit = {}
): Promise<T> {
	const base = (env.API_INTERNAL_URL ?? 'http://localhost:8080').replace(/\/$/, '');
	const headers = new Headers(init.headers);
	headers.set('accept', 'application/json');
	const cookie = event.request.headers.get('cookie');
	if (cookie) headers.set('cookie', cookie);
	if (typeof init.body === 'string' && !headers.has('content-type')) {
		headers.set('content-type', 'application/json');
	}
	const res = await fetch(base + path, { ...init, headers });
	return parseResponse<T>(res);
}
```

- [ ] **Step 10: 寫 `web/src/lib/format.ts`**

```ts
export function twd(amount: number): string {
	return 'NT$' + amount.toLocaleString('zh-TW');
}

export function priceRange(min: number, max: number): string {
	return min === max ? twd(min) : `${twd(min)} ～ ${twd(max)}`;
}

/** 顯示台北時間（規格 §8：DB 存 UTC、前端顯示台北時間） */
export function formatDate(iso: string): string {
	return new Date(iso).toLocaleString('zh-TW', { timeZone: 'Asia/Taipei', hour12: false });
}
```

- [ ] **Step 11: 寫 `web/src/lib/toast.svelte.ts` 與 `web/src/lib/components/Toasts.svelte`**

`web/src/lib/toast.svelte.ts`:

```ts
export type Toast = { id: number; message: string };

class ToastStore {
	items = $state<Toast[]>([]);
	private seq = 0;

	show(message: string, ms = 2500) {
		const id = ++this.seq;
		this.items.push({ id, message });
		setTimeout(() => this.dismiss(id), ms);
	}

	dismiss(id: number) {
		this.items = this.items.filter((t) => t.id !== id);
	}
}

export const toast = new ToastStore();
```

`web/src/lib/components/Toasts.svelte`:

```svelte
<script lang="ts">
	import { toast } from '$lib/toast.svelte';
</script>

<div class="pointer-events-none fixed inset-x-0 bottom-6 z-50 flex flex-col items-center gap-2">
	{#each toast.items as item (item.id)}
		<div class="rounded-full bg-gray-900 px-4 py-2 text-sm text-white shadow-lg">{item.message}</div>
	{/each}
</div>
```

- [ ] **Step 12: 寫 `web/src/hooks.server.ts`**

```ts
import type { Handle } from '@sveltejs/kit';
import { ApiError, serverApi } from '$lib/server/api';
import type { User } from '$lib/types';

/** 每個請求：有 sid cookie 就問 API 是誰，放進 locals.user（規格 §6.2） */
export const handle: Handle = async ({ event, resolve }) => {
	event.locals.user = null;
	if (event.cookies.get('sid')) {
		try {
			const res = await serverApi<{ user: User }>(event, '/api/auth/me');
			event.locals.user = res.user;
		} catch (e) {
			// 401 = session 過期或被登出，當沒登入；其他錯誤記 log 但頁面照常顯示
			if (!(e instanceof ApiError && e.status === 401)) console.error('auth/me failed', e);
		}
	}
	return resolve(event);
};
```

- [ ] **Step 13: 寫 `web/src/routes/+layout.server.ts`、`+layout.svelte`、`+error.svelte`，覆寫 `+page.svelte`**

`web/src/routes/+layout.server.ts`:

```ts
import type { LayoutServerLoad } from './$types';
import { serverApi } from '$lib/server/api';
import type { ShopSettings } from '$lib/types';

export const load: LayoutServerLoad = async (event) => {
	const shop = await serverApi<ShopSettings>(event, '/api/settings/public');
	return { user: event.locals.user, shop };
};
```

`web/src/routes/+layout.svelte`:

```svelte
<script lang="ts">
	import '../app.css';
	import { goto, invalidateAll } from '$app/navigation';
	import { api } from '$lib/api';
	import Toasts from '$lib/components/Toasts.svelte';
	import type { LayoutProps } from './$types';

	let { data, children }: LayoutProps = $props();

	async function logout() {
		await api('/api/auth/logout', { method: 'POST' });
		await invalidateAll();
		await goto('/');
	}
</script>

<svelte:head>
	<title>{data.shop.name}</title>
</svelte:head>

<header class="border-b border-gray-200 bg-white">
	<div class="mx-auto flex max-w-6xl items-center gap-4 px-4 py-3">
		<a href="/" class="text-lg font-bold">{data.shop.name}</a>
		<nav class="ml-auto flex items-center gap-4 text-sm">
			<a href="/products" class="hover:underline">全部商品</a>
			{#if data.user}
				{#if data.user.role === 'admin'}
					<a href="/admin" class="hover:underline">後台</a>
				{/if}
				<button type="button" class="hover:underline" onclick={logout}>登出</button>
			{:else}
				<a href="/login" class="hover:underline">登入</a>
			{/if}
		</nav>
	</div>
</header>

<main class="mx-auto min-h-[60vh] max-w-6xl px-4 py-6">
	{@render children()}
</main>

<footer class="mt-12 border-t border-gray-200 py-8 text-center text-sm text-gray-500">
	<p>{data.shop.name}</p>
	{#if data.shop.contact_email}<p>{data.shop.contact_email}</p>{/if}
</footer>

<Toasts />
```

`web/src/routes/+error.svelte`:

```svelte
<script lang="ts">
	import { page } from '$app/state';
</script>

<div class="mx-auto max-w-md py-20 text-center">
	<h1 class="text-4xl font-bold">{page.status}</h1>
	<p class="mt-3 text-gray-600">{page.error?.message ?? '發生錯誤'}</p>
	<a href="/" class="mt-6 inline-block rounded bg-gray-900 px-4 py-2 text-white">回首頁</a>
</div>
```

`web/src/routes/+page.svelte`（暫時版）:

```svelte
<h1 class="text-2xl font-bold">歡迎</h1>
<p class="mt-2 text-gray-600">首頁內容在 Task 15 補上。</p>
```

- [ ] **Step 14: 型別檢查、測試、建置**

Run: `cd web && pnpm check`
Expected: `svelte-check found 0 errors`（warnings 可以有，errors 不行）。
Run: `cd web && pnpm test`
Expected: `api.test.ts` 4 個測試通過。
Run: `cd web && pnpm build`
Expected: 產生 `web/build/`，沒有 error。

- [ ] **Step 15: 手動：開發伺服器能起來並拿到設定**

（api 要在跑：另開一個 Bash 執行 `cd api && cargo run`，看到 `api listening`。）
Run: `cd web && pnpm dev --port 5173`（背景執行），然後 `curl -s http://localhost:5173/ | head -c 400`
Expected: HTML 裡有 `dog_shop`（來自 `/api/settings/public`）。看完把 dev server 停掉。

- [ ] **Step 16: Commit**

```bash
git add web
git commit -m "feat(web): SvelteKit 骨架、Tailwind、api 包裝、hooks、版面"
```

---

### Task 12: 登入頁、後台守門、分類管理頁

**Files:**
- Create: `web/src/routes/login/+page.server.ts`、`web/src/routes/login/+page.svelte`
- Create: `web/src/routes/admin/+layout.server.ts`、`web/src/routes/admin/+layout.svelte`、`web/src/routes/admin/+page.svelte`
- Create: `web/src/routes/admin/categories/+page.server.ts`、`web/src/routes/admin/categories/+page.svelte`

**Interfaces:**
- Consumes: `api`、`serverApi`、`toast`、API `/api/auth/login`、`/api/admin/categories*`。
- Produces: `/login?redirect=<path>`（登入後回到 redirect，只接受以 `/` 開頭且不是 `//` 的路徑；admin 預設去 `/admin`、其他去 `/`）；`/admin/*` 非 admin 一律 303 到 `/login?redirect=...`（規格 §6.2）；後台側欄連結 `/admin`、`/admin/products`、`/admin/categories`。

- [ ] **Step 1: 寫 `web/src/routes/login/+page.server.ts`**

```ts
import { redirect } from '@sveltejs/kit';
import type { PageServerLoad } from './$types';

export const load: PageServerLoad = ({ locals }) => {
	if (locals.user) redirect(303, locals.user.role === 'admin' ? '/admin' : '/');
	return {};
};
```

- [ ] **Step 2: 寫 `web/src/routes/login/+page.svelte`**

```svelte
<script lang="ts">
	import { goto, invalidateAll } from '$app/navigation';
	import { page } from '$app/state';
	import { api, ApiError } from '$lib/api';
	import type { User } from '$lib/types';

	let email = $state('');
	let password = $state('');
	let error = $state('');
	let submitting = $state(false);

	function safeRedirect(target: string | null, user: User): string {
		if (target && target.startsWith('/') && !target.startsWith('//')) return target;
		return user.role === 'admin' ? '/admin' : '/';
	}

	async function submit(e: SubmitEvent) {
		e.preventDefault();
		error = '';
		submitting = true;
		try {
			const res = await api<{ user: User }>('/api/auth/login', {
				method: 'POST',
				body: JSON.stringify({ email, password })
			});
			await invalidateAll();
			await goto(safeRedirect(page.url.searchParams.get('redirect'), res.user));
		} catch (err) {
			error = err instanceof ApiError ? err.message : '登入失敗，請再試一次';
		} finally {
			submitting = false;
		}
	}
</script>

<svelte:head><title>登入</title></svelte:head>

<div class="mx-auto max-w-sm">
	<h1 class="mb-6 text-2xl font-bold">登入</h1>
	<form onsubmit={submit} class="space-y-4">
		<label class="block text-sm text-gray-700">
			Email
			<input
				type="email"
				bind:value={email}
				required
				autocomplete="email"
				class="mt-1 w-full rounded border border-gray-300 px-3 py-2 text-base"
			/>
		</label>
		<label class="block text-sm text-gray-700">
			密碼
			<input
				type="password"
				bind:value={password}
				required
				autocomplete="current-password"
				class="mt-1 w-full rounded border border-gray-300 px-3 py-2 text-base"
			/>
		</label>
		{#if error}<p class="text-sm text-red-600">{error}</p>{/if}
		<button
			type="submit"
			disabled={submitting}
			class="w-full rounded bg-gray-900 px-4 py-2 text-white disabled:opacity-50"
		>
			{submitting ? '登入中…' : '登入'}
		</button>
	</form>
</div>
```

- [ ] **Step 3: 寫後台外框 `web/src/routes/admin/+layout.server.ts`、`+layout.svelte`、`+page.svelte`**

`web/src/routes/admin/+layout.server.ts`:

```ts
import { redirect } from '@sveltejs/kit';
import type { LayoutServerLoad } from './$types';

/** /admin 底下全部要 admin；不是就去登入頁（規格 §6.2） */
export const load: LayoutServerLoad = ({ locals, url }) => {
	if (!locals.user || locals.user.role !== 'admin') {
		redirect(303, `/login?redirect=${encodeURIComponent(url.pathname)}`);
	}
	return {};
};
```

`web/src/routes/admin/+layout.svelte`:

```svelte
<script lang="ts">
	import { page } from '$app/state';
	import type { LayoutProps } from './$types';

	let { children }: LayoutProps = $props();

	const links = [
		{ href: '/admin', label: '儀表板' },
		{ href: '/admin/products', label: '商品' },
		{ href: '/admin/categories', label: '分類' }
	];

	function isActive(href: string): boolean {
		return href === '/admin' ? page.url.pathname === '/admin' : page.url.pathname.startsWith(href);
	}
</script>

<div class="flex flex-col gap-6 md:flex-row">
	<aside class="md:w-44 md:shrink-0">
		<nav class="flex gap-2 md:flex-col">
			{#each links as link (link.href)}
				<a
					href={link.href}
					class="rounded px-3 py-2 text-sm {isActive(link.href) ? 'bg-gray-900 text-white' : 'hover:bg-gray-200'}"
				>
					{link.label}
				</a>
			{/each}
		</nav>
	</aside>
	<section class="min-w-0 flex-1">
		{@render children()}
	</section>
</div>
```

`web/src/routes/admin/+page.svelte`:

```svelte
<svelte:head><title>後台</title></svelte:head>

<h1 class="text-2xl font-bold">後台</h1>
<p class="mt-2 text-gray-600">先從「商品」與「分類」開始。訂單、出貨、發票的統計會在後面的計畫加到這一頁。</p>
<div class="mt-6 flex gap-3">
	<a href="/admin/products/new" class="rounded bg-gray-900 px-4 py-2 text-sm text-white">新增商品</a>
	<a href="/admin/categories" class="rounded border border-gray-300 px-4 py-2 text-sm">管理分類</a>
</div>
```

- [ ] **Step 4: 寫分類頁 `web/src/routes/admin/categories/+page.server.ts` 與 `+page.svelte`**

`web/src/routes/admin/categories/+page.server.ts`:

```ts
import type { PageServerLoad } from './$types';
import { serverApi } from '$lib/server/api';
import type { Category } from '$lib/types';

export const load: PageServerLoad = async (event) => {
	const categories = await serverApi<Category[]>(event, '/api/admin/categories');
	return { categories };
};
```

`web/src/routes/admin/categories/+page.svelte`:

```svelte
<script lang="ts">
	import { invalidateAll } from '$app/navigation';
	import { api, ApiError } from '$lib/api';
	import { toast } from '$lib/toast.svelte';
	import type { Category } from '$lib/types';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	let newName = $state('');
	let newSlug = $state('');
	let confirmDeleteId = $state<string | null>(null);

	// 每列可編輯的複本；伺服器資料重新載入時同步回來
	let rows = $state<Category[]>([]);
	$effect(() => {
		rows = data.categories.map((c) => ({ ...c }));
	});

	function showError(err: unknown, fallback: string) {
		if (err instanceof ApiError) toast.show(err.field('slug') ?? err.field('name') ?? err.message);
		else toast.show(fallback);
	}

	async function create(e: SubmitEvent) {
		e.preventDefault();
		try {
			await api('/api/admin/categories', {
				method: 'POST',
				body: JSON.stringify({ name: newName, slug: newSlug.trim() || null, sort_order: rows.length })
			});
			newName = '';
			newSlug = '';
			toast.show('已新增分類');
			await invalidateAll();
		} catch (err) {
			showError(err, '新增失敗');
		}
	}

	async function save(row: Category) {
		try {
			await api(`/api/admin/categories/${row.id}`, {
				method: 'PUT',
				body: JSON.stringify({ name: row.name, slug: row.slug, sort_order: Number(row.sort_order) || 0 })
			});
			toast.show('已儲存');
			await invalidateAll();
		} catch (err) {
			showError(err, '儲存失敗');
		}
	}

	async function remove(id: string) {
		// 第一次按只變成「確定刪除？」，第二次才真的刪（不用 confirm() 對話框）
		if (confirmDeleteId !== id) {
			confirmDeleteId = id;
			return;
		}
		try {
			await api(`/api/admin/categories/${id}`, { method: 'DELETE' });
			toast.show('已刪除');
			confirmDeleteId = null;
			await invalidateAll();
		} catch (err) {
			showError(err, '刪除失敗');
		}
	}
</script>

<svelte:head><title>分類管理</title></svelte:head>

<h1 class="text-2xl font-bold">分類</h1>

<form onsubmit={create} class="mt-4 flex flex-wrap items-end gap-2 rounded border border-gray-200 bg-white p-4">
	<label class="block text-sm">
		名稱
		<input bind:value={newName} required class="mt-1 block rounded border border-gray-300 px-3 py-2" />
	</label>
	<label class="block text-sm">
		網址代稱（可空白，會自動產生）
		<input bind:value={newSlug} placeholder="例如 food" class="mt-1 block rounded border border-gray-300 px-3 py-2" />
	</label>
	<button type="submit" class="rounded bg-gray-900 px-4 py-2 text-sm text-white">新增</button>
</form>

<div class="mt-6 overflow-x-auto">
	<table class="w-full border-collapse bg-white text-sm">
		<thead>
			<tr class="border-b border-gray-200 text-left">
				<th class="p-2">排序</th>
				<th class="p-2">名稱</th>
				<th class="p-2">網址代稱</th>
				<th class="p-2"></th>
			</tr>
		</thead>
		<tbody>
			{#each rows as row (row.id)}
				<tr class="border-b border-gray-100">
					<td class="p-2"><input type="number" bind:value={row.sort_order} class="w-16 rounded border border-gray-300 px-2 py-1" /></td>
					<td class="p-2"><input bind:value={row.name} class="w-full rounded border border-gray-300 px-2 py-1" /></td>
					<td class="p-2"><input bind:value={row.slug} class="w-full rounded border border-gray-300 px-2 py-1" /></td>
					<td class="p-2 whitespace-nowrap">
						<button type="button" onclick={() => save(row)} class="rounded border border-gray-300 px-3 py-1">儲存</button>
						<button type="button" onclick={() => remove(row.id)} class="ml-2 rounded border border-red-300 px-3 py-1 text-red-700">
							{confirmDeleteId === row.id ? '確定刪除？' : '刪除'}
						</button>
					</td>
				</tr>
			{:else}
				<tr><td colspan="4" class="p-4 text-center text-gray-500">還沒有分類</td></tr>
			{/each}
		</tbody>
	</table>
</div>
```

- [ ] **Step 5: 型別檢查與建置**

Run: `cd web && pnpm check && pnpm build`
Expected: 0 errors、build 成功。

- [ ] **Step 6: 手動走一遍（api 與 web dev 都要在跑；沒有 admin 就先 `cd api && ADMIN_PASSWORD=admin12345 cargo run -- create-admin admin@example.com`）**

1. 開 `http://localhost:5173/admin` → 被帶到 `/login?redirect=%2Fadmin`。
2. 用 admin 帳密登入 → 回到 `/admin`，右上角有「後台」「登出」。
3. 到「分類」新增「飼料」（slug 空白）→ 表格出現一列，slug 是 8 碼。改名、存、刪都正常。
4. 按「登出」→ 回首頁，再開 `/admin` 又被帶去登入。

- [ ] **Step 7: Commit**

```bash
git add web/src
git commit -m "feat(web): 登入頁、後台守門與側欄、分類管理"
```

---

### Task 13: 後台商品頁（列表、新增、編輯、圖片、規格）

**Files:**
- Create: `web/src/lib/components/Pagination.svelte`、`web/src/lib/components/admin/ProductForm.svelte`
- Create: `web/src/routes/admin/products/+page.server.ts`、`+page.svelte`
- Create: `web/src/routes/admin/products/new/+page.server.ts`、`+page.svelte`
- Create: `web/src/routes/admin/products/[id]/+page.server.ts`、`+page.svelte`

**Interfaces:**
- Consumes: API `/api/admin/products*`、`/api/admin/uploads`、`/api/admin/categories`；`types` 的 `AdminProduct`、`AdminProductListItem`、`Category`、`Page`。
- Produces:
  - `Pagination` 元件：props `{ page: number; perPage: number; total: number }`，用目前網址加 `page` 參數產生上一頁／下一頁連結（Task 15 也用）
  - `ProductForm` 元件：props `{ product?: AdminProduct | null; categories: Category[] }`；沒 `product` 是新增（成功後跳到編輯頁），有就是編輯（成功後重新載入）；圖片上傳、拖曳排序、規格表格、依選項產生規格、下架封存

- [ ] **Step 1: 寫 `web/src/lib/components/Pagination.svelte`**

```svelte
<script lang="ts">
	import { page as currentPage } from '$app/state';

	let { page, perPage, total }: { page: number; perPage: number; total: number } = $props();

	const pages = $derived(Math.max(1, Math.ceil(total / perPage)));

	function hrefFor(target: number): string {
		const url = new URL(currentPage.url);
		url.searchParams.set('page', String(target));
		return url.pathname + url.search;
	}
</script>

{#if pages > 1}
	<nav class="mt-6 flex items-center justify-center gap-4 text-sm" aria-label="分頁">
		{#if page > 1}
			<a href={hrefFor(page - 1)} class="rounded border border-gray-300 px-3 py-1">上一頁</a>
		{/if}
		<span>第 {page} / {pages} 頁</span>
		{#if page < pages}
			<a href={hrefFor(page + 1)} class="rounded border border-gray-300 px-3 py-1">下一頁</a>
		{/if}
	</nav>
{/if}
```

- [ ] **Step 2: 寫 `web/src/lib/components/admin/ProductForm.svelte`**

```svelte
<script lang="ts">
	import { goto, invalidateAll } from '$app/navigation';
	import { api, ApiError } from '$lib/api';
	import { toast } from '$lib/toast.svelte';
	import type { AdminProduct, Category, ProductStatus } from '$lib/types';

	type VariantForm = {
		id: string | null;
		option1_value: string;
		option2_value: string;
		sku: string;
		price: number;
		compare_at_price: number | null;
		stock: number;
		is_active: boolean;
		image_path: string | null;
	};
	type ImageForm = { path: string; thumb_path: string; alt: string };

	let { product = null, categories }: { product?: AdminProduct | null; categories: Category[] } = $props();

	function emptyVariant(): VariantForm {
		return {
			id: null,
			option1_value: '',
			option2_value: '',
			sku: '',
			price: 0,
			compare_at_price: null,
			stock: 0,
			is_active: true,
			image_path: null
		};
	}
	function imagePathById(id: string | null): string | null {
		return product?.images.find((i) => i.id === id)?.path ?? null;
	}

	let name = $state(product?.name ?? '');
	let slug = $state(product?.slug ?? '');
	let description = $state(product?.description ?? '');
	let category_id = $state(product?.category_id ?? '');
	let status = $state<ProductStatus>(product?.status ?? 'draft');
	let option1_name = $state(product?.option1_name ?? '');
	let option2_name = $state(product?.option2_name ?? '');
	let sort_order = $state(product?.sort_order ?? 0);
	let images = $state<ImageForm[]>(
		product?.images.map((i) => ({ path: i.path, thumb_path: i.thumb_path, alt: i.alt })) ?? []
	);
	let variants = $state<VariantForm[]>(
		product?.variants.map((v) => ({
			id: v.id,
			option1_value: v.option1_value ?? '',
			option2_value: v.option2_value ?? '',
			sku: v.sku ?? '',
			price: v.price,
			compare_at_price: v.compare_at_price,
			stock: v.stock,
			is_active: v.is_active,
			image_path: imagePathById(v.image_id)
		})) ?? [emptyVariant()]
	);
	let opt1Input = $state('');
	let opt2Input = $state('');
	let saving = $state(false);
	let uploading = $state(false);
	let confirmArchive = $state(false);
	let errors = $state<Record<string, string>>({});
	let dragIndex = $state<number | null>(null);

	const hasOpt1 = $derived(option1_name.trim() !== '');
	const hasOpt2 = $derived(hasOpt1 && option2_name.trim() !== '');

	function splitValues(input: string): string[] {
		return input
			.split(/[,，]/)
			.map((s) => s.trim())
			.filter(Boolean);
	}

	/** 依「口味：雞,牛」×「尺寸：S,L」產生所有組合；已存在的組合保留原本的價格與庫存 */
	function generateVariants() {
		const values1 = splitValues(opt1Input);
		const values2 = hasOpt2 ? splitValues(opt2Input) : [''];
		if (values1.length === 0) {
			toast.show('請先輸入規格 1 的選項');
			return;
		}
		const next: VariantForm[] = [];
		for (const a of values1) {
			for (const b of values2) {
				const existing = variants.find((v) => v.option1_value === a && v.option2_value === b);
				next.push(existing ?? { ...emptyVariant(), option1_value: a, option2_value: b });
			}
		}
		variants = next;
	}
	function addVariant() {
		variants.push(emptyVariant());
	}
	function removeVariant(i: number) {
		variants = variants.filter((_, j) => j !== i);
	}
	function variantError(i: number): string | null {
		const keys = [
			`variants.${i}`,
			`variants.${i}.price`,
			`variants.${i}.compare_at_price`,
			`variants.${i}.stock`,
			`variants.${i}.option1_value`,
			`variants.${i}.option2_value`,
			`variants.${i}.image_path`
		];
		for (const key of keys) if (errors[key]) return errors[key];
		return null;
	}

	async function onFiles(e: Event) {
		const input = e.currentTarget as HTMLInputElement;
		const files = Array.from(input.files ?? []);
		input.value = '';
		if (images.length + files.length > 9) {
			toast.show('圖片最多 9 張');
			return;
		}
		uploading = true;
		try {
			for (const file of files) {
				const form = new FormData();
				form.append('file', file);
				const stored = await api<{ path: string; thumb_path: string }>('/api/admin/uploads', {
					method: 'POST',
					body: form
				});
				images.push({ path: stored.path, thumb_path: stored.thumb_path, alt: '' });
			}
		} catch (err) {
			toast.show(err instanceof ApiError ? (err.field('file') ?? err.message) : '上傳失敗');
		} finally {
			uploading = false;
		}
	}
	function moveImage(from: number, to: number) {
		if (from === to) return;
		const next = [...images];
		const [moved] = next.splice(from, 1);
		next.splice(to, 0, moved);
		images = next;
	}
	function removeImage(i: number) {
		const removedPath = images[i].path;
		images = images.filter((_, j) => j !== i);
		for (const v of variants) if (v.image_path === removedPath) v.image_path = null;
	}

	function toNumberOrNull(value: unknown): number | null {
		if (value === null || value === undefined || value === '') return null;
		const n = Number(value);
		return Number.isFinite(n) ? n : null;
	}

	async function save(e: SubmitEvent) {
		e.preventDefault();
		saving = true;
		errors = {};
		const body = {
			name,
			slug: slug.trim() || null,
			description,
			category_id: category_id || null,
			status,
			option1_name: hasOpt1 ? option1_name.trim() : null,
			option2_name: hasOpt2 ? option2_name.trim() : null,
			sort_order: Number(sort_order) || 0,
			images: images.map((i) => ({ path: i.path, thumb_path: i.thumb_path, alt: i.alt })),
			variants: variants.map((v) => ({
				id: v.id,
				option1_value: hasOpt1 ? v.option1_value : null,
				option2_value: hasOpt2 ? v.option2_value : null,
				sku: v.sku.trim() || null,
				price: Number(v.price) || 0,
				compare_at_price: toNumberOrNull(v.compare_at_price),
				stock: Number(v.stock) || 0,
				is_active: v.is_active,
				image_path: v.image_path
			}))
		};
		try {
			if (product) {
				await api(`/api/admin/products/${product.id}`, { method: 'PUT', body: JSON.stringify(body) });
				toast.show('已儲存');
				await invalidateAll();
			} else {
				const created = await api<AdminProduct>('/api/admin/products', {
					method: 'POST',
					body: JSON.stringify(body)
				});
				toast.show('已建立');
				await goto(`/admin/products/${created.id}`);
			}
		} catch (err) {
			if (err instanceof ApiError && err.code === 'VALIDATION') {
				errors = (err.details as { fields?: Record<string, string> } | null)?.fields ?? {};
				toast.show('有欄位沒填對，請看紅字');
			} else {
				toast.show(err instanceof ApiError ? err.message : '儲存失敗');
			}
		} finally {
			saving = false;
		}
	}

	async function archive() {
		if (!product) return;
		if (!confirmArchive) {
			confirmArchive = true;
			return;
		}
		try {
			await api(`/api/admin/products/${product.id}`, { method: 'DELETE' });
			toast.show('已下架封存');
			await goto('/admin/products');
		} catch (err) {
			toast.show(err instanceof ApiError ? err.message : '封存失敗');
		}
	}
</script>

<form onsubmit={save} class="space-y-8">
	<section class="rounded border border-gray-200 bg-white p-4">
		<h2 class="font-semibold">基本資料</h2>
		<div class="mt-3 grid gap-4 md:grid-cols-2">
			<label class="block text-sm md:col-span-2">
				商品名稱
				<input bind:value={name} required class="mt-1 w-full rounded border border-gray-300 px-3 py-2" />
				{#if errors.name}<span class="text-red-600">{errors.name}</span>{/if}
			</label>
			<label class="block text-sm">
				網址代稱（空白會自動產生）
				<input bind:value={slug} placeholder="例如 chicken-food" class="mt-1 w-full rounded border border-gray-300 px-3 py-2" />
				{#if errors.slug}<span class="text-red-600">{errors.slug}</span>{/if}
			</label>
			<label class="block text-sm">
				分類
				<select bind:value={category_id} class="mt-1 w-full rounded border border-gray-300 px-3 py-2">
					<option value="">未分類</option>
					{#each categories as c (c.id)}
						<option value={c.id}>{c.name}</option>
					{/each}
				</select>
				{#if errors.category_id}<span class="text-red-600">{errors.category_id}</span>{/if}
			</label>
			<label class="block text-sm">
				狀態
				<select bind:value={status} class="mt-1 w-full rounded border border-gray-300 px-3 py-2">
					<option value="draft">草稿（買家看不到）</option>
					<option value="active">上架</option>
					<option value="archived">已下架</option>
				</select>
			</label>
			<label class="block text-sm">
				排序（數字小的在前）
				<input type="number" bind:value={sort_order} class="mt-1 w-full rounded border border-gray-300 px-3 py-2" />
			</label>
			<label class="block text-sm md:col-span-2">
				商品描述（純文字，會保留換行）
				<textarea bind:value={description} rows="6" class="mt-1 w-full rounded border border-gray-300 px-3 py-2"></textarea>
			</label>
		</div>
	</section>

	<section class="rounded border border-gray-200 bg-white p-4">
		<h2 class="font-semibold">圖片（最多 9 張；第一張是主圖；拖曳可以換順序）</h2>
		{#if errors.images}<p class="text-sm text-red-600">{errors.images}</p>{/if}
		<ul class="mt-3 flex flex-wrap gap-3">
			{#each images as img, i (img.path)}
				<li
					class="w-32 rounded border border-gray-200 p-1 {dragIndex === i ? 'opacity-50' : ''}"
					draggable="true"
					ondragstart={() => (dragIndex = i)}
					ondragover={(e) => e.preventDefault()}
					ondrop={(e) => {
						e.preventDefault();
						if (dragIndex !== null) moveImage(dragIndex, i);
						dragIndex = null;
					}}
					ondragend={() => (dragIndex = null)}
				>
					<img src={img.thumb_path} alt={img.alt} class="aspect-square w-full rounded object-cover" />
					<input bind:value={img.alt} placeholder="圖片說明" class="mt-1 w-full rounded border border-gray-300 px-1 py-0.5 text-xs" />
					<div class="mt-1 flex justify-between text-xs">
						<span class="text-gray-500">{i === 0 ? '主圖' : `第 ${i + 1} 張`}</span>
						<button type="button" class="text-red-600" onclick={() => removeImage(i)}>移除</button>
					</div>
				</li>
			{/each}
			{#if images.length < 9}
				<li>
					<label class="flex aspect-square w-32 cursor-pointer items-center justify-center rounded border-2 border-dashed border-gray-300 text-sm text-gray-500">
						{uploading ? '上傳中…' : '＋ 加圖片'}
						<input
							type="file"
							accept="image/jpeg,image/png,image/webp,image/gif"
							multiple
							onchange={onFiles}
							disabled={uploading}
							class="hidden"
						/>
					</label>
				</li>
			{/if}
		</ul>
	</section>

	<section class="rounded border border-gray-200 bg-white p-4">
		<h2 class="font-semibold">規格</h2>
		<p class="mt-1 text-sm text-gray-500">
			沒有規格就留空，只會有一列預設規格。有規格就填名稱（例如「口味」「尺寸」），用逗號列出選項，按「依選項產生規格」。
		</p>
		{#if errors.variants}<p class="text-sm text-red-600">{errors.variants}</p>{/if}
		{#if errors.option2_name}<p class="text-sm text-red-600">{errors.option2_name}</p>{/if}
		<div class="mt-3 grid gap-4 md:grid-cols-2">
			<div class="flex gap-2">
				<label class="block w-1/3 text-sm">
					規格 1 名稱
					<input bind:value={option1_name} placeholder="口味" class="mt-1 w-full rounded border border-gray-300 px-3 py-2" />
				</label>
				<label class="block flex-1 text-sm">
					選項（逗號分隔）
					<input bind:value={opt1Input} placeholder="雞肉, 牛肉" class="mt-1 w-full rounded border border-gray-300 px-3 py-2" />
				</label>
			</div>
			<div class="flex gap-2">
				<label class="block w-1/3 text-sm">
					規格 2 名稱
					<input
						bind:value={option2_name}
						disabled={!hasOpt1}
						placeholder="尺寸"
						class="mt-1 w-full rounded border border-gray-300 px-3 py-2 disabled:bg-gray-100"
					/>
				</label>
				<label class="block flex-1 text-sm">
					選項（逗號分隔）
					<input
						bind:value={opt2Input}
						disabled={!hasOpt2}
						placeholder="S, M, L"
						class="mt-1 w-full rounded border border-gray-300 px-3 py-2 disabled:bg-gray-100"
					/>
				</label>
			</div>
		</div>
		{#if hasOpt1}
			<div class="mt-3 flex gap-2">
				<button type="button" onclick={generateVariants} class="rounded border border-gray-300 px-3 py-1 text-sm">依選項產生規格</button>
				<button type="button" onclick={addVariant} class="rounded border border-gray-300 px-3 py-1 text-sm">手動加一列</button>
			</div>
		{/if}

		<div class="mt-3 overflow-x-auto">
			<table class="w-full text-sm">
				<thead>
					<tr class="border-b border-gray-200 text-left">
						{#if hasOpt1}<th class="p-2">{option1_name}</th>{/if}
						{#if hasOpt2}<th class="p-2">{option2_name}</th>{/if}
						<th class="p-2">售價</th>
						<th class="p-2">原價（可空）</th>
						<th class="p-2">庫存</th>
						<th class="p-2">SKU</th>
						<th class="p-2">圖</th>
						<th class="p-2">啟用</th>
						<th class="p-2"></th>
					</tr>
				</thead>
				<tbody>
					{#each variants as v, i}
						<tr class="border-b border-gray-100 {v.is_active ? '' : 'opacity-60'}">
							{#if hasOpt1}<td class="p-2"><input bind:value={v.option1_value} class="w-24 rounded border border-gray-300 px-2 py-1" /></td>{/if}
							{#if hasOpt2}<td class="p-2"><input bind:value={v.option2_value} class="w-20 rounded border border-gray-300 px-2 py-1" /></td>{/if}
							<td class="p-2"><input type="number" min="0" bind:value={v.price} class="w-24 rounded border border-gray-300 px-2 py-1" /></td>
							<td class="p-2"><input type="number" min="0" bind:value={v.compare_at_price} class="w-24 rounded border border-gray-300 px-2 py-1" /></td>
							<td class="p-2"><input type="number" min="0" bind:value={v.stock} class="w-20 rounded border border-gray-300 px-2 py-1" /></td>
							<td class="p-2"><input bind:value={v.sku} class="w-28 rounded border border-gray-300 px-2 py-1" /></td>
							<td class="p-2">
								<select bind:value={v.image_path} class="rounded border border-gray-300 px-2 py-1">
									<option value={null}>—</option>
									{#each images as img, j (img.path)}
										<option value={img.path}>第 {j + 1} 張</option>
									{/each}
								</select>
							</td>
							<td class="p-2 text-center"><input type="checkbox" bind:checked={v.is_active} /></td>
							<td class="p-2">
								<button type="button" class="text-red-600 disabled:opacity-30" onclick={() => removeVariant(i)} disabled={variants.length === 1}>刪</button>
							</td>
						</tr>
						{#if variantError(i)}
							<tr><td colspan="9" class="p-2 text-red-600">{variantError(i)}</td></tr>
						{/if}
					{/each}
				</tbody>
			</table>
		</div>
	</section>

	<div class="flex items-center gap-3">
		<button type="submit" disabled={saving || uploading} class="rounded bg-gray-900 px-6 py-2 text-white disabled:opacity-50">
			{saving ? '儲存中…' : product ? '儲存' : '建立商品'}
		</button>
		<a href="/admin/products" class="text-sm text-gray-600 hover:underline">回列表</a>
		{#if product && product.status !== 'archived'}
			<button type="button" onclick={archive} class="ml-auto rounded border border-red-300 px-4 py-2 text-sm text-red-700">
				{confirmArchive ? '確定下架封存？' : '下架封存'}
			</button>
		{/if}
	</div>
</form>
```

- [ ] **Step 3: 寫商品列表 `web/src/routes/admin/products/+page.server.ts` 與 `+page.svelte`**

`web/src/routes/admin/products/+page.server.ts`:

```ts
import type { PageServerLoad } from './$types';
import { serverApi } from '$lib/server/api';
import type { AdminProductListItem, Page } from '$lib/types';

export const load: PageServerLoad = async (event) => {
	const sp = event.url.searchParams;
	const qs = new URLSearchParams();
	for (const key of ['q', 'status', 'page']) {
		const value = sp.get(key);
		if (value) qs.set(key, value);
	}
	const result = await serverApi<Page<AdminProductListItem>>(event, `/api/admin/products?${qs}`);
	return { result, q: sp.get('q') ?? '', status: sp.get('status') ?? '' };
};
```

`web/src/routes/admin/products/+page.svelte`:

```svelte
<script lang="ts">
	import Pagination from '$lib/components/Pagination.svelte';
	import { formatDate, priceRange } from '$lib/format';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	const statusLabel: Record<string, string> = { draft: '草稿', active: '上架', archived: '已下架' };
</script>

<svelte:head><title>商品管理</title></svelte:head>

<div class="flex items-center justify-between">
	<h1 class="text-2xl font-bold">商品</h1>
	<a href="/admin/products/new" class="rounded bg-gray-900 px-4 py-2 text-sm text-white">新增商品</a>
</div>

<form method="GET" class="mt-4 flex flex-wrap gap-2">
	<input name="q" value={data.q} placeholder="搜尋名稱" class="rounded border border-gray-300 px-3 py-2 text-sm" />
	<select name="status" value={data.status} class="rounded border border-gray-300 px-3 py-2 text-sm">
		<option value="">全部（不含已下架）</option>
		<option value="draft">草稿</option>
		<option value="active">上架</option>
		<option value="archived">已下架</option>
	</select>
	<button type="submit" class="rounded border border-gray-300 px-4 py-2 text-sm">篩選</button>
</form>

<div class="mt-4 overflow-x-auto">
	<table class="w-full bg-white text-sm">
		<thead>
			<tr class="border-b border-gray-200 text-left">
				<th class="p-2">圖</th>
				<th class="p-2">名稱</th>
				<th class="p-2">狀態</th>
				<th class="p-2">價格</th>
				<th class="p-2">庫存</th>
				<th class="p-2">更新</th>
			</tr>
		</thead>
		<tbody>
			{#each data.result.items as item (item.id)}
				<tr class="border-b border-gray-100">
					<td class="p-2">
						{#if item.image_thumb}<img src={item.image_thumb} alt="" class="h-12 w-12 rounded object-cover" />{/if}
					</td>
					<td class="p-2">
						<a href={`/admin/products/${item.id}`} class="font-medium hover:underline">{item.name}</a>
						<div class="text-xs text-gray-500">{item.category_name ?? '未分類'} · /products/{item.slug}</div>
					</td>
					<td class="p-2">{statusLabel[item.status] ?? item.status}</td>
					<td class="p-2">
						{item.price_min === null || item.price_max === null ? '—' : priceRange(item.price_min, item.price_max)}
					</td>
					<td class="p-2">{item.stock_total}</td>
					<td class="p-2 text-gray-500">{formatDate(item.updated_at)}</td>
				</tr>
			{:else}
				<tr><td colspan="6" class="p-6 text-center text-gray-500">沒有商品</td></tr>
			{/each}
		</tbody>
	</table>
</div>

<Pagination page={data.result.page} perPage={data.result.per_page} total={data.result.total} />
```

- [ ] **Step 4: 寫新增頁 `web/src/routes/admin/products/new/+page.server.ts` 與 `+page.svelte`**

`web/src/routes/admin/products/new/+page.server.ts`:

```ts
import type { PageServerLoad } from './$types';
import { serverApi } from '$lib/server/api';
import type { Category } from '$lib/types';

export const load: PageServerLoad = async (event) => {
	const categories = await serverApi<Category[]>(event, '/api/admin/categories');
	return { categories };
};
```

`web/src/routes/admin/products/new/+page.svelte`:

```svelte
<script lang="ts">
	import ProductForm from '$lib/components/admin/ProductForm.svelte';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
</script>

<svelte:head><title>新增商品</title></svelte:head>

<h1 class="text-2xl font-bold">新增商品</h1>
<div class="mt-4">
	<ProductForm categories={data.categories} />
</div>
```

- [ ] **Step 5: 寫編輯頁 `web/src/routes/admin/products/[id]/+page.server.ts` 與 `+page.svelte`**

`web/src/routes/admin/products/[id]/+page.server.ts`:

```ts
import { error } from '@sveltejs/kit';
import type { PageServerLoad } from './$types';
import { ApiError, serverApi } from '$lib/server/api';
import type { AdminProduct, Category } from '$lib/types';

export const load: PageServerLoad = async (event) => {
	try {
		const [product, categories] = await Promise.all([
			serverApi<AdminProduct>(event, `/api/admin/products/${event.params.id}`),
			serverApi<Category[]>(event, '/api/admin/categories')
		]);
		return { product, categories };
	} catch (e) {
		// 404 = 沒這個商品；400 = id 不是 uuid
		if (e instanceof ApiError && (e.status === 404 || e.status === 400)) error(404, '找不到這個商品');
		throw e;
	}
};
```

`web/src/routes/admin/products/[id]/+page.svelte`:

```svelte
<script lang="ts">
	import ProductForm from '$lib/components/admin/ProductForm.svelte';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
</script>

<svelte:head><title>編輯商品</title></svelte:head>

<h1 class="text-2xl font-bold">編輯商品</h1>
<p class="mt-1 text-sm text-gray-500">
	前台網址：<a href={`/products/${data.product.slug}`} class="underline" target="_blank" rel="noreferrer">/products/{data.product.slug}</a>（要上架才看得到）
</p>
<div class="mt-4">
	<!-- 存檔後 updated_at 會變，用 key 讓表單重新讀取伺服器回來的資料（新規格才會有 id） -->
	{#key data.product.updated_at}
		<ProductForm product={data.product} categories={data.categories} />
	{/key}
</div>
```

- [ ] **Step 6: 型別檢查與建置**

Run: `cd web && pnpm check && pnpm build`
Expected: 0 errors、build 成功。

- [ ] **Step 7: 手動走一遍（api、web dev 在跑，已登入 admin）**

1. `/admin/products` → 空列表，按「新增商品」。
2. 填名稱「雞肉狗糧」、分類「飼料」、規格 1「口味」選項「雞肉, 牛肉」、規格 2「尺寸」選項「S, L」→ 按「依選項產生規格」→ 表格 4 列；填價格與庫存。
3. 加 2 張圖片（任何 jpg/png）→ 縮圖出現；拖第二張到第一張前面 → 順序交換；第一列規格的「圖」選「第 1 張」。
4. 狀態選「上架」，按「建立商品」→ 跳到編輯頁，網址是 `/admin/products/<uuid>`；規格列都有值。
5. 改價格、存 → toast「已儲存」；回列表看到價格範圍、庫存加總、縮圖。
6. 故意把名稱清空存 → 名稱下方紅字「必填，最多 120 字」。

- [ ] **Step 8: Commit**

```bash
git add web/src
git commit -m "feat(web): 後台商品列表、新增與編輯表單（圖片、規格）"
```


---

### Task 14: 購物車 store、購物車頁、頁首購物車數量

**Files:**
- Create: `web/src/lib/cart.svelte.ts`、`web/src/lib/cart.test.ts`、`web/src/routes/cart/+page.svelte`
- Modify: `web/src/routes/+layout.svelte`（載入購物車、加購物車連結與數量）

**Interfaces:**
- Produces: `$lib/cart.svelte`：`type CartLine = { variant_id, product_slug, product_name, variant_label, price, image_thumb, qty }`、`class Cart { lines, loaded, count, subtotal, load(), add(line: Omit<CartLine,'qty'>, qty = 1), setQty(variant_id, qty), remove(variant_id), clear() }`、`export const cart`、`MAX_QTY = 99`。存 localStorage key `dog_shop_cart_v1`；伺服器不存購物車（規格 §6.2）。
- `/cart` 頁：顯示 lines、改數量、移除、小計；結帳按鈕先停用（計畫 2 接上驗證與結帳）。

- [ ] **Step 1: 寫 `web/src/lib/cart.svelte.ts`**

```ts
export type CartLine = {
	variant_id: string;
	product_slug: string;
	product_name: string;
	variant_label: string;
	price: number;
	image_thumb: string | null;
	qty: number;
};

const STORAGE_KEY = 'dog_shop_cart_v1';
export const MAX_QTY = 99;

function readStorage(): CartLine[] {
	try {
		const raw = globalThis.localStorage?.getItem(STORAGE_KEY);
		if (!raw) return [];
		const parsed: unknown = JSON.parse(raw);
		if (!Array.isArray(parsed)) return [];
		return (parsed as CartLine[]).filter((l) => typeof l.variant_id === 'string' && Number.isFinite(l.qty) && l.qty > 0);
	} catch {
		return [];
	}
}

/** 購物車：Svelte 5 runes 類別，內容存 localStorage；價格與庫存只是顯示用快取，結帳時伺服器會重算 */
export class Cart {
	lines = $state<CartLine[]>([]);
	loaded = $state(false);
	count = $derived(this.lines.reduce((sum, l) => sum + l.qty, 0));
	subtotal = $derived(this.lines.reduce((sum, l) => sum + l.qty * l.price, 0));

	/** 在瀏覽器（layout 的 onMount）呼叫，把 localStorage 讀進來 */
	load() {
		this.lines = readStorage();
		this.loaded = true;
	}

	private persist() {
		try {
			globalThis.localStorage?.setItem(STORAGE_KEY, JSON.stringify(this.lines));
		} catch {
			// 無痕模式或空間滿：忽略，購物車只留在記憶體
		}
	}

	add(line: Omit<CartLine, 'qty'>, qty = 1) {
		const wanted = Math.max(1, Math.floor(qty));
		const existing = this.lines.find((l) => l.variant_id === line.variant_id);
		if (existing) {
			existing.qty = Math.min(MAX_QTY, existing.qty + wanted);
			Object.assign(existing, line); // 顯示用資料以最新為準
		} else {
			this.lines.push({ ...line, qty: Math.min(MAX_QTY, wanted) });
		}
		this.persist();
	}

	setQty(variant_id: string, qty: number) {
		if (!Number.isFinite(qty) || qty <= 0) {
			this.remove(variant_id);
			return;
		}
		const line = this.lines.find((l) => l.variant_id === variant_id);
		if (!line) return;
		line.qty = Math.min(MAX_QTY, Math.floor(qty));
		this.persist();
	}

	remove(variant_id: string) {
		this.lines = this.lines.filter((l) => l.variant_id !== variant_id);
		this.persist();
	}

	clear() {
		this.lines = [];
		this.persist();
	}
}

export const cart = new Cart();
```

- [ ] **Step 2: 寫 `web/src/lib/cart.test.ts`**

```ts
import { describe, expect, it } from 'vitest';
import { Cart, MAX_QTY } from './cart.svelte';

const line = {
	variant_id: 'v1',
	product_slug: 'food',
	product_name: '狗糧',
	variant_label: '雞 / S',
	price: 300,
	image_thumb: null
};

describe('Cart', () => {
	it('starts empty; load() marks it loaded', () => {
		const c = new Cart();
		expect(c.loaded).toBe(false);
		c.load();
		expect(c.loaded).toBe(true);
		expect(c.lines).toEqual([]);
		expect(c.count).toBe(0);
		expect(c.subtotal).toBe(0);
	});

	it('adds and merges the same variant', () => {
		const c = new Cart();
		c.add(line, 1);
		c.add(line, 2);
		expect(c.lines).toHaveLength(1);
		expect(c.count).toBe(3);
		expect(c.subtotal).toBe(900);
	});

	it('caps quantity at MAX_QTY and floors fractions', () => {
		const c = new Cart();
		c.add(line, 500);
		expect(c.lines[0].qty).toBe(MAX_QTY);
		c.setQty('v1', 2.7);
		expect(c.lines[0].qty).toBe(2);
	});

	it('setQty updates; zero or less removes', () => {
		const c = new Cart();
		c.add(line);
		c.add({ ...line, variant_id: 'v2', price: 100 });
		c.setQty('v1', 5);
		expect(c.count).toBe(6);
		expect(c.subtotal).toBe(1600);
		c.setQty('v2', 0);
		expect(c.lines.map((l) => l.variant_id)).toEqual(['v1']);
		c.setQty('missing', 3);
		expect(c.count).toBe(5);
	});

	it('remove and clear', () => {
		const c = new Cart();
		c.add(line);
		c.add({ ...line, variant_id: 'v2' });
		c.remove('v1');
		expect(c.lines.map((l) => l.variant_id)).toEqual(['v2']);
		c.clear();
		expect(c.lines).toEqual([]);
	});
});
```

- [ ] **Step 3: 跑測試**

Run: `cd web && pnpm test`
Expected: `api.test.ts` 4 個 + `cart.test.ts` 5 個全部通過。

- [ ] **Step 4: 寫 `web/src/routes/cart/+page.svelte`**

```svelte
<script lang="ts">
	import { cart } from '$lib/cart.svelte';
	import { twd } from '$lib/format';
</script>

<svelte:head><title>購物車</title></svelte:head>

<h1 class="text-2xl font-bold">購物車</h1>

{#if !cart.loaded}
	<p class="mt-4 text-gray-500">載入中…</p>
{:else if cart.lines.length === 0}
	<p class="mt-4 text-gray-600">購物車是空的。<a href="/products" class="underline">去逛逛</a></p>
{:else}
	<ul class="mt-4 divide-y divide-gray-200 rounded border border-gray-200 bg-white">
		{#each cart.lines as line (line.variant_id)}
			<li class="flex flex-wrap items-center gap-4 p-4">
				<a href={`/products/${line.product_slug}`} class="h-16 w-16 shrink-0 overflow-hidden rounded bg-gray-100">
					{#if line.image_thumb}<img src={line.image_thumb} alt="" class="h-full w-full object-cover" />{/if}
				</a>
				<div class="min-w-0 flex-1">
					<a href={`/products/${line.product_slug}`} class="font-medium hover:underline">{line.product_name}</a>
					<div class="text-sm text-gray-500">{line.variant_label}</div>
					<div class="text-sm">{twd(line.price)}</div>
				</div>
				<div class="flex items-center gap-1">
					<button type="button" class="h-8 w-8 rounded border border-gray-300" onclick={() => cart.setQty(line.variant_id, line.qty - 1)} aria-label="減少">−</button>
					<input
						type="number"
						min="1"
						max="99"
						value={line.qty}
						onchange={(e) => cart.setQty(line.variant_id, Number(e.currentTarget.value))}
						class="w-14 rounded border border-gray-300 px-2 py-1 text-center"
						aria-label="數量"
					/>
					<button type="button" class="h-8 w-8 rounded border border-gray-300" onclick={() => cart.setQty(line.variant_id, line.qty + 1)} aria-label="增加">+</button>
				</div>
				<div class="w-24 text-right font-medium">{twd(line.price * line.qty)}</div>
				<button type="button" class="text-sm text-gray-500 hover:text-red-600" onclick={() => cart.remove(line.variant_id)}>移除</button>
			</li>
		{/each}
	</ul>
	<div class="mt-4 flex items-center justify-end gap-6">
		<div class="text-lg">小計 <span class="font-bold">{twd(cart.subtotal)}</span></div>
		<button type="button" disabled class="rounded bg-gray-400 px-6 py-2 text-white" title="結帳即將開放">前往結帳</button>
	</div>
	<p class="mt-2 text-right text-xs text-gray-500">價格與庫存會在結帳時重新確認。</p>
{/if}
```

- [ ] **Step 5: 修改 `web/src/routes/+layout.svelte`：載入購物車、加連結**

script 區塊加：

```ts
	import { onMount } from 'svelte';
	import { cart } from '$lib/cart.svelte';

	onMount(() => cart.load());
```

`<nav>` 裡 `<a href="/products" ...>全部商品</a>` 的下一行加：

```svelte
			<a href="/cart" class="hover:underline">
				購物車
				{#if cart.count > 0}
					<span class="ml-1 rounded-full bg-gray-900 px-2 py-0.5 text-xs text-white">{cart.count}</span>
				{/if}
			</a>
```

- [ ] **Step 6: 型別檢查、建置、commit**

Run: `cd web && pnpm check && pnpm build`
Expected: 0 errors、build 成功。

```bash
git add web/src
git commit -m "feat(web): 購物車 store（localStorage）、購物車頁、頁首數量"
```

---

### Task 15: 首頁與商品列表頁

**Files:**
- Create: `web/src/lib/components/ProductCard.svelte`
- Create: `web/src/routes/+page.server.ts`；覆寫 `web/src/routes/+page.svelte`
- Create: `web/src/routes/products/+page.server.ts`、`web/src/routes/products/+page.svelte`

**Interfaces:**
- Consumes: API `GET /api/products`、`GET /api/categories`；`Pagination`、`priceRange`。
- Produces: `ProductCard` 元件 props `{ item: ProductListItem }`；`/`（分類入口 + 最新 8 件）；`/products?q&category&sort&page`（用 GET 表單，沒有 JS 也能篩選）。

- [ ] **Step 1: 寫 `web/src/lib/components/ProductCard.svelte`**

```svelte
<script lang="ts">
	import { priceRange } from '$lib/format';
	import type { ProductListItem } from '$lib/types';

	let { item }: { item: ProductListItem } = $props();
</script>

<a href={`/products/${item.slug}`} class="block overflow-hidden rounded-lg border border-gray-200 bg-white">
	<div class="aspect-square bg-gray-100">
		{#if item.image_thumb}
			<img src={item.image_thumb} alt={item.name} class="h-full w-full object-cover" loading="lazy" />
		{/if}
	</div>
	<div class="p-3">
		<h3 class="line-clamp-2 text-sm">{item.name}</h3>
		<p class="mt-1 font-semibold">{priceRange(item.price_min, item.price_max)}</p>
		{#if !item.in_stock}<p class="text-xs text-red-600">已售完</p>{/if}
	</div>
</a>
```

- [ ] **Step 2: 寫首頁 `web/src/routes/+page.server.ts`，覆寫 `web/src/routes/+page.svelte`**

`web/src/routes/+page.server.ts`:

```ts
import type { PageServerLoad } from './$types';
import { serverApi } from '$lib/server/api';
import type { Category, Page, ProductListItem } from '$lib/types';

export const load: PageServerLoad = async (event) => {
	const [latest, categories] = await Promise.all([
		serverApi<Page<ProductListItem>>(event, '/api/products?sort=newest&per_page=8'),
		serverApi<Category[]>(event, '/api/categories')
	]);
	return { latest: latest.items, categories };
};
```

`web/src/routes/+page.svelte`:

```svelte
<script lang="ts">
	import ProductCard from '$lib/components/ProductCard.svelte';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
</script>

<section>
	<h2 class="text-lg font-bold">分類</h2>
	<div class="mt-3 flex flex-wrap gap-2">
		<a href="/products" class="rounded-full border border-gray-300 bg-white px-4 py-1.5 text-sm">全部</a>
		{#each data.categories as c (c.id)}
			<a href={`/products?category=${c.slug}`} class="rounded-full border border-gray-300 bg-white px-4 py-1.5 text-sm">{c.name}</a>
		{/each}
	</div>
</section>

<section class="mt-8">
	<div class="flex items-baseline justify-between">
		<h2 class="text-lg font-bold">最新商品</h2>
		<a href="/products" class="text-sm underline">看全部</a>
	</div>
	{#if data.latest.length === 0}
		<p class="mt-4 text-gray-500">商品準備中，請稍後再來。</p>
	{:else}
		<div class="mt-3 grid grid-cols-2 gap-3 md:grid-cols-4">
			{#each data.latest as item (item.slug)}
				<ProductCard {item} />
			{/each}
		</div>
	{/if}
</section>
```

- [ ] **Step 3: 寫列表頁 `web/src/routes/products/+page.server.ts` 與 `+page.svelte`**

`web/src/routes/products/+page.server.ts`:

```ts
import type { PageServerLoad } from './$types';
import { serverApi } from '$lib/server/api';
import type { Category, Page, ProductListItem } from '$lib/types';

export const load: PageServerLoad = async (event) => {
	const sp = event.url.searchParams;
	const qs = new URLSearchParams();
	for (const key of ['q', 'category', 'sort', 'page']) {
		const value = sp.get(key);
		if (value) qs.set(key, value);
	}
	const [result, categories] = await Promise.all([
		serverApi<Page<ProductListItem>>(event, `/api/products?${qs}`),
		serverApi<Category[]>(event, '/api/categories')
	]);
	return {
		result,
		categories,
		q: sp.get('q') ?? '',
		category: sp.get('category') ?? '',
		sort: sp.get('sort') ?? 'newest'
	};
};
```

`web/src/routes/products/+page.svelte`:

```svelte
<script lang="ts">
	import Pagination from '$lib/components/Pagination.svelte';
	import ProductCard from '$lib/components/ProductCard.svelte';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	const currentCategory = $derived(data.categories.find((c) => c.slug === data.category));
	const title = $derived(currentCategory ? currentCategory.name : '全部商品');
</script>

<svelte:head><title>{title}</title></svelte:head>

<h1 class="text-2xl font-bold">{title}</h1>

<form method="GET" class="mt-4 flex flex-wrap gap-2">
	<input name="q" value={data.q} placeholder="搜尋商品" class="min-w-0 flex-1 rounded border border-gray-300 px-3 py-2 text-sm" />
	<select name="category" value={data.category} class="rounded border border-gray-300 px-3 py-2 text-sm">
		<option value="">全部分類</option>
		{#each data.categories as c (c.id)}
			<option value={c.slug}>{c.name}</option>
		{/each}
	</select>
	<select name="sort" value={data.sort} class="rounded border border-gray-300 px-3 py-2 text-sm">
		<option value="newest">最新</option>
		<option value="price_asc">價格低到高</option>
		<option value="price_desc">價格高到低</option>
	</select>
	<button type="submit" class="rounded bg-gray-900 px-4 py-2 text-sm text-white">搜尋</button>
</form>

{#if data.result.items.length === 0}
	<p class="mt-8 text-center text-gray-500">沒有符合的商品</p>
{:else}
	<p class="mt-4 text-sm text-gray-500">共 {data.result.total} 件</p>
	<div class="mt-3 grid grid-cols-2 gap-3 md:grid-cols-4">
		{#each data.result.items as item (item.slug)}
			<ProductCard {item} />
		{/each}
	</div>
	<Pagination page={data.result.page} perPage={data.result.per_page} total={data.result.total} />
{/if}
```

- [ ] **Step 4: 型別檢查、建置**

Run: `cd web && pnpm check && pnpm build`
Expected: 0 errors、build 成功。

- [ ] **Step 5: 手動走一遍**

1. 首頁 `/` 有分類膠囊與最新商品（Task 13 建的上架商品要出現；草稿不出現）。
2. `/products?sort=price_desc` 排序正確；`/products?category=<slug>` 標題變成分類名；搜尋框輸入名稱關鍵字按「搜尋」網址帶 `q=`。
3. 商品卡片顯示價格範圍（多規格價不同時是「最低 ～ 最高」）；庫存全 0 的顯示「已售完」。

- [ ] **Step 6: Commit**

```bash
git add web/src
git commit -m "feat(web): 首頁與商品列表（搜尋、分類、排序、分頁）"
```

---

### Task 16: 商品頁（規格選擇、加入購物車、OG、JSON-LD）與 sitemap、robots

**Files:**
- Create: `web/src/lib/components/ProductView.svelte`
- Create: `web/src/routes/products/[slug]/+page.server.ts`、`web/src/routes/products/[slug]/+page.svelte`
- Create: `web/src/routes/robots.txt/+server.ts`、`web/src/routes/sitemap.xml/+server.ts`

**Interfaces:**
- Consumes: API `GET /api/products/{slug}`、`GET /api/products`；`cart.add`、`toast`。
- Produces: `/products/[slug]`（404 對應到 SvelteKit 的 404 頁）；`ProductView` 元件 props `{ product: ProductDetail }`；`/robots.txt`；`/sitemap.xml`（首頁、列表、所有上架商品）。`PUBLIC_BASE_URL` 從 `$env/dynamic/public` 讀（PUBLIC_ 開頭的變數只在那裡）。

- [ ] **Step 1: 寫 `web/src/lib/components/ProductView.svelte`（互動部分獨立成元件，換商品時用 key 重建就會重設選擇）**

```svelte
<script lang="ts">
	import { cart } from '$lib/cart.svelte';
	import { priceRange, twd } from '$lib/format';
	import { toast } from '$lib/toast.svelte';
	import type { ProductDetail } from '$lib/types';

	let { product: p }: { product: ProductDetail } = $props();

	let sel1 = $state<string | null>(null);
	let sel2 = $state<string | null>(null);
	let qty = $state(1);
	let activeImage = $state(0);

	const opt1Values = $derived([
		...new Set(p.variants.map((v) => v.option1_value).filter((v): v is string => v !== null))
	]);
	const opt2Values = $derived([
		...new Set(
			p.variants
				.filter((v) => sel1 === null || v.option1_value === sel1)
				.map((v) => v.option2_value)
				.filter((v): v is string => v !== null)
		)
	]);
	const selected = $derived.by(() => {
		if (!p.option1_name) return p.variants[0] ?? null;
		if (sel1 === null) return null;
		if (p.option2_name && sel2 === null) return null;
		return p.variants.find((v) => v.option1_value === sel1 && (!p.option2_name || v.option2_value === sel2)) ?? null;
	});
	const prices = $derived(p.variants.map((v) => v.price));
	const minPrice = $derived(prices.length ? Math.min(...prices) : 0);
	const maxPrice = $derived(prices.length ? Math.max(...prices) : 0);
	const shownImage = $derived(selected?.image_path ?? p.images[activeImage]?.path ?? null);

	function isAvailable(o1: string, o2: string | null): boolean {
		return p.variants.some((v) => v.option1_value === o1 && (o2 === null || v.option2_value === o2) && v.stock > 0);
	}

	function addToCart() {
		if (!selected || selected.stock <= 0) return;
		cart.add(
			{
				variant_id: selected.id,
				product_slug: p.slug,
				product_name: p.name,
				variant_label: [selected.option1_value, selected.option2_value].filter(Boolean).join(' / ') || '預設',
				price: selected.price,
				image_thumb: p.images[0]?.thumb_path ?? null
			},
			Math.min(Math.max(1, Number(qty) || 1), selected.stock)
		);
		toast.show('已加入購物車');
	}
</script>

<nav class="text-sm text-gray-500">
	<a href="/products" class="hover:underline">全部商品</a>
	{#if p.category}
		› <a href={`/products?category=${p.category.slug}`} class="hover:underline">{p.category.name}</a>
	{/if}
</nav>

<div class="mt-4 grid gap-8 md:grid-cols-2">
	<div>
		<div class="aspect-square overflow-hidden rounded-lg bg-gray-100">
			{#if shownImage}<img src={shownImage} alt={p.name} class="h-full w-full object-cover" />{/if}
		</div>
		{#if p.images.length > 1}
			<div class="mt-2 flex gap-2 overflow-x-auto">
				{#each p.images as img, i (img.path)}
					<button
						type="button"
						class="h-16 w-16 shrink-0 overflow-hidden rounded border {i === activeImage ? 'border-gray-900' : 'border-gray-200'}"
						onclick={() => (activeImage = i)}
						aria-label={`第 ${i + 1} 張圖`}
					>
						<img src={img.thumb_path} alt={img.alt} class="h-full w-full object-cover" />
					</button>
				{/each}
			</div>
		{/if}
	</div>

	<div>
		<h1 class="text-2xl font-bold">{p.name}</h1>
		<div class="mt-3 text-2xl font-semibold">
			{#if selected}
				{twd(selected.price)}
				{#if selected.compare_at_price && selected.compare_at_price > selected.price}
					<span class="ml-2 text-base font-normal text-gray-400 line-through">{twd(selected.compare_at_price)}</span>
				{/if}
			{:else}
				{priceRange(minPrice, maxPrice)}
			{/if}
		</div>

		{#if p.option1_name}
			<div class="mt-5">
				<div class="text-sm text-gray-600">{p.option1_name}</div>
				<div class="mt-2 flex flex-wrap gap-2">
					{#each opt1Values as value (value)}
						<button
							type="button"
							class="rounded border px-3 py-1.5 text-sm {sel1 === value ? 'border-gray-900 bg-gray-900 text-white' : 'border-gray-300'} {isAvailable(value, null) ? '' : 'opacity-40'}"
							onclick={() => {
								sel1 = value;
								sel2 = null;
							}}
						>
							{value}
						</button>
					{/each}
				</div>
			</div>
		{/if}
		{#if p.option2_name && sel1 !== null}
			<div class="mt-4">
				<div class="text-sm text-gray-600">{p.option2_name}</div>
				<div class="mt-2 flex flex-wrap gap-2">
					{#each opt2Values as value (value)}
						<button
							type="button"
							class="rounded border px-3 py-1.5 text-sm {sel2 === value ? 'border-gray-900 bg-gray-900 text-white' : 'border-gray-300'} {isAvailable(sel1 ?? '', value) ? '' : 'opacity-40'}"
							onclick={() => (sel2 = value)}
						>
							{value}
						</button>
					{/each}
				</div>
			</div>
		{/if}

		<div class="mt-5 text-sm text-gray-600">
			{#if selected}
				{selected.stock > 0 ? `庫存 ${selected.stock}` : '已售完'}
			{:else if p.option1_name}
				請選擇規格
			{/if}
		</div>

		<div class="mt-4 flex items-center gap-3">
			<input type="number" min="1" max={selected?.stock ?? 99} bind:value={qty} class="w-20 rounded border border-gray-300 px-3 py-2" aria-label="數量" />
			<button
				type="button"
				onclick={addToCart}
				disabled={!selected || selected.stock <= 0}
				class="rounded bg-gray-900 px-6 py-2 text-white disabled:opacity-40"
			>
				加入購物車
			</button>
		</div>

		{#if p.description}
			<div class="mt-8">
				<h2 class="font-semibold">商品說明</h2>
				<p class="mt-2 text-sm whitespace-pre-line text-gray-700">{p.description}</p>
			</div>
		{/if}
	</div>
</div>
```

- [ ] **Step 2: 寫 `web/src/routes/products/[slug]/+page.server.ts` 與 `+page.svelte`**

`web/src/routes/products/[slug]/+page.server.ts`:

```ts
import { error } from '@sveltejs/kit';
import type { PageServerLoad } from './$types';
import { ApiError, serverApi } from '$lib/server/api';
import type { ProductDetail } from '$lib/types';

export const load: PageServerLoad = async (event) => {
	try {
		const product = await serverApi<ProductDetail>(event, `/api/products/${encodeURIComponent(event.params.slug)}`);
		return { product };
	} catch (e) {
		if (e instanceof ApiError && e.status === 404) error(404, '找不到這個商品');
		throw e;
	}
};
```

`web/src/routes/products/[slug]/+page.svelte`:

```svelte
<script lang="ts">
	import { page } from '$app/state';
	import ProductView from '$lib/components/ProductView.svelte';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	const p = $derived(data.product);
	const absolute = (path: string) => `${page.url.origin}${path}`;
	const summary = $derived(p.description.replace(/\s+/g, ' ').trim().slice(0, 150));
	const prices = $derived(p.variants.map((v) => v.price));
	// JSON-LD Product（規格 §6.1）。把 < 換成 \u003c，避免描述裡的字串提早關掉 script 標籤
	const jsonLd = $derived(
		JSON.stringify({
			'@context': 'https://schema.org',
			'@type': 'Product',
			name: p.name,
			description: p.description,
			image: p.images.map((i) => absolute(i.path)),
			offers: {
				'@type': 'AggregateOffer',
				priceCurrency: 'TWD',
				lowPrice: prices.length ? Math.min(...prices) : 0,
				highPrice: prices.length ? Math.max(...prices) : 0,
				offerCount: p.variants.length,
				availability: p.variants.some((v) => v.stock > 0) ? 'https://schema.org/InStock' : 'https://schema.org/OutOfStock'
			}
		}).replace(/</g, '\\u003c')
	);
</script>

<svelte:head>
	<title>{p.name}</title>
	<meta name="description" content={summary} />
	<meta property="og:type" content="product" />
	<meta property="og:title" content={p.name} />
	<meta property="og:description" content={summary} />
	<meta property="og:url" content={absolute(`/products/${p.slug}`)} />
	{#if p.images[0]}<meta property="og:image" content={absolute(p.images[0].path)} />{/if}
	{@html `<script type="application/ld+json">${jsonLd}</script>`}
</svelte:head>

{#key p.id}
	<ProductView product={p} />
{/key}
```

- [ ] **Step 3: 寫 `web/src/routes/robots.txt/+server.ts`**

```ts
import { env } from '$env/dynamic/public';
import type { RequestHandler } from './$types';

export const GET: RequestHandler = ({ url }) => {
	const base = (env.PUBLIC_BASE_URL ?? url.origin).replace(/\/$/, '');
	const lines = [
		'User-agent: *',
		'Allow: /',
		'Disallow: /admin',
		'Disallow: /account',
		'Disallow: /cart',
		'Disallow: /checkout',
		'Disallow: /login',
		`Sitemap: ${base}/sitemap.xml`,
		''
	];
	return new Response(lines.join('\n'), { headers: { 'content-type': 'text/plain; charset=utf-8' } });
};
```

- [ ] **Step 4: 寫 `web/src/routes/sitemap.xml/+server.ts`**

```ts
import { env } from '$env/dynamic/public';
import type { RequestHandler } from './$types';
import { serverApi } from '$lib/server/api';
import type { Page, ProductListItem } from '$lib/types';

function escapeXml(s: string): string {
	return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
}

export const GET: RequestHandler = async (event) => {
	const base = (env.PUBLIC_BASE_URL ?? event.url.origin).replace(/\/$/, '');
	// 公開列表一頁最多 60 件，翻到沒有為止（上限 50 頁 = 3000 件，夠用）
	const slugs: string[] = [];
	for (let page = 1; page <= 50; page++) {
		const result = await serverApi<Page<ProductListItem>>(event, `/api/products?per_page=60&page=${page}`);
		slugs.push(...result.items.map((i) => i.slug));
		if (result.items.length < 60) break;
	}
	const paths = ['/', '/products', ...slugs.map((s) => `/products/${s}`)];
	const xml =
		'<?xml version="1.0" encoding="UTF-8"?>\n' +
		'<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n' +
		paths.map((p) => `  <url><loc>${escapeXml(base + p)}</loc></url>`).join('\n') +
		'\n</urlset>\n';
	return new Response(xml, { headers: { 'content-type': 'application/xml; charset=utf-8' } });
};
```

- [ ] **Step 5: 型別檢查、建置**

Run: `cd web && pnpm check && pnpm build`
Expected: 0 errors、build 成功。

- [ ] **Step 6: 手動走一遍**

1. 從列表點進一個兩層規格的商品：先選「口味」才出現「尺寸」；選完顯示該規格價格與庫存；庫存 0 的按鈕變淡、「加入購物車」停用。
2. 按「加入購物車」→ toast、頁首購物車數字 +1；到 `/cart` 看到那一列，改數量、移除都正常；重新整理頁面購物車還在。
3. `curl -s http://localhost:5173/products/<slug> | grep -o 'application/ld+json'` 有輸出；`curl -s http://localhost:5173/robots.txt` 看到 `Sitemap:`；`curl -s http://localhost:5173/sitemap.xml` 含該商品網址。
4. 開 `/products/not-exist` → 404 頁「找不到這個商品」。

- [ ] **Step 7: Commit**

```bash
git add web/src
git commit -m "feat(web): 商品頁（規格選擇、加入購物車、OG、JSON-LD）、sitemap、robots"
```

---

### Task 17: GitHub Actions CI

**Files:**
- Create: `.github/workflows/ci.yml`

**Interfaces:**
- Produces: push／PR 時跑兩個 job：`api`（Postgres 17 service；`cargo fmt --check`、`cargo clippy -D warnings`、`cargo test`）、`web`（`pnpm check`、`pnpm test`、`pnpm build`）（規格 §15）。

- [ ] **Step 1: 寫 `.github/workflows/ci.yml`**

```yaml
name: CI

on:
  push:
  pull_request:

jobs:
  api:
    name: api (Rust)
    runs-on: ubuntu-latest
    services:
      postgres:
        image: postgres:17
        env:
          POSTGRES_USER: dog_shop
          POSTGRES_PASSWORD: dog_shop
          POSTGRES_DB: dog_shop
        ports:
          - 5432:5432
        options: >-
          --health-cmd "pg_isready -U dog_shop -d dog_shop"
          --health-interval 5s
          --health-timeout 5s
          --health-retries 12
    env:
      DATABASE_URL: postgres://dog_shop:dog_shop@localhost:5432/dog_shop
    defaults:
      run:
        working-directory: api
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      - uses: Swatinem/rust-cache@v2
        with:
          workspaces: api
      - run: cargo fmt --all --check
      - run: cargo clippy --all-targets -- -D warnings
      - run: cargo test

  web:
    name: web (SvelteKit)
    runs-on: ubuntu-latest
    defaults:
      run:
        working-directory: web
    steps:
      - uses: actions/checkout@v4
      - uses: pnpm/action-setup@v4
        with:
          version: 10
      - uses: actions/setup-node@v4
        with:
          node-version: 24
          cache: pnpm
          cache-dependency-path: web/pnpm-lock.yaml
      - run: pnpm install --frozen-lockfile
      - run: pnpm check
      - run: pnpm test
      - run: pnpm build
```

- [ ] **Step 2: 本機用同樣的指令再確認一次（CI 沒法在本機跑，但指令要能過）**

Run: `cd api && cargo fmt --all --check && cargo clippy --all-targets -- -D warnings && DATABASE_URL=postgres://dog_shop:dog_shop@localhost:5435/dog_shop cargo test`
Expected: 全部通過。
Run: `cd web && pnpm install --frozen-lockfile && pnpm check && pnpm test && pnpm build`
Expected: 全部通過。

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: GitHub Actions（api 與 web）"
```

---

## 計畫 1 完成時的驗收清單

全部做完後，從乾淨狀態走一次（每一行都要成立）：

1. `docker compose -f deploy/docker-compose.dev.yml up -d db` → healthy。
2. `cd api && DATABASE_URL=postgres://dog_shop:dog_shop@localhost:5435/dog_shop cargo test` → 全綠（單元 + 9 個整合測試檔）。
3. `cd api && cargo run` → log 有 `api listening`；`curl -s localhost:8080/api/health` → `{"status":"ok"}`。
4. `cd api && cargo run -- create-admin admin@example.com` 能建管理員。
5. `cd web && pnpm test && pnpm check && pnpm build` → 全過。
6. `cd web && pnpm dev`：登入後台 → 建分類 → 建含圖片、兩層規格的商品並上架 → 前台首頁與列表看得到 → 商品頁選規格加入購物車 → 購物車頁數量正確、重新整理不消失。
7. 沒登入開 `/admin` 會被帶去 `/login`；一般會員（用 `customer_cookie` 那種方式建的帳號）打 `/api/admin/products` 得到 403。
8. `git log --oneline` 看到本計畫的 17 個 commit 都在 `worktree-mvp-design`；`git status` 乾淨；沒有 push。

## 交給計畫 2 的事項（寫計畫 2 時必看）

1. `domain/products.rs::write_images_and_variants` 的規格刪除要改成「有 `order_items` 引用就 `is_active=false`，沒有才刪」（規格 §10）。
2. `settings` 表加運費 key（超商運費、宅配運費、免運門檻）與 `GET /api/settings/public` 回傳；`PUT /api/admin/settings` 與 `/admin/settings` 頁。
3. `auth::session::delete_all_for_user` 已寫好，忘記密碼重設後呼叫它。
4. `/cart` 頁的「前往結帳」按鈕與 `POST /api/cart/validate`；`/checkout`；`/register`、`/forgot-password`、`/reset/[token]`、`/account/*`。
5. `hooks.server.ts` 已把 `locals.user` 準備好，`/account` 的守門照 `/admin/+layout.server.ts` 的寫法。
6. 台灣縣市／鄉鎮／郵遞區號的靜態 JSON。
7. 後台側欄 `web/src/routes/admin/+layout.svelte` 的 `links` 陣列加「設定」（計畫 2）、「訂單」（計畫 4）、「匯入」（計畫 5）。
