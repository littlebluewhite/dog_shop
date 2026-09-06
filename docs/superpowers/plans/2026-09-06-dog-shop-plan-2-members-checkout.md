# dog_shop 計畫 2／5：會員、購物車驗證與結帳下單、商店設定 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在計畫 1 的商品目錄之上，做出會員（註冊、忘記／重設密碼、會員中心、常用地址）、購物車伺服器驗證、結帳表單與下單（扣庫存、運費、發票資料、付款方式）、訂單頁、買家取消、後台商店設定（運費、免運、付款方式、寄件人、退貨門市）。不含綠界（計畫 3、4）、Email 寄送（計畫 3）。

**Architecture:** 後端在 `api/` 加一支 migration 建齊 `password_resets`、`addresses`、`orders`、`order_items`、`payments`、`shipments`、`jobs`、`cvs_store_selections`，新增 `domain/{jobs, password_resets, addresses, orders, cvs_stores}.rs` 與對應 routes；下單在單一交易內驗證、重算金額、`UPDATE ... WHERE stock >= qty` 扣庫存、寫 orders/order_items/shipments/payments 並排一筆 `send_email` job（outbox，worker 在計畫 3）。前端在 `web/` 加會員頁、結帳頁、訂單頁、後台設定頁，全部用瀏覽器端 `api()` 呼叫（不用 SvelteKit form actions），沿用計畫 1 的 `serverApi` 做 SSR 讀取。

**Tech Stack:** 同計畫 1：Rust 1.98 / axum 0.8 / sqlx 0.9 / PostgreSQL 17；SvelteKit 2 / Svelte 5 runes / Tailwind 4 / vitest；新增 Rust crate `sha2`、`hex`、`rand`，前端新增 `@playwright/test`（只在本機跑）。

**Spec:** `docs/superpowers/specs/2026-09-06-dog-shop-mvp-design.md`（本計畫負責 §3 剩餘資料表、§4 下單與取消、§5 庫存、§6.1 會員與結帳頁、§7 結帳流程（不含綠界）、§10 會員／訂單／設定 API、§11 忘記密碼與訪客 token、§15 Playwright 主流程）。計畫 1 已做的部分見 `docs/superpowers/plans/2026-09-06-dog-shop-plan-1-catalog.md`；計畫 1 最終審查的交接事項見 `docs/superpowers/reviews/2026-09-06-plan-1-final-review.md`。

## Global Constraints

- 前端一律用最新版 SvelteKit 2 / Svelte 5 runes（`$state`、`$derived`、`$props`、`$effect`、`$bindable`），不用 legacy `export let` / store 寫法（規格 §1.1）。所有變更請求走瀏覽器端 `api()`（自動帶 `X-Requested-With: fetch`），不用 SvelteKit form actions。
- 後端 Rust 1.98 stable、axum 0.8、sqlx 0.9、tokio；edition 2024（規格 §1.1）。
- 資料庫 PostgreSQL 17；主鍵 UUID v7（`Uuid::now_v7()`；session id 例外用 v4，見與規格不同之處 9）；金額 `integer` 新台幣；時間 `timestamptz` 存 UTC（規格 §3）。
- 錯誤回應格式固定 `{ "error": { "code", "message", "details" } }`；本計畫用到的 code：`VALIDATION`（400）、`UNAUTHORIZED`（401）、`FORBIDDEN`（403）、`NOT_FOUND`（404）、`OUT_OF_STOCK`（409，`details = { "items": [ { "variant_id", "available" } ] }`）、`CVS_AMOUNT_LIMIT`（400）、`CVS_STORE_REQUIRED`（400）、`RATE_LIMITED`（429）、`INTERNAL`（500）（規格 §10、§14）。欄位驗證錯誤 `details = { "fields": { "<欄位>": "<訊息>" } }`。
- 密碼 argon2id、最少 8 碼；忘記密碼 token 32 bytes 隨機，DB 只存 SHA-256（hex），1 小時有效，用過作廢，重設後清掉該使用者全部 session（規格 §11）。訪客訂單 `guest_token` 32 bytes 隨機（hex 64 字），只出現在訂單頁網址與 Email（規格 §11）。
- 速率限制：`/api/auth/login`、`/api/auth/register`、`/api/auth/forgot`、`/api/auth/reset` 共用同一個限流器，每 IP 每分鐘 10 次（規格 §11；`/me`、`/logout` 不限，見與規格不同之處 8、12）。
- 訂單狀態 `pending_payment | paid | shipped | completed | cancelled | refunded`（規格 §4）。本計畫只做 `pending_payment → cancelled`（買家取消、歸還庫存）；其餘轉移在計畫 3、4。
- 庫存：下單在同一個交易內對每個品項執行 `UPDATE product_variants SET stock = stock - $qty WHERE id = $id AND stock >= $qty`，任一列影響筆數為 0 就整筆 rollback，回 `OUT_OF_STOCK` 並列出不足品項；取消時把 `order_items.quantity` 加回去（規格 §5）。
- `order_no` = `DS` + 台北時間 yyMMdd + 4 碼隨機大寫字母數字（規格 §3）。`merchant_trade_no` = `order_no` + 兩碼流水，第一筆是 `01`（規格 §3）。
- 收件人：手機 `09` 開頭 10 碼數字；超商取貨收件人姓名 2～5 個中文字（規格 §8.3）；宅配收件人姓名 1～20 字。
- 運費：從 `settings` 的 `shipping` 讀「超商運費」「宅配運費」「免運門檻」，以商品小計 `subtotal` 比較；超商取貨商品小計上限 20,000 元，超過回 `CVS_AMOUNT_LIMIT`（規格 §7）。
- 發票：個人（載具 `1` 綠界會員／`2` 自然人憑證／`3` 手機條碼）、公司（統編 + 抬頭 + 發票地址）、捐贈（愛心碼）（規格 §7）。
- 樣式 Tailwind CSS 4，自寫元件，不引入元件庫；語言繁體中文（規格 §6.2）。
- 所有 commit 只在本機分支 `worktree-mvp-design`。**不要 `git push`、不要開 PR**（使用者的 CLAUDE.md 第 5 條）。
- 秘密（HashKey、密碼、DB 連線字串、重設 token）不進 log、不進 git（規格 §11）。

---

## 五份計畫的分工（規格涵蓋表，本計畫更新版）

計畫 1 已完成（`docs/superpowers/reviews/2026-09-06-plan-1-final-review.md`）。這張表更新了資料表的歸屬：**本計畫一次建齊它的下單交易會寫到的所有表**（含 `payments`、`shipments`、`jobs`、`cvs_store_selections`），計畫 3、4 只加程式碼、不再建表（`invoices` 除外，計畫 3 建）。

| 規格章節／需求 | 計畫 |
|---|---|
| §3 資料表：password_resets、addresses、orders、order_items、payments、shipments、jobs、cvs_store_selections；settings 的 shipping / payment_methods / sender / return_store | **2** |
| §3 資料表：invoices | 3 |
| §4 訂單狀態機：`pending_payment → cancelled`（買家取消、歸還庫存）；§5 下單扣庫存 | **2** |
| §4 付款成功、過期、遲到付款 | 3 |
| §4 出貨、完成、退回、退款、後台取消 | 4 |
| §6.1 買家頁：`/cart` 伺服器驗證、`/checkout`（不含綠界地圖）、`/orders/[id]`（不含輪詢與重付）、`/register`、`/forgot-password`、`/reset/[token]`、`/account/*` | **2** |
| §6.1 `/orders/[id]` 付款輪詢、重新付款 | 3 |
| §6.1 `/checkout` 超商電子地圖 | 4 |
| §6.1 後台 `/admin/settings` | **2** |
| §6.2 台灣縣市／鄉鎮／郵遞區號靜態 JSON | **2** |
| §7 結帳流程步驟 1、3、4、5、6（不含 `ecpay` 欄位） | **2** |
| §7 步驟 2 超商門市（`cvs-map`、`map-reply`） | 4 |
| §7 步驟 6 的 `ecpay` 回傳、7、8、9 | 3 |
| §10 API：auth register/forgot/reset、me/*、cart/validate、orders（POST、GET）、checkout/cvs-store/{token}、settings/public 擴充、admin/settings | **2** |
| §10 API：orders/{id}/repay、ecpay/payment/*、admin/orders/*、import、dashboard | 3、4、5 |
| §11 忘記密碼 token、訪客 token、register/forgot/reset 速率限制 | **2** |
| §12 Email、§9 jobs worker（本計畫只寫 outbox） | 3 |
| §15 Playwright 主流程（本機跑，不進 CI） | **2** |

## 與規格不同之處（已決定，執行時照這裡做）

前 9 條是計畫 1 的決定，原文照抄；10 起是本計畫的決定。

1. **圖片輸出 JPEG，不是 WebP**（規格 §11 說轉 WebP）。原因：`image` 0.25 的 WebP 編碼器只支援無損（lossless），商品照片會比 JPEG 大好幾倍；要有損 WebP 得綁 libwebp（C 函式庫）增加建置複雜度。決定：解碼後輸出 JPEG（主圖品質 82、縮圖 80、去透明層），檔名 `.jpg`。安全目的（重新解碼、不留原檔）不變。
2. **`users.email` 用 `text` + `unique index on lower(email)`，不用 `citext`**（規格 §3 寫 citext）。行為相同（不分大小寫唯一），少一個 extension、少一個 sqlx 型別對應風險。程式在存與查之前一律 `trim().to_lowercase()`。
3. **上傳圖片先回路徑，不先寫 `product_images`**。上傳 API 回 `{ path, thumb_path, width, height }`；商品建立／更新時把整組 `images[]` 送進來由伺服器寫入 `product_images`。這樣新商品也能先上圖再存。規格 §3 的資料表不變。
4. **規格的 `product_variants.image_id`** 由前端用 `image_path` 指定、伺服器對回 `image_id`（因為新商品的圖片還沒有 id）。
5. **「已有訂單的規格只能停用不能刪」**（規格 §10）在計畫 1 還沒有 `order_items` 表，所以更新商品時沒出現在 payload 的規格會直接刪除。**本計畫 Task 1** 建 `order_items` 時把 `domain/products.rs` 裡 `write_images_and_variants` 的刪除改成：先查 `order_items` 有沒有引用，有就 `is_active=false`、沒有才刪。
6. **`INTERNAL` 錯誤的 request id** 放在回應 header `x-request-id`（`tower-http` request-id 層），不放進 JSON body。
7. **速率限制的 key** 用 `SmartIpKeyExtractor`（先看 `X-Forwarded-For`／`X-Real-IP`，沒有才用連線 IP），因為正式環境前面永遠是 Caddy。測試請求一律帶 `X-Forwarded-For: 127.0.0.1`。
8. **速率限制只套在 `/api/auth/login`**（規格 §11 寫 `/api/auth/*`）：§6.2 的 SSR 每個請求都由 web 伺服器打一次 `/api/auth/me`，來源 IP 固定，套在 `/api/auth/*` 會把所有管理員一起鎖住；限流的目的是防暴力登入，`/me`、`/logout` 不需要。
9. **Session id 用 UUID v4，不是 UUID v7**（規格寫主鍵一律 UUID v7）。原因：v7 依時間排序、部分可預測，session 識別碼需要不可預測性，所以用 v4；資料表主鍵其餘照規格用 v7。
10. **本計畫建齊下單交易會寫到的所有資料表**：`payments`、`shipments`、`jobs`、`cvs_store_selections` 也在本計畫的 `0002_members_orders.sql` 建（欄位照規格 §3 全部列出），計畫 3、4 只加程式碼。原因：`POST /api/orders` 必須在同一個交易內存收件地址（在 `shipments`）、付款方式（在 `payments`）、outbox（`jobs`），拆開建表會讓計畫 3 得改寫本計畫的 `create_order`。`invoices` 只有計畫 3 的開立 job 會寫，留給計畫 3。
11. **忘記密碼的 token 由寄信的 job 在寄出當下產生**，不是 `POST /api/auth/forgot` 產生。`forgot` 只排一筆 `send_email` job，payload `{ "template": "password_reset", "user_id": "<uuid>" }`；計畫 3 的 job handler 呼叫 `password_resets::create(db, user_id)` 拿到原始 token、組網址、寄信。原因：規格 §11 要求 DB 只存 SHA-256；若 `forgot` 產生 token 再塞進 job payload，DB 裡就有明文 token。本計畫用 `password_resets::create / consume` 的直接測試與 `POST /api/auth/reset` 的整合測試覆蓋。
12. **`register`、`forgot`、`reset` 也套速率限制**，和 `login` 共用同一個 `GovernorLayer` 實例（同一個限流器，四條路徑加起來每 IP 每分鐘 10 次）。`forgot` 不論 Email 存不存在都回 202 `{ "ok": true }`。
13. **買家取消訂單的 API `POST /api/orders/{id}/cancel`**（規格 §10 沒列，但 §4 有「買家取消」）：只允許 `pending_payment`，其他狀態回 `VALIDATION`（規格錯誤碼清單沒有更貼切的）訊息「這筆訂單已經不能取消」。歸還庫存的 `orders::cancel` 也給計畫 3（過期）與計畫 4（後台取消）重用。
14. **`GET /api/settings/public` 改回巢狀 `{ shop, shipping, payment_methods }`**，不再是平的 shop 物件；設定分成獨立的 key：`shop`、`shipping`、`payment_methods`、`sender`、`return_store`（不把新欄位塞進 `shop`）。前端 `+layout.server.ts` 與 `ShopSettings` 型別在同一個任務改，分支隨時可建置。
15. **錯誤碼的 HTTP 狀態**（規格沒定）：`OUT_OF_STOCK` 409、`CVS_AMOUNT_LIMIT` 400、`CVS_STORE_REQUIRED` 400。
16. **`shipping.free_threshold = 0` 表示不免運**；`> 0` 時商品小計 ≥ 門檻免運。後台設定頁的說明文字照這個寫。超商金額上限比的是商品小計 `subtotal`（規格 §7 原文），不是含運費的 total。
17. **Playwright 只在本機跑、不進 CI**（規格 §15 的 CI 那行沒列 Playwright）。瀏覽器用 `PLAYWRIGHT_BROWSERS_PATH=0` 裝在 `web/node_modules` 裡（這個 sandbox 可能拒絕寫家目錄）。
18. **本計畫的 `POST /api/orders` 回 `{ order_id, order_no, guest_token }`，沒有 `ecpay` 欄位**；計畫 3 再加。訂單頁在 `pending_payment` 時顯示「付款功能準備中」。
19. **超商門市在本計畫的處理**：建 `cvs_store_selections` 表、`GET /api/checkout/cvs-store/{token}`，下單時 `shipping_method = "cvs"` 要帶 `cvs_store_token`，伺服器查表（未過期）取門市寫進 `shipments`；查不到回 `CVS_STORE_REQUIRED`。`POST /api/checkout/cvs-map` 與綠界 `map-reply` 回呼在計畫 4；結帳頁的「選擇門市」按鈕先停用並顯示「門市選擇功能準備中」，但 `?store=<token>` 進來時照樣還原門市（給計畫 4 用）。整合測試直接 INSERT `cvs_store_selections` 來測超商訂單。
20. **付款方式在本計畫只存下來**：`payments` 建一列 `pending`、`method`、`amount = total`、`merchant_trade_no = order_no || '01'`；`payments.method` 的 CHECK 多一個 `cod`（規格 §1.2 說要預留貨到付款），程式不接受 `cod`。
21. **`order_no` 的 4 碼隨機字元從 `ABCDEFGHJKLMNPQRSTUVWXYZ23456789`（去掉易混淆的 I、O、0、1）取**，仍是大寫字母數字。產生後先 `SELECT` 確認不存在再 INSERT（最多重試 5 次）；極少見的競態撞號會由 unique index 擋下並回 500，可重送。

## 環境事實（每個任務開始前都要知道）

- 這台機器：macOS、zsh、Node 24.2、pnpm 10.32、Docker（OrbStack）、Homebrew。Rust 1.98.1（rustup，在 `~/.cargo`）。**這個 sandbox 的新 shell 找不到 `cargo`，而且拒絕 `source "$HOME/.cargo/env"`**：每個含 cargo 的指令一律寫成 `export PATH="$HOME/.cargo/bin:$PATH" && cd api && cargo ...`。
- 工作目錄是 git worktree：`/Users/wilson08/IdeaProjects/dog_shop/.claude/worktrees/mvp-design`，分支 `worktree-mvp-design`。所有指令都從這裡跑（相對路徑以它為根）。**不要 `cd web`**：shell 的工作目錄會留到下一次呼叫，之後的相對路徑全部壞掉；前端指令一律寫 `pnpm -C web <script>`。
- 這個環境的 shell 會拒絕「複雜」的 git 指令（`-C`、放在迴圈、heredoc、`$(...)` 子 shell 裡的）。commit 步驟一律寫成純指令：`git add <檔案...>` 然後 `git commit -m "..."`。每次 Bash 呼叫都是新的 shell，環境變數不會留到下一次。建檔用 Write 工具、改檔用 Edit 工具，不要用 heredoc。`rm -rf` 會被使用者的 shell 攔下（提示用 `trash`）。
- **開發資料庫在 `localhost:5435`**（5432～5434 被別的專案占用）。連線字串：`postgres://dog_shop:dog_shop@localhost:5435/dog_shop`。`#[sqlx::test]` 從**行程環境變數** `DATABASE_URL` 讀連線（不讀 `.env`），測試指令一律寫成 `DATABASE_URL=postgres://dog_shop:dog_shop@localhost:5435/dog_shop cargo test`。它會為每個測試建獨立臨時資料庫並跑 `migrations/`。CI（`.github/workflows/ci.yml`）用 5432，那是 CI 容器裡的埠，不要改。
- `cargo run` 用 `dotenvy` 讀根目錄 `.env`（已從 `.env.example` 複製、埠 5435）。開發用管理員：`admin@example.com` / `admin12345`。
- 計畫 1 結束時的狀態：`cargo test` 56 個測試全綠（單元 23 + 整合 9 個檔）、`cargo fmt --all --check` 與 `cargo clippy --all-targets -- -D warnings` 乾淨；`pnpm -C web test` 9 個、`pnpm -C web check` 0 錯誤 0 警告、`pnpm -C web build` 成功。每個任務結束時這些都要維持（**svelte-check 的警告也算錯**）。
- sqlx 0.9：`FromRow` derive 在 `derive` feature；`#[sqlx::test]` 需要 `migrate`；**`query` / `query_as` / `query_scalar` 只接受 `&'static str`**，動態組的 SQL 要包 `sqlx::AssertSqlSafe(sql)` 且只能包沒有使用者輸入的字串。交易：`let mut tx = db.begin().await?;`，在 `fn(tx: &mut Transaction<'_, Postgres>)` 裡執行用 `&mut **tx`，在擁有 `tx` 的函式裡用 `&mut *tx`。
- axum 0.8 路徑參數寫法 `/api/orders/{id}`；handler 一律用 `AppJson` / `AppQuery` / `AppPath` 擷取器（`api/src/extract.rs`），不要直接用 `axum::Json`。權限用 `auth::extract::{CurrentUser, AuthUser, AdminUser}`。
- Rust 每次 commit 前跑 `cargo fmt --all` 與 `cargo clippy --all-targets -- -D warnings`。前端每次 commit 前跑 `pnpm -C web check`。
- **長時間指令要背景跑**（`run_in_background: true`、timeout 拉長），例如 `cargo test`、`cargo clippy`、`pnpm install`、`cargo run`、`pnpm -C web dev`；等通知，不要輪詢。
- **執行任務的人沒有瀏覽器**（不要用任何 `mcp__claude-in-chrome__*` 工具）。能用 `curl` 檢查的就用 curl；要點按的步驟跳過，但**一定要在任務回報裡逐條列出哪些手動步驟沒做**。Playwright（Task 16）是 headless Chromium，可以跑。
- Svelte 5 注意：把 prop 當 `$state` 初始值時用 `untrack(() => ...)` 包起來（否則 svelte-check 會警告 `state_referenced_locally`），並在頁面用 `{#key ...}` 讓資料重新載入時整個元件重建（計畫 1 的 `ProductForm` 與 `admin/categories` 就是這樣做）。`$app/state` 的 `page` 是 rune 版（不是 `$app/stores`）。
- IDE 對 `./$types` 的紅字是舊的 `.svelte-kit` 型別，不理它；以 `pnpm -C web check` 為準。
- 任務回報的順序：commit → 寫 report 檔 → 最後訊息。先讓成果落地。
- 子代理審查預算：任務審查 8～10 次工具呼叫、讀審查包一次、不重跑測試套件。

## 檔案結構（本計畫會建立或修改的所有檔案）

```
api/
  Cargo.toml                              修改：加 rand、sha2、hex
  migrations/0002_members_orders.sql      新增：8 張表 + settings 種子
  src/
    error.rs                              修改：OutOfStock、CvsAmountLimit、CvsStoreRequired
    app.rs                                修改：merge 新 router
    auth/mod.rs                           修改：pub mod tokens
    auth/tokens.rs                        新增：generate_token、sha256_hex
    domain/mod.rs                         修改：pub mod jobs, password_resets, addresses, orders, cvs_stores
    domain/jobs.rs                        新增：enqueue（outbox 寫入）
    domain/password_resets.rs             新增：create、consume
    domain/users.rs                       修改：update_profile、set_password
    domain/settings.rs                    修改：typed settings、get_all、put_all、validate
    domain/addresses.rs                   新增：Address CRUD
    domain/cvs_stores.rs                  新增：get_valid、insert（測試與計畫 4 用）
    domain/orders.rs                      新增：型別、驗證、金額、create_order、cancel、查詢
    domain/cart.rs                        新增：購物車核對（價格、庫存、上下架）
    domain/products.rs                    修改：規格刪除改「有訂單引用就停用」
    routes/mod.rs                         修改
    routes/auth.rs                        修改：register、forgot、reset
    routes/settings.rs                    修改：public 巢狀形狀
    routes/admin_settings.rs              新增：GET|PUT /api/admin/settings
    routes/me.rs                          新增：profile、addresses、orders
    routes/cart.rs                        新增：POST /api/cart/validate
    routes/checkout.rs                    新增：GET /api/checkout/cvs-store/{token}
    routes/orders.rs                      新增：POST /api/orders、GET /api/orders/{id}、POST /api/orders/{id}/cancel
  tests/
    common/mod.rs                         修改：register_cookie、active_product、cvs_store_token
    admin_products.rs                     修改：加「有訂單的規格只停用」測試
    auth_member.rs                        新增：register / forgot / reset
    settings.rs                           修改：public 巢狀、admin GET/PUT
    me.rs                                 新增：profile、addresses
    orders_domain.rs                      新增：create_order、並發不超賣、cancel
    orders.rs                             新增：HTTP 下單、查詢、取消、me/orders
    cart.rs                               新增：cart/validate、cvs-store
web/
  package.json                            修改：@playwright/test、test:e2e
  playwright.config.ts                    新增
  e2e/checkout.spec.ts                    新增
  src/lib/api.ts                          修改：ApiError.fields()
  src/lib/types.ts                        修改：Address、Settings、Order*、CartValidate*
  src/lib/labels.ts                       新增：狀態、超商、付款、發票的中文標籤
  src/lib/validation.ts                   新增：isTwMobile、isTaxId、isCvsRecipientName、isMobileBarcode、isCitizenCert、isLoveCode、isPostalCode、shippingFee
  src/lib/validation.test.ts              新增
  src/lib/checkout.ts、checkout.test.ts   新增：結帳表單型別、驗證、組 OrderInput
  src/lib/data/tw-districts.json          新增：縣市 → 鄉鎮 → 郵遞區號
  src/lib/tw-address.ts、tw-address.test.ts   新增：cities()、districts(city)、postalCode(city, district)
  src/lib/components/AddressFields.svelte            新增（會員地址頁與結帳頁共用）
  src/lib/components/checkout/InvoiceFields.svelte   新增
  src/routes/+layout.server.ts            修改：settings 巢狀
  src/routes/+layout.svelte               修改：會員中心連結
  src/routes/login/+page.svelte           修改：註冊、忘記密碼連結
  src/routes/register/+page.svelte        新增
  src/routes/register/+page.server.ts     新增（已登入就導走）
  src/routes/forgot-password/+page.svelte 新增
  src/routes/reset/[token]/+page.svelte   新增
  src/routes/account/+layout.server.ts    新增：未登入導 /login
  src/routes/account/+layout.svelte       新增：子選單
  src/routes/account/+page.svelte         新增：個人資料、改密碼
  src/routes/account/orders/+page.server.ts、+page.svelte      新增
  src/routes/account/addresses/+page.server.ts、+page.svelte   新增
  src/routes/cart/+page.svelte            修改：伺服器驗證、前往結帳
  src/routes/checkout/+page.server.ts、+page.svelte            新增
  src/routes/orders/[id]/+page.server.ts、+page.svelte         新增
  src/routes/admin/+layout.svelte         修改：設定連結
  src/routes/admin/settings/+page.server.ts、+page.svelte      新增
.gitignore                                修改：web/playwright-report/
```

## 介面總表（跨任務共用的名字，各任務的 Interfaces 區塊會再說一次）

後端 JSON 形狀（Rust struct 欄位 = JSON key = TS 型別欄位）：

- 註冊 `POST /api/auth/register` body `{ email, password, name, phone? }` → 201 + `Set-Cookie: sid` + `{ "user": UserPublic }`。Email 已存在 → 400 `VALIDATION`，`fields.email = "這個 Email 已經註冊過了"`。
- `POST /api/auth/forgot` body `{ email }` → 202 `{ "ok": true }`（永遠）。
- `POST /api/auth/reset` body `{ token, password }` → 204；token 無效 → 400 `VALIDATION`，`fields.token = "重設連結無效或已過期"`。
- `GET /api/me/profile` → `{ "user": UserPublic }`；`PUT /api/me/profile` body `{ name, phone, current_password?, new_password? }` → `{ "user": UserPublic }`。
- `Address = { id, recipient_name, phone, postal_code, city, district, street, is_default }`；`AddressInput = { recipient_name, phone, postal_code, city, district, street, is_default? }`。`GET /api/me/addresses` → `Address[]`；`POST` → 201 `Address`；`PUT /api/me/addresses/{id}` → `Address`；`DELETE` → 204。最多 10 筆。
- `GET /api/me/orders?page&per_page` → `Page<OrderSummary>`，`OrderSummary = { id, order_no, status, total, item_count, created_at }`。
- 設定：`ShopSettings = { name, description, contact_email, contact_phone }`、`ShippingSettings = { cvs_fee, home_fee, free_threshold }`、`PaymentMethods = { credit, atm, cvs_code }`、`SenderSettings = { name, phone }`、`ReturnStore = { sub_type, store_id, store_name }`（`sub_type ∈ "" | UNIMARTC2C | FAMIC2C | HILIFEC2C`）。`GET /api/settings/public` → `PublicSettings = { shop, shipping, payment_methods }`；`GET|PUT /api/admin/settings` → `AllSettings = { shop, shipping, payment_methods, sender, return_store }`（PUT 整組送、整組回）。
- `POST /api/cart/validate` body `{ items: [{ variant_id, qty }] }` → `CartValidateResponse = { items: CartCheckedLine[], subtotal, shipping: ShippingSettings, cvs_limit_exceeded }`，`CartCheckedLine = { variant_id, product_slug, product_name, variant_label, price, image_thumb, stock, qty, available, reason }`，`reason ∈ null | "unavailable" | "sold_out" | "qty_reduced"`；`qty` 是伺服器夾過的數量（`min(要求, 庫存)`），`subtotal` 只算 `available` 的列。
- `GET /api/checkout/cvs-store/{token}` → `CvsStore = { token, sub_type, store_id, store_name, store_address, store_phone }`；過期或不存在 404。
- 下單 `POST /api/orders` body `OrderInput`：
  ```
  { items: [{ variant_id, qty }], email, recipient_name, recipient_phone,
    shipping_method: "cvs" | "home", cvs_store_token?, address?: { postal_code, city, district, street },
    invoice: { type: "personal" | "company" | "donation", carrier_type?: "1" | "2" | "3", carrier_num?, tax_id?, title?, address?, love_code? },
    payment_method: "credit" | "atm" | "cvs_code", note? }
  ```
  → 201 `OrderCreated = { order_id, order_no, guest_token }`。
- `GET /api/orders/{id}`（登入者看自己的；訪客帶 `?t=<guest_token>`；都不符 → 404）→ `OrderDetail`：
  ```
  { id, order_no, status, email, recipient_name, recipient_phone, shipping_method, subtotal, shipping_fee, total, note,
    invoice_type, invoice_carrier_type, invoice_carrier_num, invoice_tax_id, invoice_title, invoice_address, invoice_love_code,
    created_at, paid_at, shipped_at, completed_at, cancelled_at, cancel_reason,
    items: [{ product_name, variant_label, unit_price, quantity, line_total, image_path }],
    shipment: { method, cvs_sub_type, cvs_store_id, cvs_store_name, cvs_store_address, home_postal_code, home_city, home_district, home_street, status, carrier, tracking_no } | null,
    payment: { method, status, amount, atm_bank_code, atm_vaccount, cvs_payment_no, expire_at } | null }
  ```
- `POST /api/orders/{id}/cancel`（同 GET 的權限規則）→ 204；非 `pending_payment` → 400 `VALIDATION`「這筆訂單已經不能取消」。
- 錯誤：`OUT_OF_STOCK` 409 `details.items = [{ variant_id, available }]`；`CVS_AMOUNT_LIMIT` 400；`CVS_STORE_REQUIRED` 400。

後端 Rust 名字（各任務會用到）：

- `auth::tokens::{generate_token() -> String /* 64 hex */, sha256_hex(&str) -> String}`
- `domain::jobs::enqueue(tx: &mut Transaction<'_, Postgres>, kind: &str, payload: serde_json::Value, dedupe_key: Option<&str>) -> Result<(), sqlx::Error>`；常數 `KIND_SEND_EMAIL = "send_email"`
- `domain::password_resets::{create(db, user_id) -> Result<String, sqlx::Error>, consume(db, raw_token) -> Result<Option<Uuid>, sqlx::Error>}`
- `domain::users::{update_profile(db, id, name, phone), set_password(db, id, hash)}`
- `domain::settings::{AllSettings, PublicSettings, ShopSettings, ShippingSettings, PaymentMethods, SenderSettings, ReturnStore, get_all(db), put_all(db, &AllSettings), validate(&AllSettings)}`
- `domain::addresses::{Address, AddressInput, list, create, update, delete, validate}`
- `domain::cvs_stores::{CvsStore, get_valid(db, token), insert(db, &CvsStore, ttl)}`
- `domain::orders::{OrderInput, OrderItemInput, OrderCreated, OrderDetail, OrderSummary, Viewer, create_order(db, input, user: Option<&User>), get_for_viewer(db, id, viewer), list_for_user(db, user_id, page, per_page), cancel(db, id, viewer, reason), shipping_fee(&ShippingSettings, method, subtotal), is_tw_mobile, is_tw_tax_id, is_cvs_recipient_name, is_mobile_barcode, is_citizen_cert, is_love_code, is_postal_code, CVS_SUBTOTAL_LIMIT = 20_000}`
- 狀態常數（都是 `&str`）：`orders::STATUS_PENDING_PAYMENT = "pending_payment"`、`STATUS_CANCELLED = "cancelled"`；`SHIPPING_CVS = "cvs"`、`SHIPPING_HOME = "home"`；`PAYMENT_CREDIT = "credit"`、`PAYMENT_ATM = "atm"`、`PAYMENT_CVS_CODE = "cvs_code"`；`INVOICE_PERSONAL`、`INVOICE_COMPANY`、`INVOICE_DONATION`；`CVS_SUB_TYPES = ["UNIMARTC2C", "FAMIC2C", "HILIFEC2C"]`。

前端共用（計畫 1 已有）：`api<T>(path, init?)`、`serverApi<T>(event, path, init?)`、`ApiError { status, code, message, details, field(name) }`、`toast.show(message)`、`cart.{lines, count, subtotal, loaded, load(), add(), setQty(), remove(), clear()}`、`twd()`、`formatDate()`、`Pagination`。本計畫新增：`validation.ts` 的函式（名稱同後端：`isTwMobile`、`isTaxId`、`isCvsRecipientName`、`isMobileBarcode`、`isCitizenCert`、`isLoveCode`、`isPostalCode`）、`tw-address.ts` 的 `cities()`、`districts(city)`、`postalCode(city, district)`、`shippingFee(shipping, method, subtotal)`（在 `validation.ts`）。

---

### Task 1: migration 0002、依賴、token 工具、規格刪除改停用

**Files:**
- Create: `api/migrations/0002_members_orders.sql`
- Create: `api/src/auth/tokens.rs`
- Modify: `api/Cargo.toml`（加 `rand`、`sha2`、`hex`）
- Modify: `api/src/auth/mod.rs`（加 `pub mod tokens;`）
- Modify: `api/src/domain/products.rs`（`write_images_and_variants` 的刪除邏輯）
- Test: `api/tests/admin_products.rs`（新增一個測試）

**Interfaces:**
- Consumes: 計畫 1 的 `products.rs::write_images_and_variants`、`tests/common`。
- Produces: 8 張新表與 settings 種子；`auth::tokens::generate_token()`（64 字 hex）、`auth::tokens::sha256_hex()`；`product_variants` 有訂單引用時 PUT 只停用不刪。

- [ ] **Step 1: 加依賴**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && cd api && cargo add rand@0.9 sha2@0.10 hex@0.4`
Expected: `Cargo.toml` 的 `[dependencies]` 多三行，`Cargo.lock` 更新。（若 `rand` 0.9 的 API 和下面程式不同，以能編譯為準並在回報說明；`rand::rng()` + `RngCore::fill_bytes` 是 0.9 的寫法。）

- [ ] **Step 2: 寫 migration `api/migrations/0002_members_orders.sql`**

```sql
-- 忘記密碼：DB 只存 token 的 SHA-256（hex），1 小時有效，用過作廢（規格 §11）
CREATE TABLE password_resets (
    token_hash text PRIMARY KEY,
    user_id    uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    expires_at timestamptz NOT NULL,
    used_at    timestamptz,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX password_resets_user_idx ON password_resets (user_id);

-- 會員常用地址
CREATE TABLE addresses (
    id             uuid PRIMARY KEY,
    user_id        uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    recipient_name text NOT NULL,
    phone          text NOT NULL,
    postal_code    text NOT NULL,
    city           text NOT NULL,
    district       text NOT NULL,
    street         text NOT NULL,
    is_default     boolean NOT NULL DEFAULT false,
    created_at     timestamptz NOT NULL DEFAULT now(),
    updated_at     timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX addresses_user_idx ON addresses (user_id, created_at);

-- 訂單主檔（規格 §3）。訪客訂單 user_id 為 NULL，用 guest_token 看訂單頁
CREATE TABLE orders (
    id                   uuid PRIMARY KEY,
    order_no             text NOT NULL UNIQUE,
    user_id              uuid REFERENCES users(id) ON DELETE SET NULL,
    guest_token          text NOT NULL,
    status               text NOT NULL DEFAULT 'pending_payment'
                         CHECK (status IN ('pending_payment', 'paid', 'shipped', 'completed', 'cancelled', 'refunded')),
    email                text NOT NULL,
    recipient_name       text NOT NULL,
    recipient_phone      text NOT NULL,
    shipping_method      text NOT NULL CHECK (shipping_method IN ('cvs', 'home')),
    subtotal             integer NOT NULL CHECK (subtotal >= 0),
    shipping_fee         integer NOT NULL CHECK (shipping_fee >= 0),
    total                integer NOT NULL CHECK (total >= 0),
    note                 text NOT NULL DEFAULT '',
    invoice_type         text NOT NULL CHECK (invoice_type IN ('personal', 'company', 'donation')),
    invoice_carrier_type text,
    invoice_carrier_num  text,
    invoice_tax_id       text,
    invoice_title        text,
    invoice_address      text,
    invoice_love_code    text,
    needs_refund         boolean NOT NULL DEFAULT false,
    created_at           timestamptz NOT NULL DEFAULT now(),
    paid_at              timestamptz,
    shipped_at           timestamptz,
    completed_at         timestamptz,
    cancelled_at         timestamptz,
    cancel_reason        text
);
CREATE INDEX orders_user_created_idx ON orders (user_id, created_at DESC);
CREATE INDEX orders_status_idx ON orders (status);
CREATE INDEX orders_created_idx ON orders (created_at DESC);

-- 下單當時的快照。variant_id 用 RESTRICT：有訂單的規格不能真刪（規格 §10）
CREATE TABLE order_items (
    id            uuid PRIMARY KEY,
    order_id      uuid NOT NULL REFERENCES orders(id) ON DELETE CASCADE,
    variant_id    uuid NOT NULL REFERENCES product_variants(id) ON DELETE RESTRICT,
    product_name  text NOT NULL,
    variant_label text NOT NULL,
    unit_price    integer NOT NULL CHECK (unit_price >= 0),
    quantity      integer NOT NULL CHECK (quantity > 0),
    line_total    integer NOT NULL CHECK (line_total >= 0),
    image_path    text,
    sort_order    integer NOT NULL DEFAULT 0
);
CREATE INDEX order_items_order_idx ON order_items (order_id, sort_order);
CREATE INDEX order_items_variant_idx ON order_items (variant_id);

-- 一次付款嘗試一列（規格 §3）。計畫 3 才會填綠界欄位；cod 預留給貨到付款（規格 §1.2）
CREATE TABLE payments (
    id                uuid PRIMARY KEY,
    order_id          uuid NOT NULL REFERENCES orders(id) ON DELETE CASCADE,
    merchant_trade_no text NOT NULL UNIQUE,
    method            text NOT NULL CHECK (method IN ('credit', 'atm', 'cvs_code', 'cod')),
    status            text NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'paid', 'failed', 'expired')),
    amount            integer NOT NULL CHECK (amount >= 0),
    ecpay_trade_no    text,
    payment_type      text,
    payment_date      timestamptz,
    atm_bank_code     text,
    atm_vaccount      text,
    cvs_payment_no    text,
    expire_at         timestamptz,
    raw               jsonb,
    created_at        timestamptz NOT NULL DEFAULT now(),
    updated_at        timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX payments_order_idx ON payments (order_id, created_at DESC);

-- 一筆訂單一筆出貨（規格 §3）。本計畫只填地址／門市與 pending；物流欄位計畫 4 填
CREATE TABLE shipments (
    id                      uuid PRIMARY KEY,
    order_id                uuid NOT NULL UNIQUE REFERENCES orders(id) ON DELETE CASCADE,
    method                  text NOT NULL CHECK (method IN ('cvs', 'home')),
    cvs_sub_type            text CHECK (cvs_sub_type IN ('UNIMARTC2C', 'FAMIC2C', 'HILIFEC2C')),
    cvs_store_id            text,
    cvs_store_name          text,
    cvs_store_address       text,
    cvs_store_phone         text,
    home_postal_code        text,
    home_city               text,
    home_district           text,
    home_street             text,
    status                  text NOT NULL DEFAULT 'pending'
                            CHECK (status IN ('pending', 'created', 'in_transit', 'arrived', 'picked_up', 'returned', 'shipped')),
    ecpay_logistics_id      text,
    ecpay_merchant_trade_no text,
    cvs_payment_no          text,
    cvs_validation_no       text,
    carrier                 text,
    tracking_no             text,
    last_status_code        text,
    last_status_msg         text,
    raw                     jsonb,
    created_at              timestamptz NOT NULL DEFAULT now(),
    updated_at              timestamptz NOT NULL DEFAULT now()
);

-- 背景工作 outbox（規格 §9）。本計畫只寫入；worker 在計畫 3
CREATE TABLE jobs (
    id           bigserial PRIMARY KEY,
    kind         text NOT NULL,
    payload      jsonb NOT NULL DEFAULT '{}'::jsonb,
    dedupe_key   text UNIQUE,
    status       text NOT NULL DEFAULT 'queued' CHECK (status IN ('queued', 'running', 'done', 'failed')),
    attempts     integer NOT NULL DEFAULT 0,
    max_attempts integer NOT NULL DEFAULT 5,
    run_at       timestamptz NOT NULL DEFAULT now(),
    last_error   text,
    created_at   timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX jobs_status_run_at_idx ON jobs (status, run_at);

-- 綠界地圖選完門市的暫存，1 小時過期（規格 §3）。本計畫只讀（下單）與測試寫入；計畫 4 的 map-reply 才會寫
CREATE TABLE cvs_store_selections (
    token         text PRIMARY KEY,
    sub_type      text NOT NULL CHECK (sub_type IN ('UNIMARTC2C', 'FAMIC2C', 'HILIFEC2C')),
    store_id      text NOT NULL,
    store_name    text NOT NULL,
    store_address text NOT NULL,
    store_phone   text NOT NULL DEFAULT '',
    created_at    timestamptz NOT NULL DEFAULT now(),
    expires_at    timestamptz NOT NULL
);

-- 商店設定的新 key（與 shop 分開，不塞進 shop）
INSERT INTO settings (key, value) VALUES
    ('shipping', '{"cvs_fee": 60, "home_fee": 100, "free_threshold": 0}'),
    ('payment_methods', '{"credit": true, "atm": true, "cvs_code": true}'),
    ('sender', '{"name": "", "phone": ""}'),
    ('return_store', '{"sub_type": "", "store_id": "", "store_name": ""}')
ON CONFLICT (key) DO NOTHING;
```

- [ ] **Step 3: 寫 `api/src/auth/tokens.rs`**

```rust
use rand::RngCore;
use sha2::{Digest, Sha256};

/// 32 bytes 隨機 → 64 字 hex。忘記密碼 token、訪客訂單 token 都用這個（規格 §11）
pub fn generate_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

/// DB 只存 token 的 SHA-256 hex（規格 §11）
pub fn sha256_hex(raw: &str) -> String {
    hex::encode(Sha256::digest(raw.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_is_64_hex_and_random() {
        let a = generate_token();
        let b = generate_token();
        assert_eq!(a.len(), 64);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
    }

    #[test]
    fn sha256_known_vector() {
        // echo -n abc | sha256sum
        assert_eq!(
            sha256_hex("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
```

在 `api/src/auth/mod.rs` 加一行 `pub mod tokens;`（保持字母順序）。

- [ ] **Step 4: 改 `api/src/domain/products.rs` 的規格刪除**

找到 `write_images_and_variants` 最後那段：

```rust
    for old in existing.iter().filter(|id| !kept.contains(id)) {
        sqlx::query("DELETE FROM product_variants WHERE id = $1")
            .bind(old)
            .execute(&mut **tx)
            .await?;
    }
```

改成：

```rust
    // 沒出現在 payload 的規格：有訂單引用就只停用（規格 §10），沒有才真刪
    for old in existing.iter().filter(|id| !kept.contains(id)) {
        let deleted = sqlx::query(
            "DELETE FROM product_variants
             WHERE id = $1 AND NOT EXISTS (SELECT 1 FROM order_items WHERE variant_id = $1)",
        )
        .bind(old)
        .execute(&mut **tx)
        .await?
        .rows_affected();
        if deleted == 0 {
            sqlx::query("UPDATE product_variants SET is_active = false WHERE id = $1")
                .bind(old)
                .execute(&mut **tx)
                .await?;
        }
    }
```

並把函式上方註解「計畫 2 建了 order_items 之後，這裡的刪除要改成…」那句刪掉（已做）。

- [ ] **Step 5: 寫測試（加到 `api/tests/admin_products.rs` 檔尾）**

```rust
/// 規格 §10：已有訂單的規格只能停用不能刪；沒訂單的照樣刪
#[sqlx::test(migrations = "./migrations")]
async fn variant_with_orders_is_deactivated_not_deleted(pool: PgPool) {
    let app = common::app(pool.clone());
    let cookie = common::admin_cookie(&app, &pool).await;
    let (status, body, _) = common::send(
        &app,
        common::req(
            "POST",
            "/api/admin/products",
            Some(&cookie),
            Some(sample_product("active")),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let id = body["id"].as_str().unwrap().to_string();
    let ordered_variant = body["variants"][0]["id"].as_str().unwrap().to_string();
    let free_variant = body["variants"][1]["id"].as_str().unwrap().to_string();
    let kept_variant = body["variants"][2].clone();

    // 直接塞一筆訂單引用第一個規格（訂單 API 在 Task 8 才有）
    let order_id = uuid::Uuid::now_v7();
    sqlx::query(
        "INSERT INTO orders (id, order_no, guest_token, email, recipient_name, recipient_phone, shipping_method,
                             subtotal, shipping_fee, total, invoice_type)
         VALUES ($1, 'DS260906TEST', 'tok', 'a@b.co', '王小明', '0912345678', 'home', 300, 100, 400, 'personal')",
    )
    .bind(order_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO order_items (id, order_id, variant_id, product_name, variant_label, unit_price, quantity, line_total)
         VALUES ($1, $2, $3, '雞肉狗糧', '雞肉 / S', 300, 1, 300)",
    )
    .bind(uuid::Uuid::now_v7())
    .bind(order_id)
    .bind(uuid::Uuid::parse_str(&ordered_variant).unwrap())
    .execute(&pool)
    .await
    .unwrap();

    // PUT 只留第三個規格：第一個（有訂單）變 is_active=false 留著，第二個（沒訂單）真的被刪
    let update = json!({
        "name": "雞肉狗糧",
        "status": "active",
        "option1_name": "口味",
        "option2_name": "尺寸",
        "images": [],
        "variants": [ {
            "id": kept_variant["id"], "option1_value": "牛肉", "option2_value": "S", "price": 320, "stock": 2
        } ]
    });
    let (status, body, _) = common::send(
        &app,
        common::req(
            "PUT",
            &format!("/api/admin/products/{id}"),
            Some(&cookie),
            Some(update),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let variants = body["variants"].as_array().unwrap();
    assert_eq!(variants.len(), 2, "{body}");
    let ordered = variants
        .iter()
        .find(|v| v["id"] == ordered_variant)
        .expect("有訂單的規格還在");
    assert_eq!(ordered["is_active"], false);
    assert!(variants.iter().all(|v| v["id"] != free_variant));
    assert!(variants.iter().any(|v| v["id"] == kept_variant["id"]));
}
```

- [ ] **Step 6: 跑測試**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && cd api && DATABASE_URL=postgres://dog_shop:dog_shop@localhost:5435/dog_shop cargo test`（背景跑）
Expected: 全綠；單元多 2 個（tokens）、整合多 1 個 → 59 個。migration 0002 在每個測試的臨時資料庫跑過（任何 SQL 錯誤會讓所有整合測試失敗）。

- [ ] **Step 7: 格式與 lint**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && cd api && cargo fmt --all && cargo clippy --all-targets -- -D warnings`
Expected: 沒有 warning。

- [ ] **Step 8: Commit**

```bash
git add api/Cargo.toml api/Cargo.lock api/migrations/0002_members_orders.sql api/src/auth/mod.rs api/src/auth/tokens.rs api/src/domain/products.rs api/tests/admin_products.rs
git commit -m "feat(api): migration 0002（會員、訂單、付款、出貨、jobs）、token 工具、有訂單的規格只停用"
```

### Task 2: 新錯誤碼、jobs outbox、忘記密碼 token

**Files:**
- Modify: `api/src/error.rs`（`OutOfStock`、`CvsAmountLimit`、`CvsStoreRequired`）
- Create: `api/src/domain/jobs.rs`、`api/src/domain/password_resets.rs`
- Modify: `api/src/domain/mod.rs`
- Test: `api/tests/jobs.rs`、`api/tests/password_resets.rs`（新增）

**Interfaces:**
- Consumes: Task 1 的 `auth::tokens`、`jobs` 與 `password_resets` 表。
- Produces: `ApiError::OutOfStock(Vec<ShortItem>)`（409）、`ApiError::CvsAmountLimit`（400）、`ApiError::CvsStoreRequired`（400）；`jobs::enqueue(tx, kind, payload, dedupe_key)`、`jobs::KIND_SEND_EMAIL`；`password_resets::create(db, user_id) -> raw token`、`password_resets::consume(db, raw) -> Option<user_id>`。

- [ ] **Step 1: 改 `api/src/error.rs`**

在檔案頂端的 `use` 加：

```rust
use serde::Serialize;
use uuid::Uuid;
```

在 `ApiError` enum 的 `NotFound` 之後加三個變體：

```rust
    /// 下單時庫存不足：列出每個不足的規格與目前可買數量（規格 §5、§10）
    #[error("部分商品庫存不足")]
    OutOfStock(Vec<ShortItem>),
    #[error("超商取貨的商品金額不能超過 20,000 元，請改用宅配")]
    CvsAmountLimit,
    #[error("請先選擇取貨門市")]
    CvsStoreRequired,
```

在 `pub type ApiResult<T>` 上方加：

```rust
#[derive(Debug, Clone, Serialize)]
pub struct ShortItem {
    pub variant_id: Uuid,
    pub available: i32,
}
```

`code()` 加：

```rust
            Self::OutOfStock(_) => "OUT_OF_STOCK",
            Self::CvsAmountLimit => "CVS_AMOUNT_LIMIT",
            Self::CvsStoreRequired => "CVS_STORE_REQUIRED",
```

`status()` 加：

```rust
            Self::OutOfStock(_) => StatusCode::CONFLICT,
            Self::CvsAmountLimit | Self::CvsStoreRequired => StatusCode::BAD_REQUEST,
```

`into_response` 的 `match &self` 在 `Self::Internal(err)` 分支前加：

```rust
            Self::OutOfStock(items) => (self.to_string(), json!({ "items": items })),
```

在 `mod tests` 加：

```rust
    #[tokio::test]
    async fn out_of_stock_envelope() {
        let id = Uuid::now_v7();
        let response = ApiError::OutOfStock(vec![ShortItem {
            variant_id: id,
            available: 2,
        }])
        .into_response();
        assert_eq!(response.status(), StatusCode::CONFLICT);
        let v = body_json(response).await;
        assert_eq!(v["error"]["code"], "OUT_OF_STOCK");
        assert_eq!(v["error"]["details"]["items"][0]["variant_id"], id.to_string());
        assert_eq!(v["error"]["details"]["items"][0]["available"], 2);
    }
```

- [ ] **Step 2: 寫 `api/src/domain/jobs.rs`**

```rust
use serde_json::Value;
use sqlx::{Postgres, Transaction};

pub const KIND_SEND_EMAIL: &str = "send_email";

/// outbox：和業務資料在同一個交易裡寫入，不會漏（規格 §9）。
/// dedupe_key 已存在就不再排（ON CONFLICT DO NOTHING）；None 不去重。worker 在計畫 3。
pub async fn enqueue(
    tx: &mut Transaction<'_, Postgres>,
    kind: &str,
    payload: Value,
    dedupe_key: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO jobs (kind, payload, dedupe_key) VALUES ($1, $2, $3)
         ON CONFLICT (dedupe_key) DO NOTHING",
    )
    .bind(kind)
    .bind(payload)
    .bind(dedupe_key)
    .execute(&mut **tx)
    .await?;
    Ok(())
}
```

- [ ] **Step 3: 寫 `api/src/domain/password_resets.rs`**

```rust
use chrono::{Duration, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::auth::tokens::{generate_token, sha256_hex};

pub const RESET_TTL_MINUTES: i64 = 60;

/// 產生原始 token 回給呼叫者（計畫 3 的寄信 job 用它組網址）；DB 只存 SHA-256（規格 §11）
pub async fn create(db: &PgPool, user_id: Uuid) -> Result<String, sqlx::Error> {
    let raw = generate_token();
    sqlx::query("INSERT INTO password_resets (token_hash, user_id, expires_at) VALUES ($1, $2, $3)")
        .bind(sha256_hex(&raw))
        .bind(user_id)
        .bind(Utc::now() + Duration::minutes(RESET_TTL_MINUTES))
        .execute(db)
        .await?;
    Ok(raw)
}

/// 用原始 token 換 user_id：要存在、沒用過、沒過期；成功同時標記用過（只能用一次）
pub async fn consume(db: &PgPool, raw: &str) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar::<_, Uuid>(
        "UPDATE password_resets SET used_at = now()
         WHERE token_hash = $1 AND used_at IS NULL AND expires_at > now()
         RETURNING user_id",
    )
    .bind(sha256_hex(raw))
    .fetch_optional(db)
    .await
}
```

`api/src/domain/mod.rs` 加 `pub mod jobs;` 與 `pub mod password_resets;`（字母順序）。

- [ ] **Step 4: 寫 `api/tests/jobs.rs`**

```rust
mod common;

use dog_shop_api::domain::jobs;
use serde_json::json;
use sqlx::PgPool;

#[sqlx::test(migrations = "./migrations")]
async fn enqueue_dedupes_by_key(pool: PgPool) {
    let mut tx = pool.begin().await.unwrap();
    jobs::enqueue(&mut tx, jobs::KIND_SEND_EMAIL, json!({ "template": "x" }), Some("email:x:1"))
        .await
        .unwrap();
    jobs::enqueue(&mut tx, jobs::KIND_SEND_EMAIL, json!({ "template": "x" }), Some("email:x:1"))
        .await
        .unwrap();
    jobs::enqueue(&mut tx, jobs::KIND_SEND_EMAIL, json!({ "template": "y" }), None)
        .await
        .unwrap();
    jobs::enqueue(&mut tx, jobs::KIND_SEND_EMAIL, json!({ "template": "y" }), None)
        .await
        .unwrap();
    tx.commit().await.unwrap();

    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM jobs")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 3, "同 key 只排一次，None 不去重");
    let (kind, status, attempts): (String, String, i32) =
        sqlx::query_as("SELECT kind, status, attempts FROM jobs ORDER BY id LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!((kind.as_str(), status.as_str(), attempts), ("send_email", "queued", 0));
}
```

- [ ] **Step 5: 寫 `api/tests/password_resets.rs`**

```rust
mod common;

use dog_shop_api::domain::{password_resets, users};
use sqlx::PgPool;

async fn user_id(pool: &PgPool) -> uuid::Uuid {
    let hash = dog_shop_api::auth::password::hash_password("password123").unwrap();
    users::create(pool, "reset@test.local", &hash, "小美", "customer")
        .await
        .unwrap()
        .id
}

#[sqlx::test(migrations = "./migrations")]
async fn token_is_single_use(pool: PgPool) {
    let uid = user_id(&pool).await;
    let raw = password_resets::create(&pool, uid).await.unwrap();
    assert_eq!(raw.len(), 64);
    // DB 裡沒有明文
    let stored: String = sqlx::query_scalar("SELECT token_hash FROM password_resets")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_ne!(stored, raw);

    assert_eq!(password_resets::consume(&pool, &raw).await.unwrap(), Some(uid));
    assert_eq!(password_resets::consume(&pool, &raw).await.unwrap(), None, "第二次要失敗");
    assert_eq!(password_resets::consume(&pool, "nope").await.unwrap(), None);
}

#[sqlx::test(migrations = "./migrations")]
async fn expired_token_is_rejected(pool: PgPool) {
    let uid = user_id(&pool).await;
    let raw = password_resets::create(&pool, uid).await.unwrap();
    sqlx::query("UPDATE password_resets SET expires_at = now() - interval '1 minute'")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(password_resets::consume(&pool, &raw).await.unwrap(), None);
}
```

- [ ] **Step 6: 跑測試、格式、lint**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && cd api && DATABASE_URL=postgres://dog_shop:dog_shop@localhost:5435/dog_shop cargo test`（背景）
Expected: 全綠，多 1 個單元（`out_of_stock_envelope`）、3 個整合 → 63 個。
Run: `export PATH="$HOME/.cargo/bin:$PATH" && cd api && cargo fmt --all && cargo clippy --all-targets -- -D warnings`

- [ ] **Step 7: Commit**

```bash
git add api/src/error.rs api/src/domain/mod.rs api/src/domain/jobs.rs api/src/domain/password_resets.rs api/tests/jobs.rs api/tests/password_resets.rs
git commit -m "feat(api): OUT_OF_STOCK 等錯誤碼、jobs outbox、忘記密碼 token"
```

### Task 3: 註冊、忘記密碼、重設密碼 API

**Files:**
- Modify: `api/src/routes/auth.rs`（三個 handler、限流套到四條路徑）
- Modify: `api/src/domain/users.rs`（`is_tw_mobile`、`create_customer`、`update_profile`、`set_password`）
- Modify: `api/tests/common/mod.rs`（`register_cookie`）
- Test: `api/tests/auth_member.rs`（新增）

**Interfaces:**
- Consumes: Task 2 的 `jobs::enqueue`、`password_resets::consume`；計畫 1 的 `session`、`cookie`、`password`。
- Produces: `POST /api/auth/register|forgot|reset`（形狀見介面總表）；`users::is_tw_mobile(&str) -> bool`（Task 6 的 `orders.rs` 會 `pub use` 它）、`users::create_customer(db, email, hash, name, phone)`、`users::update_profile(db, id, name, phone)`、`users::set_password(db, id, hash)`；測試工具 `common::register_cookie(app, email, password, name)`。

- [ ] **Step 1: 改 `api/src/domain/users.rs`**

在 `is_valid_email` 之後加：

```rust
/// 台灣手機：09 開頭共 10 碼數字（規格 §8.3）
pub fn is_tw_mobile(phone: &str) -> bool {
    phone.len() == 10 && phone.starts_with("09") && phone.bytes().all(|b| b.is_ascii_digit())
}
```

在 `create` 之後加：

```rust
/// 前台註冊：role 固定 customer，phone 可選
pub async fn create_customer(
    db: &PgPool,
    email: &str,
    password_hash: &str,
    name: &str,
    phone: Option<&str>,
) -> Result<User, sqlx::Error> {
    sqlx::query_as::<_, User>(
        "INSERT INTO users (id, email, password_hash, name, phone, role) VALUES ($1, $2, $3, $4, $5, $6) RETURNING *",
    )
    .bind(Uuid::now_v7())
    .bind(normalize_email(email))
    .bind(password_hash)
    .bind(name)
    .bind(phone)
    .bind(ROLE_CUSTOMER)
    .fetch_one(db)
    .await
}

pub async fn update_profile(
    db: &PgPool,
    id: Uuid,
    name: &str,
    phone: Option<&str>,
) -> Result<User, sqlx::Error> {
    sqlx::query_as::<_, User>(
        "UPDATE users SET name = $2, phone = $3, updated_at = now() WHERE id = $1 RETURNING *",
    )
    .bind(id)
    .bind(name)
    .bind(phone)
    .fetch_one(db)
    .await
}

pub async fn set_password(db: &PgPool, id: Uuid, password_hash: &str) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE users SET password_hash = $2, updated_at = now() WHERE id = $1")
        .bind(id)
        .bind(password_hash)
        .execute(db)
        .await?;
    Ok(())
}
```

`mod tests` 的 `email_rules` 之後加：

```rust
    #[test]
    fn mobile_rules() {
        assert!(is_tw_mobile("0912345678"));
        assert!(!is_tw_mobile("091234567"));
        assert!(!is_tw_mobile("0212345678"));
        assert!(!is_tw_mobile("09123456７8"));
    }
```

- [ ] **Step 2: 改 `api/src/routes/auth.rs`**

`use crate::{...}` 改成：

```rust
use crate::{
    auth::{
        cookie::{self, SESSION_COOKIE},
        extract::AuthUser,
        password::{MIN_PASSWORD_CHARS, hash_password_async, verify_password_async},
        session,
    },
    domain::{jobs, password_resets, users},
    error::{ApiError, ApiResult, FieldErrors},
    extract::AppJson,
    state::AppState,
};
```

`router()` 最後的 `Router::new()...` 改成（`GovernorLayer` 可以 `clone()`，四條路徑共用同一個限流器；若編譯器說不能 clone，就用同一個 `governor_conf` 建四個 layer，效果一樣）：

```rust
    Router::new()
        .route("/api/auth/login", post(login).layer(governor_layer.clone()))
        .route("/api/auth/register", post(register).layer(governor_layer.clone()))
        .route("/api/auth/forgot", post(forgot).layer(governor_layer.clone()))
        .route("/api/auth/reset", post(reset).layer(governor_layer))
        .route("/api/auth/logout", post(logout))
        .route("/api/auth/me", get(me))
```

並把 `router()` 上方註解的第一句改成「`/api/auth/login|register|forgot|reset` 共用一個限流器：每個 IP 每分鐘 10 次（規格 §11）」。

在 `me` 之後加：

```rust
#[derive(Deserialize)]
pub struct RegisterBody {
    pub email: String,
    pub password: String,
    pub name: String,
    pub phone: Option<String>,
}

/// 去空白；空字串當沒填
fn clean_opt(value: &Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

async fn register(
    State(state): State<AppState>,
    AppJson(body): AppJson<RegisterBody>,
) -> ApiResult<impl IntoResponse> {
    let mut errors = FieldErrors::new();
    let email = users::normalize_email(&body.email);
    if !users::is_valid_email(&email) {
        errors.add("email", "Email 格式不正確");
    }
    if body.password.chars().count() < MIN_PASSWORD_CHARS {
        errors.add("password", "密碼至少 8 碼");
    }
    let name = body.name.trim();
    let name_len = name.chars().count();
    if name_len == 0 || name_len > 50 {
        errors.add("name", "必填，最多 50 字");
    }
    let phone = clean_opt(&body.phone);
    if let Some(p) = &phone
        && !users::is_tw_mobile(p)
    {
        errors.add("phone", "手機格式：09 開頭共 10 碼");
    }
    errors.into_result()?;

    let hash = hash_password_async(body.password).await?;
    let user = match users::create_customer(&state.db, &email, &hash, name, phone.as_deref()).await
    {
        Ok(user) => user,
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
            return Err(ApiError::field("email", "這個 Email 已經註冊過了"));
        }
        Err(e) => return Err(e.into()),
    };
    let sid = session::create(&state.db, user.id).await?;
    let cookie = cookie::session_cookie(&sid.to_string(), state.config.cookie_secure);
    Ok((
        StatusCode::CREATED,
        AppendHeaders([(SET_COOKIE, cookie)]),
        Json(json!({ "user": user.public() })),
    ))
}

#[derive(Deserialize)]
pub struct ForgotBody {
    pub email: String,
}

/// 永遠回 202，不透露帳號存不存在。有帳號才排寄信 job；token 由計畫 3 的 job 在寄出時產生（見與規格不同之處 11）
async fn forgot(
    State(state): State<AppState>,
    AppJson(body): AppJson<ForgotBody>,
) -> ApiResult<impl IntoResponse> {
    let email = users::normalize_email(&body.email);
    if !users::is_valid_email(&email) {
        return Err(ApiError::field("email", "Email 格式不正確"));
    }
    if let Some(user) = users::find_by_email(&state.db, &email).await? {
        let mut tx = state.db.begin().await?;
        jobs::enqueue(
            &mut tx,
            jobs::KIND_SEND_EMAIL,
            json!({ "template": "password_reset", "user_id": user.id }),
            None,
        )
        .await?;
        tx.commit().await?;
    }
    Ok((StatusCode::ACCEPTED, Json(json!({ "ok": true }))))
}

#[derive(Deserialize)]
pub struct ResetBody {
    pub token: String,
    pub password: String,
}

/// token 有效才換密碼；換完把該使用者所有 session 登出（規格 §11）
async fn reset(
    State(state): State<AppState>,
    AppJson(body): AppJson<ResetBody>,
) -> ApiResult<StatusCode> {
    if body.password.chars().count() < MIN_PASSWORD_CHARS {
        return Err(ApiError::field("password", "密碼至少 8 碼"));
    }
    let Some(user_id) = password_resets::consume(&state.db, body.token.trim()).await? else {
        return Err(ApiError::field("token", "重設連結無效或已過期"));
    };
    let hash = hash_password_async(body.password).await?;
    users::set_password(&state.db, user_id, &hash).await?;
    session::delete_all_for_user(&state.db, user_id).await?;
    Ok(StatusCode::NO_CONTENT)
}
```

- [ ] **Step 3: 在 `api/tests/common/mod.rs` 檔尾加測試工具**

```rust
/// 用註冊 API 建一個會員並回 cookie（Task 3 起可用）
pub async fn register_cookie(app: &Router, email: &str, password: &str, name: &str) -> String {
    let (status, body, headers) = send(
        app,
        req(
            "POST",
            "/api/auth/register",
            None,
            Some(json!({ "email": email, "password": password, "name": name })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "register failed: {body}");
    headers
        .get(header::SET_COOKIE)
        .expect("set-cookie")
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string()
}
```

- [ ] **Step 4: 寫 `api/tests/auth_member.rs`**

```rust
mod common;

use axum::http::StatusCode;
use dog_shop_api::domain::password_resets;
use serde_json::json;
use sqlx::PgPool;

#[sqlx::test(migrations = "./migrations")]
async fn register_logs_in_and_stores_phone(pool: PgPool) {
    let app = common::app(pool.clone());
    let (status, body, headers) = common::send(
        &app,
        common::req(
            "POST",
            "/api/auth/register",
            None,
            Some(json!({
                "email": " New@Test.local ", "password": "password123", "name": " 小新 ", "phone": "0912345678"
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["user"]["email"], "new@test.local");
    assert_eq!(body["user"]["name"], "小新");
    assert_eq!(body["user"]["phone"], "0912345678");
    assert_eq!(body["user"]["role"], "customer");
    assert!(body["user"].get("password_hash").is_none());
    let cookie = headers
        .get("set-cookie")
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    assert!(cookie.starts_with("sid="));
    let (status, body, _) = common::send(
        &app,
        common::req("GET", "/api/auth/me", Some(&cookie), None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["user"]["email"], "new@test.local");
}

#[sqlx::test(migrations = "./migrations")]
async fn register_validation_and_duplicate(pool: PgPool) {
    let app = common::app(pool.clone());
    let (status, body, _) = common::send(
        &app,
        common::req(
            "POST",
            "/api/auth/register",
            None,
            Some(json!({ "email": "bad", "password": "short", "name": "", "phone": "123" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let fields = &body["error"]["details"]["fields"];
    assert_eq!(fields["email"], "Email 格式不正確");
    assert_eq!(fields["password"], "密碼至少 8 碼");
    assert_eq!(fields["name"], "必填，最多 50 字");
    assert_eq!(fields["phone"], "手機格式：09 開頭共 10 碼");

    common::register_cookie(&app, "dup@test.local", "password123", "甲").await;
    let (status, body, _) = common::send(
        &app,
        common::req(
            "POST",
            "/api/auth/register",
            None,
            Some(json!({ "email": "DUP@test.local", "password": "password123", "name": "乙" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["details"]["fields"]["email"], "這個 Email 已經註冊過了");
}

#[sqlx::test(migrations = "./migrations")]
async fn forgot_is_quiet_and_enqueues_only_for_known_email(pool: PgPool) {
    let app = common::app(pool.clone());
    let (status, body, _) = common::send(
        &app,
        common::req(
            "POST",
            "/api/auth/forgot",
            None,
            Some(json!({ "email": "ghost@test.local" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert_eq!(body["ok"], true);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM jobs")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);

    common::register_cookie(&app, "known@test.local", "password123", "甲").await;
    let (status, _, _) = common::send(
        &app,
        common::req(
            "POST",
            "/api/auth/forgot",
            None,
            Some(json!({ "email": "Known@test.local" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let (kind, payload): (String, serde_json::Value) =
        sqlx::query_as("SELECT kind, payload FROM jobs ORDER BY id DESC LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(kind, "send_email");
    assert_eq!(payload["template"], "password_reset");
    assert!(payload["user_id"].is_string());
    assert!(payload.get("token").is_none(), "payload 不能有 token");

    let (status, _, _) = common::send(
        &app,
        common::req("POST", "/api/auth/forgot", None, Some(json!({ "email": "bad" }))),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "./migrations")]
async fn reset_changes_password_and_logs_out_everywhere(pool: PgPool) {
    let app = common::app(pool.clone());
    let old_cookie = common::register_cookie(&app, "r@test.local", "password123", "甲").await;
    let user_id: uuid::Uuid = sqlx::query_scalar("SELECT id FROM users WHERE email = 'r@test.local'")
        .fetch_one(&pool)
        .await
        .unwrap();
    let raw = password_resets::create(&pool, user_id).await.unwrap();

    // 太短
    let (status, body, _) = common::send(
        &app,
        common::req(
            "POST",
            "/api/auth/reset",
            None,
            Some(json!({ "token": raw, "password": "short" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["details"]["fields"]["password"], "密碼至少 8 碼");

    let (status, _, _) = common::send(
        &app,
        common::req(
            "POST",
            "/api/auth/reset",
            None,
            Some(json!({ "token": raw, "password": "newpassword9" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    // 舊 session 全部失效
    let (status, _, _) = common::send(
        &app,
        common::req("GET", "/api/auth/me", Some(&old_cookie), None),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    // 舊密碼不能登入、新密碼可以
    let (status, _, _) = common::send(
        &app,
        common::req(
            "POST",
            "/api/auth/login",
            None,
            Some(json!({ "email": "r@test.local", "password": "password123" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    common::login(&app, "r@test.local", "newpassword9").await;

    // token 用過就失效
    let (status, body, _) = common::send(
        &app,
        common::req(
            "POST",
            "/api/auth/reset",
            None,
            Some(json!({ "token": raw, "password": "newpassword9" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["details"]["fields"]["token"], "重設連結無效或已過期");
}

#[sqlx::test(migrations = "./migrations")]
async fn register_shares_the_login_rate_limit(pool: PgPool) {
    let app = common::app(pool);
    // 同一個 IP：register 與 login 加起來 burst 10，第 11 次 429
    for i in 0..10 {
        let path = if i % 2 == 0 { "/api/auth/register" } else { "/api/auth/login" };
        let (status, _, _) = common::send(
            &app,
            common::req(
                "POST",
                path,
                None,
                Some(json!({ "email": "bad", "password": "x", "name": "" })),
            ),
        )
        .await;
        assert_ne!(status, StatusCode::TOO_MANY_REQUESTS, "第 {i} 次不該被限");
    }
    let (status, _, _) = common::send(
        &app,
        common::req(
            "POST",
            "/api/auth/forgot",
            None,
            Some(json!({ "email": "a@b.co" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
}
```

- [ ] **Step 5: 跑測試、格式、lint**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && cd api && DATABASE_URL=postgres://dog_shop:dog_shop@localhost:5435/dog_shop cargo test`（背景）
Expected: 全綠；單元多 1（`mobile_rules`）、整合多 5 → 69 個。
Run: `export PATH="$HOME/.cargo/bin:$PATH" && cd api && cargo fmt --all && cargo clippy --all-targets -- -D warnings`

- [ ] **Step 6: Commit**

```bash
git add api/src/routes/auth.rs api/src/domain/users.rs api/tests/common/mod.rs api/tests/auth_member.rs
git commit -m "feat(api): 註冊、忘記密碼、重設密碼，auth 四條路徑共用限流"
```

### Task 4: 商店設定（typed settings、public 巢狀、後台 GET/PUT）與前端對應

**Files:**
- Modify: `api/src/domain/settings.rs`（整檔重寫）
- Modify: `api/src/routes/settings.rs`
- Create: `api/src/routes/admin_settings.rs`
- Modify: `api/src/routes/mod.rs`、`api/src/app.rs`
- Modify: `web/src/lib/types.ts`、`web/src/routes/+layout.server.ts`
- Test: `api/tests/settings.rs`（整檔重寫）

**Interfaces:**
- Consumes: `users::{is_valid_email, is_tw_mobile}`（Task 3）、`AdminUser`。
- Produces: `settings::{AllSettings, PublicSettings, ShopSettings, ShippingSettings, PaymentMethods, SenderSettings, ReturnStore, CVS_SUB_TYPES, get_all, put_all, validate}`；`GET /api/settings/public` → `PublicSettings`；`GET|PUT /api/admin/settings` → `AllSettings`。前端 `PublicSettings` 型別與 layout 的 `data.settings`（`data.shop` 保留給頁首）。

- [ ] **Step 1: 重寫 `api/src/domain/settings.rs`**

```rust
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use sqlx::PgPool;

use crate::domain::users::{is_tw_mobile, is_valid_email};
use crate::error::{ApiError, FieldErrors};

pub const SHOP_KEY: &str = "shop";
pub const SHIPPING_KEY: &str = "shipping";
pub const PAYMENT_METHODS_KEY: &str = "payment_methods";
pub const SENDER_KEY: &str = "sender";
pub const RETURN_STORE_KEY: &str = "return_store";
/// 綠界 C2C 超商代碼（規格 §3）
pub const CVS_SUB_TYPES: &[&str] = &["UNIMARTC2C", "FAMIC2C", "HILIFEC2C"];

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ShopSettings {
    pub name: String,
    pub description: String,
    pub contact_email: String,
    pub contact_phone: String,
}

/// 運費（元）。free_threshold = 0 表示不免運；> 0 時商品小計 >= 門檻就免運
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ShippingSettings {
    pub cvs_fee: i32,
    pub home_fee: i32,
    pub free_threshold: i32,
}

impl Default for ShippingSettings {
    fn default() -> Self {
        Self {
            cvs_fee: 60,
            home_fee: 100,
            free_threshold: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PaymentMethods {
    pub credit: bool,
    pub atm: bool,
    pub cvs_code: bool,
}

impl Default for PaymentMethods {
    fn default() -> Self {
        Self {
            credit: true,
            atm: true,
            cvs_code: true,
        }
    }
}

/// 寄件人（計畫 4 建物流單用）
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SenderSettings {
    pub name: String,
    pub phone: String,
}

/// 退貨門市（計畫 4 建物流單用）。sub_type 空字串 = 還沒設定
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ReturnStore {
    pub sub_type: String,
    pub store_id: String,
    pub store_name: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AllSettings {
    pub shop: ShopSettings,
    pub shipping: ShippingSettings,
    pub payment_methods: PaymentMethods,
    pub sender: SenderSettings,
    pub return_store: ReturnStore,
}

/// 給前台看的部分：不含寄件人與退貨門市
#[derive(Debug, Clone, Serialize)]
pub struct PublicSettings {
    pub shop: ShopSettings,
    pub shipping: ShippingSettings,
    pub payment_methods: PaymentMethods,
}

impl AllSettings {
    pub fn public(&self) -> PublicSettings {
        PublicSettings {
            shop: self.shop.clone(),
            shipping: self.shipping.clone(),
            payment_methods: self.payment_methods.clone(),
        }
    }

    /// 去頭尾空白（後台 PUT 進來時用）
    pub fn trimmed(mut self) -> Self {
        for s in [
            &mut self.shop.name,
            &mut self.shop.description,
            &mut self.shop.contact_email,
            &mut self.shop.contact_phone,
            &mut self.sender.name,
            &mut self.sender.phone,
            &mut self.return_store.sub_type,
            &mut self.return_store.store_id,
            &mut self.return_store.store_name,
        ] {
            *s = s.trim().to_string();
        }
        self
    }
}

pub async fn get(db: &PgPool, key: &str) -> Result<Option<Value>, sqlx::Error> {
    sqlx::query_scalar::<_, Value>("SELECT value FROM settings WHERE key = $1")
        .bind(key)
        .fetch_optional(db)
        .await
}

/// 壞掉或缺欄位的 JSON 一律退回預設值，設定頁永遠打得開
fn parse<T: DeserializeOwned + Default>(value: Value) -> T {
    serde_json::from_value(value).unwrap_or_default()
}

pub async fn get_all(db: &PgPool) -> Result<AllSettings, sqlx::Error> {
    let rows: Vec<(String, Value)> = sqlx::query_as("SELECT key, value FROM settings")
        .fetch_all(db)
        .await?;
    let mut all = AllSettings::default();
    for (key, value) in rows {
        match key.as_str() {
            SHOP_KEY => all.shop = parse(value),
            SHIPPING_KEY => all.shipping = parse(value),
            PAYMENT_METHODS_KEY => all.payment_methods = parse(value),
            SENDER_KEY => all.sender = parse(value),
            RETURN_STORE_KEY => all.return_store = parse(value),
            _ => {}
        }
    }
    Ok(all)
}

/// 五把 key 整組 upsert（一個交易）
pub async fn put_all(db: &PgPool, all: &AllSettings) -> Result<(), ApiError> {
    let entries = [
        (SHOP_KEY, serde_json::to_value(&all.shop)),
        (SHIPPING_KEY, serde_json::to_value(&all.shipping)),
        (PAYMENT_METHODS_KEY, serde_json::to_value(&all.payment_methods)),
        (SENDER_KEY, serde_json::to_value(&all.sender)),
        (RETURN_STORE_KEY, serde_json::to_value(&all.return_store)),
    ];
    let mut tx = db.begin().await?;
    for (key, value) in entries {
        let value = value.map_err(|e| ApiError::Internal(e.into()))?;
        sqlx::query(
            "INSERT INTO settings (key, value, updated_at) VALUES ($1, $2, now())
             ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = now()",
        )
        .bind(key)
        .bind(value)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

pub fn validate(all: &AllSettings) -> Result<(), ApiError> {
    let mut errors = FieldErrors::new();
    let name_len = all.shop.name.chars().count();
    if name_len == 0 || name_len > 60 {
        errors.add("shop.name", "必填，最多 60 字");
    }
    if all.shop.description.chars().count() > 500 {
        errors.add("shop.description", "最多 500 字");
    }
    if !all.shop.contact_email.is_empty() && !is_valid_email(&all.shop.contact_email) {
        errors.add("shop.contact_email", "Email 格式不正確");
    }
    if all.shop.contact_phone.chars().count() > 20 {
        errors.add("shop.contact_phone", "最多 20 字");
    }
    for (field, value) in [
        ("shipping.cvs_fee", all.shipping.cvs_fee),
        ("shipping.home_fee", all.shipping.home_fee),
    ] {
        if !(0..=10_000).contains(&value) {
            errors.add(field, "0 到 10000 之間的整數");
        }
    }
    if !(0..=1_000_000).contains(&all.shipping.free_threshold) {
        errors.add(
            "shipping.free_threshold",
            "0 到 1000000 之間的整數（0 表示不免運）",
        );
    }
    let pm = &all.payment_methods;
    if !(pm.credit || pm.atm || pm.cvs_code) {
        errors.add("payment_methods", "至少要開一種付款方式");
    }
    if all.sender.name.chars().count() > 10 {
        errors.add("sender.name", "最多 10 字");
    }
    if !all.sender.phone.is_empty() && !is_tw_mobile(&all.sender.phone) {
        errors.add("sender.phone", "手機格式：09 開頭共 10 碼");
    }
    if !all.return_store.sub_type.is_empty()
        && !CVS_SUB_TYPES.contains(&all.return_store.sub_type.as_str())
    {
        errors.add(
            "return_store.sub_type",
            "只能是 UNIMARTC2C、FAMIC2C 或 HILIFEC2C",
        );
    }
    if all.return_store.store_id.chars().count() > 10 {
        errors.add("return_store.store_id", "最多 10 字");
    }
    if all.return_store.store_name.chars().count() > 30 {
        errors.add("return_store.store_name", "最多 30 字");
    }
    errors.into_result()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn defaults_are_valid() {
        let mut all = AllSettings::default();
        all.shop.name = "店".to_string();
        assert!(validate(&all).is_ok());
    }

    #[test]
    fn bad_values_are_field_errors() {
        let all = AllSettings {
            shop: ShopSettings {
                name: String::new(),
                contact_email: "nope".to_string(),
                ..Default::default()
            },
            shipping: ShippingSettings {
                cvs_fee: -1,
                home_fee: 20_000,
                free_threshold: 5,
            },
            payment_methods: PaymentMethods {
                credit: false,
                atm: false,
                cvs_code: false,
            },
            sender: SenderSettings {
                name: String::new(),
                phone: "123".to_string(),
            },
            return_store: ReturnStore {
                sub_type: "SEVEN".to_string(),
                ..Default::default()
            },
        };
        let err = validate(&all).unwrap_err();
        let ApiError::Validation { details, .. } = err else {
            panic!("expected validation");
        };
        let fields = &details["fields"];
        for key in [
            "shop.name",
            "shop.contact_email",
            "shipping.cvs_fee",
            "shipping.home_fee",
            "payment_methods",
            "sender.phone",
            "return_store.sub_type",
        ] {
            assert!(fields.get(key).is_some(), "缺 {key}: {fields}");
        }
        assert!(fields.get("shipping.free_threshold").is_none());
    }

    #[test]
    fn parse_falls_back_to_default() {
        let s: ShippingSettings = parse(json!({ "cvs_fee": "not a number" }));
        assert_eq!(s.cvs_fee, 60);
        let s: ShippingSettings = parse(json!({ "cvs_fee": 80 }));
        assert_eq!((s.cvs_fee, s.home_fee), (80, 100));
    }
}
```

- [ ] **Step 2: 改 `api/src/routes/settings.rs`**

```rust
use axum::{Json, Router, extract::State, routing::get};

use crate::{
    domain::settings::{self, PublicSettings},
    error::ApiResult,
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new().route("/api/settings/public", get(public_settings))
}

/// 前台需要的設定：商店資訊、運費、付款方式（不含寄件人與退貨門市）
async fn public_settings(State(state): State<AppState>) -> ApiResult<Json<PublicSettings>> {
    Ok(Json(settings::get_all(&state.db).await?.public()))
}
```

- [ ] **Step 3: 寫 `api/src/routes/admin_settings.rs`**

```rust
use axum::{Json, Router, extract::State, routing::get};

use crate::{
    auth::extract::AdminUser,
    domain::settings::{self, AllSettings},
    error::ApiResult,
    extract::AppJson,
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new().route("/api/admin/settings", get(get_all).put(put_all))
}

async fn get_all(_admin: AdminUser, State(state): State<AppState>) -> ApiResult<Json<AllSettings>> {
    Ok(Json(settings::get_all(&state.db).await?))
}

/// 整組送、整組存、整組回
async fn put_all(
    _admin: AdminUser,
    State(state): State<AppState>,
    AppJson(input): AppJson<AllSettings>,
) -> ApiResult<Json<AllSettings>> {
    let input = input.trimmed();
    settings::validate(&input)?;
    settings::put_all(&state.db, &input).await?;
    Ok(Json(settings::get_all(&state.db).await?))
}
```

`api/src/routes/mod.rs` 加 `pub mod admin_settings;`；`api/src/app.rs` 在 `.merge(routes::settings::router())` 下一行加 `.merge(routes::admin_settings::router())`。

- [ ] **Step 4: 重寫 `api/tests/settings.rs`**

```rust
mod common;

use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

#[sqlx::test(migrations = "./migrations")]
async fn public_settings_are_nested_with_defaults(pool: PgPool) {
    let app = common::app(pool);
    let (status, body, _) =
        common::send(&app, common::req("GET", "/api/settings/public", None, None)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["shop"]["name"], "dog_shop");
    assert_eq!(body["shipping"]["cvs_fee"], 60);
    assert_eq!(body["shipping"]["home_fee"], 100);
    assert_eq!(body["shipping"]["free_threshold"], 0);
    assert_eq!(body["payment_methods"]["credit"], true);
    assert!(body.get("sender").is_none(), "寄件人不公開");
    assert!(body.get("return_store").is_none());
}

#[sqlx::test(migrations = "./migrations")]
async fn admin_settings_require_admin(pool: PgPool) {
    let app = common::app(pool.clone());
    let (status, _, _) =
        common::send(&app, common::req("GET", "/api/admin/settings", None, None)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let customer = common::customer_cookie(&app, &pool).await;
    let (status, _, _) = common::send(
        &app,
        common::req("GET", "/api/admin/settings", Some(&customer), None),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = "./migrations")]
async fn admin_can_read_validate_and_update(pool: PgPool) {
    let app = common::app(pool.clone());
    let cookie = common::admin_cookie(&app, &pool).await;

    let (status, body, _) = common::send(
        &app,
        common::req("GET", "/api/admin/settings", Some(&cookie), None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["sender"]["name"], "");
    assert_eq!(body["return_store"]["sub_type"], "");

    // 驗證失敗：整組回欄位錯誤（key 用點分路徑）
    let mut bad = body.clone();
    bad["shop"]["name"] = json!("");
    bad["payment_methods"] = json!({ "credit": false, "atm": false, "cvs_code": false });
    let (status, err, _) = common::send(
        &app,
        common::req("PUT", "/api/admin/settings", Some(&cookie), Some(bad)),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(err["error"]["details"]["fields"]["shop.name"], "必填，最多 60 字");
    assert_eq!(err["error"]["details"]["fields"]["payment_methods"], "至少要開一種付款方式");

    // 成功：回整組，public 也跟著變
    let mut good = body.clone();
    good["shop"]["name"] = json!(" 汪汪商店 ");
    good["shipping"]["free_threshold"] = json!(1000);
    good["payment_methods"]["cvs_code"] = json!(false);
    good["sender"] = json!({ "name": "老闆", "phone": "0912345678" });
    good["return_store"] = json!({ "sub_type": "UNIMARTC2C", "store_id": "123456", "store_name": "測試門市" });
    let (status, saved, _) = common::send(
        &app,
        common::req("PUT", "/api/admin/settings", Some(&cookie), Some(good)),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{saved}");
    assert_eq!(saved["shop"]["name"], "汪汪商店");
    assert_eq!(saved["shipping"]["free_threshold"], 1000);
    assert_eq!(saved["payment_methods"]["cvs_code"], false);
    assert_eq!(saved["return_store"]["store_name"], "測試門市");

    let (_, public, _) =
        common::send(&app, common::req("GET", "/api/settings/public", None, None)).await;
    assert_eq!(public["shop"]["name"], "汪汪商店");
    assert_eq!(public["shipping"]["free_threshold"], 1000);
    assert_eq!(public["payment_methods"]["cvs_code"], false);
}
```

- [ ] **Step 5: 前端跟著改**

`web/src/lib/types.ts` 的 `ShopSettings` 之後加：

```ts
export type ShippingSettings = { cvs_fee: number; home_fee: number; free_threshold: number };
export type PaymentMethods = { credit: boolean; atm: boolean; cvs_code: boolean };
export type SenderSettings = { name: string; phone: string };
export type ReturnStore = { sub_type: '' | 'UNIMARTC2C' | 'FAMIC2C' | 'HILIFEC2C'; store_id: string; store_name: string };
export type PublicSettings = { shop: ShopSettings; shipping: ShippingSettings; payment_methods: PaymentMethods };
export type AllSettings = PublicSettings & { sender: SenderSettings; return_store: ReturnStore };
```

`web/src/routes/+layout.server.ts` 整檔改成：

```ts
import type { LayoutServerLoad } from './$types';
import { serverApi } from '$lib/server/api';
import type { PublicSettings } from '$lib/types';

export const load: LayoutServerLoad = async (event) => {
	const settings = await serverApi<PublicSettings>(event, '/api/settings/public');
	return { user: event.locals.user, shop: settings.shop, settings };
};
```

（`+layout.svelte` 用的 `data.shop.name` 不用改。）

- [ ] **Step 6: 跑測試、格式、lint、前端檢查**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && cd api && DATABASE_URL=postgres://dog_shop:dog_shop@localhost:5435/dog_shop cargo test`（背景）
Expected: 全綠；settings 單元 3 個、整合 3 個（取代原本的 1 個）。
Run: `export PATH="$HOME/.cargo/bin:$PATH" && cd api && cargo fmt --all && cargo clippy --all-targets -- -D warnings`
Run: `pnpm -C web check && pnpm -C web build`
Expected: 0 errors 0 warnings；build 成功。

- [ ] **Step 7: Commit**

```bash
git add api/src/domain/settings.rs api/src/routes/settings.rs api/src/routes/admin_settings.rs api/src/routes/mod.rs api/src/app.rs api/tests/settings.rs web/src/lib/types.ts web/src/routes/+layout.server.ts
git commit -m "feat: 商店設定分 key（運費、付款方式、寄件人、退貨門市），public 巢狀，後台 GET/PUT"
```

### Task 5: 會員個人資料與常用地址 API

**Files:**
- Create: `api/src/domain/addresses.rs`、`api/src/routes/me.rs`
- Modify: `api/src/domain/mod.rs`、`api/src/routes/mod.rs`、`api/src/app.rs`
- Test: `api/tests/me.rs`（新增）

**Interfaces:**
- Consumes: `AuthUser`、`users::{update_profile, set_password, is_tw_mobile}`、`password::{verify_password_async, hash_password_async, MIN_PASSWORD_CHARS}`。
- Produces: `addresses::{Address, AddressInput, MAX_ADDRESSES, is_postal_code, validate, list, create, update, delete}`；`GET|PUT /api/me/profile`、`GET|POST /api/me/addresses`、`PUT|DELETE /api/me/addresses/{id}`（`GET /api/me/orders` 在 Task 8 加進同一個 router）。

- [ ] **Step 1: 寫 `api/src/domain/addresses.rs`**

```rust
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::users::is_tw_mobile;
use crate::error::{ApiError, FieldErrors};

pub const MAX_ADDRESSES: i64 = 10;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Address {
    pub id: Uuid,
    pub recipient_name: String,
    pub phone: String,
    pub postal_code: String,
    pub city: String,
    pub district: String,
    pub street: String,
    pub is_default: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AddressInput {
    pub recipient_name: String,
    pub phone: String,
    pub postal_code: String,
    pub city: String,
    pub district: String,
    pub street: String,
    #[serde(default)]
    pub is_default: bool,
}

/// 郵遞區號 3～5 碼數字（前端從靜態 JSON 帶入，這裡只驗格式，不驗對不對）
pub fn is_postal_code(code: &str) -> bool {
    (3..=5).contains(&code.len()) && code.bytes().all(|b| b.is_ascii_digit())
}

fn len_between(value: &str, min: usize, max: usize) -> bool {
    let n = value.chars().count();
    n >= min && n <= max
}

impl AddressInput {
    pub fn trimmed(mut self) -> Self {
        for s in [
            &mut self.recipient_name,
            &mut self.phone,
            &mut self.postal_code,
            &mut self.city,
            &mut self.district,
            &mut self.street,
        ] {
            *s = s.trim().to_string();
        }
        self
    }
}

pub fn validate(input: &AddressInput) -> Result<(), ApiError> {
    let mut errors = FieldErrors::new();
    if !len_between(&input.recipient_name, 1, 20) {
        errors.add("recipient_name", "必填，最多 20 字");
    }
    if !is_tw_mobile(&input.phone) {
        errors.add("phone", "手機格式：09 開頭共 10 碼");
    }
    if !is_postal_code(&input.postal_code) {
        errors.add("postal_code", "郵遞區號 3～5 碼數字");
    }
    if !len_between(&input.city, 1, 10) {
        errors.add("city", "請選縣市");
    }
    if !len_between(&input.district, 1, 10) {
        errors.add("district", "請選鄉鎮市區");
    }
    if !len_between(&input.street, 1, 100) {
        errors.add("street", "必填，最多 100 字");
    }
    errors.into_result()
}

// 三個查詢的 SELECT / RETURNING 欄位順序都要和 Address 的欄位一致
pub async fn list(db: &PgPool, user_id: Uuid) -> Result<Vec<Address>, sqlx::Error> {
    sqlx::query_as::<_, Address>(
        "SELECT id, recipient_name, phone, postal_code, city, district, street, is_default
         FROM addresses WHERE user_id = $1 ORDER BY is_default DESC, created_at",
    )
    .bind(user_id)
    .fetch_all(db)
    .await
}

/// 第一筆自動成為預設；is_default = true 會把其他筆取消預設。最多 MAX_ADDRESSES 筆
pub async fn create(db: &PgPool, user_id: Uuid, input: AddressInput) -> Result<Address, ApiError> {
    let input = input.trimmed();
    validate(&input)?;
    let mut tx = db.begin().await?;
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM addresses WHERE user_id = $1")
        .bind(user_id)
        .fetch_one(&mut *tx)
        .await?;
    if count >= MAX_ADDRESSES {
        return Err(ApiError::field("recipient_name", "最多 10 筆常用地址"));
    }
    let make_default = input.is_default || count == 0;
    if make_default {
        sqlx::query("UPDATE addresses SET is_default = false WHERE user_id = $1")
            .bind(user_id)
            .execute(&mut *tx)
            .await?;
    }
    let address = sqlx::query_as::<_, Address>(
        "INSERT INTO addresses (id, user_id, recipient_name, phone, postal_code, city, district, street, is_default)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
         RETURNING id, recipient_name, phone, postal_code, city, district, street, is_default",
    )
    .bind(Uuid::now_v7())
    .bind(user_id)
    .bind(&input.recipient_name)
    .bind(&input.phone)
    .bind(&input.postal_code)
    .bind(&input.city)
    .bind(&input.district)
    .bind(&input.street)
    .bind(make_default)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(address)
}

/// 只能改自己的；找不到回 NotFound
pub async fn update(
    db: &PgPool,
    user_id: Uuid,
    id: Uuid,
    input: AddressInput,
) -> Result<Address, ApiError> {
    let input = input.trimmed();
    validate(&input)?;
    let mut tx = db.begin().await?;
    if input.is_default {
        sqlx::query("UPDATE addresses SET is_default = false WHERE user_id = $1")
            .bind(user_id)
            .execute(&mut *tx)
            .await?;
    }
    let address = sqlx::query_as::<_, Address>(
        "UPDATE addresses SET recipient_name = $3, phone = $4, postal_code = $5, city = $6, district = $7,
                street = $8, is_default = $9, updated_at = now()
         WHERE id = $1 AND user_id = $2
         RETURNING id, recipient_name, phone, postal_code, city, district, street, is_default",
    )
    .bind(id)
    .bind(user_id)
    .bind(&input.recipient_name)
    .bind(&input.phone)
    .bind(&input.postal_code)
    .bind(&input.city)
    .bind(&input.district)
    .bind(&input.street)
    .bind(input.is_default)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(ApiError::NotFound)?;
    tx.commit().await?;
    Ok(address)
}

pub async fn delete(db: &PgPool, user_id: Uuid, id: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM addresses WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(user_id)
        .execute(db)
        .await?;
    Ok(result.rows_affected() > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn postal_code_rules() {
        assert!(is_postal_code("100"));
        assert!(is_postal_code("10058"));
        assert!(!is_postal_code("10"));
        assert!(!is_postal_code("100a"));
        assert!(!is_postal_code("100058"));
    }
}
```

- [ ] **Step 2: 寫 `api/src/routes/me.rs`**

```rust
use axum::{Json, Router, extract::State, http::StatusCode, routing::get};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    auth::{
        extract::AuthUser,
        password::{MIN_PASSWORD_CHARS, hash_password_async, verify_password_async},
    },
    domain::{
        addresses::{self, Address, AddressInput},
        users,
    },
    error::{ApiError, ApiResult, FieldErrors},
    extract::{AppJson, AppPath},
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/me/profile", get(get_profile).put(put_profile))
        .route("/api/me/addresses", get(list_addresses).post(create_address))
        .route(
            "/api/me/addresses/{id}",
            axum::routing::put(update_address).delete(delete_address),
        )
}

async fn get_profile(AuthUser(user): AuthUser) -> Json<Value> {
    Json(json!({ "user": user.public() }))
}

#[derive(Deserialize)]
pub struct ProfileBody {
    pub name: String,
    pub phone: Option<String>,
    pub current_password: Option<String>,
    pub new_password: Option<String>,
}

fn clean_opt(value: &Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// 改名字、手機；要改密碼就要一起送 current_password（驗證）與 new_password
async fn put_profile(
    AuthUser(user): AuthUser,
    State(state): State<AppState>,
    AppJson(body): AppJson<ProfileBody>,
) -> ApiResult<Json<Value>> {
    let mut errors = FieldErrors::new();
    let name = body.name.trim();
    let name_len = name.chars().count();
    if name_len == 0 || name_len > 50 {
        errors.add("name", "必填，最多 50 字");
    }
    let phone = clean_opt(&body.phone);
    if let Some(p) = &phone
        && !users::is_tw_mobile(p)
    {
        errors.add("phone", "手機格式：09 開頭共 10 碼");
    }
    let new_password = clean_opt(&body.new_password);
    if let Some(new) = &new_password {
        if new.chars().count() < MIN_PASSWORD_CHARS {
            errors.add("new_password", "密碼至少 8 碼");
        }
        let current = body.current_password.clone().unwrap_or_default();
        if !verify_password_async(current, user.password_hash.clone()).await? {
            errors.add("current_password", "目前密碼錯誤");
        }
    }
    errors.into_result()?;

    let updated = users::update_profile(&state.db, user.id, name, phone.as_deref()).await?;
    if let Some(new) = new_password {
        let hash = hash_password_async(new).await?;
        users::set_password(&state.db, user.id, &hash).await?;
    }
    Ok(Json(json!({ "user": updated.public() })))
}

async fn list_addresses(
    AuthUser(user): AuthUser,
    State(state): State<AppState>,
) -> ApiResult<Json<Vec<Address>>> {
    Ok(Json(addresses::list(&state.db, user.id).await?))
}

async fn create_address(
    AuthUser(user): AuthUser,
    State(state): State<AppState>,
    AppJson(input): AppJson<AddressInput>,
) -> ApiResult<(StatusCode, Json<Address>)> {
    let address = addresses::create(&state.db, user.id, input).await?;
    Ok((StatusCode::CREATED, Json(address)))
}

async fn update_address(
    AuthUser(user): AuthUser,
    State(state): State<AppState>,
    AppPath(id): AppPath<Uuid>,
    AppJson(input): AppJson<AddressInput>,
) -> ApiResult<Json<Address>> {
    Ok(Json(addresses::update(&state.db, user.id, id, input).await?))
}

async fn delete_address(
    AuthUser(user): AuthUser,
    State(state): State<AppState>,
    AppPath(id): AppPath<Uuid>,
) -> ApiResult<StatusCode> {
    if addresses::delete(&state.db, user.id, id).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound)
    }
}
```

`api/src/domain/mod.rs` 加 `pub mod addresses;`；`api/src/routes/mod.rs` 加 `pub mod me;`；`api/src/app.rs` 在 `.merge(routes::auth::router())` 下一行加 `.merge(routes::me::router())`。

- [ ] **Step 3: 寫 `api/tests/me.rs`**

```rust
mod common;

use axum::http::StatusCode;
use serde_json::{Value, json};
use sqlx::PgPool;

fn address(name: &str, is_default: bool) -> Value {
    json!({
        "recipient_name": name, "phone": "0912345678", "postal_code": "100",
        "city": "臺北市", "district": "中正區", "street": "重慶南路一段 122 號", "is_default": is_default
    })
}

#[sqlx::test(migrations = "./migrations")]
async fn profile_update_and_password_change(pool: PgPool) {
    let app = common::app(pool.clone());
    let cookie = common::register_cookie(&app, "p@test.local", "password123", "甲").await;

    let (status, body, _) = common::send(
        &app,
        common::req("GET", "/api/me/profile", Some(&cookie), None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["user"]["name"], "甲");

    // 改名 + 手機
    let (status, body, _) = common::send(
        &app,
        common::req(
            "PUT",
            "/api/me/profile",
            Some(&cookie),
            Some(json!({ "name": "乙", "phone": "0987654321" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["user"]["name"], "乙");
    assert_eq!(body["user"]["phone"], "0987654321");

    // 目前密碼錯 → 欄位錯誤，名字也不會被改
    let (status, body, _) = common::send(
        &app,
        common::req(
            "PUT",
            "/api/me/profile",
            Some(&cookie),
            Some(json!({ "name": "丙", "current_password": "wrong", "new_password": "newpassword9" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["details"]["fields"]["current_password"], "目前密碼錯誤");
    let (_, body, _) = common::send(
        &app,
        common::req("GET", "/api/me/profile", Some(&cookie), None),
    )
    .await;
    assert_eq!(body["user"]["name"], "乙");

    // 改密碼成功 → 新密碼能登入
    let (status, _, _) = common::send(
        &app,
        common::req(
            "PUT",
            "/api/me/profile",
            Some(&cookie),
            Some(json!({ "name": "乙", "current_password": "password123", "new_password": "newpassword9" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    common::login(&app, "p@test.local", "newpassword9").await;

    // 沒登入 → 401
    let (status, _, _) = common::send(&app, common::req("GET", "/api/me/profile", None, None)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "./migrations")]
async fn address_crud_and_default_handling(pool: PgPool) {
    let app = common::app(pool.clone());
    let cookie = common::register_cookie(&app, "a@test.local", "password123", "甲").await;
    let other = common::register_cookie(&app, "b@test.local", "password123", "乙").await;

    // 驗證
    let (status, body, _) = common::send(
        &app,
        common::req(
            "POST",
            "/api/me/addresses",
            Some(&cookie),
            Some(json!({ "recipient_name": "", "phone": "1", "postal_code": "x", "city": "", "district": "", "street": "" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let fields = &body["error"]["details"]["fields"];
    for key in ["recipient_name", "phone", "postal_code", "city", "district", "street"] {
        assert!(fields.get(key).is_some(), "缺 {key}");
    }

    // 第一筆自動預設
    let (status, first, _) = common::send(
        &app,
        common::req("POST", "/api/me/addresses", Some(&cookie), Some(address("甲", false))),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{first}");
    assert_eq!(first["is_default"], true);
    // 第二筆指定預設 → 第一筆取消預設
    let (status, second, _) = common::send(
        &app,
        common::req("POST", "/api/me/addresses", Some(&cookie), Some(address("乙", true))),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(second["is_default"], true);
    let (_, list, _) = common::send(
        &app,
        common::req("GET", "/api/me/addresses", Some(&cookie), None),
    )
    .await;
    let list = list.as_array().unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0]["id"], second["id"], "預設排最前");
    assert_eq!(list[1]["is_default"], false);

    // 別人看不到、改不到、刪不到
    let first_id = first["id"].as_str().unwrap();
    let (_, other_list, _) = common::send(
        &app,
        common::req("GET", "/api/me/addresses", Some(&other), None),
    )
    .await;
    assert_eq!(other_list.as_array().unwrap().len(), 0);
    let (status, _, _) = common::send(
        &app,
        common::req(
            "PUT",
            &format!("/api/me/addresses/{first_id}"),
            Some(&other),
            Some(address("駭客", false)),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _, _) = common::send(
        &app,
        common::req("DELETE", &format!("/api/me/addresses/{first_id}"), Some(&other), None),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // 自己改、自己刪
    let (status, updated, _) = common::send(
        &app,
        common::req(
            "PUT",
            &format!("/api/me/addresses/{first_id}"),
            Some(&cookie),
            Some(address("甲改", true)),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{updated}");
    assert_eq!(updated["recipient_name"], "甲改");
    assert_eq!(updated["is_default"], true);
    let (status, _, _) = common::send(
        &app,
        common::req("DELETE", &format!("/api/me/addresses/{first_id}"), Some(&cookie), None),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, list, _) = common::send(
        &app,
        common::req("GET", "/api/me/addresses", Some(&cookie), None),
    )
    .await;
    assert_eq!(list.as_array().unwrap().len(), 1);
}

#[sqlx::test(migrations = "./migrations")]
async fn at_most_ten_addresses(pool: PgPool) {
    let app = common::app(pool.clone());
    let cookie = common::register_cookie(&app, "m@test.local", "password123", "甲").await;
    for i in 0..10 {
        let (status, body, _) = common::send(
            &app,
            common::req(
                "POST",
                "/api/me/addresses",
                Some(&cookie),
                Some(address(&format!("第{i}"), false)),
            ),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
    }
    let (status, body, _) = common::send(
        &app,
        common::req("POST", "/api/me/addresses", Some(&cookie), Some(address("第11", false))),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["details"]["fields"]["recipient_name"], "最多 10 筆常用地址");
}
```

- [ ] **Step 4: 跑測試、格式、lint**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && cd api && DATABASE_URL=postgres://dog_shop:dog_shop@localhost:5435/dog_shop cargo test`（背景）
Expected: 全綠；單元多 2、整合多 3。
Run: `export PATH="$HOME/.cargo/bin:$PATH" && cd api && cargo fmt --all && cargo clippy --all-targets -- -D warnings`

- [ ] **Step 5: Commit**

```bash
git add api/src/domain/addresses.rs api/src/domain/mod.rs api/src/routes/me.rs api/src/routes/mod.rs api/src/app.rs api/tests/me.rs
git commit -m "feat(api): 會員個人資料、改密碼、常用地址 CRUD"
```

### Task 6: 訂單領域（一）：型別、驗證、運費（純函式）

**Files:**
- Create: `api/src/domain/orders.rs`（本任務寫前半；Task 7 接著加後半）
- Modify: `api/src/domain/mod.rs`

**Interfaces:**
- Consumes: `settings::{ShippingSettings, PaymentMethods, CVS_SUB_TYPES}`、`users::{is_valid_email, is_tw_mobile}`、`addresses::is_postal_code`、`error::{ApiError, FieldErrors}`。
- Produces: 常數（`STATUS_*`、`SHIPPING_*`、`PAYMENT_*`、`INVOICE_*`、`CVS_SUBTOTAL_LIMIT`、`MAX_QTY_PER_LINE`、`MAX_LINES`）；型別 `OrderInput`、`OrderItemInput`、`HomeAddress`、`InvoiceInput`；函式 `is_cvs_recipient_name`、`is_mobile_barcode`、`is_citizen_cert`、`is_love_code`、`is_tw_tax_id`、`shipping_fee`、`merge_items`、`validate_input`；`pub use` 的 `is_tw_mobile`、`is_postal_code`。

- [ ] **Step 1: 寫 `api/src/domain/orders.rs`（前半）**

```rust
use serde::Deserialize;
use uuid::Uuid;

pub use crate::domain::addresses::is_postal_code;
pub use crate::domain::users::is_tw_mobile;
use crate::domain::settings::{PaymentMethods, ShippingSettings};
use crate::domain::users::{is_valid_email, normalize_email};
use crate::error::{ApiError, FieldErrors};

// ───── 常數（字串欄位的合法值，與 migration 的 CHECK 一致）─────

pub const STATUS_PENDING_PAYMENT: &str = "pending_payment";
pub const STATUS_PAID: &str = "paid";
pub const STATUS_SHIPPED: &str = "shipped";
pub const STATUS_COMPLETED: &str = "completed";
pub const STATUS_CANCELLED: &str = "cancelled";
pub const STATUS_REFUNDED: &str = "refunded";

pub const SHIPPING_CVS: &str = "cvs";
pub const SHIPPING_HOME: &str = "home";

pub const PAYMENT_CREDIT: &str = "credit";
pub const PAYMENT_ATM: &str = "atm";
pub const PAYMENT_CVS_CODE: &str = "cvs_code";

pub const INVOICE_PERSONAL: &str = "personal";
pub const INVOICE_COMPANY: &str = "company";
pub const INVOICE_DONATION: &str = "donation";

/// 超商取貨商品小計上限（綠界 C2C 限制，規格 §7）
pub const CVS_SUBTOTAL_LIMIT: i32 = 20_000;
pub const MAX_QTY_PER_LINE: i32 = 99;
pub const MAX_LINES: usize = 50;

// ───── 輸入 ─────

#[derive(Debug, Clone, Deserialize)]
pub struct OrderItemInput {
    pub variant_id: Uuid,
    pub qty: i32,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct HomeAddress {
    pub postal_code: String,
    pub city: String,
    pub district: String,
    pub street: String,
}

/// 發票資料（規格 §7 第 4 點）。JSON 的 key 是 `type`，Rust 用 `kind`
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct InvoiceInput {
    #[serde(rename = "type")]
    pub kind: String,
    pub carrier_type: String,
    pub carrier_num: String,
    pub tax_id: String,
    pub title: String,
    pub address: String,
    pub love_code: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OrderInput {
    pub items: Vec<OrderItemInput>,
    pub email: String,
    pub recipient_name: String,
    pub recipient_phone: String,
    pub shipping_method: String,
    #[serde(default)]
    pub cvs_store_token: Option<String>,
    #[serde(default)]
    pub address: Option<HomeAddress>,
    #[serde(default)]
    pub invoice: InvoiceInput,
    pub payment_method: String,
    #[serde(default)]
    pub note: String,
}

impl OrderInput {
    /// 去頭尾空白、Email 轉小寫、載具號碼轉大寫
    pub fn normalized(mut self) -> Self {
        self.email = normalize_email(&self.email);
        for s in [
            &mut self.recipient_name,
            &mut self.recipient_phone,
            &mut self.shipping_method,
            &mut self.payment_method,
            &mut self.note,
            &mut self.invoice.kind,
            &mut self.invoice.carrier_type,
            &mut self.invoice.tax_id,
            &mut self.invoice.title,
            &mut self.invoice.address,
            &mut self.invoice.love_code,
        ] {
            *s = s.trim().to_string();
        }
        self.invoice.carrier_num = self.invoice.carrier_num.trim().to_ascii_uppercase();
        if let Some(t) = &mut self.cvs_store_token {
            *t = t.trim().to_string();
        }
        if let Some(a) = &mut self.address {
            for s in [&mut a.postal_code, &mut a.city, &mut a.district, &mut a.street] {
                *s = s.trim().to_string();
            }
        }
        self
    }
}

// ───── 格式檢查（前端 validation.ts 有一模一樣的規則）─────

/// 超商取貨收件人：2～5 個中文字（規格 §8.3）
pub fn is_cvs_recipient_name(name: &str) -> bool {
    let n = name.chars().count();
    (2..=5).contains(&n) && name.chars().all(|c| ('\u{4e00}'..='\u{9fff}').contains(&c))
}

/// 手機條碼載具：/ 開頭 + 7 碼（0-9、A-Z、+、-、.）
pub fn is_mobile_barcode(s: &str) -> bool {
    s.len() == 8
        && s.starts_with('/')
        && s[1..]
            .bytes()
            .all(|b| b.is_ascii_digit() || b.is_ascii_uppercase() || matches!(b, b'+' | b'-' | b'.'))
}

/// 自然人憑證條碼：2 個大寫英文字母 + 14 碼數字
pub fn is_citizen_cert(s: &str) -> bool {
    s.len() == 16
        && s[..2].bytes().all(|b| b.is_ascii_uppercase())
        && s[2..].bytes().all(|b| b.is_ascii_digit())
}

/// 捐贈愛心碼：3～7 碼數字
pub fn is_love_code(s: &str) -> bool {
    (3..=7).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_digit())
}

/// 統一編號檢查（財政部規則）：8 碼數字各乘 1,2,1,2,1,2,4,1，
/// 每個乘積「十位數 + 個位數」相加後總和能被 5 整除即合法（2023 年起由 10 改 5）；
/// 第 7 碼是 7 時（乘積 28 → 10，可視為 1），總和 + 1 能被 5 整除也合法。全 0 不算。
pub fn is_tw_tax_id(s: &str) -> bool {
    if s.len() != 8 || !s.bytes().all(|b| b.is_ascii_digit()) || s == "00000000" {
        return false;
    }
    const WEIGHTS: [u32; 8] = [1, 2, 1, 2, 1, 2, 4, 1];
    let digits: Vec<u32> = s.bytes().map(|b| u32::from(b - b'0')).collect();
    let sum: u32 = digits
        .iter()
        .zip(WEIGHTS)
        .map(|(d, w)| {
            let p = d * w;
            p / 10 + p % 10
        })
        .sum();
    sum % 5 == 0 || (digits[6] == 7 && (sum + 1) % 5 == 0)
}

// ───── 金額 ─────

/// 免運門檻 0 = 不免運；否則商品小計 >= 門檻就免運（與規格不同之處 16）
pub fn shipping_fee(shipping: &ShippingSettings, method: &str, subtotal: i32) -> i32 {
    if shipping.free_threshold > 0 && subtotal >= shipping.free_threshold {
        return 0;
    }
    if method == SHIPPING_CVS {
        shipping.cvs_fee
    } else {
        shipping.home_fee
    }
}

/// 同一個規格出現多次就合併數量；數量夾在 1..=MAX_QTY_PER_LINE；保留第一次出現的順序
pub fn merge_items(items: &[OrderItemInput]) -> Vec<(Uuid, i32)> {
    let mut merged: Vec<(Uuid, i32)> = Vec::new();
    for item in items {
        let qty = item.qty.max(1);
        match merged.iter_mut().find(|(id, _)| *id == item.variant_id) {
            Some((_, q)) => *q = (*q + qty).min(MAX_QTY_PER_LINE),
            None => merged.push((item.variant_id, qty.min(MAX_QTY_PER_LINE))),
        }
    }
    merged
}

fn len_between(value: &str, min: usize, max: usize) -> bool {
    let n = value.chars().count();
    n >= min && n <= max
}

/// 欄位層級驗證（已 normalized 的輸入）。門市 token 是否存在、庫存夠不夠在 create_order 裡查資料庫才知道。
pub fn validate_input(input: &OrderInput, payment_methods: &PaymentMethods) -> Result<(), ApiError> {
    let mut errors = FieldErrors::new();

    if input.items.is_empty() {
        errors.add("items", "購物車是空的");
    } else if input.items.len() > MAX_LINES {
        errors.add("items", "一次最多 50 種商品");
    } else if input.items.iter().any(|i| i.qty < 1 || i.qty > MAX_QTY_PER_LINE) {
        errors.add("items", "數量要在 1 到 99 之間");
    }

    if !is_valid_email(&input.email) {
        errors.add("email", "Email 格式不正確");
    }
    if !is_tw_mobile(&input.recipient_phone) {
        errors.add("recipient_phone", "手機格式：09 開頭共 10 碼");
    }

    match input.shipping_method.as_str() {
        SHIPPING_CVS => {
            if !is_cvs_recipient_name(&input.recipient_name) {
                errors.add("recipient_name", "超商取貨收件人請填 2～5 個中文字的本名");
            }
        }
        SHIPPING_HOME => {
            if !len_between(&input.recipient_name, 1, 20) {
                errors.add("recipient_name", "必填，最多 20 字");
            }
            match &input.address {
                None => errors.add("address.street", "請填寫收件地址"),
                Some(a) => {
                    if !is_postal_code(&a.postal_code) {
                        errors.add("address.postal_code", "郵遞區號 3～5 碼數字");
                    }
                    if !len_between(&a.city, 1, 10) {
                        errors.add("address.city", "請選縣市");
                    }
                    if !len_between(&a.district, 1, 10) {
                        errors.add("address.district", "請選鄉鎮市區");
                    }
                    if !len_between(&a.street, 1, 100) {
                        errors.add("address.street", "必填，最多 100 字");
                    }
                }
            }
        }
        _ => errors.add("shipping_method", "請選擇取貨方式"),
    }

    let inv = &input.invoice;
    match inv.kind.as_str() {
        INVOICE_PERSONAL => match inv.carrier_type.as_str() {
            "1" => {}
            "2" => {
                if !is_citizen_cert(&inv.carrier_num) {
                    errors.add("invoice.carrier_num", "自然人憑證條碼：2 個英文字母 + 14 碼數字");
                }
            }
            "3" => {
                if !is_mobile_barcode(&inv.carrier_num) {
                    errors.add("invoice.carrier_num", "手機條碼：/ 開頭共 8 碼");
                }
            }
            _ => errors.add("invoice.carrier_type", "請選擇載具"),
        },
        INVOICE_COMPANY => {
            if !is_tw_tax_id(&inv.tax_id) {
                errors.add("invoice.tax_id", "統一編號格式不正確");
            }
            if !len_between(&inv.title, 1, 60) {
                errors.add("invoice.title", "必填，最多 60 字");
            }
            if !len_between(&inv.address, 1, 100) {
                errors.add("invoice.address", "必填，最多 100 字");
            }
        }
        INVOICE_DONATION => {
            if !is_love_code(&inv.love_code) {
                errors.add("invoice.love_code", "愛心碼 3～7 碼數字");
            }
        }
        _ => errors.add("invoice.type", "請選擇發票類型"),
    }

    let enabled = match input.payment_method.as_str() {
        PAYMENT_CREDIT => payment_methods.credit,
        PAYMENT_ATM => payment_methods.atm,
        PAYMENT_CVS_CODE => payment_methods.cvs_code,
        _ => {
            errors.add("payment_method", "請選擇付款方式");
            true
        }
    };
    if !enabled {
        errors.add("payment_method", "這個付款方式目前沒有開放");
    }

    if input.note.chars().count() > 200 {
        errors.add("note", "最多 200 字");
    }

    errors.into_result()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn home_input() -> OrderInput {
        OrderInput {
            items: vec![OrderItemInput {
                variant_id: Uuid::now_v7(),
                qty: 1,
            }],
            email: "a@b.co".to_string(),
            recipient_name: "王小明".to_string(),
            recipient_phone: "0912345678".to_string(),
            shipping_method: SHIPPING_HOME.to_string(),
            cvs_store_token: None,
            address: Some(HomeAddress {
                postal_code: "100".to_string(),
                city: "臺北市".to_string(),
                district: "中正區".to_string(),
                street: "重慶南路一段 122 號".to_string(),
            }),
            invoice: InvoiceInput {
                kind: INVOICE_PERSONAL.to_string(),
                carrier_type: "1".to_string(),
                ..Default::default()
            },
            payment_method: PAYMENT_CREDIT.to_string(),
            note: String::new(),
        }
    }

    fn fields(err: ApiError) -> serde_json::Value {
        match err {
            ApiError::Validation { details, .. } => details["fields"].clone(),
            other => panic!("expected validation, got {other:?}"),
        }
    }

    #[test]
    fn tax_id_checksum() {
        // 04595257：乘積 0,8,5,18,5,4,20,7 → 0+8+5+9+5+4+2+7 = 40 → 整除 5
        assert!(is_tw_tax_id("04595257"));
        // 10000004：1 + 4 = 5
        assert!(is_tw_tax_id("10000004"));
        // 12345675：1+4+3+8+5+3+10+5 = 39，第 7 碼是 7，39+1 = 40 → 合法
        assert!(is_tw_tax_id("12345675"));
        // 12345678：總和 42，第 7 碼是 7 但 43 也不整除 → 不合法
        assert!(!is_tw_tax_id("12345678"));
        // 12345674：總和 38、39 都不整除
        assert!(!is_tw_tax_id("12345674"));
        assert!(!is_tw_tax_id("1234567"));
        assert!(!is_tw_tax_id("1234567a"));
        assert!(!is_tw_tax_id("00000000"));
    }

    #[test]
    fn format_helpers() {
        assert!(is_cvs_recipient_name("王小明"));
        assert!(!is_cvs_recipient_name("王"));
        assert!(!is_cvs_recipient_name("John"));
        assert!(!is_cvs_recipient_name("王小明王小明"));
        assert!(is_mobile_barcode("/ABC+123"));
        assert!(!is_mobile_barcode("ABC+1234"));
        assert!(!is_mobile_barcode("/abc+123"));
        assert!(is_citizen_cert("AB12345678901234"));
        assert!(!is_citizen_cert("A123456789012345"));
        assert!(is_love_code("168"));
        assert!(!is_love_code("12"));
        assert!(!is_love_code("12345678"));
    }

    #[test]
    fn fee_rules() {
        let s = ShippingSettings {
            cvs_fee: 60,
            home_fee: 100,
            free_threshold: 0,
        };
        assert_eq!(shipping_fee(&s, SHIPPING_CVS, 99_999), 60);
        assert_eq!(shipping_fee(&s, SHIPPING_HOME, 1), 100);
        let s = ShippingSettings {
            free_threshold: 1000,
            ..s
        };
        assert_eq!(shipping_fee(&s, SHIPPING_HOME, 999), 100);
        assert_eq!(shipping_fee(&s, SHIPPING_HOME, 1000), 0);
        assert_eq!(shipping_fee(&s, SHIPPING_CVS, 1000), 0);
    }

    #[test]
    fn merge_items_sums_and_caps() {
        let a = Uuid::now_v7();
        let b = Uuid::now_v7();
        let merged = merge_items(&[
            OrderItemInput { variant_id: a, qty: 2 },
            OrderItemInput { variant_id: b, qty: 1 },
            OrderItemInput { variant_id: a, qty: 98 },
        ]);
        assert_eq!(merged, vec![(a, 99), (b, 1)]);
    }

    #[test]
    fn valid_home_input_passes() {
        assert!(validate_input(&home_input(), &PaymentMethods::default()).is_ok());
    }

    #[test]
    fn home_needs_address_and_cvs_needs_chinese_name() {
        let mut input = home_input();
        input.address = None;
        let f = fields(validate_input(&input, &PaymentMethods::default()).unwrap_err());
        assert_eq!(f["address.street"], "請填寫收件地址");

        let mut input = home_input();
        input.shipping_method = SHIPPING_CVS.to_string();
        input.recipient_name = "Amy".to_string();
        let f = fields(validate_input(&input, &PaymentMethods::default()).unwrap_err());
        assert!(f["recipient_name"].as_str().unwrap().contains("中文"));
    }

    #[test]
    fn invoice_rules() {
        let mut input = home_input();
        input.invoice = InvoiceInput {
            kind: INVOICE_COMPANY.to_string(),
            tax_id: "12345678".to_string(),
            ..Default::default()
        };
        let f = fields(validate_input(&input, &PaymentMethods::default()).unwrap_err());
        assert_eq!(f["invoice.tax_id"], "統一編號格式不正確");
        assert!(f.get("invoice.title").is_some());
        assert!(f.get("invoice.address").is_some());

        let mut input = home_input();
        input.invoice = InvoiceInput {
            kind: INVOICE_PERSONAL.to_string(),
            carrier_type: "3".to_string(),
            carrier_num: "/ABC+123".to_string(),
            ..Default::default()
        };
        assert!(validate_input(&input, &PaymentMethods::default()).is_ok());

        let mut input = home_input();
        input.invoice = InvoiceInput {
            kind: INVOICE_DONATION.to_string(),
            love_code: "x".to_string(),
            ..Default::default()
        };
        let f = fields(validate_input(&input, &PaymentMethods::default()).unwrap_err());
        assert_eq!(f["invoice.love_code"], "愛心碼 3～7 碼數字");
    }

    #[test]
    fn disabled_payment_method_is_rejected() {
        let mut input = home_input();
        input.payment_method = PAYMENT_ATM.to_string();
        let pm = PaymentMethods {
            atm: false,
            ..Default::default()
        };
        let f = fields(validate_input(&input, &pm).unwrap_err());
        assert_eq!(f["payment_method"], "這個付款方式目前沒有開放");
        input.payment_method = "bitcoin".to_string();
        let f = fields(validate_input(&input, &pm).unwrap_err());
        assert_eq!(f["payment_method"], "請選擇付款方式");
    }

    #[test]
    fn empty_items_and_long_note() {
        let mut input = home_input();
        input.items = vec![];
        input.note = "x".repeat(201);
        let f = fields(validate_input(&input, &PaymentMethods::default()).unwrap_err());
        assert_eq!(f["items"], "購物車是空的");
        assert_eq!(f["note"], "最多 200 字");
    }
}
```

`api/src/domain/mod.rs` 加 `pub mod orders;`。

- [ ] **Step 2: 跑單元測試、格式、lint**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && cd api && cargo test --lib orders`
Expected: 9 個測試通過。
Run: `export PATH="$HOME/.cargo/bin:$PATH" && cd api && cargo fmt --all && cargo clippy --all-targets -- -D warnings`
Expected: 乾淨。

- [ ] **Step 3: Commit**

```bash
git add api/src/domain/orders.rs api/src/domain/mod.rs
git commit -m "feat(api): 訂單輸入型別、欄位驗證（統編、載具、手機）、運費計算"
```

### Task 7: 訂單領域（二）：下單交易、取消歸還、查詢、超商門市

**Files:**
- Modify: `api/src/domain/orders.rs`（接在 Task 6 的內容之後、`#[cfg(test)]` 之前）
- Create: `api/src/domain/cvs_stores.rs`
- Modify: `api/src/domain/mod.rs`、`api/tests/common/mod.rs`
- Test: `api/tests/orders_domain.rs`（新增）

**Interfaces:**
- Consumes: Task 6 的型別與驗證；`settings::get_all`；`jobs::enqueue`；`auth::tokens::generate_token`；`error::{ApiError, ShortItem}`。
- Produces: `cvs_stores::{CvsStore, get_valid, insert}`；`orders::{OrderCreated, Viewer, OrderDetail, OrderSummary, create_order, get_for_viewer, list_for_user, cancel, cancel_in_tx}`；測試工具 `common::active_product(pool, name, price, stock) -> (variant_id, slug)`、`common::cvs_store_token(pool) -> String`。

- [ ] **Step 1: 寫 `api/src/domain/cvs_stores.rs`**

```rust
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgExecutor, PgPool};

/// 綠界地圖選完的門市（規格 §3 cvs_store_selections）。本計畫只讀；計畫 4 的 map-reply 才會寫
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct CvsStore {
    pub token: String,
    pub sub_type: String,
    pub store_id: String,
    pub store_name: String,
    pub store_address: String,
    pub store_phone: String,
}

/// 未過期才回
pub async fn get_valid<'e, E: PgExecutor<'e>>(
    exec: E,
    token: &str,
) -> Result<Option<CvsStore>, sqlx::Error> {
    sqlx::query_as::<_, CvsStore>(
        "SELECT token, sub_type, store_id, store_name, store_address, store_phone
         FROM cvs_store_selections WHERE token = $1 AND expires_at > now()",
    )
    .bind(token)
    .fetch_optional(exec)
    .await
}

/// 計畫 4 的 map-reply 與測試用
pub async fn insert(db: &PgPool, store: &CvsStore, ttl_minutes: i64) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO cvs_store_selections (token, sub_type, store_id, store_name, store_address, store_phone, expires_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(&store.token)
    .bind(&store.sub_type)
    .bind(&store.store_id)
    .bind(&store.store_name)
    .bind(&store.store_address)
    .bind(&store.store_phone)
    .bind(Utc::now() + Duration::minutes(ttl_minutes))
    .execute(db)
    .await?;
    Ok(())
}
```

`api/src/domain/mod.rs` 加 `pub mod cvs_stores;`。

- [ ] **Step 2: 在 `api/src/domain/orders.rs` 加後半**

檔案頂端的 `use` 改成（保留 Task 6 的，加上新的）：

```rust
use chrono::{DateTime, FixedOffset, Utc};
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

pub use crate::domain::addresses::is_postal_code;
pub use crate::domain::users::is_tw_mobile;
use crate::auth::tokens::generate_token;
use crate::domain::cvs_stores::{self, CvsStore};
use crate::domain::jobs;
use crate::domain::settings::{self, PaymentMethods, ShippingSettings};
use crate::domain::users::{User, is_valid_email, normalize_email};
use crate::error::{ApiError, FieldErrors, ShortItem};
```

在 `validate_input` 之後、`#[cfg(test)]` 之前加：

```rust
// ───── 輸出 ─────

#[derive(Debug, Serialize)]
pub struct OrderCreated {
    pub order_id: Uuid,
    pub order_no: String,
    pub guest_token: String,
}

/// 誰在看訂單：登入會員（看自己的）或帶 guest_token 的訪客
#[derive(Debug, Clone)]
pub enum Viewer {
    User(Uuid),
    Guest(String),
}

impl Viewer {
    fn user_id(&self) -> Option<Uuid> {
        match self {
            Viewer::User(id) => Some(*id),
            Viewer::Guest(_) => None,
        }
    }

    fn token(&self) -> Option<&str> {
        match self {
            Viewer::User(_) => None,
            Viewer::Guest(t) => Some(t.as_str()),
        }
    }
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct OrderRow {
    pub id: Uuid,
    pub order_no: String,
    #[serde(skip)]
    pub user_id: Option<Uuid>,
    #[serde(skip)]
    pub guest_token: String,
    pub status: String,
    pub email: String,
    pub recipient_name: String,
    pub recipient_phone: String,
    pub shipping_method: String,
    pub subtotal: i32,
    pub shipping_fee: i32,
    pub total: i32,
    pub note: String,
    pub invoice_type: String,
    pub invoice_carrier_type: Option<String>,
    pub invoice_carrier_num: Option<String>,
    pub invoice_tax_id: Option<String>,
    pub invoice_title: Option<String>,
    pub invoice_address: Option<String>,
    pub invoice_love_code: Option<String>,
    #[serde(skip)]
    pub needs_refund: bool,
    pub created_at: DateTime<Utc>,
    pub paid_at: Option<DateTime<Utc>>,
    pub shipped_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub cancelled_at: Option<DateTime<Utc>>,
    pub cancel_reason: Option<String>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct OrderItemRow {
    pub product_name: String,
    pub variant_label: String,
    pub unit_price: i32,
    pub quantity: i32,
    pub line_total: i32,
    pub image_path: Option<String>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ShipmentRow {
    pub method: String,
    pub cvs_sub_type: Option<String>,
    pub cvs_store_id: Option<String>,
    pub cvs_store_name: Option<String>,
    pub cvs_store_address: Option<String>,
    pub home_postal_code: Option<String>,
    pub home_city: Option<String>,
    pub home_district: Option<String>,
    pub home_street: Option<String>,
    pub status: String,
    pub carrier: Option<String>,
    pub tracking_no: Option<String>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct PaymentRow {
    pub method: String,
    pub status: String,
    pub amount: i32,
    pub atm_bank_code: Option<String>,
    pub atm_vaccount: Option<String>,
    pub cvs_payment_no: Option<String>,
    pub expire_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
pub struct OrderDetail {
    #[serde(flatten)]
    pub order: OrderRow,
    pub items: Vec<OrderItemRow>,
    pub shipment: Option<ShipmentRow>,
    pub payment: Option<PaymentRow>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct OrderSummary {
    pub id: Uuid,
    pub order_no: String,
    pub status: String,
    pub total: i32,
    pub item_count: i32,
    pub created_at: DateTime<Utc>,
    #[serde(skip)]
    pub total_count: i64,
}

// ───── 下單 ─────

/// 去掉易混淆的 I、O、0、1（與規格不同之處 21）
const ORDER_NO_ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";

/// DS + 台北日期 yyMMdd + 4 碼隨機（規格 §3）
fn generate_order_no() -> String {
    let taipei = Utc::now().with_timezone(&FixedOffset::east_opt(8 * 3600).expect("utc+8"));
    let mut rng = rand::rng();
    let suffix: String = (0..4)
        .map(|_| ORDER_NO_ALPHABET[rng.random_range(0..ORDER_NO_ALPHABET.len())] as char)
        .collect();
    format!("DS{}{}", taipei.format("%y%m%d"), suffix)
}

async fn unique_order_no(tx: &mut Transaction<'_, Postgres>) -> Result<String, ApiError> {
    for _ in 0..5 {
        let candidate = generate_order_no();
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM orders WHERE order_no = $1)")
                .bind(&candidate)
                .fetch_one(&mut **tx)
                .await?;
        if !exists {
            return Ok(candidate);
        }
    }
    Err(ApiError::Internal(anyhow::anyhow!("order_no 連續撞號 5 次")))
}

#[derive(sqlx::FromRow)]
struct SnapshotRow {
    variant_id: Uuid,
    product_name: String,
    option1_value: Option<String>,
    option2_value: Option<String>,
    price: i32,
    image_path: Option<String>,
}

/// 「雞肉 / S」；沒規格就是「預設」。Task 8 的 domain/cart.rs 也用
pub fn variant_label(option1: Option<&str>, option2: Option<&str>) -> String {
    let label: Vec<&str> = [option1, option2].into_iter().flatten().collect();
    if label.is_empty() {
        "預設".to_string()
    } else {
        label.join(" / ")
    }
}

/// 規格 §7 第 6 點：一個交易內驗證、重算金額與運費、扣庫存（規格 §5）、寫 orders / order_items /
/// shipments / payments(pending)，排 send_email:order_created job。任何失敗整筆 rollback。
pub async fn create_order(
    db: &PgPool,
    input: OrderInput,
    user: Option<&User>,
) -> Result<OrderCreated, ApiError> {
    let input = input.normalized();
    let settings = settings::get_all(db).await?;
    validate_input(&input, &settings.payment_methods)?;
    let items = merge_items(&input.items);

    let mut tx = db.begin().await?;

    // 超商門市先查，免得扣了庫存才發現沒門市（雖然 rollback 也會還回去）
    let cvs_store: Option<CvsStore> = if input.shipping_method == SHIPPING_CVS {
        let token = input.cvs_store_token.as_deref().unwrap_or("");
        Some(
            cvs_stores::get_valid(&mut *tx, token)
                .await?
                .ok_or(ApiError::CvsStoreRequired)?,
        )
    } else {
        None
    };

    // 扣庫存 + 快照：影響筆數 0 = 不夠（或已下架）。全部檢查完再一次回報（規格 §5）
    let mut lines: Vec<(SnapshotRow, i32)> = Vec::with_capacity(items.len());
    let mut short: Vec<ShortItem> = Vec::new();
    for (variant_id, qty) in &items {
        let row: Option<SnapshotRow> = sqlx::query_as(
            "UPDATE product_variants v SET stock = v.stock - $2
             FROM products p
             WHERE v.id = $1 AND v.product_id = p.id AND p.status = 'active' AND v.is_active AND v.stock >= $2
             RETURNING v.id AS variant_id, p.name AS product_name, v.option1_value, v.option2_value, v.price,
                       COALESCE((SELECT i.path FROM product_images i WHERE i.id = v.image_id),
                                (SELECT i.path FROM product_images i WHERE i.product_id = p.id ORDER BY i.sort_order LIMIT 1)) AS image_path",
        )
        .bind(variant_id)
        .bind(qty)
        .fetch_optional(&mut *tx)
        .await?;
        match row {
            Some(row) => lines.push((row, *qty)),
            None => {
                let available: Option<i32> = sqlx::query_scalar(
                    "SELECT CASE WHEN p.status = 'active' AND v.is_active THEN v.stock ELSE 0 END
                     FROM product_variants v JOIN products p ON p.id = v.product_id WHERE v.id = $1",
                )
                .bind(variant_id)
                .fetch_optional(&mut *tx)
                .await?;
                short.push(ShortItem {
                    variant_id: *variant_id,
                    available: available.unwrap_or(0),
                });
            }
        }
    }
    if !short.is_empty() {
        return Err(ApiError::OutOfStock(short)); // tx 被 drop → rollback
    }

    let subtotal: i32 = lines.iter().map(|(row, qty)| row.price * qty).sum();
    if input.shipping_method == SHIPPING_CVS && subtotal > CVS_SUBTOTAL_LIMIT {
        return Err(ApiError::CvsAmountLimit);
    }
    let shipping_fee = shipping_fee(&settings.shipping, &input.shipping_method, subtotal);
    let total = subtotal + shipping_fee;

    let order_id = Uuid::now_v7();
    let order_no = unique_order_no(&mut tx).await?;
    let guest_token = generate_token();
    let inv = &input.invoice;
    let opt = |s: &str| (!s.is_empty()).then(|| s.to_string());
    let (carrier_type, carrier_num) = if inv.kind == INVOICE_PERSONAL {
        (
            opt(&inv.carrier_type),
            if inv.carrier_type == "1" { None } else { opt(&inv.carrier_num) },
        )
    } else {
        (None, None)
    };
    sqlx::query(
        "INSERT INTO orders (id, order_no, user_id, guest_token, status, email, recipient_name, recipient_phone,
                             shipping_method, subtotal, shipping_fee, total, note, invoice_type, invoice_carrier_type,
                             invoice_carrier_num, invoice_tax_id, invoice_title, invoice_address, invoice_love_code)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20)",
    )
    .bind(order_id)
    .bind(&order_no)
    .bind(user.map(|u| u.id))
    .bind(&guest_token)
    .bind(STATUS_PENDING_PAYMENT)
    .bind(&input.email)
    .bind(&input.recipient_name)
    .bind(&input.recipient_phone)
    .bind(&input.shipping_method)
    .bind(subtotal)
    .bind(shipping_fee)
    .bind(total)
    .bind(&input.note)
    .bind(&inv.kind)
    .bind(carrier_type)
    .bind(carrier_num)
    .bind((inv.kind == INVOICE_COMPANY).then(|| inv.tax_id.clone()))
    .bind((inv.kind == INVOICE_COMPANY).then(|| inv.title.clone()))
    .bind((inv.kind == INVOICE_COMPANY).then(|| inv.address.clone()))
    .bind((inv.kind == INVOICE_DONATION).then(|| inv.love_code.clone()))
    .execute(&mut *tx)
    .await?;

    for (i, (row, qty)) in lines.iter().enumerate() {
        sqlx::query(
            "INSERT INTO order_items (id, order_id, variant_id, product_name, variant_label, unit_price, quantity, line_total, image_path, sort_order)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
        )
        .bind(Uuid::now_v7())
        .bind(order_id)
        .bind(row.variant_id)
        .bind(&row.product_name)
        .bind(variant_label(row.option1_value.as_deref(), row.option2_value.as_deref()))
        .bind(row.price)
        .bind(qty)
        .bind(row.price * qty)
        .bind(&row.image_path)
        .bind(i as i32)
        .execute(&mut *tx)
        .await?;
    }

    let address = input.address.as_ref();
    sqlx::query(
        "INSERT INTO shipments (id, order_id, method, cvs_sub_type, cvs_store_id, cvs_store_name, cvs_store_address, cvs_store_phone,
                                home_postal_code, home_city, home_district, home_street)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
    )
    .bind(Uuid::now_v7())
    .bind(order_id)
    .bind(&input.shipping_method)
    .bind(cvs_store.as_ref().map(|s| s.sub_type.clone()))
    .bind(cvs_store.as_ref().map(|s| s.store_id.clone()))
    .bind(cvs_store.as_ref().map(|s| s.store_name.clone()))
    .bind(cvs_store.as_ref().map(|s| s.store_address.clone()))
    .bind(cvs_store.as_ref().map(|s| s.store_phone.clone()))
    .bind(address.map(|a| a.postal_code.clone()))
    .bind(address.map(|a| a.city.clone()))
    .bind(address.map(|a| a.district.clone()))
    .bind(address.map(|a| a.street.clone()))
    .execute(&mut *tx)
    .await?;

    // 第一筆付款嘗試：merchant_trade_no = order_no + "01"（規格 §3）；綠界欄位計畫 3 填
    sqlx::query(
        "INSERT INTO payments (id, order_id, merchant_trade_no, method, amount) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(Uuid::now_v7())
    .bind(order_id)
    .bind(format!("{order_no}01"))
    .bind(&input.payment_method)
    .bind(total)
    .execute(&mut *tx)
    .await?;

    jobs::enqueue(
        &mut tx,
        jobs::KIND_SEND_EMAIL,
        json!({ "template": "order_created", "order_id": order_id }),
        Some(&format!("email:order_created:{order_id}")),
    )
    .await?;

    tx.commit().await?;
    Ok(OrderCreated {
        order_id,
        order_no,
        guest_token,
    })
}

// ───── 查詢 ─────

const ORDER_COLUMNS: &str = "id, order_no, user_id, guest_token, status, email, recipient_name, recipient_phone, shipping_method,
     subtotal, shipping_fee, total, note, invoice_type, invoice_carrier_type, invoice_carrier_num, invoice_tax_id,
     invoice_title, invoice_address, invoice_love_code, needs_refund, created_at, paid_at, shipped_at, completed_at,
     cancelled_at, cancel_reason";

/// 會員看自己的、訪客用 guest_token；都不符回 None（→ 404，不用 403 免得被猜 id）
pub async fn get_for_viewer(
    db: &PgPool,
    id: Uuid,
    viewer: &Viewer,
) -> Result<Option<OrderDetail>, ApiError> {
    let sql = format!(
        "SELECT {ORDER_COLUMNS} FROM orders
         WHERE id = $1 AND ((user_id IS NOT NULL AND user_id = $2) OR ($3::text IS NOT NULL AND guest_token = $3))"
    );
    let Some(order) = sqlx::query_as::<_, OrderRow>(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .bind(viewer.user_id())
        .bind(viewer.token())
        .fetch_optional(db)
        .await?
    else {
        return Ok(None);
    };
    let items = sqlx::query_as::<_, OrderItemRow>(
        "SELECT product_name, variant_label, unit_price, quantity, line_total, image_path
         FROM order_items WHERE order_id = $1 ORDER BY sort_order",
    )
    .bind(id)
    .fetch_all(db)
    .await?;
    let shipment = sqlx::query_as::<_, ShipmentRow>(
        "SELECT method, cvs_sub_type, cvs_store_id, cvs_store_name, cvs_store_address, home_postal_code, home_city,
                home_district, home_street, status, carrier, tracking_no
         FROM shipments WHERE order_id = $1",
    )
    .bind(id)
    .fetch_optional(db)
    .await?;
    let payment = sqlx::query_as::<_, PaymentRow>(
        "SELECT method, status, amount, atm_bank_code, atm_vaccount, cvs_payment_no, expire_at
         FROM payments WHERE order_id = $1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(id)
    .fetch_optional(db)
    .await?;
    Ok(Some(OrderDetail {
        order,
        items,
        shipment,
        payment,
    }))
}

/// 會員中心的訂單列表（新到舊）
pub async fn list_for_user(
    db: &PgPool,
    user_id: Uuid,
    page: i64,
    per_page: i64,
) -> Result<crate::domain::products::Page<OrderSummary>, ApiError> {
    let rows = sqlx::query_as::<_, OrderSummary>(
        "SELECT o.id, o.order_no, o.status, o.total, o.created_at,
                (SELECT COALESCE(SUM(oi.quantity), 0) FROM order_items oi WHERE oi.order_id = o.id)::int AS item_count,
                COUNT(*) OVER () AS total_count
         FROM orders o WHERE o.user_id = $1
         ORDER BY o.created_at DESC LIMIT $2 OFFSET $3",
    )
    .bind(user_id)
    .bind(per_page)
    .bind((page - 1) * per_page)
    .fetch_all(db)
    .await?;
    let total = rows.first().map(|r| r.total_count).unwrap_or(0);
    Ok(crate::domain::products::Page {
        items: rows,
        total,
        page,
        per_page,
    })
}

// ───── 取消 ─────

/// 只有 pending_payment 能取消；成功就把 order_items 的數量加回庫存（規格 §4、§5）。
/// 回 Ok(false) 表示狀態不允許。計畫 3 的過期 job、計畫 4 的後台取消也用這個。
pub async fn cancel_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    order_id: Uuid,
    reason: &str,
) -> Result<bool, ApiError> {
    let updated = sqlx::query(
        "UPDATE orders SET status = $3, cancelled_at = now(), cancel_reason = $2 WHERE id = $1 AND status = $4",
    )
    .bind(order_id)
    .bind(reason)
    .bind(STATUS_CANCELLED)
    .bind(STATUS_PENDING_PAYMENT)
    .execute(&mut **tx)
    .await?
    .rows_affected();
    if updated == 0 {
        return Ok(false);
    }
    sqlx::query(
        "UPDATE product_variants v SET stock = v.stock + oi.quantity
         FROM order_items oi WHERE oi.order_id = $1 AND v.id = oi.variant_id",
    )
    .bind(order_id)
    .execute(&mut **tx)
    .await?;
    Ok(true)
}

/// 買家取消（與規格不同之處 13）：看不到 → NotFound；狀態不對 → VALIDATION
pub async fn cancel(db: &PgPool, id: Uuid, viewer: &Viewer, reason: &str) -> Result<(), ApiError> {
    let mut tx = db.begin().await?;
    let visible: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM orders
                        WHERE id = $1 AND ((user_id IS NOT NULL AND user_id = $2) OR ($3::text IS NOT NULL AND guest_token = $3)))",
    )
    .bind(id)
    .bind(viewer.user_id())
    .bind(viewer.token())
    .fetch_one(&mut *tx)
    .await?;
    if !visible {
        return Err(ApiError::NotFound);
    }
    if !cancel_in_tx(&mut tx, id, reason).await? {
        return Err(ApiError::Validation {
            message: "這筆訂單已經不能取消".to_string(),
            details: Value::Null,
        });
    }
    tx.commit().await?;
    Ok(())
}
```

- [ ] **Step 3: 在 `api/tests/common/mod.rs` 檔尾加測試工具**

```rust
/// 建一個上架商品（單一預設規格），回 (variant_id, slug)
pub async fn active_product(pool: &PgPool, name: &str, price: i32, stock: i32) -> (uuid::Uuid, String) {
    use dog_shop_api::domain::products::{self, ProductInput, VariantInput};
    let product = products::create(
        pool,
        ProductInput {
            name: name.to_string(),
            slug: None,
            description: None,
            category_id: None,
            status: "active".to_string(),
            option1_name: None,
            option2_name: None,
            sort_order: None,
            variants: vec![VariantInput {
                id: None,
                option1_value: None,
                option2_value: None,
                sku: None,
                price,
                compare_at_price: None,
                stock,
                is_active: None,
                image_path: None,
            }],
            images: vec![],
        },
    )
    .await
    .unwrap();
    (product.variants[0].id, product.product.slug.clone())
}

/// 塞一筆有效的門市選擇，回 token
pub async fn cvs_store_token(pool: &PgPool) -> String {
    use dog_shop_api::domain::cvs_stores::{self, CvsStore};
    let token = dog_shop_api::auth::tokens::generate_token();
    cvs_stores::insert(
        pool,
        &CvsStore {
            token: token.clone(),
            sub_type: "UNIMARTC2C".to_string(),
            store_id: "131386".to_string(),
            store_name: "測試門市".to_string(),
            store_address: "台北市中正區重慶南路一段 122 號".to_string(),
            store_phone: "0223456789".to_string(),
        },
        60,
    )
    .await
    .unwrap();
    token
}
```

- [ ] **Step 4: 寫 `api/tests/orders_domain.rs`**

```rust
mod common;

use dog_shop_api::domain::orders::{
    self, HomeAddress, InvoiceInput, OrderInput, OrderItemInput, Viewer,
};
use dog_shop_api::domain::settings;
use dog_shop_api::error::ApiError;
use sqlx::PgPool;
use uuid::Uuid;

fn input(items: Vec<(Uuid, i32)>, shipping_method: &str, token: Option<String>) -> OrderInput {
    OrderInput {
        items: items
            .into_iter()
            .map(|(variant_id, qty)| OrderItemInput { variant_id, qty })
            .collect(),
        email: "Buyer@Test.local".to_string(),
        recipient_name: "王小明".to_string(),
        recipient_phone: "0912345678".to_string(),
        shipping_method: shipping_method.to_string(),
        cvs_store_token: token,
        address: (shipping_method == "home").then(|| HomeAddress {
            postal_code: "100".to_string(),
            city: "臺北市".to_string(),
            district: "中正區".to_string(),
            street: "重慶南路一段 122 號".to_string(),
        }),
        invoice: InvoiceInput {
            kind: "personal".to_string(),
            carrier_type: "1".to_string(),
            ..Default::default()
        },
        payment_method: "credit".to_string(),
        note: " 請小心輕放 ".to_string(),
    }
}

async fn stock_of(pool: &PgPool, variant_id: Uuid) -> i32 {
    sqlx::query_scalar("SELECT stock FROM product_variants WHERE id = $1")
        .bind(variant_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrations = "./migrations")]
async fn home_order_deducts_stock_and_writes_everything(pool: PgPool) {
    let (variant, _) = common::active_product(&pool, "雞肉狗糧", 300, 5).await;
    let created = orders::create_order(&pool, input(vec![(variant, 2)], "home", None), None)
        .await
        .unwrap();
    assert!(
        created.order_no.len() == 12 && created.order_no.starts_with("DS"),
        "{}",
        created.order_no
    );
    assert_eq!(created.guest_token.len(), 64);
    assert_eq!(stock_of(&pool, variant).await, 3);

    let detail = orders::get_for_viewer(&pool, created.order_id, &Viewer::Guest(created.guest_token.clone()))
        .await
        .unwrap()
        .expect("guest token 看得到");
    assert_eq!(detail.order.status, "pending_payment");
    assert_eq!(detail.order.email, "buyer@test.local");
    assert_eq!(detail.order.note, "請小心輕放");
    assert_eq!((detail.order.subtotal, detail.order.shipping_fee, detail.order.total), (600, 100, 700));
    assert_eq!(detail.items.len(), 1);
    assert_eq!(detail.items[0].variant_label, "預設");
    assert_eq!(detail.items[0].line_total, 600);
    let shipment = detail.shipment.unwrap();
    assert_eq!(shipment.method, "home");
    assert_eq!(shipment.home_city.as_deref(), Some("臺北市"));
    assert_eq!(shipment.status, "pending");
    let payment = detail.payment.unwrap();
    assert_eq!((payment.method.as_str(), payment.status.as_str(), payment.amount), ("credit", "pending", 700));
    let mtn: String = sqlx::query_scalar("SELECT merchant_trade_no FROM payments")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(mtn, format!("{}01", created.order_no));
    let (kind, dedupe): (String, Option<String>) =
        sqlx::query_as("SELECT kind, dedupe_key FROM jobs").fetch_one(&pool).await.unwrap();
    assert_eq!(kind, "send_email");
    assert_eq!(dedupe.unwrap(), format!("email:order_created:{}", created.order_id));

    // 錯的 token、沒登入的會員視角都看不到
    assert!(orders::get_for_viewer(&pool, created.order_id, &Viewer::Guest("nope".to_string()))
        .await
        .unwrap()
        .is_none());
    assert!(orders::get_for_viewer(&pool, created.order_id, &Viewer::User(Uuid::now_v7()))
        .await
        .unwrap()
        .is_none());
}

#[sqlx::test(migrations = "./migrations")]
async fn free_shipping_threshold_applies(pool: PgPool) {
    let mut all = settings::get_all(&pool).await.unwrap();
    all.shipping.free_threshold = 500;
    settings::put_all(&pool, &all).await.unwrap();
    let (variant, _) = common::active_product(&pool, "牛肉狗糧", 250, 5).await;
    let created = orders::create_order(&pool, input(vec![(variant, 2)], "home", None), None)
        .await
        .unwrap();
    let detail = orders::get_for_viewer(&pool, created.order_id, &Viewer::Guest(created.guest_token))
        .await
        .unwrap()
        .unwrap();
    assert_eq!((detail.order.subtotal, detail.order.shipping_fee, detail.order.total), (500, 0, 500));
}

#[sqlx::test(migrations = "./migrations")]
async fn out_of_stock_lists_shortages_and_rolls_back(pool: PgPool) {
    let (a, _) = common::active_product(&pool, "A", 100, 5).await;
    let (b, _) = common::active_product(&pool, "B", 100, 1).await;
    let err = orders::create_order(&pool, input(vec![(a, 2), (b, 3)], "home", None), None)
        .await
        .unwrap_err();
    let ApiError::OutOfStock(items) = err else {
        panic!("expected OutOfStock, got {err:?}");
    };
    assert_eq!(items.len(), 1);
    assert_eq!((items[0].variant_id, items[0].available), (b, 1));
    // A 的扣庫存被 rollback，沒有訂單
    assert_eq!(stock_of(&pool, a).await, 5);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM orders").fetch_one(&pool).await.unwrap();
    assert_eq!(count, 0);

    // 未知規格 → available 0
    let ghost = Uuid::now_v7();
    let err = orders::create_order(&pool, input(vec![(ghost, 1)], "home", None), None)
        .await
        .unwrap_err();
    let ApiError::OutOfStock(items) = err else {
        panic!("expected OutOfStock");
    };
    assert_eq!((items[0].variant_id, items[0].available), (ghost, 0));
}

/// 規格 §15：並發下單不超賣。6 個人同時搶 3 件，正好 3 個成功。
#[sqlx::test(migrations = "./migrations")]
async fn concurrent_orders_do_not_oversell(pool: PgPool) {
    let (variant, _) = common::active_product(&pool, "限量", 100, 3).await;
    let mut set = tokio::task::JoinSet::new();
    for _ in 0..6 {
        let pool = pool.clone();
        set.spawn(async move {
            orders::create_order(&pool, input(vec![(variant, 1)], "home", None), None).await
        });
    }
    let mut ok = 0;
    let mut short = 0;
    while let Some(result) = set.join_next().await {
        match result.unwrap() {
            Ok(_) => ok += 1,
            Err(ApiError::OutOfStock(_)) => short += 1,
            Err(other) => panic!("unexpected {other:?}"),
        }
    }
    assert_eq!((ok, short), (3, 3));
    assert_eq!(stock_of(&pool, variant).await, 0);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM orders").fetch_one(&pool).await.unwrap();
    assert_eq!(count, 3);
}

#[sqlx::test(migrations = "./migrations")]
async fn cvs_order_needs_store_and_respects_limit(pool: PgPool) {
    let (variant, _) = common::active_product(&pool, "C", 60, 10).await;
    let err = orders::create_order(&pool, input(vec![(variant, 1)], "cvs", None), None)
        .await
        .unwrap_err();
    assert!(matches!(err, ApiError::CvsStoreRequired), "{err:?}");
    let err = orders::create_order(&pool, input(vec![(variant, 1)], "cvs", Some("expired-or-fake".to_string())), None)
        .await
        .unwrap_err();
    assert!(matches!(err, ApiError::CvsStoreRequired));
    assert_eq!(stock_of(&pool, variant).await, 10, "沒門市不該扣庫存");

    let token = common::cvs_store_token(&pool).await;
    let created = orders::create_order(&pool, input(vec![(variant, 1)], "cvs", Some(token)), None)
        .await
        .unwrap();
    let detail = orders::get_for_viewer(&pool, created.order_id, &Viewer::Guest(created.guest_token))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(detail.order.shipping_fee, 60);
    let shipment = detail.shipment.unwrap();
    assert_eq!(shipment.cvs_sub_type.as_deref(), Some("UNIMARTC2C"));
    assert_eq!(shipment.cvs_store_name.as_deref(), Some("測試門市"));
    assert!(shipment.home_city.is_none());

    // 小計超過 20,000 → CVS_AMOUNT_LIMIT，庫存 rollback
    let (pricey, _) = common::active_product(&pool, "貴", 25_000, 2).await;
    let token = common::cvs_store_token(&pool).await;
    let err = orders::create_order(&pool, input(vec![(pricey, 1)], "cvs", Some(token)), None)
        .await
        .unwrap_err();
    assert!(matches!(err, ApiError::CvsAmountLimit), "{err:?}");
    assert_eq!(stock_of(&pool, pricey).await, 2);
}

#[sqlx::test(migrations = "./migrations")]
async fn cancel_returns_stock_once(pool: PgPool) {
    let (variant, _) = common::active_product(&pool, "D", 100, 5).await;
    let created = orders::create_order(&pool, input(vec![(variant, 4)], "home", None), None)
        .await
        .unwrap();
    assert_eq!(stock_of(&pool, variant).await, 1);
    let viewer = Viewer::Guest(created.guest_token.clone());

    let err = orders::cancel(&pool, created.order_id, &Viewer::Guest("wrong".to_string()), "buyer")
        .await
        .unwrap_err();
    assert!(matches!(err, ApiError::NotFound));

    orders::cancel(&pool, created.order_id, &viewer, "buyer").await.unwrap();
    assert_eq!(stock_of(&pool, variant).await, 5);
    let detail = orders::get_for_viewer(&pool, created.order_id, &viewer).await.unwrap().unwrap();
    assert_eq!(detail.order.status, "cancelled");
    assert_eq!(detail.order.cancel_reason.as_deref(), Some("buyer"));
    assert!(detail.order.cancelled_at.is_some());

    let err = orders::cancel(&pool, created.order_id, &viewer, "buyer").await.unwrap_err();
    assert!(matches!(err, ApiError::Validation { .. }), "{err:?}");
    assert_eq!(stock_of(&pool, variant).await, 5, "不能加兩次");
}

#[sqlx::test(migrations = "./migrations")]
async fn member_orders_are_listed_newest_first(pool: PgPool) {
    let app = common::app(pool.clone());
    common::register_cookie(&app, "m@test.local", "password123", "甲").await;
    let user = dog_shop_api::domain::users::find_by_email(&pool, "m@test.local")
        .await
        .unwrap()
        .unwrap();
    let (variant, _) = common::active_product(&pool, "E", 100, 10).await;
    let first = orders::create_order(&pool, input(vec![(variant, 1)], "home", None), Some(&user))
        .await
        .unwrap();
    let second = orders::create_order(&pool, input(vec![(variant, 2)], "home", None), Some(&user))
        .await
        .unwrap();
    let page = orders::list_for_user(&pool, user.id, 1, 20).await.unwrap();
    assert_eq!(page.total, 2);
    assert_eq!(page.items[0].order_no, second.order_no);
    assert_eq!(page.items[0].item_count, 2);
    assert_eq!(page.items[1].order_no, first.order_no);
    // 會員看自己的不用 token
    assert!(orders::get_for_viewer(&pool, first.order_id, &Viewer::User(user.id))
        .await
        .unwrap()
        .is_some());
}
```

- [ ] **Step 5: 跑測試、格式、lint**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && cd api && DATABASE_URL=postgres://dog_shop:dog_shop@localhost:5435/dog_shop cargo test`（背景，第一次會多編譯一個測試檔）
Expected: 全綠；整合多 7 個。並發那個測試若偶爾失敗，是真的 bug，不是 flaky：扣庫存那句 `UPDATE ... WHERE stock >= $2` 就是為了這個。
Run: `export PATH="$HOME/.cargo/bin:$PATH" && cd api && cargo fmt --all && cargo clippy --all-targets -- -D warnings`

- [ ] **Step 6: Commit**

```bash
git add api/src/domain/orders.rs api/src/domain/cvs_stores.rs api/src/domain/mod.rs api/tests/common/mod.rs api/tests/orders_domain.rs
git commit -m "feat(api): 下單交易（扣庫存、快照、運費、出貨與付款列、outbox）、取消歸還、訂單查詢、門市暫存"
```

### Task 8: 訂單、購物車驗證、門市查詢的 HTTP 路由

**Files:**
- Create: `api/src/domain/cart.rs`、`api/src/routes/orders.rs`、`api/src/routes/cart.rs`、`api/src/routes/checkout.rs`
- Modify: `api/src/routes/me.rs`（加 `GET /api/me/orders`）、`api/src/domain/mod.rs`、`api/src/routes/mod.rs`、`api/src/app.rs`
- Test: `api/tests/orders.rs`、`api/tests/cart.rs`（新增）

**Interfaces:**
- Consumes: Task 7 的 `orders::*`、`cvs_stores::*`；`CurrentUser`、`AuthUser`；`products::{clamp_paging, Page}`；`settings::get_all`。
- Produces: `POST /api/orders`、`GET /api/orders/{id}?t=`、`POST /api/orders/{id}/cancel?t=`、`GET /api/me/orders`、`POST /api/cart/validate`、`GET /api/checkout/cvs-store/{token}`（形狀見介面總表）；`domain::cart::{CheckedLine, CartCheck, check(db, items)}`。

- [ ] **Step 1: 寫 `api/src/domain/cart.rs`**

```rust
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::orders::{self, OrderItemInput};
use crate::error::ApiError;

/// 購物車一列經伺服器核對後的結果（規格 §6.1 /cart 重新向伺服器驗證價格與庫存）
#[derive(Debug, Serialize)]
pub struct CheckedLine {
    pub variant_id: Uuid,
    pub product_slug: String,
    pub product_name: String,
    pub variant_label: String,
    pub price: i32,
    pub image_thumb: Option<String>,
    pub stock: i32,
    /// 伺服器夾過的數量：min(要求, 庫存)；不可買時 0
    pub qty: i32,
    pub available: bool,
    /// null | "unavailable"（下架／不存在）| "sold_out" | "qty_reduced"
    pub reason: Option<&'static str>,
}

#[derive(Debug, Serialize)]
pub struct CartCheck {
    pub items: Vec<CheckedLine>,
    /// 只算 available 的列（用夾過的數量）
    pub subtotal: i32,
}

#[derive(sqlx::FromRow)]
struct LineRow {
    product_slug: String,
    product_name: String,
    option1_value: Option<String>,
    option2_value: Option<String>,
    price: i32,
    stock: i32,
    sellable: bool,
    image_thumb: Option<String>,
}

pub async fn check(db: &PgPool, items: &[OrderItemInput]) -> Result<CartCheck, ApiError> {
    let merged = orders::merge_items(items);
    if merged.len() > orders::MAX_LINES {
        return Err(ApiError::field("items", "一次最多 50 種商品"));
    }
    let mut checked = Vec::with_capacity(merged.len());
    let mut subtotal = 0;
    for (variant_id, wanted) in merged {
        let row: Option<LineRow> = sqlx::query_as(
            "SELECT p.slug AS product_slug, p.name AS product_name, v.option1_value, v.option2_value, v.price, v.stock,
                    (p.status = 'active' AND v.is_active) AS sellable,
                    COALESCE((SELECT i.thumb_path FROM product_images i WHERE i.id = v.image_id),
                             (SELECT i.thumb_path FROM product_images i WHERE i.product_id = p.id ORDER BY i.sort_order LIMIT 1)) AS image_thumb
             FROM product_variants v JOIN products p ON p.id = v.product_id
             WHERE v.id = $1",
        )
        .bind(variant_id)
        .fetch_optional(db)
        .await?;
        let line = match row {
            None => CheckedLine {
                variant_id,
                product_slug: String::new(),
                product_name: "已下架的商品".to_string(),
                variant_label: String::new(),
                price: 0,
                image_thumb: None,
                stock: 0,
                qty: 0,
                available: false,
                reason: Some("unavailable"),
            },
            Some(row) => {
                let (qty, available, reason) = if !row.sellable {
                    (0, false, Some("unavailable"))
                } else if row.stock <= 0 {
                    (0, false, Some("sold_out"))
                } else {
                    let qty = wanted.min(row.stock);
                    (qty, true, (qty < wanted).then_some("qty_reduced"))
                };
                if available {
                    subtotal += row.price * qty;
                }
                CheckedLine {
                    variant_id,
                    variant_label: orders::variant_label(
                        row.option1_value.as_deref(),
                        row.option2_value.as_deref(),
                    ),
                    product_slug: row.product_slug,
                    product_name: row.product_name,
                    price: row.price,
                    image_thumb: row.image_thumb,
                    stock: row.stock.max(0),
                    qty,
                    available,
                    reason,
                }
            }
        };
        checked.push(line);
    }
    Ok(CartCheck {
        items: checked,
        subtotal,
    })
}
```

`api/src/domain/mod.rs` 加 `pub mod cart;`。

- [ ] **Step 2: 寫 `api/src/routes/cart.rs`**

```rust
use axum::{Json, Router, extract::State, routing::post};
use serde::{Deserialize, Serialize};

use crate::{
    domain::{
        cart::{self, CheckedLine},
        orders::{CVS_SUBTOTAL_LIMIT, OrderItemInput},
        settings::{self, ShippingSettings},
    },
    error::ApiResult,
    extract::AppJson,
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new().route("/api/cart/validate", post(validate))
}

#[derive(Deserialize)]
pub struct ValidateBody {
    #[serde(default)]
    pub items: Vec<OrderItemInput>,
}

#[derive(Serialize)]
pub struct CartValidateResponse {
    pub items: Vec<CheckedLine>,
    pub subtotal: i32,
    pub shipping: ShippingSettings,
    pub cvs_limit_exceeded: bool,
}

/// 購物車頁與結帳頁用：核對價格、庫存、上下架，並附上運費設定讓前端算運費
async fn validate(
    State(state): State<AppState>,
    AppJson(body): AppJson<ValidateBody>,
) -> ApiResult<Json<CartValidateResponse>> {
    let check = cart::check(&state.db, &body.items).await?;
    let settings = settings::get_all(&state.db).await?;
    Ok(Json(CartValidateResponse {
        cvs_limit_exceeded: check.subtotal > CVS_SUBTOTAL_LIMIT,
        items: check.items,
        subtotal: check.subtotal,
        shipping: settings.shipping,
    }))
}
```

- [ ] **Step 3: 寫 `api/src/routes/checkout.rs`**

```rust
use axum::{Json, Router, extract::State, routing::get};

use crate::{
    domain::cvs_stores::{self, CvsStore},
    error::{ApiError, ApiResult},
    extract::AppPath,
    state::AppState,
};

/// 計畫 4 會在這裡加 POST /api/checkout/cvs-map
pub fn router() -> Router<AppState> {
    Router::new().route("/api/checkout/cvs-store/{token}", get(cvs_store))
}

/// 結帳頁用 ?store=<token> 還原門市顯示；過期或不存在 404
async fn cvs_store(
    State(state): State<AppState>,
    AppPath(token): AppPath<String>,
) -> ApiResult<Json<CvsStore>> {
    cvs_stores::get_valid(&state.db, &token)
        .await?
        .map(Json)
        .ok_or(ApiError::NotFound)
}
```

- [ ] **Step 4: 寫 `api/src/routes/orders.rs`**

```rust
use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    routing::{get, post},
};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    auth::extract::CurrentUser,
    domain::{
        orders::{self, OrderCreated, OrderDetail, OrderInput, Viewer},
        users::User,
    },
    error::{ApiError, ApiResult},
    extract::{AppJson, AppPath, AppQuery},
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/orders", post(create))
        .route("/api/orders/{id}", get(detail))
        .route("/api/orders/{id}/cancel", post(cancel))
}

/// 會員或訪客都能下單（規格 §7）；登入者的訂單掛在帳號下
async fn create(
    CurrentUser(user): CurrentUser,
    State(state): State<AppState>,
    AppJson(input): AppJson<OrderInput>,
) -> ApiResult<(StatusCode, Json<OrderCreated>)> {
    let created = orders::create_order(&state.db, input, user.as_ref()).await?;
    Ok((StatusCode::CREATED, Json(created)))
}

#[derive(Deserialize)]
pub struct ViewerQuery {
    /// 訪客的 guest_token（規格 §6.1：訪客要帶 ?t=）
    pub t: Option<String>,
}

/// 先用帳號看（會員看自己的），看不到再用 ?t=；都不行 404（不用 403，免得被拿來猜訂單 id）
async fn find_order(
    state: &AppState,
    id: Uuid,
    user: Option<&User>,
    token: Option<&str>,
) -> ApiResult<(OrderDetail, Viewer)> {
    if let Some(user) = user {
        let viewer = Viewer::User(user.id);
        if let Some(detail) = orders::get_for_viewer(&state.db, id, &viewer).await? {
            return Ok((detail, viewer));
        }
    }
    if let Some(token) = token.map(str::trim).filter(|t| !t.is_empty()) {
        let viewer = Viewer::Guest(token.to_string());
        if let Some(detail) = orders::get_for_viewer(&state.db, id, &viewer).await? {
            return Ok((detail, viewer));
        }
    }
    Err(ApiError::NotFound)
}

async fn detail(
    CurrentUser(user): CurrentUser,
    State(state): State<AppState>,
    AppPath(id): AppPath<Uuid>,
    AppQuery(query): AppQuery<ViewerQuery>,
) -> ApiResult<Json<OrderDetail>> {
    let (detail, _) = find_order(&state, id, user.as_ref(), query.t.as_deref()).await?;
    Ok(Json(detail))
}

/// 買家取消（與規格不同之處 13）
async fn cancel(
    CurrentUser(user): CurrentUser,
    State(state): State<AppState>,
    AppPath(id): AppPath<Uuid>,
    AppQuery(query): AppQuery<ViewerQuery>,
) -> ApiResult<StatusCode> {
    let (_, viewer) = find_order(&state, id, user.as_ref(), query.t.as_deref()).await?;
    orders::cancel(&state.db, id, &viewer, "buyer").await?;
    Ok(StatusCode::NO_CONTENT)
}
```

- [ ] **Step 5: 在 `api/src/routes/me.rs` 加訂單列表**

`use crate::{...}` 裡 `domain::{...}` 改成：

```rust
    domain::{
        addresses::{self, Address, AddressInput},
        orders::{self, OrderSummary},
        products::{self, Page},
        users,
    },
```

`extract::{AppJson, AppPath}` 改成 `extract::{AppJson, AppPath, AppQuery}`。`router()` 加一行 `.route("/api/me/orders", get(list_orders))`。檔尾加：

```rust
#[derive(Deserialize)]
pub struct OrdersQuery {
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

async fn list_orders(
    AuthUser(user): AuthUser,
    State(state): State<AppState>,
    AppQuery(query): AppQuery<OrdersQuery>,
) -> ApiResult<Json<Page<OrderSummary>>> {
    let (page, per_page) = products::clamp_paging(query.page, query.per_page, 20, 50);
    Ok(Json(
        orders::list_for_user(&state.db, user.id, page, per_page).await?,
    ))
}
```

`api/src/routes/mod.rs` 加 `pub mod cart;`、`pub mod checkout;`、`pub mod orders;`；`api/src/app.rs` 在 `.merge(routes::products::router())` 之後加：

```rust
        .merge(routes::cart::router())
        .merge(routes::checkout::router())
        .merge(routes::orders::router())
```

- [ ] **Step 6: 寫 `api/tests/orders.rs`**

```rust
mod common;

use axum::http::StatusCode;
use serde_json::{Value, json};
use sqlx::PgPool;

fn order_body(variant: &str, qty: i32, method: &str, token: Option<&str>) -> Value {
    json!({
        "items": [{ "variant_id": variant, "qty": qty }],
        "email": "buyer@test.local",
        "recipient_name": "王小明",
        "recipient_phone": "0912345678",
        "shipping_method": method,
        "cvs_store_token": token,
        "address": if method == "home" {
            json!({ "postal_code": "100", "city": "臺北市", "district": "中正區", "street": "重慶南路一段 122 號" })
        } else { Value::Null },
        "invoice": { "type": "personal", "carrier_type": "1" },
        "payment_method": "credit",
        "note": ""
    })
}

#[sqlx::test(migrations = "./migrations")]
async fn guest_checkout_view_and_cancel(pool: PgPool) {
    let app = common::app(pool.clone());
    let (variant, _) = common::active_product(&pool, "雞肉狗糧", 300, 5).await;
    let (status, created, _) = common::send(
        &app,
        common::req("POST", "/api/orders", None, Some(order_body(&variant.to_string(), 2, "home", None))),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let id = created["order_id"].as_str().unwrap().to_string();
    let token = created["guest_token"].as_str().unwrap().to_string();
    assert!(created["order_no"].as_str().unwrap().starts_with("DS"));

    // 沒 token 看不到
    let (status, _, _) =
        common::send(&app, common::req("GET", &format!("/api/orders/{id}"), None, None)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _, _) = common::send(
        &app,
        common::req("GET", &format!("/api/orders/{id}?t=wrong"), None, None),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, detail, _) = common::send(
        &app,
        common::req("GET", &format!("/api/orders/{id}?t={token}"), None, None),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{detail}");
    assert_eq!(detail["status"], "pending_payment");
    assert_eq!(detail["total"], 700);
    assert_eq!(detail["items"][0]["product_name"], "雞肉狗糧");
    assert_eq!(detail["shipment"]["home_street"], "重慶南路一段 122 號");
    assert_eq!(detail["payment"]["method"], "credit");
    assert!(detail.get("guest_token").is_none(), "回應不含 guest_token");
    assert!(detail.get("user_id").is_none());

    // 取消 → 204，狀態變 cancelled，再取消 400
    let (status, _, _) = common::send(
        &app,
        common::req("POST", &format!("/api/orders/{id}/cancel?t={token}"), None, None),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, detail, _) = common::send(
        &app,
        common::req("GET", &format!("/api/orders/{id}?t={token}"), None, None),
    )
    .await;
    assert_eq!(detail["status"], "cancelled");
    let (status, body, _) = common::send(
        &app,
        common::req("POST", &format!("/api/orders/{id}/cancel?t={token}"), None, None),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["message"], "這筆訂單已經不能取消");
    let (status, _, _) = common::send(
        &app,
        common::req("POST", &format!("/api/orders/{id}/cancel?t=wrong"), None, None),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "./migrations")]
async fn member_checkout_and_order_list(pool: PgPool) {
    let app = common::app(pool.clone());
    let cookie = common::register_cookie(&app, "m@test.local", "password123", "甲").await;
    let other = common::register_cookie(&app, "o@test.local", "password123", "乙").await;
    let (variant, _) = common::active_product(&pool, "E", 100, 10).await;

    let (status, created, _) = common::send(
        &app,
        common::req(
            "POST",
            "/api/orders",
            Some(&cookie),
            Some(order_body(&variant.to_string(), 1, "home", None)),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let id = created["order_id"].as_str().unwrap().to_string();

    // 自己看不用 token；別人看不到
    let (status, detail, _) = common::send(
        &app,
        common::req("GET", &format!("/api/orders/{id}"), Some(&cookie), None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(detail["order_no"], created["order_no"]);
    let (status, _, _) = common::send(
        &app,
        common::req("GET", &format!("/api/orders/{id}"), Some(&other), None),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // 列表
    let (status, page, _) = common::send(
        &app,
        common::req("GET", "/api/me/orders", Some(&cookie), None),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert_eq!(page["total"], 1);
    assert_eq!(page["items"][0]["order_no"], created["order_no"]);
    assert_eq!(page["items"][0]["item_count"], 1);
    let (status, _, _) = common::send(&app, common::req("GET", "/api/me/orders", None, None)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "./migrations")]
async fn order_errors_over_http(pool: PgPool) {
    let app = common::app(pool.clone());
    let (variant, _) = common::active_product(&pool, "F", 300, 2).await;

    // 欄位驗證
    let mut bad = order_body(&variant.to_string(), 1, "home", None);
    bad["email"] = json!("nope");
    bad["recipient_phone"] = json!("123");
    bad["invoice"] = json!({ "type": "" });
    let (status, body, _) =
        common::send(&app, common::req("POST", "/api/orders", None, Some(bad))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let fields = &body["error"]["details"]["fields"];
    assert_eq!(fields["email"], "Email 格式不正確");
    assert_eq!(fields["recipient_phone"], "手機格式：09 開頭共 10 碼");
    assert_eq!(fields["invoice.type"], "請選擇發票類型");

    // 庫存不足 → 409
    let (status, body, _) = common::send(
        &app,
        common::req("POST", "/api/orders", None, Some(order_body(&variant.to_string(), 3, "home", None))),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "OUT_OF_STOCK");
    assert_eq!(body["error"]["details"]["items"][0]["variant_id"], variant.to_string());
    assert_eq!(body["error"]["details"]["items"][0]["available"], 2);

    // 超商沒門市 → 400 CVS_STORE_REQUIRED
    let (status, body, _) = common::send(
        &app,
        common::req("POST", "/api/orders", None, Some(order_body(&variant.to_string(), 1, "cvs", None))),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "CVS_STORE_REQUIRED");

    // 超商小計超過 20,000 → 400 CVS_AMOUNT_LIMIT
    let (pricey, _) = common::active_product(&pool, "貴", 25_000, 1).await;
    let token = common::cvs_store_token(&pool).await;
    let (status, body, _) = common::send(
        &app,
        common::req(
            "POST",
            "/api/orders",
            None,
            Some(order_body(&pricey.to_string(), 1, "cvs", Some(&token))),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "CVS_AMOUNT_LIMIT");

    // JSON 壞掉 → VALIDATION（details.detail）
    let (status, body, _) = common::send(
        &app,
        common::req("POST", "/api/orders", None, Some(json!({ "items": "x" }))),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "VALIDATION");
}
```

- [ ] **Step 7: 寫 `api/tests/cart.rs`**

```rust
mod common;

use axum::http::StatusCode;
use serde_json::json;
use sqlx::PgPool;

#[sqlx::test(migrations = "./migrations")]
async fn validate_reports_price_stock_and_availability(pool: PgPool) {
    let app = common::app(pool.clone());
    let (a, slug_a) = common::active_product(&pool, "A", 300, 5).await;
    let (b, _) = common::active_product(&pool, "B", 200, 0).await;
    let (c, _) = common::active_product(&pool, "C", 100, 9).await;
    let product_c: uuid::Uuid = sqlx::query_scalar("SELECT product_id FROM product_variants WHERE id = $1")
        .bind(c)
        .fetch_one(&pool)
        .await
        .unwrap();
    dog_shop_api::domain::products::archive(&pool, product_c).await.unwrap();
    let ghost = uuid::Uuid::now_v7();

    let (status, body, _) = common::send(
        &app,
        common::req(
            "POST",
            "/api/cart/validate",
            None,
            Some(json!({ "items": [
                { "variant_id": a, "qty": 7 },
                { "variant_id": b, "qty": 1 },
                { "variant_id": c, "qty": 1 },
                { "variant_id": ghost, "qty": 2 }
            ] })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 4);
    assert_eq!(items[0]["product_slug"], slug_a);
    assert_eq!(items[0]["variant_label"], "預設");
    assert_eq!(items[0]["qty"], 5);
    assert_eq!(items[0]["stock"], 5);
    assert_eq!(items[0]["available"], true);
    assert_eq!(items[0]["reason"], "qty_reduced");
    assert_eq!(items[1]["available"], false);
    assert_eq!(items[1]["reason"], "sold_out");
    assert_eq!(items[2]["reason"], "unavailable");
    assert_eq!(items[3]["reason"], "unavailable");
    assert_eq!(items[3]["product_name"], "已下架的商品");
    assert_eq!(body["subtotal"], 1500);
    assert_eq!(body["shipping"]["cvs_fee"], 60);
    assert_eq!(body["cvs_limit_exceeded"], false);

    // 空購物車也是 200
    let (status, body, _) = common::send(
        &app,
        common::req("POST", "/api/cart/validate", None, Some(json!({ "items": [] }))),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["items"].as_array().unwrap().len(), 0);
    assert_eq!(body["subtotal"], 0);

    // 小計超過 20,000 → cvs_limit_exceeded
    let (pricey, _) = common::active_product(&pool, "貴", 25_000, 1).await;
    let (_, body, _) = common::send(
        &app,
        common::req(
            "POST",
            "/api/cart/validate",
            None,
            Some(json!({ "items": [ { "variant_id": pricey, "qty": 1 } ] })),
        ),
    )
    .await;
    assert_eq!(body["cvs_limit_exceeded"], true);
}

#[sqlx::test(migrations = "./migrations")]
async fn cvs_store_lookup(pool: PgPool) {
    let app = common::app(pool.clone());
    let token = common::cvs_store_token(&pool).await;
    let (status, body, _) = common::send(
        &app,
        common::req("GET", &format!("/api/checkout/cvs-store/{token}"), None, None),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["store_name"], "測試門市");
    assert_eq!(body["sub_type"], "UNIMARTC2C");
    let (status, _, _) = common::send(
        &app,
        common::req("GET", "/api/checkout/cvs-store/nope", None, None),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    sqlx::query("UPDATE cvs_store_selections SET expires_at = now() - interval '1 minute'")
        .execute(&pool)
        .await
        .unwrap();
    let (status, _, _) = common::send(
        &app,
        common::req("GET", &format!("/api/checkout/cvs-store/{token}"), None, None),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
```

- [ ] **Step 8: 跑測試、格式、lint**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && cd api && DATABASE_URL=postgres://dog_shop:dog_shop@localhost:5435/dog_shop cargo test`（背景）
Expected: 全綠；整合多 5 個。
Run: `export PATH="$HOME/.cargo/bin:$PATH" && cd api && cargo fmt --all && cargo clippy --all-targets -- -D warnings`

- [ ] **Step 9: Commit**

```bash
git add api/src/domain/cart.rs api/src/domain/mod.rs api/src/routes/orders.rs api/src/routes/cart.rs api/src/routes/checkout.rs api/src/routes/me.rs api/src/routes/mod.rs api/src/app.rs api/tests/orders.rs api/tests/cart.rs
git commit -m "feat(api): 下單、訂單查詢與取消、會員訂單列表、購物車驗證、門市查詢 API"
```

### Task 9: 前端型別、驗證工具、台灣地址資料

**Files:**
- Modify: `web/src/lib/types.ts`
- Create: `web/src/lib/labels.ts`、`web/src/lib/validation.ts`、`web/src/lib/validation.test.ts`、`web/src/lib/data/tw-districts.json`、`web/src/lib/tw-address.ts`、`web/src/lib/tw-address.test.ts`

**Interfaces:**
- Consumes: Task 4 的 `ShippingSettings` 型別。
- Produces: 型別 `Address`、`AddressInput`、`ShippingMethod`、`PaymentMethod`、`InvoiceType`、`CvsSubType`、`CvsStore`、`CartCheckedLine`、`CartValidateResponse`、`OrderStatus`、`OrderInput`、`OrderCreated`、`OrderItem`、`OrderShipment`、`OrderPayment`、`OrderDetail`、`OrderSummary`；`labels.ts` 的 `ORDER_STATUS_LABELS`、`CVS_LABELS`、`PAYMENT_LABELS`、`INVOICE_LABELS`；`validation.ts` 的 `isTwMobile`、`isPostalCode`、`isCvsRecipientName`、`isMobileBarcode`、`isCitizenCert`、`isLoveCode`、`isTaxId`、`shippingFee`、`CVS_SUBTOTAL_LIMIT`、`MAX_QTY`；`tw-address.ts` 的 `cities()`、`districts(city)`、`postalCode(city, district)`。

- [ ] **Step 1: 在 `web/src/lib/types.ts` 檔尾加型別**

```ts
export type Address = {
	id: string;
	recipient_name: string;
	phone: string;
	postal_code: string;
	city: string;
	district: string;
	street: string;
	is_default: boolean;
};
export type AddressInput = Omit<Address, 'id'>;

export type ShippingMethod = 'cvs' | 'home';
export type PaymentMethod = 'credit' | 'atm' | 'cvs_code';
export type InvoiceType = 'personal' | 'company' | 'donation';
export type CvsSubType = 'UNIMARTC2C' | 'FAMIC2C' | 'HILIFEC2C';

export type CvsStore = {
	token: string;
	sub_type: CvsSubType;
	store_id: string;
	store_name: string;
	store_address: string;
	store_phone: string;
};

export type CartCheckedLine = {
	variant_id: string;
	product_slug: string;
	product_name: string;
	variant_label: string;
	price: number;
	image_thumb: string | null;
	stock: number;
	qty: number;
	available: boolean;
	reason: null | 'unavailable' | 'sold_out' | 'qty_reduced';
};
export type CartValidateResponse = {
	items: CartCheckedLine[];
	subtotal: number;
	shipping: ShippingSettings;
	cvs_limit_exceeded: boolean;
};

export type OrderStatus = 'pending_payment' | 'paid' | 'shipped' | 'completed' | 'cancelled' | 'refunded';

export type HomeAddressInput = { postal_code: string; city: string; district: string; street: string };
export type InvoiceInput = {
	type: InvoiceType;
	carrier_type?: '1' | '2' | '3';
	carrier_num?: string;
	tax_id?: string;
	title?: string;
	address?: string;
	love_code?: string;
};
export type OrderInput = {
	items: { variant_id: string; qty: number }[];
	email: string;
	recipient_name: string;
	recipient_phone: string;
	shipping_method: ShippingMethod;
	cvs_store_token?: string | null;
	address?: HomeAddressInput | null;
	invoice: InvoiceInput;
	payment_method: PaymentMethod;
	note?: string;
};
export type OrderCreated = { order_id: string; order_no: string; guest_token: string };

export type OrderItem = {
	product_name: string;
	variant_label: string;
	unit_price: number;
	quantity: number;
	line_total: number;
	image_path: string | null;
};
export type OrderShipment = {
	method: ShippingMethod;
	cvs_sub_type: CvsSubType | null;
	cvs_store_id: string | null;
	cvs_store_name: string | null;
	cvs_store_address: string | null;
	home_postal_code: string | null;
	home_city: string | null;
	home_district: string | null;
	home_street: string | null;
	status: string;
	carrier: string | null;
	tracking_no: string | null;
};
export type OrderPayment = {
	method: PaymentMethod;
	status: 'pending' | 'paid' | 'failed' | 'expired';
	amount: number;
	atm_bank_code: string | null;
	atm_vaccount: string | null;
	cvs_payment_no: string | null;
	expire_at: string | null;
};
export type OrderDetail = {
	id: string;
	order_no: string;
	status: OrderStatus;
	email: string;
	recipient_name: string;
	recipient_phone: string;
	shipping_method: ShippingMethod;
	subtotal: number;
	shipping_fee: number;
	total: number;
	note: string;
	invoice_type: InvoiceType;
	invoice_carrier_type: string | null;
	invoice_carrier_num: string | null;
	invoice_tax_id: string | null;
	invoice_title: string | null;
	invoice_address: string | null;
	invoice_love_code: string | null;
	created_at: string;
	paid_at: string | null;
	shipped_at: string | null;
	completed_at: string | null;
	cancelled_at: string | null;
	cancel_reason: string | null;
	items: OrderItem[];
	shipment: OrderShipment | null;
	payment: OrderPayment | null;
};
export type OrderSummary = {
	id: string;
	order_no: string;
	status: OrderStatus;
	total: number;
	item_count: number;
	created_at: string;
};
```

- [ ] **Step 2: 寫 `web/src/lib/labels.ts`**

```ts
import type { CvsSubType, InvoiceType, OrderStatus, PaymentMethod } from '$lib/types';

export const ORDER_STATUS_LABELS: Record<OrderStatus, string> = {
	pending_payment: '待付款',
	paid: '已付款',
	shipped: '已出貨',
	completed: '已完成',
	cancelled: '已取消',
	refunded: '已退款'
};

export const CVS_LABELS: Record<CvsSubType, string> = {
	UNIMARTC2C: '7-ELEVEN',
	FAMIC2C: '全家',
	HILIFEC2C: '萊爾富'
};

export const PAYMENT_LABELS: Record<PaymentMethod, string> = {
	credit: '信用卡',
	atm: 'ATM 轉帳',
	cvs_code: '超商代碼繳費'
};

export const INVOICE_LABELS: Record<InvoiceType, string> = {
	personal: '個人（電子發票）',
	company: '公司（統一編號）',
	donation: '捐贈'
};
```

- [ ] **Step 3: 寫 `web/src/lib/validation.ts`（規則與後端 `domain/orders.rs` 一模一樣）**

```ts
import type { ShippingMethod, ShippingSettings } from '$lib/types';

/** 超商取貨商品小計上限（綠界 C2C） */
export const CVS_SUBTOTAL_LIMIT = 20000;
export const MAX_QTY = 99;

/** 台灣手機：09 開頭共 10 碼 */
export function isTwMobile(phone: string): boolean {
	return /^09\d{8}$/.test(phone);
}

/** 郵遞區號 3～5 碼數字 */
export function isPostalCode(code: string): boolean {
	return /^\d{3,5}$/.test(code);
}

/** 超商取貨收件人：2～5 個中文字 */
export function isCvsRecipientName(name: string): boolean {
	return /^[\u4e00-\u9fff]{2,5}$/.test(name);
}

/** 手機條碼載具：/ 開頭 + 7 碼（0-9、A-Z、+、-、.） */
export function isMobileBarcode(s: string): boolean {
	return /^\/[0-9A-Z+\-.]{7}$/.test(s);
}

/** 自然人憑證條碼：2 個大寫英文字母 + 14 碼數字 */
export function isCitizenCert(s: string): boolean {
	return /^[A-Z]{2}\d{14}$/.test(s);
}

/** 捐贈愛心碼：3～7 碼數字 */
export function isLoveCode(s: string): boolean {
	return /^\d{3,7}$/.test(s);
}

/**
 * 統一編號（財政部規則）：各位數乘 1,2,1,2,1,2,4,1，每個乘積十位＋個位相加，
 * 總和能被 5 整除即合法；第 7 碼是 7 時總和 + 1 能被 5 整除也合法。全 0 不算。
 */
export function isTaxId(s: string): boolean {
	if (!/^\d{8}$/.test(s) || s === '00000000') return false;
	const weights = [1, 2, 1, 2, 1, 2, 4, 1];
	const digits = [...s].map(Number);
	const sum = digits.reduce((acc, d, i) => {
		const p = d * weights[i];
		return acc + Math.floor(p / 10) + (p % 10);
	}, 0);
	return sum % 5 === 0 || (digits[6] === 7 && (sum + 1) % 5 === 0);
}

/** 免運門檻 0 = 不免運；否則小計 >= 門檻免運（與後端 shipping_fee 相同） */
export function shippingFee(shipping: ShippingSettings, method: ShippingMethod, subtotal: number): number {
	if (shipping.free_threshold > 0 && subtotal >= shipping.free_threshold) return 0;
	return method === 'cvs' ? shipping.cvs_fee : shipping.home_fee;
}
```

- [ ] **Step 4: 寫 `web/src/lib/validation.test.ts`**

```ts
import { describe, expect, it } from 'vitest';
import {
	isCitizenCert,
	isCvsRecipientName,
	isLoveCode,
	isMobileBarcode,
	isPostalCode,
	isTaxId,
	isTwMobile,
	shippingFee
} from './validation';

describe('validation', () => {
	it('mobile / postal code', () => {
		expect(isTwMobile('0912345678')).toBe(true);
		expect(isTwMobile('091234567')).toBe(false);
		expect(isTwMobile('0212345678')).toBe(false);
		expect(isPostalCode('100')).toBe(true);
		expect(isPostalCode('10058')).toBe(true);
		expect(isPostalCode('10')).toBe(false);
		expect(isPostalCode('100a')).toBe(false);
	});

	it('names and carriers', () => {
		expect(isCvsRecipientName('王小明')).toBe(true);
		expect(isCvsRecipientName('王')).toBe(false);
		expect(isCvsRecipientName('John')).toBe(false);
		expect(isMobileBarcode('/ABC+123')).toBe(true);
		expect(isMobileBarcode('/abc+123')).toBe(false);
		expect(isCitizenCert('AB12345678901234')).toBe(true);
		expect(isCitizenCert('A123456789012345')).toBe(false);
		expect(isLoveCode('168')).toBe(true);
		expect(isLoveCode('12')).toBe(false);
	});

	it('tax id checksum matches the backend vectors', () => {
		expect(isTaxId('04595257')).toBe(true);
		expect(isTaxId('10000004')).toBe(true);
		expect(isTaxId('12345675')).toBe(true);
		expect(isTaxId('12345678')).toBe(false);
		expect(isTaxId('12345674')).toBe(false);
		expect(isTaxId('1234567')).toBe(false);
		expect(isTaxId('00000000')).toBe(false);
	});

	it('shipping fee with free threshold', () => {
		const s = { cvs_fee: 60, home_fee: 100, free_threshold: 0 };
		expect(shippingFee(s, 'cvs', 99999)).toBe(60);
		expect(shippingFee(s, 'home', 1)).toBe(100);
		const free = { ...s, free_threshold: 1000 };
		expect(shippingFee(free, 'home', 999)).toBe(100);
		expect(shippingFee(free, 'home', 1000)).toBe(0);
	});
});
```

- [ ] **Step 5: 寫 `web/src/lib/data/tw-districts.json`（縣市 → 鄉鎮市區 → 郵遞區號前 3 碼；以中華郵政資料為準，Step 8 會抽查）**

```json
{
"臺北市": {"中正區":"100","大同區":"103","中山區":"104","松山區":"105","大安區":"106","萬華區":"108","信義區":"110","士林區":"111","北投區":"112","內湖區":"114","南港區":"115","文山區":"116"},
"新北市": {"萬里區":"207","金山區":"208","板橋區":"220","汐止區":"221","深坑區":"222","石碇區":"223","瑞芳區":"224","平溪區":"226","雙溪區":"227","貢寮區":"228","新店區":"231","坪林區":"232","烏來區":"233","永和區":"234","中和區":"235","土城區":"236","三峽區":"237","樹林區":"238","鶯歌區":"239","三重區":"241","新莊區":"242","泰山區":"243","林口區":"244","蘆洲區":"247","五股區":"248","八里區":"249","淡水區":"251","三芝區":"252","石門區":"253"},
"基隆市": {"仁愛區":"200","信義區":"201","中正區":"202","中山區":"203","安樂區":"204","暖暖區":"205","七堵區":"206"},
"桃園市": {"中壢區":"320","平鎮區":"324","龍潭區":"325","楊梅區":"326","新屋區":"327","觀音區":"328","桃園區":"330","龜山區":"333","八德區":"334","大溪區":"335","復興區":"336","大園區":"337","蘆竹區":"338"},
"新竹市": {"東區":"300","北區":"300","香山區":"300"},
"新竹縣": {"竹北市":"302","湖口鄉":"303","新豐鄉":"304","新埔鎮":"305","關西鎮":"306","芎林鄉":"307","寶山鄉":"308","竹東鎮":"310","五峰鄉":"311","橫山鄉":"312","尖石鄉":"313","北埔鄉":"314","峨眉鄉":"315"},
"苗栗縣": {"竹南鎮":"350","頭份市":"351","三灣鄉":"352","南庄鄉":"353","獅潭鄉":"354","後龍鎮":"356","通霄鎮":"357","苑裡鎮":"358","苗栗市":"360","造橋鄉":"361","頭屋鄉":"362","公館鄉":"363","大湖鄉":"364","泰安鄉":"365","銅鑼鄉":"366","三義鄉":"367","西湖鄉":"368","卓蘭鎮":"369"},
"臺中市": {"中區":"400","東區":"401","南區":"402","西區":"403","北區":"404","北屯區":"406","西屯區":"407","南屯區":"408","太平區":"411","大里區":"412","霧峰區":"413","烏日區":"414","豐原區":"420","后里區":"421","石岡區":"422","東勢區":"423","和平區":"424","新社區":"426","潭子區":"427","大雅區":"428","神岡區":"429","大肚區":"432","沙鹿區":"433","龍井區":"434","梧棲區":"435","清水區":"436","大甲區":"437","外埔區":"438","大安區":"439"},
"彰化縣": {"彰化市":"500","芬園鄉":"502","花壇鄉":"503","秀水鄉":"504","鹿港鎮":"505","福興鄉":"506","線西鄉":"507","和美鎮":"508","伸港鄉":"509","員林市":"510","社頭鄉":"511","永靖鄉":"512","埔心鄉":"513","溪湖鎮":"514","大村鄉":"515","埔鹽鄉":"516","田中鎮":"520","北斗鎮":"521","田尾鄉":"522","埤頭鄉":"523","溪州鄉":"524","竹塘鄉":"525","二林鎮":"526","大城鄉":"527","芳苑鄉":"528","二水鄉":"530"},
"南投縣": {"南投市":"540","中寮鄉":"541","草屯鎮":"542","國姓鄉":"544","埔里鎮":"545","仁愛鄉":"546","名間鄉":"551","集集鎮":"552","水里鄉":"553","魚池鄉":"555","信義鄉":"556","竹山鎮":"557","鹿谷鄉":"558"},
"雲林縣": {"斗南鎮":"630","大埤鄉":"631","虎尾鎮":"632","土庫鎮":"633","褒忠鄉":"634","東勢鄉":"635","臺西鄉":"636","崙背鄉":"637","麥寮鄉":"638","斗六市":"640","林內鄉":"643","古坑鄉":"646","莿桐鄉":"647","西螺鎮":"648","二崙鄉":"649","北港鎮":"651","水林鄉":"652","口湖鄉":"653","四湖鄉":"654","元長鄉":"655"},
"嘉義市": {"東區":"600","西區":"600"},
"嘉義縣": {"番路鄉":"602","梅山鄉":"603","竹崎鄉":"604","阿里山鄉":"605","中埔鄉":"606","大埔鄉":"607","水上鄉":"608","鹿草鄉":"611","太保市":"612","朴子市":"613","東石鄉":"614","六腳鄉":"615","新港鄉":"616","民雄鄉":"621","大林鎮":"622","溪口鄉":"623","義竹鄉":"624","布袋鎮":"625"},
"臺南市": {"中西區":"700","東區":"701","南區":"702","北區":"704","安平區":"708","安南區":"709","永康區":"710","歸仁區":"711","新化區":"712","左鎮區":"713","玉井區":"714","楠西區":"715","南化區":"716","仁德區":"717","關廟區":"718","龍崎區":"719","官田區":"720","麻豆區":"721","佳里區":"722","西港區":"723","七股區":"724","將軍區":"725","學甲區":"726","北門區":"727","新營區":"730","後壁區":"731","白河區":"732","東山區":"733","六甲區":"734","下營區":"735","柳營區":"736","鹽水區":"737","善化區":"741","大內區":"742","山上區":"743","新市區":"744","安定區":"745"},
"高雄市": {"新興區":"800","前金區":"801","苓雅區":"802","鹽埕區":"803","鼓山區":"804","旗津區":"805","前鎮區":"806","三民區":"807","楠梓區":"811","小港區":"812","左營區":"813","仁武區":"814","大社區":"815","岡山區":"820","路竹區":"821","阿蓮區":"822","田寮區":"823","燕巢區":"824","橋頭區":"825","梓官區":"826","彌陀區":"827","永安區":"828","湖內區":"829","鳳山區":"830","大寮區":"831","林園區":"832","鳥松區":"833","大樹區":"840","旗山區":"842","美濃區":"843","六龜區":"844","內門區":"845","杉林區":"846","甲仙區":"847","桃源區":"848","那瑪夏區":"849","茂林區":"851","茄萣區":"852"},
"屏東縣": {"屏東市":"900","三地門鄉":"901","霧臺鄉":"902","瑪家鄉":"903","九如鄉":"904","里港鄉":"905","高樹鄉":"906","鹽埔鄉":"907","長治鄉":"908","麟洛鄉":"909","竹田鄉":"911","內埔鄉":"912","萬丹鄉":"913","潮州鎮":"920","泰武鄉":"921","來義鄉":"922","萬巒鄉":"923","崁頂鄉":"924","新埤鄉":"925","南州鄉":"926","林邊鄉":"927","東港鎮":"928","琉球鄉":"929","佳冬鄉":"931","新園鄉":"932","枋寮鄉":"940","枋山鄉":"941","春日鄉":"942","獅子鄉":"943","車城鄉":"944","牡丹鄉":"945","恆春鎮":"946","滿州鄉":"947"},
"宜蘭縣": {"宜蘭市":"260","頭城鎮":"261","礁溪鄉":"262","壯圍鄉":"263","員山鄉":"264","羅東鎮":"265","三星鄉":"266","大同鄉":"267","五結鄉":"268","冬山鄉":"269","蘇澳鎮":"270","南澳鄉":"272"},
"花蓮縣": {"花蓮市":"970","新城鄉":"971","秀林鄉":"972","吉安鄉":"973","壽豐鄉":"974","鳳林鎮":"975","光復鄉":"976","豐濱鄉":"977","瑞穗鄉":"978","萬榮鄉":"979","玉里鎮":"981","卓溪鄉":"982","富里鄉":"983"},
"臺東縣": {"臺東市":"950","綠島鄉":"951","蘭嶼鄉":"952","延平鄉":"953","卑南鄉":"954","鹿野鄉":"955","關山鎮":"956","海端鄉":"957","池上鄉":"958","東河鄉":"959","成功鎮":"961","長濱鄉":"962","太麻里鄉":"963","金峰鄉":"964","大武鄉":"965","達仁鄉":"966"},
"澎湖縣": {"馬公市":"880","西嶼鄉":"881","望安鄉":"882","七美鄉":"883","白沙鄉":"884","湖西鄉":"885"},
"金門縣": {"金沙鎮":"890","金湖鎮":"891","金寧鄉":"892","金城鎮":"893","烈嶼鄉":"894","烏坵鄉":"896"},
"連江縣": {"南竿鄉":"209","北竿鄉":"210","莒光鄉":"211","東引鄉":"212"}
}
```

- [ ] **Step 6: 寫 `web/src/lib/tw-address.ts` 與測試**

```ts
import data from './data/tw-districts.json';

const cityMap = data as Record<string, Record<string, string>>;

/** 縣市列表（JSON 的順序：北到南、再離島） */
export function cities(): string[] {
	return Object.keys(cityMap);
}

export function districts(city: string): string[] {
	return Object.keys(cityMap[city] ?? {});
}

/** 找不到回空字串（表單會讓使用者自己填） */
export function postalCode(city: string, district: string): string {
	return cityMap[city]?.[district] ?? '';
}
```

`web/src/lib/tw-address.test.ts`：

```ts
import { describe, expect, it } from 'vitest';
import { cities, districts, postalCode } from './tw-address';

describe('tw-address', () => {
	it('has all 22 cities and counties', () => {
		expect(cities()).toHaveLength(22);
		expect(cities()[0]).toBe('臺北市');
	});
	it('looks up districts and postal codes', () => {
		expect(districts('臺北市')).toContain('中正區');
		expect(postalCode('臺北市', '中正區')).toBe('100');
		expect(postalCode('高雄市', '鳳山區')).toBe('830');
		expect(postalCode('連江縣', '南竿鄉')).toBe('209');
		expect(postalCode('火星', '陨石區')).toBe('');
		expect(districts('火星')).toEqual([]);
	});
	it('every postal code is 3 digits', () => {
		for (const city of cities()) {
			for (const d of districts(city)) expect(postalCode(city, d)).toMatch(/^\d{3}$/);
		}
	});
});
```

- [ ] **Step 7: 跑測試與檢查**

Run: `pnpm -C web test`
Expected: 4 個測試檔（api、cart、validation、tw-address）全綠，共 16 個測試。
Run: `pnpm -C web check`
Expected: 0 errors 0 warnings（JSON import 靠 SvelteKit 產生的 tsconfig 的 `resolveJsonModule`；若報「Cannot find module './data/tw-districts.json'」，在 `web/tsconfig.json` 的 `compilerOptions` 加 `"resolveJsonModule": true` 並在回報說明）。

- [ ] **Step 8: 抽查郵遞區號**

用 `curl` 或你知道的資料抽查 5 筆（例如 臺北市信義區 110、臺中市西屯區 407、臺南市安平區 708、高雄市左營區 813、花蓮縣吉安鄉 973）。錯的直接改 JSON；在回報裡列出檢查了哪幾筆。

- [ ] **Step 9: Commit**

```bash
git add web/src/lib/types.ts web/src/lib/labels.ts web/src/lib/validation.ts web/src/lib/validation.test.ts web/src/lib/data/tw-districts.json web/src/lib/tw-address.ts web/src/lib/tw-address.test.ts
git commit -m "feat(web): 訂單與會員型別、表單驗證工具、台灣縣市鄉鎮郵遞區號資料"
```

### Task 10: 註冊、忘記密碼、重設密碼頁

**Files:**
- Modify: `web/src/lib/api.ts`（`ApiError.fields()`）、`web/src/lib/api.test.ts`
- Create: `web/src/routes/register/+page.server.ts`、`web/src/routes/register/+page.svelte`、`web/src/routes/forgot-password/+page.svelte`、`web/src/routes/reset/[token]/+page.svelte`
- Modify: `web/src/routes/login/+page.svelte`（加兩個連結）

**Interfaces:**
- Consumes: Task 3 的三支 API；計畫 1 的 `api()`、`toast`、`goto`。
- Produces: `ApiError.fields(): Record<string, string>`（沒有欄位錯誤時回 `{}`）；三個頁面。

- [ ] **Step 1: `web/src/lib/api.ts` 的 `ApiError` 加方法**

在 `field(name)` 方法之後加：

```ts
	/** 驗證錯誤的整張欄位表；不是欄位錯誤時回空物件 */
	fields(): Record<string, string> {
		return (this.details as { fields?: Record<string, string> } | null)?.fields ?? {};
	}
```

`web/src/lib/api.test.ts` 檔尾加：

```ts
it('fields() returns the whole map or {}', () => {
	const err = new ApiError(400, 'VALIDATION', '輸入資料有誤', { fields: { email: '格式不正確' } });
	expect(err.fields()).toEqual({ email: '格式不正確' });
	expect(new ApiError(500, 'INTERNAL', 'x').fields()).toEqual({});
});
```

（若該檔的 `it`/`expect` 是在 `describe` 區塊裡 import 的，就把這段放進同一個 `describe`。）

- [ ] **Step 2: 寫 `web/src/routes/register/+page.server.ts`**

```ts
import { redirect } from '@sveltejs/kit';
import type { PageServerLoad } from './$types';

export const load: PageServerLoad = ({ locals }) => {
	if (locals.user) redirect(303, locals.user.role === 'admin' ? '/admin' : '/account');
	return {};
};
```

- [ ] **Step 3: 寫 `web/src/routes/register/+page.svelte`**

```svelte
<script lang="ts">
	import { goto, invalidateAll } from '$app/navigation';
	import { api, ApiError } from '$lib/api';
	import type { User } from '$lib/types';

	let email = $state('');
	let password = $state('');
	let name = $state('');
	let phone = $state('');
	let errors = $state<Record<string, string>>({});
	let message = $state('');
	let submitting = $state(false);

	async function submit(e: SubmitEvent) {
		e.preventDefault();
		errors = {};
		message = '';
		submitting = true;
		try {
			await api<{ user: User }>('/api/auth/register', {
				method: 'POST',
				body: JSON.stringify({ email, password, name, phone: phone.trim() || null })
			});
			await invalidateAll();
			await goto('/');
		} catch (err) {
			if (err instanceof ApiError) {
				errors = err.fields();
				if (Object.keys(errors).length === 0) message = err.message;
			} else {
				message = '註冊失敗，請再試一次';
			}
		} finally {
			submitting = false;
		}
	}
</script>

<svelte:head><title>註冊</title></svelte:head>

<div class="mx-auto max-w-sm">
	<h1 class="mb-6 text-2xl font-bold">註冊</h1>
	<form onsubmit={submit} class="space-y-4" novalidate>
		<label class="block text-sm text-gray-700">
			Email
			<input type="email" bind:value={email} required autocomplete="email" class="mt-1 w-full rounded border border-gray-300 px-3 py-2 text-base" />
			{#if errors.email}<span class="text-red-600">{errors.email}</span>{/if}
		</label>
		<label class="block text-sm text-gray-700">
			密碼（至少 8 碼）
			<input type="password" bind:value={password} required autocomplete="new-password" class="mt-1 w-full rounded border border-gray-300 px-3 py-2 text-base" />
			{#if errors.password}<span class="text-red-600">{errors.password}</span>{/if}
		</label>
		<label class="block text-sm text-gray-700">
			姓名
			<input type="text" bind:value={name} required autocomplete="name" class="mt-1 w-full rounded border border-gray-300 px-3 py-2 text-base" />
			{#if errors.name}<span class="text-red-600">{errors.name}</span>{/if}
		</label>
		<label class="block text-sm text-gray-700">
			手機（選填）
			<input type="tel" bind:value={phone} autocomplete="tel" placeholder="09xxxxxxxx" class="mt-1 w-full rounded border border-gray-300 px-3 py-2 text-base" />
			{#if errors.phone}<span class="text-red-600">{errors.phone}</span>{/if}
		</label>
		{#if message}<p class="text-sm text-red-600">{message}</p>{/if}
		<button type="submit" disabled={submitting} class="w-full rounded bg-gray-900 px-4 py-2 text-white disabled:opacity-50">
			{submitting ? '註冊中…' : '建立帳號'}
		</button>
	</form>
	<p class="mt-4 text-sm text-gray-600">已經有帳號？<a href="/login" class="underline">登入</a></p>
</div>
```

- [ ] **Step 4: 寫 `web/src/routes/forgot-password/+page.svelte`**

```svelte
<script lang="ts">
	import { api, ApiError } from '$lib/api';

	let email = $state('');
	let error = $state('');
	let sent = $state(false);
	let submitting = $state(false);

	async function submit(e: SubmitEvent) {
		e.preventDefault();
		error = '';
		submitting = true;
		try {
			await api<{ ok: boolean }>('/api/auth/forgot', { method: 'POST', body: JSON.stringify({ email }) });
			sent = true;
		} catch (err) {
			error = err instanceof ApiError ? (err.field('email') ?? err.message) : '送出失敗，請再試一次';
		} finally {
			submitting = false;
		}
	}
</script>

<svelte:head><title>忘記密碼</title></svelte:head>

<div class="mx-auto max-w-sm">
	<h1 class="mb-6 text-2xl font-bold">忘記密碼</h1>
	{#if sent}
		<p class="rounded border border-green-200 bg-green-50 p-4 text-sm text-green-800">
			如果這個 Email 有註冊過，我們會寄出重設密碼的連結（1 小時內有效），請到信箱查看。
		</p>
	{:else}
		<form onsubmit={submit} class="space-y-4" novalidate>
			<label class="block text-sm text-gray-700">
				註冊時用的 Email
				<input type="email" bind:value={email} required autocomplete="email" class="mt-1 w-full rounded border border-gray-300 px-3 py-2 text-base" />
			</label>
			{#if error}<p class="text-sm text-red-600">{error}</p>{/if}
			<button type="submit" disabled={submitting} class="w-full rounded bg-gray-900 px-4 py-2 text-white disabled:opacity-50">
				{submitting ? '送出中…' : '寄送重設連結'}
			</button>
		</form>
	{/if}
	<p class="mt-4 text-sm text-gray-600"><a href="/login" class="underline">回登入</a></p>
</div>
```

- [ ] **Step 5: 寫 `web/src/routes/reset/[token]/+page.svelte`**

```svelte
<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { api, ApiError } from '$lib/api';
	import { toast } from '$lib/toast.svelte';

	let password = $state('');
	let confirm = $state('');
	let error = $state('');
	let submitting = $state(false);

	async function submit(e: SubmitEvent) {
		e.preventDefault();
		error = '';
		if (password !== confirm) {
			error = '兩次輸入的密碼不一樣';
			return;
		}
		submitting = true;
		try {
			await api('/api/auth/reset', {
				method: 'POST',
				body: JSON.stringify({ token: page.params.token, password })
			});
			toast.show('密碼已更新，請重新登入');
			await goto('/login');
		} catch (err) {
			error = err instanceof ApiError ? (err.field('token') ?? err.field('password') ?? err.message) : '重設失敗，請再試一次';
		} finally {
			submitting = false;
		}
	}
</script>

<svelte:head><title>重設密碼</title></svelte:head>

<div class="mx-auto max-w-sm">
	<h1 class="mb-6 text-2xl font-bold">重設密碼</h1>
	<form onsubmit={submit} class="space-y-4" novalidate>
		<label class="block text-sm text-gray-700">
			新密碼（至少 8 碼）
			<input type="password" bind:value={password} required autocomplete="new-password" class="mt-1 w-full rounded border border-gray-300 px-3 py-2 text-base" />
		</label>
		<label class="block text-sm text-gray-700">
			再輸入一次
			<input type="password" bind:value={confirm} required autocomplete="new-password" class="mt-1 w-full rounded border border-gray-300 px-3 py-2 text-base" />
		</label>
		{#if error}<p class="text-sm text-red-600">{error}</p>{/if}
		<button type="submit" disabled={submitting} class="w-full rounded bg-gray-900 px-4 py-2 text-white disabled:opacity-50">
			{submitting ? '更新中…' : '更新密碼'}
		</button>
	</form>
</div>
```

- [ ] **Step 6: 登入頁加連結**

`web/src/routes/login/+page.svelte` 的 `</form>` 之後（`</div>` 之前）加：

```svelte
	<div class="mt-4 flex justify-between text-sm text-gray-600">
		<a href="/register" class="underline">還沒有帳號？註冊</a>
		<a href="/forgot-password" class="underline">忘記密碼？</a>
	</div>
```

- [ ] **Step 7: 檢查、測試、建置**

Run: `pnpm -C web check && pnpm -C web test && pnpm -C web build`
Expected: 0 errors 0 warnings；測試 17 個；build 成功。
Curl 檢查（`pnpm -C web dev` 背景跑；不需要 api）：`curl -s http://localhost:5173/register | grep -c 建立帳號`、`/forgot-password | grep -c 寄送重設連結`、`/reset/abc | grep -c 更新密碼` 都是 1。跑完把 dev server 關掉。

- [ ] **Step 8: Commit**

```bash
git add web/src/lib/api.ts web/src/lib/api.test.ts web/src/routes/register web/src/routes/forgot-password web/src/routes/reset web/src/routes/login/+page.svelte
git commit -m "feat(web): 註冊、忘記密碼、重設密碼頁"
```

### Task 11: 會員中心（守門、個人資料、常用地址、訂單列表）與頁首連結

**Files:**
- Create: `web/src/lib/components/AddressFields.svelte`
- Create: `web/src/routes/account/+layout.server.ts`、`+layout.svelte`、`+page.svelte`
- Create: `web/src/routes/account/orders/+page.server.ts`、`+page.svelte`
- Create: `web/src/routes/account/addresses/+page.server.ts`、`+page.svelte`
- Modify: `web/src/routes/+layout.svelte`（會員中心連結）

**Interfaces:**
- Consumes: Task 5 的 `/api/me/*`、Task 8 的 `/api/me/orders`；Task 9 的型別、`validation.ts`、`tw-address.ts`、`labels.ts`；計畫 1 的 `Pagination`、`formatDate`、`twd`。
- Produces: `AddressFields` 元件，props `{ address: HomeAddressInput（bindable）, errors: Record<string, string>, prefix?: string }`（錯誤 key 是 `prefix + 'postal_code'` 等，結帳頁用 `prefix="address."`）；`/account` 三頁。

- [ ] **Step 1: 寫 `web/src/lib/components/AddressFields.svelte`（縣市 → 鄉鎮 → 郵遞區號自動帶入，可改）**

```svelte
<script lang="ts">
	import type { HomeAddressInput } from '$lib/types';
	import { cities, districts, postalCode } from '$lib/tw-address';

	let {
		address = $bindable(),
		errors = {},
		prefix = ''
	}: { address: HomeAddressInput; errors?: Record<string, string>; prefix?: string } = $props();

	const cityOptions = cities();
	const districtOptions = $derived(districts(address.city));

	function onCity() {
		address.district = '';
		address.postal_code = '';
	}
	function onDistrict() {
		address.postal_code = postalCode(address.city, address.district);
	}
	function err(key: string): string | undefined {
		return errors[prefix + key];
	}
</script>

<div class="grid grid-cols-3 gap-3">
	<label class="block text-sm text-gray-700">
		縣市
		<select bind:value={address.city} onchange={onCity} class="mt-1 w-full rounded border border-gray-300 px-2 py-2">
			<option value="">請選擇</option>
			{#each cityOptions as c (c)}<option value={c}>{c}</option>{/each}
		</select>
		{#if err('city')}<span class="text-red-600">{err('city')}</span>{/if}
	</label>
	<label class="block text-sm text-gray-700">
		鄉鎮市區
		<select bind:value={address.district} onchange={onDistrict} class="mt-1 w-full rounded border border-gray-300 px-2 py-2">
			<option value="">請選擇</option>
			{#each districtOptions as d (d)}<option value={d}>{d}</option>{/each}
		</select>
		{#if err('district')}<span class="text-red-600">{err('district')}</span>{/if}
	</label>
	<label class="block text-sm text-gray-700">
		郵遞區號
		<input type="text" bind:value={address.postal_code} inputmode="numeric" class="mt-1 w-full rounded border border-gray-300 px-3 py-2" />
		{#if err('postal_code')}<span class="text-red-600">{err('postal_code')}</span>{/if}
	</label>
</div>
<label class="mt-3 block text-sm text-gray-700">
	地址
	<input type="text" bind:value={address.street} autocomplete="street-address" class="mt-1 w-full rounded border border-gray-300 px-3 py-2" />
	{#if err('street')}<span class="text-red-600">{err('street')}</span>{/if}
</label>
```

- [ ] **Step 2: 寫 `web/src/routes/account/+layout.server.ts` 與 `+layout.svelte`**

```ts
import { redirect } from '@sveltejs/kit';
import type { LayoutServerLoad } from './$types';

/** /account 底下要登入；沒登入就去登入頁（規格 §6.2） */
export const load: LayoutServerLoad = ({ locals, url }) => {
	if (!locals.user) redirect(303, `/login?redirect=${encodeURIComponent(url.pathname)}`);
	return {};
};
```

```svelte
<script lang="ts">
	import { page } from '$app/state';
	import type { LayoutProps } from './$types';

	let { children }: LayoutProps = $props();

	const links = [
		{ href: '/account', label: '個人資料' },
		{ href: '/account/orders', label: '我的訂單' },
		{ href: '/account/addresses', label: '常用地址' }
	];

	function isActive(href: string): boolean {
		return href === '/account' ? page.url.pathname === '/account' : page.url.pathname.startsWith(href);
	}
</script>

<div class="flex flex-col gap-6 md:flex-row">
	<aside class="md:w-44 md:shrink-0">
		<nav class="flex gap-2 md:flex-col">
			{#each links as link (link.href)}
				<a href={link.href} class="rounded px-3 py-2 text-sm {isActive(link.href) ? 'bg-gray-900 text-white' : 'hover:bg-gray-200'}">{link.label}</a>
			{/each}
		</nav>
	</aside>
	<section class="min-w-0 flex-1">
		{@render children()}
	</section>
</div>
```

- [ ] **Step 3: 寫 `web/src/routes/account/+page.svelte`（個人資料、改密碼）**

```svelte
<script lang="ts">
	import { untrack } from 'svelte';
	import { invalidateAll } from '$app/navigation';
	import { api, ApiError } from '$lib/api';
	import { toast } from '$lib/toast.svelte';
	import type { User } from '$lib/types';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	// 初始值刻意只取一次；存檔後 invalidateAll 會更新 data.user，但表單保留使用者剛存的值
	let name = $state(untrack(() => data.user?.name ?? ''));
	let phone = $state(untrack(() => data.user?.phone ?? ''));
	let currentPassword = $state('');
	let newPassword = $state('');
	let errors = $state<Record<string, string>>({});
	let saving = $state(false);

	async function save(e: SubmitEvent) {
		e.preventDefault();
		errors = {};
		saving = true;
		try {
			const body: Record<string, string | null> = { name, phone: phone.trim() || null };
			if (newPassword) {
				body.current_password = currentPassword;
				body.new_password = newPassword;
			}
			await api<{ user: User }>('/api/me/profile', { method: 'PUT', body: JSON.stringify(body) });
			currentPassword = '';
			newPassword = '';
			await invalidateAll();
			toast.show('已儲存');
		} catch (err) {
			if (err instanceof ApiError) {
				errors = err.fields();
				if (Object.keys(errors).length === 0) toast.show(err.message);
			} else toast.show('儲存失敗');
		} finally {
			saving = false;
		}
	}
</script>

<svelte:head><title>個人資料</title></svelte:head>

<h1 class="text-2xl font-bold">個人資料</h1>
<p class="mt-1 text-sm text-gray-500">{data.user?.email}</p>

<form onsubmit={save} class="mt-6 max-w-md space-y-4" novalidate>
	<label class="block text-sm text-gray-700">
		姓名
		<input type="text" bind:value={name} class="mt-1 w-full rounded border border-gray-300 px-3 py-2" />
		{#if errors.name}<span class="text-red-600">{errors.name}</span>{/if}
	</label>
	<label class="block text-sm text-gray-700">
		手機
		<input type="tel" bind:value={phone} placeholder="09xxxxxxxx" class="mt-1 w-full rounded border border-gray-300 px-3 py-2" />
		{#if errors.phone}<span class="text-red-600">{errors.phone}</span>{/if}
	</label>

	<fieldset class="rounded border border-gray-200 p-4">
		<legend class="px-1 text-sm text-gray-600">更改密碼（不改就留空）</legend>
		<label class="block text-sm text-gray-700">
			目前密碼
			<input type="password" bind:value={currentPassword} autocomplete="current-password" class="mt-1 w-full rounded border border-gray-300 px-3 py-2" />
			{#if errors.current_password}<span class="text-red-600">{errors.current_password}</span>{/if}
		</label>
		<label class="mt-3 block text-sm text-gray-700">
			新密碼（至少 8 碼）
			<input type="password" bind:value={newPassword} autocomplete="new-password" class="mt-1 w-full rounded border border-gray-300 px-3 py-2" />
			{#if errors.new_password}<span class="text-red-600">{errors.new_password}</span>{/if}
		</label>
	</fieldset>

	<button type="submit" disabled={saving} class="rounded bg-gray-900 px-4 py-2 text-white disabled:opacity-50">
		{saving ? '儲存中…' : '儲存'}
	</button>
</form>
```

- [ ] **Step 4: 寫 `web/src/routes/account/orders/+page.server.ts` 與 `+page.svelte`**

```ts
import type { PageServerLoad } from './$types';
import { serverApi } from '$lib/server/api';
import type { OrderSummary, Page } from '$lib/types';

export const load: PageServerLoad = async (event) => {
	const page = Number(event.url.searchParams.get('page') ?? '1') || 1;
	const result = await serverApi<Page<OrderSummary>>(event, `/api/me/orders?page=${page}`);
	return { result };
};
```

```svelte
<script lang="ts">
	import Pagination from '$lib/components/Pagination.svelte';
	import { formatDate, twd } from '$lib/format';
	import { ORDER_STATUS_LABELS } from '$lib/labels';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
</script>

<svelte:head><title>我的訂單</title></svelte:head>

<h1 class="text-2xl font-bold">我的訂單</h1>

{#if data.result.items.length === 0}
	<p class="mt-4 text-gray-600">還沒有訂單。<a href="/products" class="underline">去逛逛</a></p>
{:else}
	<div class="mt-4 overflow-x-auto rounded border border-gray-200 bg-white">
		<table class="w-full text-sm">
			<thead class="bg-gray-50 text-left text-gray-600">
				<tr><th class="p-3">訂單編號</th><th class="p-3">日期</th><th class="p-3">狀態</th><th class="p-3">件數</th><th class="p-3 text-right">金額</th></tr>
			</thead>
			<tbody>
				{#each data.result.items as o (o.id)}
					<tr class="border-t border-gray-100">
						<td class="p-3"><a href={`/orders/${o.id}`} class="font-medium underline">{o.order_no}</a></td>
						<td class="p-3 text-gray-600">{formatDate(o.created_at)}</td>
						<td class="p-3">{ORDER_STATUS_LABELS[o.status]}</td>
						<td class="p-3">{o.item_count}</td>
						<td class="p-3 text-right">{twd(o.total)}</td>
					</tr>
				{/each}
			</tbody>
		</table>
	</div>
	<Pagination page={data.result.page} perPage={data.result.per_page} total={data.result.total} />
{/if}
```

- [ ] **Step 5: 寫 `web/src/routes/account/addresses/+page.server.ts` 與 `+page.svelte`**

```ts
import type { PageServerLoad } from './$types';
import { serverApi } from '$lib/server/api';
import type { Address } from '$lib/types';

export const load: PageServerLoad = async (event) => {
	const addresses = await serverApi<Address[]>(event, '/api/me/addresses');
	return { addresses };
};
```

```svelte
<script lang="ts">
	import { invalidateAll } from '$app/navigation';
	import { api, ApiError } from '$lib/api';
	import AddressFields from '$lib/components/AddressFields.svelte';
	import { toast } from '$lib/toast.svelte';
	import type { Address, AddressInput, HomeAddressInput } from '$lib/types';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	function blank(): AddressInput {
		return { recipient_name: '', phone: '', postal_code: '', city: '', district: '', street: '', is_default: false };
	}

	let editing = $state<string | 'new' | null>(null);
	let form = $state<AddressInput>(blank());
	let errors = $state<Record<string, string>>({});
	let saving = $state(false);
	let confirmDeleteId = $state<string | null>(null);

	// AddressFields 只管地址四欄；用一個 derived-like 物件把它綁回 form
	let addressPart = $state<HomeAddressInput>({ postal_code: '', city: '', district: '', street: '' });
	$effect(() => {
		form.postal_code = addressPart.postal_code;
		form.city = addressPart.city;
		form.district = addressPart.district;
		form.street = addressPart.street;
	});

	function startNew() {
		editing = 'new';
		form = blank();
		addressPart = { postal_code: '', city: '', district: '', street: '' };
		errors = {};
	}
	function startEdit(a: Address) {
		editing = a.id;
		form = { ...a };
		addressPart = { postal_code: a.postal_code, city: a.city, district: a.district, street: a.street };
		errors = {};
	}
	function cancel() {
		editing = null;
	}

	async function save(e: SubmitEvent) {
		e.preventDefault();
		errors = {};
		saving = true;
		try {
			if (editing === 'new') {
				await api<Address>('/api/me/addresses', { method: 'POST', body: JSON.stringify(form) });
			} else {
				await api<Address>(`/api/me/addresses/${editing}`, { method: 'PUT', body: JSON.stringify(form) });
			}
			editing = null;
			await invalidateAll();
			toast.show('已儲存');
		} catch (err) {
			if (err instanceof ApiError) {
				errors = err.fields();
				if (Object.keys(errors).length === 0) toast.show(err.message);
			} else toast.show('儲存失敗');
		} finally {
			saving = false;
		}
	}

	async function remove(id: string) {
		try {
			await api(`/api/me/addresses/${id}`, { method: 'DELETE' });
			confirmDeleteId = null;
			await invalidateAll();
			toast.show('已刪除');
		} catch {
			toast.show('刪除失敗');
		}
	}
</script>

<svelte:head><title>常用地址</title></svelte:head>

<div class="flex items-center justify-between">
	<h1 class="text-2xl font-bold">常用地址</h1>
	{#if editing === null && data.addresses.length < 10}
		<button type="button" onclick={startNew} class="rounded bg-gray-900 px-4 py-2 text-sm text-white">新增地址</button>
	{/if}
</div>

{#if editing !== null}
	<form onsubmit={save} class="mt-4 max-w-lg space-y-3 rounded border border-gray-200 bg-white p-4" novalidate>
		<div class="grid grid-cols-2 gap-3">
			<label class="block text-sm text-gray-700">
				收件人
				<input type="text" bind:value={form.recipient_name} class="mt-1 w-full rounded border border-gray-300 px-3 py-2" />
				{#if errors.recipient_name}<span class="text-red-600">{errors.recipient_name}</span>{/if}
			</label>
			<label class="block text-sm text-gray-700">
				手機
				<input type="tel" bind:value={form.phone} placeholder="09xxxxxxxx" class="mt-1 w-full rounded border border-gray-300 px-3 py-2" />
				{#if errors.phone}<span class="text-red-600">{errors.phone}</span>{/if}
			</label>
		</div>
		<AddressFields bind:address={addressPart} {errors} />
		<label class="flex items-center gap-2 text-sm text-gray-700">
			<input type="checkbox" bind:checked={form.is_default} /> 設為預設地址
		</label>
		<div class="flex gap-2">
			<button type="submit" disabled={saving} class="rounded bg-gray-900 px-4 py-2 text-white disabled:opacity-50">{saving ? '儲存中…' : '儲存'}</button>
			<button type="button" onclick={cancel} class="rounded border border-gray-300 px-4 py-2">取消</button>
		</div>
	</form>
{/if}

{#if data.addresses.length === 0 && editing === null}
	<p class="mt-4 text-gray-600">還沒有常用地址。</p>
{:else}
	<ul class="mt-4 divide-y divide-gray-200 rounded border border-gray-200 bg-white">
		{#each data.addresses as a (a.id)}
			<li class="flex flex-wrap items-center gap-3 p-4">
				<div class="min-w-0 flex-1 text-sm">
					<div class="font-medium">
						{a.recipient_name}
						{#if a.is_default}<span class="ml-2 rounded bg-gray-900 px-2 py-0.5 text-xs text-white">預設</span>{/if}
					</div>
					<div class="text-gray-600">{a.phone}</div>
					<div class="text-gray-600">{a.postal_code} {a.city}{a.district}{a.street}</div>
				</div>
				<button type="button" onclick={() => startEdit(a)} class="text-sm underline">編輯</button>
				{#if confirmDeleteId === a.id}
					<button type="button" onclick={() => remove(a.id)} class="text-sm text-red-600 underline">確定刪除？</button>
				{:else}
					<button type="button" onclick={() => (confirmDeleteId = a.id)} class="text-sm text-gray-500 underline">刪除</button>
				{/if}
			</li>
		{/each}
	</ul>
{/if}
```

- [ ] **Step 6: 頁首加會員中心連結**

`web/src/routes/+layout.svelte` 的 `{#if data.user.role === 'admin'} ... {/if}` 之後（`登出` 按鈕之前）加：

```svelte
					{#if data.user.role !== 'admin'}
						<a href="/account" class="hover:underline">會員中心</a>
					{/if}
```

- [ ] **Step 7: 檢查、建置、curl**

Run: `pnpm -C web check && pnpm -C web build`
Expected: 0 errors 0 warnings。若 `addresses/+page.svelte` 的 `$effect` 觸發 `state_referenced_locally` 或 `ownership_invalid_binding` 類警告，改成在 `save()` 裡送出前把 `addressPart` 的四欄併進 `form`（`{ ...form, ...addressPart }`）並拿掉 `$effect`，在回報說明。
Curl（api 與 web dev 背景跑）：未登入 `curl -s -o /dev/null -w '%{http_code}' http://localhost:5173/account` → 303；登入 cookie 打 `/account`、`/account/orders`、`/account/addresses` → 200 且含「個人資料」「我的訂單」「常用地址」。跑完關掉 dev server。

- [ ] **Step 8: Commit**

```bash
git add web/src/lib/components/AddressFields.svelte web/src/routes/account web/src/routes/+layout.svelte
git commit -m "feat(web): 會員中心（個人資料、我的訂單、常用地址）、地址欄位元件、頁首連結"
```

### Task 12: 購物車頁伺服器驗證與前往結帳

**Files:**
- Modify: `web/src/lib/cart.svelte.ts`（加 `update()`）、`web/src/lib/cart.test.ts`
- Modify: `web/src/routes/cart/+page.svelte`

**Interfaces:**
- Consumes: Task 8 的 `POST /api/cart/validate`；Task 9 的 `CartValidateResponse`、`CVS_SUBTOTAL_LIMIT`。
- Produces: `cart.update(variant_id, patch: Partial<Omit<CartLine, 'variant_id' | 'qty'>>)`；`/cart` 頁顯示伺服器核對結果、「前往結帳」連到 `/checkout`。

- [ ] **Step 1: `web/src/lib/cart.svelte.ts` 的 `Cart` 加方法（放在 `setQty` 之前）**

```ts
	/** 用伺服器核對後的資料更新顯示用欄位（價格、名稱、圖）；沒這列就忽略 */
	update(variant_id: string, patch: Partial<Omit<CartLine, 'variant_id' | 'qty'>>) {
		const line = this.lines.find((l) => l.variant_id === variant_id);
		if (!line) return;
		Object.assign(line, patch);
		this.persist();
	}
```

`web/src/lib/cart.test.ts` 加一個測試（放在同一個 `describe` 裡）：

```ts
	it('update patches display fields only', () => {
		const cart = new Cart();
		cart.add(line, 2);
		cart.update('v1', { price: 999, product_name: '新名字' });
		expect(cart.lines[0].price).toBe(999);
		expect(cart.lines[0].product_name).toBe('新名字');
		expect(cart.lines[0].qty).toBe(2);
		cart.update('nope', { price: 1 });
		expect(cart.lines).toHaveLength(1);
	});
```

（`line` 是該測試檔既有的範例 `CartLine` 常數，`variant_id` 為 `'v1'`；若名字不同就用該檔的。）

- [ ] **Step 2: 改寫 `web/src/routes/cart/+page.svelte`**

```svelte
<script lang="ts">
	import { api } from '$lib/api';
	import { cart } from '$lib/cart.svelte';
	import { twd } from '$lib/format';
	import { toast } from '$lib/toast.svelte';
	import type { CartValidateResponse } from '$lib/types';
	import { CVS_SUBTOTAL_LIMIT } from '$lib/validation';

	let checked = $state<CartValidateResponse | null>(null);
	let checking = $state(false);
	let checkedOnce = false;

	/** 向伺服器核對價格與庫存（規格 §6.1）；把結果同步回 localStorage 的購物車 */
	async function validate() {
		if (cart.lines.length === 0) {
			checked = null;
			return;
		}
		checking = true;
		try {
			const res = await api<CartValidateResponse>('/api/cart/validate', {
				method: 'POST',
				body: JSON.stringify({ items: cart.lines.map((l) => ({ variant_id: l.variant_id, qty: l.qty })) })
			});
			for (const item of res.items) {
				if (!item.available) continue;
				cart.update(item.variant_id, {
					price: item.price,
					product_name: item.product_name,
					variant_label: item.variant_label,
					image_thumb: item.image_thumb,
					product_slug: item.product_slug
				});
				if (item.qty !== cart.lines.find((l) => l.variant_id === item.variant_id)?.qty) {
					cart.setQty(item.variant_id, item.qty);
				}
			}
			checked = res;
		} catch {
			toast.show('無法確認庫存，請稍後再試');
		} finally {
			checking = false;
		}
	}

	// 購物車從 localStorage 載入後核對一次
	$effect(() => {
		if (cart.loaded && !checkedOnce) {
			checkedOnce = true;
			void validate();
		}
	});

	const problems = $derived(checked?.items.filter((i) => !i.available) ?? []);
	function problemOf(variant_id: string) {
		return checked?.items.find((i) => i.variant_id === variant_id);
	}
	function removeUnavailable() {
		for (const p of problems) cart.remove(p.variant_id);
		void validate();
	}
	const canCheckout = $derived(cart.lines.length > 0 && !checking && problems.length === 0);
</script>

<svelte:head><title>購物車</title></svelte:head>

<h1 class="text-2xl font-bold">購物車</h1>

{#if !cart.loaded}
	<p class="mt-4 text-gray-500">載入中…</p>
{:else if cart.lines.length === 0}
	<p class="mt-4 text-gray-600">購物車是空的。<a href="/products" class="underline">去逛逛</a></p>
{:else}
	{#if problems.length > 0}
		<div class="mt-4 flex flex-wrap items-center gap-3 rounded border border-yellow-300 bg-yellow-50 p-3 text-sm">
			<span>有 {problems.length} 項商品目前無法購買，請移除後再結帳。</span>
			<button type="button" onclick={removeUnavailable} class="rounded bg-gray-900 px-3 py-1 text-white">移除無法購買的商品</button>
		</div>
	{/if}
	<ul class="mt-4 divide-y divide-gray-200 rounded border border-gray-200 bg-white">
		{#each cart.lines as line (line.variant_id)}
			{@const p = problemOf(line.variant_id)}
			<li class="flex flex-wrap items-center gap-4 p-4">
				<a href={`/products/${line.product_slug}`} class="h-16 w-16 shrink-0 overflow-hidden rounded bg-gray-100">
					{#if line.image_thumb}<img src={line.image_thumb} alt="" class="h-full w-full object-cover" />{/if}
				</a>
				<div class="min-w-0 flex-1">
					<a href={`/products/${line.product_slug}`} class="font-medium hover:underline">{line.product_name}</a>
					<div class="text-sm text-gray-500">{line.variant_label}</div>
					<div class="text-sm">{twd(line.price)}</div>
					{#if p?.reason === 'unavailable'}
						<div class="text-sm text-red-600">已下架</div>
					{:else if p?.reason === 'sold_out'}
						<div class="text-sm text-red-600">已售完</div>
					{:else if p?.reason === 'qty_reduced'}
						<div class="text-sm text-yellow-700">庫存只剩 {p.stock} 件，數量已調整</div>
					{/if}
				</div>
				<div class="flex items-center gap-1">
					<button type="button" class="h-8 w-8 rounded border border-gray-300" onclick={() => cart.setQty(line.variant_id, line.qty - 1)} aria-label="減少">−</button>
					<input
						type="number"
						min="1"
						max={p?.available ? p.stock : 99}
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
	<div class="mt-4 flex flex-wrap items-center justify-end gap-4">
		<button type="button" onclick={validate} disabled={checking} class="text-sm underline disabled:opacity-50">
			{checking ? '確認中…' : '重新確認庫存'}
		</button>
		<div class="text-lg">小計 <span class="font-bold">{twd(cart.subtotal)}</span></div>
		{#if canCheckout}
			<a href="/checkout" class="rounded bg-gray-900 px-6 py-2 text-white">前往結帳</a>
		{:else}
			<span class="rounded bg-gray-400 px-6 py-2 text-white">前往結帳</span>
		{/if}
	</div>
	{#if cart.subtotal > CVS_SUBTOTAL_LIMIT}
		<p class="mt-2 text-right text-sm text-yellow-700">商品小計超過 {twd(CVS_SUBTOTAL_LIMIT)}，只能選擇宅配。</p>
	{/if}
	<p class="mt-2 text-right text-xs text-gray-500">價格與庫存會在結帳時再次確認。</p>
{/if}
```

- [ ] **Step 3: 檢查、測試、建置**

Run: `pnpm -C web check && pnpm -C web test && pnpm -C web build`
Expected: 0 errors 0 warnings；測試 18 個；build 成功。
Curl（api、web 背景跑）：`curl -s http://localhost:5173/cart | grep -c 購物車` ≥ 1（SSR 只會看到「載入中」，互動在瀏覽器）。

- [ ] **Step 4: Commit**

```bash
git add web/src/lib/cart.svelte.ts web/src/lib/cart.test.ts web/src/routes/cart/+page.svelte
git commit -m "feat(web): 購物車頁向伺服器核對價格庫存、前往結帳"
```

### Task 13: 結帳頁

**Files:**
- Create: `web/src/lib/checkout.ts`、`web/src/lib/checkout.test.ts`
- Create: `web/src/lib/components/checkout/InvoiceFields.svelte`
- Create: `web/src/routes/checkout/+page.server.ts`、`web/src/routes/checkout/+page.svelte`

**Interfaces:**
- Consumes: Task 8 的 `POST /api/cart/validate`、`POST /api/orders`、`GET /api/checkout/cvs-store/{token}`、Task 5 的 `/api/me/addresses`；Task 9 的型別與 `validation.ts`；Task 11 的 `AddressFields`；layout 的 `data.settings`、`data.user`；`cart`。
- Produces: `checkout.ts` 的 `CheckoutForm`、`defaultForm(user)`、`validateForm(form, ctx)`、`toOrderInput(form, items, storeToken)`、`CHECKOUT_STORAGE_KEY`；`InvoiceFields` 元件 props `{ invoice（bindable）, errors }`；`/checkout` 頁。

- [ ] **Step 1: 寫 `web/src/lib/checkout.ts`（純函式，方便測）**

```ts
import type {
	CvsSubType,
	HomeAddressInput,
	InvoiceInput,
	InvoiceType,
	OrderInput,
	PaymentMethod,
	ShippingMethod,
	User
} from '$lib/types';
import {
	CVS_SUBTOTAL_LIMIT,
	isCitizenCert,
	isCvsRecipientName,
	isLoveCode,
	isMobileBarcode,
	isPostalCode,
	isTaxId,
	isTwMobile
} from '$lib/validation';

export const CHECKOUT_STORAGE_KEY = 'dog_shop_checkout_v1';

export type InvoiceForm = {
	type: InvoiceType;
	carrier_type: '1' | '2' | '3';
	carrier_num: string;
	tax_id: string;
	title: string;
	address: string;
	love_code: string;
};

export type CheckoutForm = {
	email: string;
	recipient_name: string;
	recipient_phone: string;
	shipping_method: ShippingMethod;
	cvs_sub_type: CvsSubType;
	address: HomeAddressInput;
	invoice: InvoiceForm;
	payment_method: PaymentMethod;
	note: string;
};

export function defaultForm(user: User | null, firstPayment: PaymentMethod): CheckoutForm {
	return {
		email: user?.email ?? '',
		recipient_name: user?.name ?? '',
		recipient_phone: user?.phone ?? '',
		shipping_method: 'home',
		cvs_sub_type: 'UNIMARTC2C',
		address: { postal_code: '', city: '', district: '', street: '' },
		invoice: { type: 'personal', carrier_type: '1', carrier_num: '', tax_id: '', title: '', address: '', love_code: '' },
		payment_method: firstPayment,
		note: ''
	};
}

export type ValidateContext = {
	subtotal: number;
	hasStore: boolean;
	enabledPayments: PaymentMethod[];
};

/** 與後端 `validate_input` 相同的規則，錯誤 key 也相同（`address.street`、`invoice.tax_id`…） */
export function validateForm(form: CheckoutForm, ctx: ValidateContext): Record<string, string> {
	const errors: Record<string, string> = {};
	const email = form.email.trim();
	if (!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email)) errors.email = 'Email 格式不正確';
	if (!isTwMobile(form.recipient_phone.trim())) errors.recipient_phone = '手機格式：09 開頭共 10 碼';
	const name = form.recipient_name.trim();
	if (form.shipping_method === 'cvs') {
		if (!isCvsRecipientName(name)) errors.recipient_name = '超商取貨收件人請填 2～5 個中文字的本名';
		if (ctx.subtotal > CVS_SUBTOTAL_LIMIT) errors.shipping_method = '商品小計超過 20,000 元，請改用宅配';
		if (!ctx.hasStore) errors.cvs_store = '請先選擇取貨門市';
	} else {
		if (name.length === 0 || name.length > 20) errors.recipient_name = '必填，最多 20 字';
		const a = form.address;
		if (!isPostalCode(a.postal_code.trim())) errors['address.postal_code'] = '郵遞區號 3～5 碼數字';
		if (!a.city) errors['address.city'] = '請選縣市';
		if (!a.district) errors['address.district'] = '請選鄉鎮市區';
		const street = a.street.trim();
		if (street.length === 0 || street.length > 100) errors['address.street'] = '必填，最多 100 字';
	}
	const inv = form.invoice;
	if (inv.type === 'personal') {
		const num = inv.carrier_num.trim().toUpperCase();
		if (inv.carrier_type === '2' && !isCitizenCert(num)) errors['invoice.carrier_num'] = '自然人憑證條碼：2 個英文字母 + 14 碼數字';
		if (inv.carrier_type === '3' && !isMobileBarcode(num)) errors['invoice.carrier_num'] = '手機條碼：/ 開頭共 8 碼';
	} else if (inv.type === 'company') {
		if (!isTaxId(inv.tax_id.trim())) errors['invoice.tax_id'] = '統一編號格式不正確';
		const title = inv.title.trim();
		if (title.length === 0 || title.length > 60) errors['invoice.title'] = '必填，最多 60 字';
		const addr = inv.address.trim();
		if (addr.length === 0 || addr.length > 100) errors['invoice.address'] = '必填，最多 100 字';
	} else if (!isLoveCode(inv.love_code.trim())) {
		errors['invoice.love_code'] = '愛心碼 3～7 碼數字';
	}
	if (!ctx.enabledPayments.includes(form.payment_method)) errors.payment_method = '請選擇付款方式';
	if (form.note.length > 200) errors.note = '最多 200 字';
	return errors;
}

/** 組成 POST /api/orders 的 body（只送需要的發票欄位） */
export function toOrderInput(
	form: CheckoutForm,
	items: { variant_id: string; qty: number }[],
	storeToken: string | null
): OrderInput {
	const inv = form.invoice;
	const invoice: InvoiceInput =
		inv.type === 'personal'
			? {
					type: 'personal',
					carrier_type: inv.carrier_type,
					carrier_num: inv.carrier_type === '1' ? undefined : inv.carrier_num.trim().toUpperCase()
				}
			: inv.type === 'company'
				? { type: 'company', tax_id: inv.tax_id.trim(), title: inv.title.trim(), address: inv.address.trim() }
				: { type: 'donation', love_code: inv.love_code.trim() };
	return {
		items,
		email: form.email.trim(),
		recipient_name: form.recipient_name.trim(),
		recipient_phone: form.recipient_phone.trim(),
		shipping_method: form.shipping_method,
		cvs_store_token: form.shipping_method === 'cvs' ? storeToken : null,
		address: form.shipping_method === 'home' ? form.address : null,
		invoice,
		payment_method: form.payment_method,
		note: form.note.trim()
	};
}
```

- [ ] **Step 2: 寫 `web/src/lib/checkout.test.ts`**

```ts
import { describe, expect, it } from 'vitest';
import { defaultForm, toOrderInput, validateForm, type CheckoutForm } from './checkout';

function homeForm(): CheckoutForm {
	const f = defaultForm(null, 'credit');
	f.email = 'a@b.co';
	f.recipient_name = '王小明';
	f.recipient_phone = '0912345678';
	f.address = { postal_code: '100', city: '臺北市', district: '中正區', street: '重慶南路一段 122 號' };
	return f;
}
const ctx = { subtotal: 500, hasStore: false, enabledPayments: ['credit', 'atm'] as const };

describe('checkout form', () => {
	it('valid home form has no errors', () => {
		expect(validateForm(homeForm(), { ...ctx, enabledPayments: [...ctx.enabledPayments] })).toEqual({});
	});

	it('cvs needs a chinese name, a store, and subtotal under the limit', () => {
		const f = homeForm();
		f.shipping_method = 'cvs';
		f.recipient_name = 'Amy';
		const errors = validateForm(f, { subtotal: 25000, hasStore: false, enabledPayments: ['credit'] });
		expect(errors.recipient_name).toContain('中文');
		expect(errors.cvs_store).toBe('請先選擇取貨門市');
		expect(errors.shipping_method).toContain('宅配');
		expect(errors['address.street']).toBeUndefined();
	});

	it('invoice rules and payment method', () => {
		const f = homeForm();
		f.invoice.type = 'company';
		f.invoice.tax_id = '12345678';
		f.payment_method = 'cvs_code';
		const errors = validateForm(f, { subtotal: 1, hasStore: false, enabledPayments: ['credit'] });
		expect(errors['invoice.tax_id']).toBe('統一編號格式不正確');
		expect(errors['invoice.title']).toBeDefined();
		expect(errors.payment_method).toBe('請選擇付款方式');
	});

	it('toOrderInput sends only the relevant invoice fields', () => {
		const f = homeForm();
		f.invoice = { type: 'personal', carrier_type: '3', carrier_num: '/abc+123', tax_id: '99', title: 'x', address: 'y', love_code: '1' };
		const body = toOrderInput(f, [{ variant_id: 'v1', qty: 2 }], 'tok');
		expect(body.invoice).toEqual({ type: 'personal', carrier_type: '3', carrier_num: '/ABC+123' });
		expect(body.cvs_store_token).toBeNull();
		expect(body.address?.city).toBe('臺北市');
		f.shipping_method = 'cvs';
		const cvs = toOrderInput(f, [], 'tok');
		expect(cvs.cvs_store_token).toBe('tok');
		expect(cvs.address).toBeNull();
	});
});
```

- [ ] **Step 3: 寫 `web/src/lib/components/checkout/InvoiceFields.svelte`**

```svelte
<script lang="ts">
	import type { InvoiceForm } from '$lib/checkout';
	import { INVOICE_LABELS } from '$lib/labels';
	import type { InvoiceType } from '$lib/types';

	let { invoice = $bindable(), errors = {} }: { invoice: InvoiceForm; errors?: Record<string, string> } = $props();

	const types: InvoiceType[] = ['personal', 'company', 'donation'];
	const carriers: { value: '1' | '2' | '3'; label: string }[] = [
		{ value: '1', label: '綠界會員載具（發票寄到 Email）' },
		{ value: '2', label: '自然人憑證' },
		{ value: '3', label: '手機條碼' }
	];
	const input = 'mt-1 w-full rounded border border-gray-300 px-3 py-2';
</script>

<fieldset class="space-y-3">
	<legend class="text-sm font-medium">發票</legend>
	<div class="flex flex-wrap gap-4 text-sm">
		{#each types as t (t)}
			<label class="flex items-center gap-2"><input type="radio" bind:group={invoice.type} value={t} /> {INVOICE_LABELS[t]}</label>
		{/each}
	</div>
	{#if invoice.type === 'personal'}
		<label class="block text-sm text-gray-700">
			載具
			<select bind:value={invoice.carrier_type} class={input}>
				{#each carriers as c (c.value)}<option value={c.value}>{c.label}</option>{/each}
			</select>
		</label>
		{#if invoice.carrier_type !== '1'}
			<label class="block text-sm text-gray-700">
				{invoice.carrier_type === '2' ? '自然人憑證條碼' : '手機條碼'}
				<input type="text" bind:value={invoice.carrier_num} placeholder={invoice.carrier_type === '2' ? 'AB12345678901234' : '/ABC+123'} class={input} />
				{#if errors['invoice.carrier_num']}<span class="text-red-600">{errors['invoice.carrier_num']}</span>{/if}
			</label>
		{/if}
	{:else if invoice.type === 'company'}
		<label class="block text-sm text-gray-700">
			統一編號
			<input type="text" bind:value={invoice.tax_id} inputmode="numeric" class={input} />
			{#if errors['invoice.tax_id']}<span class="text-red-600">{errors['invoice.tax_id']}</span>{/if}
		</label>
		<label class="block text-sm text-gray-700">
			發票抬頭
			<input type="text" bind:value={invoice.title} class={input} />
			{#if errors['invoice.title']}<span class="text-red-600">{errors['invoice.title']}</span>{/if}
		</label>
		<label class="block text-sm text-gray-700">
			發票地址
			<input type="text" bind:value={invoice.address} class={input} />
			{#if errors['invoice.address']}<span class="text-red-600">{errors['invoice.address']}</span>{/if}
		</label>
	{:else}
		<label class="block text-sm text-gray-700">
			愛心碼
			<input type="text" bind:value={invoice.love_code} inputmode="numeric" placeholder="例如 168" class={input} />
			{#if errors['invoice.love_code']}<span class="text-red-600">{errors['invoice.love_code']}</span>{/if}
		</label>
	{/if}
</fieldset>
```

- [ ] **Step 4: 寫 `web/src/routes/checkout/+page.server.ts`**

```ts
import type { PageServerLoad } from './$types';
import { ApiError, serverApi } from '$lib/server/api';
import type { Address, CvsStore } from '$lib/types';

/** 會員帶常用地址；?store=<token> 是計畫 4 地圖選完門市回來的入口，本計畫先能還原顯示 */
export const load: PageServerLoad = async (event) => {
	const addresses = event.locals.user ? await serverApi<Address[]>(event, '/api/me/addresses') : [];
	const token = event.url.searchParams.get('store');
	let store: CvsStore | null = null;
	if (token) {
		try {
			store = await serverApi<CvsStore>(event, `/api/checkout/cvs-store/${encodeURIComponent(token)}`);
		} catch (e) {
			if (!(e instanceof ApiError && e.status === 404)) throw e;
		}
	}
	return { addresses, store };
};
```

- [ ] **Step 5: 寫 `web/src/routes/checkout/+page.svelte`**

```svelte
<script lang="ts">
	import { onMount, untrack } from 'svelte';
	import { goto } from '$app/navigation';
	import { api, ApiError } from '$lib/api';
	import { cart } from '$lib/cart.svelte';
	import { CHECKOUT_STORAGE_KEY, defaultForm, toOrderInput, validateForm, type CheckoutForm } from '$lib/checkout';
	import AddressFields from '$lib/components/AddressFields.svelte';
	import InvoiceFields from '$lib/components/checkout/InvoiceFields.svelte';
	import { twd } from '$lib/format';
	import { CVS_LABELS, PAYMENT_LABELS } from '$lib/labels';
	import { toast } from '$lib/toast.svelte';
	import type { CartValidateResponse, CvsStore, CvsSubType, OrderCreated, PaymentMethod } from '$lib/types';
	import { CVS_SUBTOTAL_LIMIT, shippingFee } from '$lib/validation';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	const shipping = data.settings.shipping;
	const enabledPayments = (['credit', 'atm', 'cvs_code'] as PaymentMethod[]).filter((m) => data.settings.payment_methods[m]);
	const cvsTypes: CvsSubType[] = ['UNIMARTC2C', 'FAMIC2C', 'HILIFEC2C'];

	// 初始值刻意只取一次（SSR 與 hydration 一致）；sessionStorage 的草稿在 onMount 才還原
	let form = $state<CheckoutForm>(untrack(() => defaultForm(data.user, enabledPayments[0] ?? 'credit')));
	let store = $state<CvsStore | null>(untrack(() => data.store));
	let checked = $state<CartValidateResponse | null>(null);
	let checking = $state(false);
	let errors = $state<Record<string, string>>({});
	let submitting = $state(false);
	let selectedAddressId = $state('');

	const lines = $derived(checked?.items.filter((i) => i.available) ?? []);
	const subtotal = $derived(checked?.subtotal ?? cart.subtotal);
	const cvsBlocked = $derived(subtotal > CVS_SUBTOTAL_LIMIT);
	const fee = $derived(shippingFee(shipping, form.shipping_method, subtotal));
	const total = $derived(subtotal + fee);

	async function validateCart() {
		if (cart.lines.length === 0) {
			checked = null;
			return;
		}
		checking = true;
		try {
			const res = await api<CartValidateResponse>('/api/cart/validate', {
				method: 'POST',
				body: JSON.stringify({ items: cart.lines.map((l) => ({ variant_id: l.variant_id, qty: l.qty })) })
			});
			for (const item of res.items) {
				if (item.available && item.qty !== cart.lines.find((l) => l.variant_id === item.variant_id)?.qty) {
					cart.setQty(item.variant_id, item.qty);
				}
			}
			checked = res;
		} catch {
			toast.show('無法確認庫存，請稍後再試');
		} finally {
			checking = false;
		}
	}

	function restoreDraft() {
		try {
			const raw = sessionStorage.getItem(CHECKOUT_STORAGE_KEY);
			if (!raw) return;
			const saved = JSON.parse(raw) as Partial<CheckoutForm>;
			form = { ...form, ...saved, address: { ...form.address, ...(saved.address ?? {}) }, invoice: { ...form.invoice, ...(saved.invoice ?? {}) } };
		} catch {
			// 壞掉的草稿就不管
		}
	}

	onMount(() => {
		restoreDraft();
		if (store) {
			form.shipping_method = 'cvs';
			form.cvs_sub_type = store.sub_type;
		}
		if (!enabledPayments.includes(form.payment_method)) form.payment_method = enabledPayments[0] ?? 'credit';
	});

	// 購物車載入後核對一次；表單每次變動就存草稿（計畫 4 去綠界地圖前後要用）
	let checkedOnce = false;
	$effect(() => {
		if (cart.loaded && !checkedOnce) {
			checkedOnce = true;
			void validateCart();
		}
	});
	$effect(() => {
		const snapshot = JSON.stringify(form);
		try {
			sessionStorage.setItem(CHECKOUT_STORAGE_KEY, snapshot);
		} catch {
			// 無痕模式：不存
		}
	});

	function useAddress() {
		const a = data.addresses.find((x) => x.id === selectedAddressId);
		if (!a) return;
		form.recipient_name = a.recipient_name;
		form.recipient_phone = a.phone;
		form.address = { postal_code: a.postal_code, city: a.city, district: a.district, street: a.street };
	}

	async function submit(e: SubmitEvent) {
		e.preventDefault();
		errors = validateForm(form, { subtotal, hasStore: store !== null, enabledPayments });
		if (Object.keys(errors).length > 0) {
			toast.show('請檢查紅字欄位');
			return;
		}
		if (lines.length === 0) {
			toast.show('購物車沒有可購買的商品');
			return;
		}
		submitting = true;
		try {
			const body = toOrderInput(
				form,
				lines.map((l) => ({ variant_id: l.variant_id, qty: l.qty })),
				store?.token ?? null
			);
			const created = await api<OrderCreated>('/api/orders', { method: 'POST', body: JSON.stringify(body) });
			cart.clear();
			try {
				sessionStorage.removeItem(CHECKOUT_STORAGE_KEY);
			} catch {
				// ignore
			}
			await goto(data.user ? `/orders/${created.order_id}` : `/orders/${created.order_id}?t=${created.guest_token}`);
		} catch (err) {
			if (err instanceof ApiError && err.code === 'VALIDATION') {
				errors = err.fields();
				toast.show(Object.keys(errors).length > 0 ? '請檢查紅字欄位' : err.message);
			} else if (err instanceof ApiError && err.code === 'OUT_OF_STOCK') {
				toast.show('部分商品庫存不足，已重新確認，請檢查數量');
				await validateCart();
			} else if (err instanceof ApiError) {
				toast.show(err.message);
			} else {
				toast.show('送出失敗，請再試一次');
			}
		} finally {
			submitting = false;
		}
	}

	const input = 'mt-1 w-full rounded border border-gray-300 px-3 py-2';
</script>

<svelte:head><title>結帳</title></svelte:head>

<h1 class="text-2xl font-bold">結帳</h1>

{#if !cart.loaded}
	<p class="mt-4 text-gray-500">載入中…</p>
{:else if cart.lines.length === 0}
	<p class="mt-4 text-gray-600">購物車是空的。<a href="/products" class="underline">去逛逛</a></p>
{:else}
	<form onsubmit={submit} class="mt-6 grid gap-8 lg:grid-cols-[1fr_20rem]" novalidate>
		<div class="space-y-8">
			<!-- 聯絡與收件人 -->
			<section class="space-y-3 rounded border border-gray-200 bg-white p-4">
				<h2 class="font-medium">聯絡資料</h2>
				{#if !data.user}
					<p class="text-sm text-gray-600">
						訪客結帳；<a href="/login?redirect=/checkout" class="underline">登入</a>可以用常用地址、在會員中心看訂單。
					</p>
				{/if}
				<label class="block text-sm text-gray-700">
					Email（訂單通知寄到這裡）
					<input type="email" bind:value={form.email} autocomplete="email" class={input} />
					{#if errors.email}<span class="text-red-600">{errors.email}</span>{/if}
				</label>
				{#if data.addresses.length > 0}
					<label class="block text-sm text-gray-700">
						常用地址
						<div class="mt-1 flex gap-2">
							<select bind:value={selectedAddressId} class="w-full rounded border border-gray-300 px-2 py-2">
								<option value="">選一個帶入</option>
								{#each data.addresses as a (a.id)}
									<option value={a.id}>{a.recipient_name}｜{a.city}{a.district}{a.street}</option>
								{/each}
							</select>
							<button type="button" onclick={useAddress} class="shrink-0 rounded border border-gray-300 px-3">帶入</button>
						</div>
					</label>
				{/if}
				<div class="grid grid-cols-2 gap-3">
					<label class="block text-sm text-gray-700">
						收件人
						<input type="text" bind:value={form.recipient_name} autocomplete="name" class={input} />
						{#if errors.recipient_name}<span class="text-red-600">{errors.recipient_name}</span>{/if}
					</label>
					<label class="block text-sm text-gray-700">
						手機
						<input type="tel" bind:value={form.recipient_phone} autocomplete="tel" placeholder="09xxxxxxxx" class={input} />
						{#if errors.recipient_phone}<span class="text-red-600">{errors.recipient_phone}</span>{/if}
					</label>
				</div>
			</section>

			<!-- 取貨方式 -->
			<section class="space-y-3 rounded border border-gray-200 bg-white p-4">
				<h2 class="font-medium">取貨方式</h2>
				<div class="flex flex-wrap gap-4 text-sm">
					<label class="flex items-center gap-2"><input type="radio" bind:group={form.shipping_method} value="home" /> 宅配（{twd(shipping.home_fee)}）</label>
					<label class="flex items-center gap-2">
						<input type="radio" bind:group={form.shipping_method} value="cvs" disabled={cvsBlocked} /> 超商取貨（{twd(shipping.cvs_fee)}）
					</label>
				</div>
				{#if shipping.free_threshold > 0}
					<p class="text-sm text-gray-500">商品小計滿 {twd(shipping.free_threshold)} 免運。</p>
				{/if}
				{#if cvsBlocked}<p class="text-sm text-yellow-700">商品小計超過 {twd(CVS_SUBTOTAL_LIMIT)}，只能宅配。</p>{/if}
				{#if errors.shipping_method}<p class="text-sm text-red-600">{errors.shipping_method}</p>{/if}

				{#if form.shipping_method === 'home'}
					<AddressFields bind:address={form.address} {errors} prefix="address." />
				{:else}
					<div class="flex flex-wrap gap-4 text-sm">
						{#each cvsTypes as t (t)}
							<label class="flex items-center gap-2"><input type="radio" bind:group={form.cvs_sub_type} value={t} /> {CVS_LABELS[t]}</label>
						{/each}
					</div>
					{#if store}
						<div class="rounded bg-gray-50 p-3 text-sm">
							<div class="font-medium">{CVS_LABELS[store.sub_type]} {store.store_name}（{store.store_id}）</div>
							<div class="text-gray-600">{store.store_address}</div>
						</div>
					{/if}
					<button type="button" disabled class="rounded border border-gray-300 px-3 py-2 text-sm disabled:opacity-50" title="綠界電子地圖在下一個階段接上">
						選擇門市（門市選擇功能準備中）
					</button>
					{#if errors.cvs_store}<p class="text-sm text-red-600">{errors.cvs_store}</p>{/if}
					<p class="text-xs text-gray-500">超商取貨收件人請填 2～5 個中文字的本名，取貨時要核對證件。</p>
				{/if}
			</section>

			<!-- 發票 -->
			<section class="rounded border border-gray-200 bg-white p-4">
				<InvoiceFields bind:invoice={form.invoice} {errors} />
			</section>

			<!-- 付款方式 -->
			<section class="space-y-3 rounded border border-gray-200 bg-white p-4">
				<h2 class="font-medium">付款方式</h2>
				<div class="flex flex-wrap gap-4 text-sm">
					{#each enabledPayments as m (m)}
						<label class="flex items-center gap-2"><input type="radio" bind:group={form.payment_method} value={m} /> {PAYMENT_LABELS[m]}</label>
					{/each}
				</div>
				{#if errors.payment_method}<p class="text-sm text-red-600">{errors.payment_method}</p>{/if}
				<label class="block text-sm text-gray-700">
					備註（選填，最多 200 字）
					<textarea bind:value={form.note} rows="2" class={input}></textarea>
					{#if errors.note}<span class="text-red-600">{errors.note}</span>{/if}
				</label>
			</section>
		</div>

		<!-- 摘要 -->
		<aside class="h-fit space-y-3 rounded border border-gray-200 bg-white p-4 text-sm lg:sticky lg:top-4">
			<h2 class="font-medium">訂單摘要</h2>
			{#if checking}<p class="text-gray-500">確認庫存中…</p>{/if}
			<ul class="divide-y divide-gray-100">
				{#each lines as l (l.variant_id)}
					<li class="flex justify-between gap-2 py-2">
						<span class="min-w-0 truncate">{l.product_name}<span class="text-gray-500">（{l.variant_label}）× {l.qty}</span></span>
						<span class="shrink-0">{twd(l.price * l.qty)}</span>
					</li>
				{/each}
			</ul>
			{#if checked && checked.items.some((i) => !i.available)}
				<p class="text-red-600">有商品無法購買，請回<a href="/cart" class="underline">購物車</a>處理。</p>
			{/if}
			<div class="flex justify-between"><span>商品小計</span><span>{twd(subtotal)}</span></div>
			<div class="flex justify-between"><span>運費</span><span>{fee === 0 ? '免運' : twd(fee)}</span></div>
			<div class="flex justify-between text-base font-bold"><span>總計</span><span>{twd(total)}</span></div>
			<button type="submit" disabled={submitting || checking || lines.length === 0} class="w-full rounded bg-gray-900 px-4 py-2 text-white disabled:opacity-50">
				{submitting ? '送出中…' : '送出訂單'}
			</button>
			<p class="text-xs text-gray-500">送出後會建立訂單並帶你到訂單頁；線上付款功能在下一個階段開放。</p>
		</aside>
	</form>
{/if}
```

- [ ] **Step 6: 檢查、測試、建置、curl**

Run: `pnpm -C web check && pnpm -C web test && pnpm -C web build`
Expected: 0 errors 0 warnings；測試 22 個；build 成功。若 `$effect` 存草稿那段觸發 `state_referenced_locally`，改成 `$effect(() => { const snapshot = $state.snapshot(form); ... })`（`$state.snapshot` 是 rune，可讀 proxy），在回報說明。
Curl（api、web 背景跑）：`curl -s http://localhost:5173/checkout | grep -c 結帳` ≥ 1；`/checkout?store=nope` 也是 200（404 的門市被吞掉）。

- [ ] **Step 7: Commit**

```bash
git add web/src/lib/checkout.ts web/src/lib/checkout.test.ts web/src/lib/components/checkout/InvoiceFields.svelte web/src/routes/checkout
git commit -m "feat(web): 結帳頁（收件、取貨方式、發票、付款方式、摘要、下單）"
```

### Task 14: 訂單頁（明細、狀態、買家取消）

**Files:**
- Create: `web/src/routes/orders/[id]/+page.server.ts`、`web/src/routes/orders/[id]/+page.svelte`

**Interfaces:**
- Consumes: Task 8 的 `GET /api/orders/{id}?t=`、`POST /api/orders/{id}/cancel?t=`；Task 9 的 `OrderDetail`、`labels.ts`；`formatDate`、`twd`。
- Produces: `/orders/[id]` 頁（訪客要帶 `?t=`；規格 §6.1）。計畫 3 會在這頁加付款輪詢、繳費資訊、重新付款。

- [ ] **Step 1: 寫 `web/src/routes/orders/[id]/+page.server.ts`**

```ts
import { error } from '@sveltejs/kit';
import type { PageServerLoad } from './$types';
import { ApiError, serverApi } from '$lib/server/api';
import type { OrderDetail } from '$lib/types';

/** 會員看自己的不用 token；訪客帶 ?t=。看不到（或 id 不是 uuid）一律 404 */
export const load: PageServerLoad = async (event) => {
	const token = event.url.searchParams.get('t');
	const qs = token ? `?t=${encodeURIComponent(token)}` : '';
	try {
		const order = await serverApi<OrderDetail>(event, `/api/orders/${encodeURIComponent(event.params.id)}${qs}`);
		return { order, token };
	} catch (e) {
		if (e instanceof ApiError && (e.status === 404 || e.status === 400)) error(404, '找不到這筆訂單');
		throw e;
	}
};
```

- [ ] **Step 2: 寫 `web/src/routes/orders/[id]/+page.svelte`**

```svelte
<script lang="ts">
	import { invalidateAll } from '$app/navigation';
	import { api, ApiError } from '$lib/api';
	import { formatDate, twd } from '$lib/format';
	import { CVS_LABELS, INVOICE_LABELS, ORDER_STATUS_LABELS, PAYMENT_LABELS } from '$lib/labels';
	import { toast } from '$lib/toast.svelte';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
	const o = $derived(data.order);
	let confirming = $state(false);
	let cancelling = $state(false);

	async function cancel() {
		cancelling = true;
		try {
			const qs = data.token ? `?t=${encodeURIComponent(data.token)}` : '';
			await api(`/api/orders/${o.id}/cancel${qs}`, { method: 'POST' });
			confirming = false;
			await invalidateAll();
			toast.show('訂單已取消');
		} catch (err) {
			toast.show(err instanceof ApiError ? err.message : '取消失敗');
		} finally {
			cancelling = false;
		}
	}
</script>

<svelte:head><title>訂單 {o.order_no}</title></svelte:head>

<div class="flex flex-wrap items-baseline justify-between gap-2">
	<h1 class="text-2xl font-bold">訂單 {o.order_no}</h1>
	<span class="rounded bg-gray-900 px-3 py-1 text-sm text-white">{ORDER_STATUS_LABELS[o.status]}</span>
</div>
<p class="mt-1 text-sm text-gray-500">成立時間 {formatDate(o.created_at)}</p>

{#if o.status === 'pending_payment'}
	<div class="mt-4 rounded border border-yellow-300 bg-yellow-50 p-4 text-sm">
		<p class="font-medium">付款方式：{PAYMENT_LABELS[o.payment?.method ?? 'credit']}</p>
		<p class="mt-1 text-gray-700">線上付款功能準備中；訂單已為你保留商品。</p>
	</div>
{:else if o.status === 'cancelled'}
	<div class="mt-4 rounded border border-gray-300 bg-gray-50 p-4 text-sm">
		這筆訂單已取消{#if o.cancelled_at}（{formatDate(o.cancelled_at)}）{/if}。
	</div>
{/if}

{#if data.token && !data.user}
	<p class="mt-4 text-sm text-gray-600">請把這個網頁的網址存起來，之後用它查看訂單。</p>
{/if}

<section class="mt-6 rounded border border-gray-200 bg-white">
	<ul class="divide-y divide-gray-100">
		{#each o.items as item, i (i)}
			<li class="flex items-center gap-4 p-4">
				<div class="h-14 w-14 shrink-0 overflow-hidden rounded bg-gray-100">
					{#if item.image_path}<img src={item.image_path} alt="" class="h-full w-full object-cover" />{/if}
				</div>
				<div class="min-w-0 flex-1">
					<div class="font-medium">{item.product_name}</div>
					<div class="text-sm text-gray-500">{item.variant_label} × {item.quantity}</div>
				</div>
				<div class="text-right">
					<div>{twd(item.line_total)}</div>
					<div class="text-xs text-gray-500">單價 {twd(item.unit_price)}</div>
				</div>
			</li>
		{/each}
	</ul>
	<div class="space-y-1 border-t border-gray-200 p-4 text-sm">
		<div class="flex justify-between"><span>商品小計</span><span>{twd(o.subtotal)}</span></div>
		<div class="flex justify-between"><span>運費</span><span>{o.shipping_fee === 0 ? '免運' : twd(o.shipping_fee)}</span></div>
		<div class="flex justify-between text-base font-bold"><span>總計</span><span>{twd(o.total)}</span></div>
	</div>
</section>

<div class="mt-6 grid gap-4 md:grid-cols-2">
	<section class="rounded border border-gray-200 bg-white p-4 text-sm">
		<h2 class="font-medium">取貨</h2>
		<p class="mt-2">{o.recipient_name}　{o.recipient_phone}</p>
		{#if o.shipment?.method === 'cvs'}
			<p class="mt-1">超商取貨：{o.shipment.cvs_sub_type ? CVS_LABELS[o.shipment.cvs_sub_type] : ''} {o.shipment.cvs_store_name}</p>
			<p class="text-gray-600">{o.shipment.cvs_store_address}</p>
		{:else if o.shipment}
			<p class="mt-1">宅配：{o.shipment.home_postal_code} {o.shipment.home_city}{o.shipment.home_district}{o.shipment.home_street}</p>
			{#if o.shipment.tracking_no}<p class="text-gray-600">{o.shipment.carrier} {o.shipment.tracking_no}</p>{/if}
		{/if}
		{#if o.note}<p class="mt-2 text-gray-600">備註：{o.note}</p>{/if}
	</section>
	<section class="rounded border border-gray-200 bg-white p-4 text-sm">
		<h2 class="font-medium">發票</h2>
		<p class="mt-2">{INVOICE_LABELS[o.invoice_type]}</p>
		{#if o.invoice_type === 'company'}
			<p class="text-gray-600">統編 {o.invoice_tax_id}｜{o.invoice_title}</p>
			<p class="text-gray-600">{o.invoice_address}</p>
		{:else if o.invoice_type === 'donation'}
			<p class="text-gray-600">愛心碼 {o.invoice_love_code}</p>
		{:else if o.invoice_carrier_num}
			<p class="text-gray-600">載具 {o.invoice_carrier_num}</p>
		{:else}
			<p class="text-gray-600">發票會寄到 {o.email}</p>
		{/if}
	</section>
</div>

{#if o.status === 'pending_payment'}
	<div class="mt-6 flex items-center gap-3">
		{#if confirming}
			<button type="button" onclick={cancel} disabled={cancelling} class="rounded bg-red-600 px-4 py-2 text-white disabled:opacity-50">
				{cancelling ? '取消中…' : '確定取消這筆訂單'}
			</button>
			<button type="button" onclick={() => (confirming = false)} class="rounded border border-gray-300 px-4 py-2">保留</button>
		{:else}
			<button type="button" onclick={() => (confirming = true)} class="text-sm text-gray-600 underline">取消訂單</button>
		{/if}
	</div>
{/if}

{#if data.user}
	<p class="mt-8 text-sm"><a href="/account/orders" class="underline">回我的訂單</a></p>
{/if}
```

- [ ] **Step 3: 檢查、建置、curl**

Run: `pnpm -C web check && pnpm -C web build`
Expected: 0 errors 0 warnings。
Curl（api、web 背景跑）：先用 `curl -s http://localhost:8080/api/products/<任一 active 商品的 slug>` 拿 `variants[0].id`，再下一張訪客訂單：

```bash
curl -s -X POST http://localhost:8080/api/orders -H 'Content-Type: application/json' -H 'Origin: http://localhost:5173' -H 'X-Requested-With: fetch' -d '{"items":[{"variant_id":"<variant id>","qty":1}],"email":"buyer@test.local","recipient_name":"王小明","recipient_phone":"0912345678","shipping_method":"home","address":{"postal_code":"100","city":"臺北市","district":"中正區","street":"重慶南路一段 122 號"},"invoice":{"type":"personal","carrier_type":"1"},"payment_method":"credit","note":""}'
```

拿回應的 `order_id` 與 `guest_token`，然後：`curl -s "http://localhost:5173/orders/<id>?t=<token>" | grep -c 待付款` = 1；不帶 `?t=` → 404；`/orders/not-a-uuid` → 404。跑完把 dev server 關掉，並用 curl `POST /api/orders/<id>/cancel?t=<token>` 把庫存還回去。

- [ ] **Step 4: Commit**

```bash
git add web/src/routes/orders
git commit -m "feat(web): 訂單頁（明細、取貨與發票資料、買家取消）"
```

### Task 15: 後台商店設定頁

**Files:**
- Create: `web/src/routes/admin/settings/+page.server.ts`、`web/src/routes/admin/settings/+page.svelte`
- Modify: `web/src/routes/admin/+layout.svelte`（側欄加「設定」）

**Interfaces:**
- Consumes: Task 4 的 `GET|PUT /api/admin/settings`、`AllSettings` 型別；`CVS_LABELS`。
- Produces: `/admin/settings` 頁；側欄連結。

- [ ] **Step 1: 寫 `web/src/routes/admin/settings/+page.server.ts`**

```ts
import type { PageServerLoad } from './$types';
import { serverApi } from '$lib/server/api';
import type { AllSettings } from '$lib/types';

export const load: PageServerLoad = async (event) => {
	const settings = await serverApi<AllSettings>(event, '/api/admin/settings');
	return { settings };
};
```

- [ ] **Step 2: 寫 `web/src/routes/admin/settings/+page.svelte`**

```svelte
<script lang="ts">
	import { untrack } from 'svelte';
	import { invalidateAll } from '$app/navigation';
	import { api, ApiError } from '$lib/api';
	import { CVS_LABELS } from '$lib/labels';
	import { toast } from '$lib/toast.svelte';
	import type { AllSettings, CvsSubType } from '$lib/types';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	// 初始值刻意只取一次；存檔成功後用伺服器回傳的整組覆蓋
	let form = $state<AllSettings>(untrack(() => structuredClone(data.settings)));
	let errors = $state<Record<string, string>>({});
	let saving = $state(false);

	const cvsTypes: CvsSubType[] = ['UNIMARTC2C', 'FAMIC2C', 'HILIFEC2C'];
	const input = 'mt-1 w-full rounded border border-gray-300 px-3 py-2';

	async function save(e: SubmitEvent) {
		e.preventDefault();
		errors = {};
		saving = true;
		try {
			const saved = await api<AllSettings>('/api/admin/settings', { method: 'PUT', body: JSON.stringify(form) });
			form = saved;
			await invalidateAll();
			toast.show('已儲存');
		} catch (err) {
			if (err instanceof ApiError) {
				errors = err.fields();
				toast.show(Object.keys(errors).length > 0 ? '請檢查紅字欄位' : err.message);
			} else toast.show('儲存失敗');
		} finally {
			saving = false;
		}
	}
</script>

<svelte:head><title>商店設定</title></svelte:head>

<h1 class="text-2xl font-bold">商店設定</h1>

<form onsubmit={save} class="mt-6 max-w-2xl space-y-6" novalidate>
	<section class="space-y-3 rounded border border-gray-200 bg-white p-4">
		<h2 class="font-medium">商店資訊</h2>
		<label class="block text-sm text-gray-700">
			商店名稱
			<input type="text" bind:value={form.shop.name} class={input} />
			{#if errors['shop.name']}<span class="text-red-600">{errors['shop.name']}</span>{/if}
		</label>
		<label class="block text-sm text-gray-700">
			簡介（選填）
			<textarea bind:value={form.shop.description} rows="3" class={input}></textarea>
			{#if errors['shop.description']}<span class="text-red-600">{errors['shop.description']}</span>{/if}
		</label>
		<div class="grid grid-cols-2 gap-3">
			<label class="block text-sm text-gray-700">
				聯絡 Email
				<input type="email" bind:value={form.shop.contact_email} class={input} />
				{#if errors['shop.contact_email']}<span class="text-red-600">{errors['shop.contact_email']}</span>{/if}
			</label>
			<label class="block text-sm text-gray-700">
				聯絡電話
				<input type="text" bind:value={form.shop.contact_phone} class={input} />
				{#if errors['shop.contact_phone']}<span class="text-red-600">{errors['shop.contact_phone']}</span>{/if}
			</label>
		</div>
	</section>

	<section class="space-y-3 rounded border border-gray-200 bg-white p-4">
		<h2 class="font-medium">運費</h2>
		<div class="grid grid-cols-3 gap-3">
			<label class="block text-sm text-gray-700">
				超商取貨（元）
				<input type="number" min="0" bind:value={form.shipping.cvs_fee} class={input} />
				{#if errors['shipping.cvs_fee']}<span class="text-red-600">{errors['shipping.cvs_fee']}</span>{/if}
			</label>
			<label class="block text-sm text-gray-700">
				宅配（元）
				<input type="number" min="0" bind:value={form.shipping.home_fee} class={input} />
				{#if errors['shipping.home_fee']}<span class="text-red-600">{errors['shipping.home_fee']}</span>{/if}
			</label>
			<label class="block text-sm text-gray-700">
				免運門檻（元，0 = 不免運）
				<input type="number" min="0" bind:value={form.shipping.free_threshold} class={input} />
				{#if errors['shipping.free_threshold']}<span class="text-red-600">{errors['shipping.free_threshold']}</span>{/if}
			</label>
		</div>
		<p class="text-xs text-gray-500">免運以商品小計比較；超商取貨商品小計上限 20,000 元（綠界規定）。</p>
	</section>

	<section class="space-y-3 rounded border border-gray-200 bg-white p-4">
		<h2 class="font-medium">付款方式</h2>
		<div class="flex flex-wrap gap-4 text-sm">
			<label class="flex items-center gap-2"><input type="checkbox" bind:checked={form.payment_methods.credit} /> 信用卡</label>
			<label class="flex items-center gap-2"><input type="checkbox" bind:checked={form.payment_methods.atm} /> ATM 轉帳</label>
			<label class="flex items-center gap-2"><input type="checkbox" bind:checked={form.payment_methods.cvs_code} /> 超商代碼繳費</label>
		</div>
		{#if errors.payment_methods}<p class="text-sm text-red-600">{errors.payment_methods}</p>{/if}
	</section>

	<section class="space-y-3 rounded border border-gray-200 bg-white p-4">
		<h2 class="font-medium">寄件人（超商物流單用）</h2>
		<div class="grid grid-cols-2 gap-3">
			<label class="block text-sm text-gray-700">
				姓名
				<input type="text" bind:value={form.sender.name} class={input} />
				{#if errors['sender.name']}<span class="text-red-600">{errors['sender.name']}</span>{/if}
			</label>
			<label class="block text-sm text-gray-700">
				手機
				<input type="tel" bind:value={form.sender.phone} placeholder="09xxxxxxxx" class={input} />
				{#if errors['sender.phone']}<span class="text-red-600">{errors['sender.phone']}</span>{/if}
			</label>
		</div>
	</section>

	<section class="space-y-3 rounded border border-gray-200 bg-white p-4">
		<h2 class="font-medium">退貨門市（買家未取件時退回這裡）</h2>
		<div class="grid grid-cols-3 gap-3">
			<label class="block text-sm text-gray-700">
				超商
				<select bind:value={form.return_store.sub_type} class={input}>
					<option value="">未設定</option>
					{#each cvsTypes as t (t)}<option value={t}>{CVS_LABELS[t]}</option>{/each}
				</select>
				{#if errors['return_store.sub_type']}<span class="text-red-600">{errors['return_store.sub_type']}</span>{/if}
			</label>
			<label class="block text-sm text-gray-700">
				門市代號
				<input type="text" bind:value={form.return_store.store_id} class={input} />
				{#if errors['return_store.store_id']}<span class="text-red-600">{errors['return_store.store_id']}</span>{/if}
			</label>
			<label class="block text-sm text-gray-700">
				門市名稱
				<input type="text" bind:value={form.return_store.store_name} class={input} />
				{#if errors['return_store.store_name']}<span class="text-red-600">{errors['return_store.store_name']}</span>{/if}
			</label>
		</div>
	</section>

	<button type="submit" disabled={saving} class="rounded bg-gray-900 px-4 py-2 text-white disabled:opacity-50">
		{saving ? '儲存中…' : '儲存設定'}
	</button>
</form>
```

- [ ] **Step 3: 側欄加連結**

`web/src/routes/admin/+layout.svelte` 的 `links` 陣列在 `分類` 之後加 `{ href: '/admin/settings', label: '設定' }`。

- [ ] **Step 4: 檢查、建置、curl**

Run: `pnpm -C web check && pnpm -C web build`
Expected: 0 errors 0 warnings。
Curl（api、web 背景跑；用 Task 13 的方式登入 admin 拿 cookie）：`curl -s -b cookies.txt http://localhost:5173/admin/settings | grep -c 商店設定` = 1；未登入 → 303。跑完關掉 dev server、刪 cookies.txt。

- [ ] **Step 5: Commit**

```bash
git add web/src/routes/admin/settings web/src/routes/admin/+layout.svelte
git commit -m "feat(web): 後台商店設定頁（商店資訊、運費、付款方式、寄件人、退貨門市）"
```

### Task 16: Playwright 主流程（本機）

**Files:**
- Modify: `web/package.json`（`@playwright/test`、`test:e2e` script）、`web/pnpm-lock.yaml`
- Create: `web/playwright.config.ts`、`web/e2e/checkout.spec.ts`
- Modify: `.gitignore`（`web/playwright-report/`）

**Interfaces:**
- Consumes: 全部前後端。
- Produces: `pnpm -C web test:e2e`：瀏覽 → 加入購物車 → 購物車 → 結帳（宅配）→ 訂單頁（規格 §15 的主流程；綠界表單攔截在計畫 3 加上）。

- [ ] **Step 1: 裝 Playwright（瀏覽器裝在 node_modules 裡，與規格不同之處 17）**

Run: `pnpm -C web add -D @playwright/test`（背景）
Run: `PLAYWRIGHT_BROWSERS_PATH=0 pnpm -C web exec playwright install chromium`（背景，會下載約 150 MB）
Expected: 兩個都成功。若下載被擋，Task 停在這裡回報 BLOCKED（附錯誤訊息），其餘檔案照樣建好並 commit。

- [ ] **Step 2: 寫 `web/playwright.config.ts`**

```ts
import { defineConfig } from '@playwright/test';

/** 假設 api（:8080）與 web dev（:5173）已經在跑；不進 CI（與規格不同之處 17） */
export default defineConfig({
	testDir: 'e2e',
	timeout: 60_000,
	retries: 0,
	reporter: 'list',
	use: {
		baseURL: process.env.E2E_BASE_URL ?? 'http://localhost:5173',
		headless: true,
		locale: 'zh-TW'
	}
});
```

`web/package.json` 的 `scripts` 加 `"test:e2e": "playwright test"`。`.gitignore` 的 `web/test-results/` 下一行加 `web/playwright-report/`。

- [ ] **Step 3: 寫 `web/e2e/checkout.spec.ts`**

```ts
import { expect, test, type APIRequestContext } from '@playwright/test';

const API = process.env.E2E_API_URL ?? 'http://localhost:8080';
const ORIGIN = process.env.E2E_BASE_URL ?? 'http://localhost:5173';
const ADMIN = {
	email: process.env.E2E_ADMIN_EMAIL ?? 'admin@example.com',
	password: process.env.E2E_ADMIN_PASSWORD ?? 'admin12345'
};
// api 的 CSRF 檢查：變更請求要帶這兩個 header
const HEADERS = { 'x-requested-with': 'fetch', origin: ORIGIN };

/** 用後台 API 建一個上架商品，回 slug 與名稱 */
async function seedProduct(request: APIRequestContext): Promise<{ slug: string; name: string }> {
	const login = await request.post(`${API}/api/auth/login`, { headers: HEADERS, data: ADMIN });
	expect(login.ok(), `admin 登入失敗：${login.status()} ${await login.text()}`).toBeTruthy();
	const name = `E2E 狗糧 ${Date.now()}`;
	const created = await request.post(`${API}/api/admin/products`, {
		headers: HEADERS,
		data: { name, status: 'active', variants: [{ price: 300, stock: 5 }], images: [] }
	});
	expect(created.status(), await created.text()).toBe(201);
	const body = await created.json();
	return { slug: body.slug, name };
}

test('瀏覽 → 加入購物車 → 結帳（宅配）→ 訂單頁', async ({ page, request }) => {
	const product = await seedProduct(request);

	await page.goto(`/products/${product.slug}`);
	await expect(page.getByRole('heading', { name: product.name })).toBeVisible();
	await page.getByRole('button', { name: '加入購物車' }).click();
	await expect(page.getByText('已加入購物車')).toBeVisible();

	await page.goto('/cart');
	await expect(page.getByText(product.name)).toBeVisible();
	await expect(page.getByText('小計')).toBeVisible();
	await page.getByRole('link', { name: '前往結帳' }).click();
	await expect(page).toHaveURL(/\/checkout$/);

	await page.getByLabel('Email').fill('e2e@test.local');
	await page.getByLabel('收件人').fill('王小明');
	await page.getByLabel('手機').fill('0912345678');
	await page.getByLabel('縣市').selectOption('臺北市');
	await page.getByLabel('鄉鎮市區').selectOption('中正區');
	await expect(page.getByLabel('郵遞區號')).toHaveValue('100');
	await page.getByLabel('地址').fill('重慶南路一段 122 號');
	await expect(page.getByText('總計')).toBeVisible();
	await page.getByRole('button', { name: '送出訂單' }).click();

	await expect(page).toHaveURL(/\/orders\/[0-9a-f-]{36}\?t=[0-9a-f]{64}$/);
	await expect(page.getByText(/DS\d{6}[A-Z0-9]{4}/)).toBeVisible();
	await expect(page.getByText('待付款')).toBeVisible();
	await expect(page.getByText(product.name)).toBeVisible();
	await expect(page.getByText('重慶南路一段 122 號')).toBeVisible();

	// 取消 → 狀態變已取消
	await page.getByRole('button', { name: '取消訂單' }).click();
	await page.getByRole('button', { name: '確定取消這筆訂單' }).click();
	await expect(page.getByText('已取消', { exact: false }).first()).toBeVisible();
});
```

- [ ] **Step 4: 本機跑**

前置（都背景跑）：`export PATH="$HOME/.cargo/bin:$PATH" && cd api && cargo run`、`pnpm -C web dev`。確認 `curl -s localhost:8080/api/health` 回 `{"status":"ok"}`、`curl -s -o /dev/null -w '%{http_code}' localhost:5173/` 回 200。開發用管理員 `admin@example.com` / `admin12345` 要存在（計畫 1 已建；不在就 `cargo run -- create-admin admin@example.com` 用 `ADMIN_PASSWORD=admin12345` 環境變數建）。

Run: `PLAYWRIGHT_BROWSERS_PATH=0 pnpm -C web test:e2e`（背景，第一次約 1～2 分鐘）
Expected: `1 passed`。失敗時看 `web/test-results/` 的截圖與 trace；先確認是測試選擇器問題還是頁面 bug，修對的那一邊。

跑完關掉兩個 dev server。

- [ ] **Step 5: `pnpm -C web check && pnpm -C web test && pnpm -C web build`**

Expected: check 0/0（`e2e/` 不在 `src/`，svelte-check 會掃到 `playwright.config.ts`，型別要過）；vitest 仍 22 個（`vitest` 的 `include` 只有 `src/**/*.test.ts`，不會撿到 e2e）；build 成功。

- [ ] **Step 6: Commit**

```bash
git add web/package.json web/pnpm-lock.yaml web/playwright.config.ts web/e2e/checkout.spec.ts .gitignore
git commit -m "test(web): Playwright 主流程（瀏覽、加入購物車、結帳、訂單頁、取消）"
```

---

## 計畫 2 完成時的驗收清單

全部做完後，從乾淨狀態走一次（每一行都要成立）：

1. `docker compose -f deploy/docker-compose.dev.yml up -d db` → healthy；`cd api && cargo run` 啟動時跑完 `0002_members_orders.sql`（log 沒有 migration 錯誤）。
2. `cd api && DATABASE_URL=postgres://dog_shop:dog_shop@localhost:5435/dog_shop cargo test` → 全綠（單元 + 16 個整合測試檔），含「並發下單不超賣」。
3. `pnpm -C web test`（22 個）、`pnpm -C web check`（0 錯誤 0 警告）、`pnpm -C web build` 全過。
4. `PLAYWRIGHT_BROWSERS_PATH=0 pnpm -C web test:e2e` → 1 passed（api 與 web dev 都在跑）。
5. 瀏覽器手動：`/register` 註冊 → 自動登入、頁首出現「會員中心」→ `/account` 改姓名手機、改密碼 → `/account/addresses` 新增兩筆、切預設、刪一筆 → 商品頁加入購物車 → `/cart` 顯示核對結果、改數量 → `/checkout` 用「常用地址帶入」、選公司發票填統編（用 `04595257`）→ 送出 → `/orders/<id>` 顯示待付款、明細、統編 → 取消 → 狀態已取消，後台商品頁看到庫存回來 → `/account/orders` 有這筆。
6. 訪客：登出後再走一次購物 → 結帳 → 訂單頁網址含 `?t=`；去掉 `?t=` 變 404。
7. `/forgot-password` 送出顯示「如果這個 Email 有註冊過…」（信件在計畫 3）；`/reset/隨便打` 送出顯示「重設連結無效或已過期」。
8. `/admin/settings` 改免運門檻為 100、關掉超商代碼 → `/checkout` 的運費變免運、付款方式只剩兩種。
9. 超商取貨：`/checkout` 選超商 → 「選擇門市」按鈕停用、送出時提示「請先選擇取貨門市」（真門市要等計畫 4）。
10. `git log --oneline` 看到本計畫 16 個 commit 都在 `worktree-mvp-design`；`git status` 乾淨；沒有 push。

## 交給計畫 3 的事項（寫計畫 3 時必看）

1. **`POST /api/orders` 回傳要加 `ecpay: { action, fields }`**（規格 §7 第 6 點）；`create_order` 內要多 INSERT `invoices(pending)`（表由計畫 3 的 migration 建）。前端 `checkout/+page.svelte` 的 `submit()` 拿到 `ecpay` 後改成用隱藏表單 POST 到綠界（現在是直接 `goto` 訂單頁）。
2. **jobs worker**（規格 §9）：`SELECT ... WHERE status='queued' AND run_at <= now() ORDER BY id FOR UPDATE SKIP LOCKED LIMIT 10`，每 2 秒；失敗退避 `2^attempts` 分鐘、超過 `max_attempts` 標 `failed`。`send_email` 的 payload 形狀：`{ "template": "<名字>", ... }`，本計畫已排的兩種：`order_created { order_id }`、`password_reset { user_id }`。**`password_reset` 的 handler 要在寄信當下呼叫 `password_resets::create(db, user_id)` 產生 token、組 `{PUBLIC_BASE_URL}/reset/{token}`**（與規格不同之處 11）。job 做完把 `payload` 清成 `'{}'`，不留個資。
3. **過期未付款**（規格 §5）：每 10 分鐘掃 `pending_payment`，到期呼叫 `orders::cancel_in_tx(&mut tx, order_id, "expired")`（已含歸還庫存）。
4. **`payments`**：表已存在，第一筆 `pending` 列已由 `create_order` 建（`merchant_trade_no = order_no || '01'`）。`ReturnURL` / `PaymentInfoURL` 更新該筆；`POST /api/orders/{id}/repay` 建新列，流水用 `count(*) + 1` 補兩碼。遲到付款規則見規格 §4；`orders.needs_refund` 欄位已有。
5. **訂單頁** `web/src/routes/orders/[id]/+page.svelte`：`pending_payment` 區塊現在是「線上付款功能準備中」，要換成：付款輪詢（每 3 秒、最多 2 分鐘）、ATM 虛擬帳號／超商代碼與期限（`payment.atm_bank_code`、`atm_vaccount`、`cvs_payment_no`、`expire_at` 已在 `OrderDetail`）、「重新付款」按鈕。
6. **Email 模板要的資料**：`orders::get_for_viewer` 回的 `OrderDetail` 已含 items、shipment、payment；worker 要一個不看權限的 `orders::get_detail(db, id)`（把 `get_for_viewer` 的查詢抽出來即可）。
7. **給計畫 4**：`settings.sender`、`settings.return_store` 已存好；`cvs_stores::insert` 給 `map-reply` 用；`POST /api/checkout/cvs-map` 放進 `routes/checkout.rs`；結帳頁「選擇門市」按鈕改成呼叫它並用隱藏表單 POST 到綠界（表單草稿已存 sessionStorage `dog_shop_checkout_v1`，`?store=<token>` 回來會自動還原並切到超商取貨）。`orders::cancel_in_tx` 給後台取消用；退款規則（已付未出貨才歸還庫存）在計畫 4 寫。
8. 後台側欄 `web/src/routes/admin/+layout.svelte` 的 `links` 之後加「訂單」（計畫 4）、「匯入」（計畫 5）。
9. 計畫 1 最終審查留下的可出貨小項（`docs/superpowers/reviews/2026-09-06-plan-1-final-review.md` 的 Minor 1、4、8、9、11～14）還沒做；計畫 5 上線前挑值得做的。

<!-- PLAN2-END -->
