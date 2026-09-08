# dog_shop 計畫 3／5：綠界金流、背景工作、Email、電子發票 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 讓計畫 2 的訂單真的能收錢：下單後把買家送到綠界全方位金流付款（信用卡／ATM／超商代碼），接收綠界的付款結果（`ReturnURL`）與繳費資訊（`PaymentInfoURL`）回呼，過期未付款自動取消並歸還庫存，買家可重新付款；建一個 jobs worker 執行 outbox 裡的工作：寄 Email（lettre + askama，六種模板）、開立電子發票（綠界 B2C）；訂單頁顯示繳費資訊、付款輪詢、重新付款與發票號碼。

**Architecture:** 後端 `api/` 新增 `ecpay/`（`mac.rs` CheckMacValue、`time.rs` 台北時間、`aio.rs` 付款表單與回呼解析、`aes.rs` 發票加解密、`invoice.rs` 發票 API 客戶端與可替換閘道）、`jobs/`（`worker.rs` 每 2 秒用 `FOR UPDATE SKIP LOCKED` 認領並退避重試、`scheduled.rs` 過期取消／自動完成／清理、`handlers.rs` 兩種 job）、`mail/`（`Mailer` enum：SMTP／只記 log／測試擷取；askama 模板）、`domain/{payments,invoices}.rs`、`routes/ecpay_payment.rs`（兩條綠界回呼，純文字回應）。`orders::create_order` 簽名不變（12 個測試呼叫點），`POST /api/orders` 在 route 層把 `ecpay: { action, fields }` 包進回應；`AppState` 多 `mailer`、`invoices` 兩個可替換的閘道，測試不打網路。前端 `web/` 把結帳送出改成隱藏表單 POST 到綠界、訂單頁改成付款狀態頁、Playwright 攔截送往綠界的表單檢查欄位。

**Tech Stack:** 同計畫 2（Rust 1.98 / axum 0.8 / sqlx 0.9 / PostgreSQL 17；SvelteKit 2 / Svelte 5 runes / Tailwind 4 / vitest / Playwright 1.63）。新增 Rust crate：`lettre` 0.11（`tokio1` + `tokio1-rustls-tls`）、`askama` 0.16、`reqwest` 0.13（`rustls-no-provider`）、`rustls` 0.23（`ring` provider，啟動時明確安裝）、`aes` 0.8 + `cbc` 0.1、`base64` 0.22、`percent-encoding` 2、`form_urlencoded` 1。

**Spec:** `docs/superpowers/specs/2026-09-06-dog-shop-mvp-design.md`（本計畫負責 §3 `invoices` 表、§4 付款成功／過期／遲到付款、§5 未付款過期、§6.1 訂單頁付款輪詢與重新付款、§7 步驟 6 的 `ecpay` 回傳與步驟 7、8、9、§8 共用 CheckMacValue／§8.2 全方位金流／§8.4 電子發票、§9 背景工作、§10 `orders/{id}/repay` 與 `ecpay/payment/*`、§12 Email、§14 綠界回呼錯誤處理、§15 對應測試）。計畫 2 的交接事項在 `docs/superpowers/plans/2026-09-06-dog-shop-plan-2-members-checkout.md` 的「交給計畫 3 的事項」（6911–6935 行）；計畫 2 最終審查的補充交接在 `docs/superpowers/reviews/2026-09-07-plan-2-final-review.md` 的「交給計畫 3 的事項」。

## Global Constraints

- 前端一律用最新版 SvelteKit 2 / Svelte 5 runes（`$state`、`$derived`、`$props`、`$effect`、`$bindable`），不用 legacy `export let` / store 寫法（規格 §1.1）。所有變更請求走瀏覽器端 `api()`（自動帶 `X-Requested-With: fetch`），不用 SvelteKit form actions。
- 後端 Rust 1.98 stable、axum 0.8、sqlx 0.9、tokio；edition 2024（規格 §1.1）。`cargo clippy --all-targets -- -D warnings` 與 `cargo fmt --all --check` 每個任務結束都要乾淨。
- 資料庫 PostgreSQL 17；主鍵 UUID v7（`Uuid::now_v7()`；session id 例外用 v4）；金額 `integer` 新台幣；時間 `timestamptz` 存 UTC（規格 §3）。
- 錯誤回應格式固定 `{ "error": { "code", "message", "details" } }`；本計畫新增的 code：`ORDER_NOT_PAYABLE`（400，訂單不是待付款卻要付款）、`ECPAY_ERROR`（502，綠界同步呼叫失敗；本計畫只定義，計畫 4 的物流建單用）（規格 §10、§14）。
- **時間（規格 §8 原文）：** DB 一律存 UTC。送給綠界的格式化日期字串（`MerchantTradeDate` 等 `yyyy/MM/dd HH:mm:ss`）用台北時間（UTC+8）。電子發票的 `RqHeader.Timestamp` 是 Unix epoch 秒數，沒有時區，不能加 8 小時（綠界只接受約 10 分鐘內的請求）。前端顯示台北時間。
- **CheckMacValue（規格 §8.1 原文）：** 參數依 key 排序（不分大小寫）→ 串成 `HashKey=...&k1=v1&...&HashIV=...` → 依綠界規定做 URL encode（.NET 風格，再套用綠界的字元替換表）→ 轉小寫 → 雜湊 → 轉大寫。全方位金流用 SHA256（`EncryptType=1`）。實作時以綠界文件上的範例做單元測試。
- **全方位金流欄位（規格 §8.2 原文）：** POST 表單到 `/Cashier/AioCheckOut/V5`。欄位：`MerchantID`、`MerchantTradeNo`、`MerchantTradeDate`、`PaymentType=aio`、`TotalAmount`、`TradeDesc`、`ItemName`（品項用 `#` 連接，超長截斷）、`ReturnURL`、`ChoosePayment ∈ {Credit, ATM, CVS}`、`ClientBackURL`、`PaymentInfoURL`、`ExpireDate=3`、`StoreExpireDate=4320`、`NeedExtraPaidInfo=N`、`EncryptType=1`、`CustomField1=order_id`、`CheckMacValue`。
- **回呼契約（規格 §8.2、§14 原文）：** 回呼驗證：重算 `CheckMacValue` 比對；不符回 HTTP 400 `0|CheckMacValue Error` 並記 log。找不到 `MerchantTradeNo` 回 `0|Unknown MerchantTradeNo`。測試環境有「模擬付款」按鈕，通知會帶 `SimulatePaid=1`，只記 log 不改狀態。回呼到達時訂單已 `cancelled` 或已 `paid`：依 §4「遲到的付款」處理，仍回 `1|OK`。綠界回呼要嚴格區分：簽章錯誤回 400；已處理過的重複通知回 `1|OK` 不重做；暫時性失敗（DB 連不上）回 500 讓綠界重送。任何寫入綠界的呼叫都先把請求存 DB 再送，回應也存，方便對帳與重試。
- **遲到的付款（規格 §4 原文）：** 付款成功回呼到達時，訂單已是 `cancelled`（過期）或已因另一筆付款嘗試變成 `paid`，就只把該筆 `payment` 標 `paid`，不動庫存、不改訂單狀態，把 `orders.needs_refund` 設為 true。
- **未付款過期（規格 §5 原文）：** worker 每 10 分鐘掃 `pending_payment` 的訂單。到期時間以該訂單所有 `payments.expire_at`（來自 `PaymentInfoURL` 的繳費期限）的最大值加 2 小時緩衝為準；沒有任何繳費期限（例如只嘗試過信用卡）就用 `created_at` 加 3 天。到期就標 `cancelled(reason=expired)` 並歸還庫存。
- **背景工作（規格 §9 原文）：** `jobs` 表當 outbox。一個 tokio task 每 2 秒 `SELECT ... WHERE status='queued' AND run_at <= now() ORDER BY id FOR UPDATE SKIP LOCKED LIMIT 10`，逐筆執行。失敗：`attempts + 1`，`run_at = now() + 2^attempts 分鐘`，超過 `max_attempts` 標 `failed`。job 種類：`send_email`、`issue_invoice`。排程型工作（不走 jobs 表）：`expire_unpaid_orders`（每 10 分鐘）、`auto_complete_shipped`（每小時，出貨超過 14 天且 `shipments.status` 不是 `returned` 的訂單轉 completed）、`purge_expired_sessions`（每天）。
- **Email（規格 §12）：** `lettre` 走 SMTP（STARTTLS 或 TLS）。`askama` 模板，純文字 + 簡單 HTML 各一份。六種：`order_created`、`payment_instructions`、`payment_received`、`order_shipped`、`invoice_issued`、`password_reset`。寄件失敗由 job 重試。
- **電子發票（規格 §8.4）：** 端點 `/B2CInvoice/Issue`，JSON。外層 `{ MerchantID, RqHeader: { Timestamp }, Data }`。`Data` = 內層 JSON → URL encode → AES-128-CBC（HashKey 為 key、HashIV 為 iv、PKCS7）→ Base64。內層欄位 `RelateNumber=order_no`、`CustomerEmail`、`CustomerPhone`、`Print`、`Donation`、`LoveCode`、`CarrierType`（`1` 綠界載具 / `2` 自然人憑證 / `3` 手機條碼）、`CarrierNum`、`CustomerIdentifier`、`CustomerName`、`CustomerAddr`、`TaxType=1`、`SalesAmount=total`、`InvType=07`、`vat=1`、`Items[]`（每個 `order_item` 一列，運費大於 0 時多一列「運費」）。公司戶不帶載具、`Print=1`、帶抬頭與發票地址。`dedupe_key = invoice:{order_id}`。
- `merchant_trade_no` = `order_no` + 兩碼流水，第一筆 `01`，重付換號（規格 §3）。
- 樣式 Tailwind CSS 4，自寫元件；語言繁體中文（規格 §6.2）。
- 所有 commit 只在本機分支 `worktree-mvp-design`。**不要 `git push`、不要開 PR**（使用者的 CLAUDE.md 第 5 條）。
- 秘密（HashKey、HashIV、SMTP 密碼、DB 連線字串、重設 token、guest_token）不進 log、不進 git（規格 §11）。綠界原始 payload 存進各表的 `raw` 欄位。stage 憑證是公開測試資料，只能用於 stage；正式憑證只放伺服器的 `.env`。

---

## 五份計畫的分工（規格涵蓋表，本計畫更新版）

計畫 1、2 已完成（`docs/superpowers/reviews/2026-09-06-plan-1-final-review.md`、`docs/superpowers/reviews/2026-09-07-plan-2-final-review.md`）。粗體是本計畫。

| 規格章節／需求 | 計畫 |
|---|---|
| §3 資料表：invoices | **3** |
| §4 付款成功、過期、遲到付款 | **3** |
| §4 出貨、完成、退回、退款、後台取消（`auto_complete_shipped` 的排程在 **3**，`shipped_at` 由 4 寫） | 4 |
| §5 未付款過期（`expire_unpaid_orders`） | **3** |
| §6.1 `/orders/[id]` 付款輪詢、繳費資訊、重新付款、發票號碼 | **3** |
| §6.1 `/checkout` 超商電子地圖 | 4 |
| §7 步驟 6 的 `ecpay` 回傳、7（隱藏表單 POST）、8（回呼、ClientBackURL）、9（重新付款） | **3** |
| §7 步驟 2 超商門市（`cvs-map`、`map-reply`） | 4 |
| §8 共用時間規則、§8.1 CheckMacValue（SHA256；MD5 版在 4）、§8.2 全方位金流、§8.4 電子發票 | **3** |
| §8.3 物流 | 4 |
| §9 jobs worker、排程工作 | **3** |
| §10 API：`orders/{id}/repay`、`ecpay/payment/return`、`ecpay/payment/info` | **3** |
| §10 API：`ecpay/logistics/*`、`admin/orders/*`（含 `retry-invoice`、`mark-refunded`）、`admin/dashboard` | 4 |
| §10 API：`admin/import/*` | 5 |
| §12 Email（六種模板；`order_shipped` 的觸發點在 4） | **3** |
| §14 綠界回呼錯誤處理 | **3** |
| §15 mac／aes 單元測試、回呼冪等、過期歸還庫存整合測試、Playwright 攔截綠界表單 | **3** |
| §16 部署、§13 匯入 | 5 |

## 與規格不同之處（已決定，執行時照這裡做）

前 21 條是計畫 1、2 的決定，原文見計畫 2 的同名章節；22 起是本計畫的決定。

22. **SMTP 未設定時 Email 只記 log，不寄。** `SMTP_HOST` 空白 → `Mailer::Log`：以 `info` 記 `to` 與 `subject`，內文只在 `RUST_LOG` 開 `mail_body=debug` 時輸出（內文含重設連結與訪客訂單網址，正式環境不要開）。原因：開發機與 CI 沒有 SMTP，規格 §12 只描述正式寄送；規格 §11「不在 log 輸出重設 token」靠預設不印內文守住。正式環境必須設定 `SMTP_HOST`／`SMTP_FROM`，啟動時沒設會 `warn`。
23. **`POST /api/orders` 的回應由 route 層組成 `{ order_id, order_no, guest_token, ecpay: { action, fields } }`**；`orders::create_order` 簽名與回傳值不變（12 個測試呼叫點），綠界表單由 `routes/orders.rs` 讀 `orders::get_detail` 後呼叫 `ecpay::aio::checkout_form`。`POST /api/orders/{id}/repay` 回 `{ ecpay }`，body 必須是 JSON（至少 `{}`），可帶 `payment_method` 換付款方式；沒帶就沿用最近一筆的方式。
24. **綠界憑證的讀法**：`ECPAY_ENV=stage`（預設）時，`ECPAY_AIO_*`、`ECPAY_INVOICE_*` 空白就退回公開測試憑證（AIO 3002607 / `pwFHCqoQZGmho4w6` / `EkRm7iFT261dpevs`；發票 2000132 / `ejCk326UnaZWKisg` / `q9jcZX8Ib9LM8wYk`）；`ECPAY_ENV=prod` 時任何一個空白就啟動失敗。原因：根目錄 `.env` 是從舊的 `.env.example` 複製的，發票憑證是空的，不能讓 api 起不來。
25. **忘記密碼寄信節流**：`password_reset` job 在寄信前查同一使用者 10 分鐘內是否已建立過重設 token；有就略過（job 標 done、記 info）。寄信失敗時把剛建立的 token 刪掉再回錯誤，重試時才能再產生。原因：計畫 2 審查 Minor 7 — `forgot` 沒去重，worker 上線後會變成對受害者 Email 的轟炸。
26. **回呼的 HTTP 狀態與金額檢查**：簽章錯 400 `0|CheckMacValue Error`；缺 `MerchantTradeNo`／`RtnCode` 400 `0|Missing Field`；找不到 `MerchantTradeNo` 200 `0|Unknown MerchantTradeNo`（綠界重送也不會變好）；DB 錯誤 500 `0|Server Error`；其餘（成功、重複、模擬付款、非成功碼、遲到）200 `1|OK`。`RtnCode=1` 但 `TradeAmt` ≠ `payments.amount` 時當「遲到的付款」處理（payment 標 paid、`needs_refund=true`、訂單不動、記 warn）。`RtnCode≠1`（含信用卡 10300066 待確認）：payment 標 `failed`、存 raw、訂單不動。
27. **Job 生命週期細節**：認領時 `attempts + 1`（第一次執行 `attempts=1`，第一次失敗退避 2 分鐘）；做完 `payload` 清成 `{}`（不留個資）；`failed` 的保留 payload 給計畫 4 的後台重試看；`running` 超過 10 分鐘視為當機殘留，`expire_unpaid_orders` 的每一輪開頭重新排隊；一輪認領滿 10 筆就立刻再跑下一輪，不等 2 秒。
28. **過期取消時把該訂單 `pending` 的 payments 標 `expired`**；重新付款不改舊列的狀態（保留對帳）。`OrderDetail.payment` 一律是最新一列（`created_at DESC, id DESC`）。
29. **電子發票**：`RelateNumber = order_no`；`CarrierType=1`（綠界會員載具）時 `CarrierNum` 與 `CustomerID` 都留空（文件已確認 `CustomerID` 非必填）；個人與捐贈的 `CustomerName` 帶收件人姓名、公司帶抬頭。開立成功但我們沒存到（極少見）→ 下次重試會拿到重複 `RelateNumber` 的錯誤 → 5 次後標 `failed`，後台重試前先到綠界後台查（計畫 5 可加 `GetIssue` 查詢）。只對 `paid`／`shipped`／`completed` 的訂單開立；job 跑到時訂單已取消或退款就略過（done）。
30. **`ecpay::mac` 本計畫只做 SHA256**（全方位金流、發票不用 mac）；物流用的 MD5 版本與 `ECPAY_LOGISTICS_*` 憑證計畫 4 加。
31. **訂單頁顯示發票號碼、隨機碼、開立時間**（`OrderDetail.invoice`；規格 §6.1 沒要求，小而有用）；開立失敗不告訴買家細節，只顯示「發票開立中」。
32. **TLS provider**：`reqwest` 用 `rustls-no-provider`，`main.rs` 第一行 `rustls::crypto::ring::default_provider().install_default()`，與 sqlx（`tls-rustls-ring`）、lettre（`rustls-tls` = ring）一致，避免同時編進 aws-lc-rs 與 ring 讓 rustls 在啟動時 panic。`rustls-platform-verifier` 在 Linux 讀系統 CA，正式映像要裝 `ca-certificates`（交給計畫 5）。
33. **過期規則照規格 §5**（3 天／繳費期限 + 2 小時）。計畫 2 審查建議「從未到過綠界頁的訂單」縮短到例如 2 小時，是產品決定，列在交接給使用者決定；本計畫先讓過期機制存在。

## 環境事實（每個任務開始前都要知道）

- 這台機器：macOS、zsh、Node 24、pnpm 10、Docker（OrbStack）。Rust 1.98.1（rustup，在 `~/.cargo`）。**這個 sandbox 的新 shell 找不到 `cargo`，而且拒絕 `source`、heredoc、`$(...)`、`for` 迴圈**：每個含 cargo 的指令一律寫成 `export PATH="$HOME/.cargo/bin:$PATH" && cargo <子指令> --manifest-path api/Cargo.toml`（不要 `cd api`）。`#[sqlx::test(migrations = "./migrations")]` 與 askama 的 `templates/` 都相對於 `CARGO_MANIFEST_DIR`（= `api/`），用 `--manifest-path` 沒問題；`cargo run` 用 `dotenvy` 讀**工作目錄**的 `.env`（repo 根目錄，已存在）。
- 工作目錄是 git worktree：`/Users/wilson08/IdeaProjects/dog_shop/.claude/worktrees/mvp-design`，分支 `worktree-mvp-design`。所有指令都從這裡跑。**不要 `cd web`**：前端指令一律寫 `pnpm -C web <script>`。
- commit 步驟一律寫成純指令：`git add <檔案...>` 然後 `git commit -m "..."`。建檔用 Write 工具、改檔用 Edit 工具。`rm -rf` 會被攔（用 `trash`）。停伺服器用 `lsof -ti :8080`／`lsof -ti :5173` 取 pid 再 `kill`（`pkill -f` 對 vite 無效）。
- **開發資料庫在 `localhost:5435`**（Docker 容器 `dog_shop-db-1`，PostgreSQL 17）。`#[sqlx::test]` 從**行程環境變數** `DATABASE_URL` 讀連線（不讀 `.env`）：測試指令一律 `DATABASE_URL=postgres://dog_shop:dog_shop@localhost:5435/dog_shop cargo test --manifest-path api/Cargo.toml`。它會為每個測試建獨立臨時資料庫並跑 `migrations/`。CI 用 5432（容器內），不要改。
- 根目錄 `.env`（不進 git）含 `ECPAY_ENV=stage` 與 AIO 測試憑證，`ECPAY_INVOICE_HASH_KEY`／`HASH_IV` 是空的（Task 1 之後會退回 stage 預設）、`SMTP_HOST` 空（Email 只記 log）、`PUBLIC_BASE_URL=http://localhost:5173`。開發用管理員：`admin@example.com` / `admin12345`。
- 計畫 2 結束時的狀態（HEAD `7de68d0`）：`cargo test` 103 個全綠（單元 + 16 個整合測試檔）、`cargo fmt --all --check` 與 `cargo clippy --all-targets -- -D warnings` 乾淨；`pnpm -C web test` 24 個、`pnpm -C web check` 0 錯誤 0 警告、`pnpm -C web build` 成功；`pnpm -C web test:e2e` 1 passed（要 api 與 web dev 都在跑）。每個任務結束時這些都要維持（**svelte-check 的警告也算錯**）。
- **新相依套件的第一次編譯很慢**（lettre、reqwest、aes 等，3～6 分鐘）：`cargo test`、`cargo clippy`、`cargo run` 一律 `run_in_background: true`、timeout 拉到 600000，等通知，不要輪詢。
- sqlx 0.9：`query` / `query_as` / `query_scalar` 只接受 `&'static str`，動態組的 SQL 要包 `sqlx::AssertSqlSafe(sql)` 且只能包沒有使用者輸入的字串。交易：`let mut tx = db.begin().await?;`，在 `fn(tx: &mut Transaction<'_, Postgres>)` 裡執行用 `&mut **tx`，在擁有 `tx` 的函式裡用 `&mut *tx`。tuple 也能當 `query_as` 的目標（`(String, i32)`）。
- axum 0.8 路徑參數寫法 `/api/orders/{id}`；handler 用 `AppJson` / `AppQuery` / `AppPath`（`api/src/extract.rs`）。綠界回呼不是 JSON，用 `body: String` 擷取器（要放在參數最後）自己解 form-urlencoded。`/api/ecpay/` 前綴已在 `api/src/auth/csrf.rs:11` 的 `EXEMPT_PREFIXES` 內，回呼不用帶 Origin。
- 綠界文件範例（用來當測試向量，已用 `shasum -a 256` 驗過）：HashKey `pwFHCqoQZGmho4w6`、HashIV `EkRm7iFT261dpevs`、參數 `TradeDesc=促銷方案`、`PaymentType=aio`、`MerchantTradeDate=2023/03/12 15:30:23`、`MerchantTradeNo=ecpay20230312153023`、`MerchantID=3002607`、`ReturnURL=https://www.ecpay.com.tw/receive.php`、`ItemName=Apple iphone 15`、`TotalAmount=30000`、`ChoosePayment=ALL`、`EncryptType=1` → CheckMacValue `6C51C9E6888DE861FD62FB1DD17029FC742634498FD813DC43D4243B5685B840`。AES 向量（用 `openssl enc -aes-128-cbc -nosalt` 算的）在 Task 10。
- 綠界回呼欄位名（文件）：`ReturnURL` POST `MerchantID`、`MerchantTradeNo`、`StoreID`、`RtnCode`（1 = 付款成功；信用卡 10300066 = 待確認）、`RtnMsg`、`TradeNo`、`TradeAmt`、`PaymentDate`（`yyyy/MM/dd HH:mm:ss`）、`PaymentType`（`Credit_CreditCard`、`ATM_xxx`、`CVS_CVS`）、`PaymentTypeChargeFee`、`TradeDate`、`SimulatePaid`（0/1）、`CustomField1`～`4`、`CheckMacValue`。`PaymentInfoURL` POST 同一組核心欄位加 `RtnCode`（ATM 取號成功 = 2；超商代碼取號成功 = 10100073）、`BankCode`、`vAccount`（注意小寫 v）、`ExpireDate`（ATM 是 `yyyy/MM/dd`，代表該日 23:59:59；超商是 `yyyy/MM/dd HH:mm:ss`）、`PaymentNo`、`Barcode1`～`3`。兩者都要回純文字 `1|OK`。
- 電子發票回應：外層 `{ MerchantID, RpHeader: { Timestamp }, TransCode (1 = 成功), TransMsg, Data }`，`Data` 解開後 `{ RtnCode (1 = 成功), RtnMsg, InvoiceNo (10 碼), InvoiceDate ("yyyy-MM-dd HH:mm:ss"，台北), RandomNumber (4 碼) }`。

## 檔案結構

```
api/Cargo.toml                          Modify（Task 6、9、10 各加當時用到的 crate）
api/migrations/0003_invoices.sql        Create（Task 2）
api/src/lib.rs                          Modify（Task 3 ecpay、Task 8 jobs、Task 9 mail）
api/src/main.rs                         Modify（Task 9 mailer + worker 啟動、Task 10 rustls provider + 發票閘道）
api/src/config.rs                       Modify（Task 1：EcpayConfig、SmtpConfig、for_tests）
api/src/state.rs                        Modify（Task 9 mailer、Task 10 invoices）
api/src/error.rs                        Modify（Task 4：OrderNotPayable、EcpayError）
api/src/app.rs                          Modify（Task 6：merge ecpay_payment）
api/src/routes/mod.rs                   Modify（Task 6）
api/src/routes/orders.rs                Modify（Task 4 create 回 ecpay、Task 7 repay）
api/src/routes/ecpay_payment.rs         Create（Task 6）
api/src/domain/mod.rs                   Modify（Task 2 invoices、Task 6 payments）
api/src/domain/jobs.rs                  Modify（Task 2：KIND_ISSUE_INVOICE）
api/src/domain/orders.rs                Modify（Task 2：get_detail、OrderDetail.invoice、PaymentRow 欄位、invoices pending、逐列歸還）
api/src/domain/invoices.rs              Create（Task 2；Task 10 加開立相關函式）
api/src/domain/payments.rs              Create（Task 6；Task 7 加 create_repayment）
api/src/ecpay/mod.rs                    Create（Task 3；Task 4、10 加 mod）
api/src/ecpay/mac.rs                    Create（Task 3）
api/src/ecpay/time.rs                   Create（Task 3）
api/src/ecpay/aio.rs                    Create（Task 4；Task 6 加回呼解析）
api/src/ecpay/aes.rs                    Create（Task 10）
api/src/ecpay/invoice.rs                Create（Task 10）
api/src/jobs/mod.rs                     Create（Task 8）
api/src/jobs/worker.rs                  Create（Task 8）
api/src/jobs/scheduled.rs               Create（Task 8）
api/src/jobs/handlers.rs                Create（Task 8 骨架；Task 9 send_email；Task 10 issue_invoice）
api/src/mail/mod.rs                     Create（Task 9）
api/src/mail/templates.rs               Create（Task 9）
api/templates/mail/*.txt|*.html         Create（Task 9，13 個檔）
api/tests/common/mod.rs                 Modify（Task 1、9、10）
api/tests/orders_domain.rs              Modify（Task 2）
api/tests/orders.rs                     Modify（Task 4）
api/tests/ecpay_payment.rs              Create（Task 6）
api/tests/repay.rs                      Create（Task 7）
api/tests/jobs_worker.rs                Create（Task 8）
api/tests/mail_jobs.rs                  Create（Task 9）
api/tests/invoice_job.rs                Create（Task 10）
web/src/lib/types.ts                    Modify（Task 5、11）
web/src/lib/ecpay.ts                    Create（Task 5）
web/src/lib/orderPage.ts                Create（Task 11）
web/src/lib/orderPage.test.ts           Create（Task 11）
web/src/routes/checkout/+page.svelte    Modify（Task 5）
web/src/routes/orders/[id]/+page.svelte Modify（Task 11）
web/e2e/checkout.spec.ts                Modify（Task 5）
.env.example                            Modify（Task 1）
docs/dev/ecpay-stage.md                 Create（Task 12）
```

## 任務總覽

| # | 任務 | 主要產出 |
|---|---|---|
| 1 | 設定：綠界憑證、SMTP、`Config::for_tests` | `config.rs`、`tests/common`、`.env.example` |
| 2 | orders 領域準備：`invoices` 表、`get_detail`、`OrderDetail.invoice`、逐列歸還庫存 | `0003_invoices.sql`、`domain/invoices.rs`、`domain/orders.rs` |
| 3 | `ecpay::mac`（CheckMacValue）與 `ecpay::time` | 純函式 + 文件向量測試 |
| 4 | `ecpay::aio::checkout_form`；`POST /api/orders` 回 `ecpay`；新錯誤碼 | `aio.rs`、`routes/orders.rs`、`error.rs` |
| 5 | 前端結帳改 POST 綠界；Playwright 攔截綠界表單 | `ecpay.ts`、`checkout/+page.svelte`、`e2e` |
| 6 | 回呼：`ReturnURL`／`PaymentInfoURL`；`domain/payments.rs` | `routes/ecpay_payment.rs`、`aio.rs` 解析 |
| 7 | `POST /api/orders/{id}/repay` | `payments::create_repayment`、route |
| 8 | jobs worker 核心 + 排程工作（過期、自動完成、清理） | `jobs/{worker,scheduled,handlers}.rs` |
| 9 | Email：`Mailer`、askama 模板、`send_email` handler、worker 隨 api 啟動 | `mail/`、`templates/`、`main.rs` |
| 10 | 電子發票：`aes.rs`、`invoice.rs`、`issue_invoice` handler | `ecpay/{aes,invoice}.rs`、`domain/invoices.rs` |
| 11 | 訂單頁：繳費資訊、輪詢、重新付款、已付款、發票號碼 | `orders/[id]/+page.svelte`、`orderPage.ts` |
| 12 | 文件與整體驗證 | `docs/dev/ecpay-stage.md`、全套測試 |

---
### Task 1: 設定：綠界憑證、SMTP、`Config::for_tests`

**Files:**
- Modify: `api/src/config.rs`（整檔改寫，內容如下）
- Modify: `api/tests/common/mod.rs:19-31`（`state()` 改用 `Config::for_tests`）
- Modify: `.env.example:19-36`

**Interfaces:**
- Consumes: 現有 `Config { database_url, public_base_url, cookie_secure, upload_dir }`。
- Produces（後面所有任務都用）：
  - `pub struct Config { …既有欄位…, pub ecpay: EcpayConfig, pub smtp: Option<SmtpConfig> }`
  - `pub enum EcpayEnv { Stage, Prod }`（Copy）
  - `pub struct EcpayCredentials { pub merchant_id: String, pub hash_key: String, pub hash_iv: String }`（Debug 遮蔽 key/iv）
  - `pub struct EcpayConfig { pub env: EcpayEnv, pub aio: EcpayCredentials, pub invoice: EcpayCredentials }` 與方法 `aio_checkout_url(&self) -> &'static str`、`invoice_issue_url(&self) -> &'static str`
  - `pub struct SmtpConfig { pub host: String, pub port: u16, pub user: Option<String>, pub pass: Option<String>, pub from: String }`
  - `pub const STAGE_AIO: (&str, &str, &str)`、`pub const STAGE_INVOICE: (&str, &str, &str)`（merchant_id, hash_key, hash_iv）
  - `Config::for_tests(upload_dir: PathBuf) -> Config`（stage 憑證、`smtp: None`、`public_base_url = "http://localhost:5173"`、`cookie_secure = false`）

- [ ] **Step 1: 寫失敗的單元測試**

把 `api/src/config.rs` 最下面的 `#[cfg(test)] mod tests` 換成：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(base: &str) -> Config {
        Config {
            public_base_url: base.to_string(),
            ..Config::for_tests(PathBuf::from("/tmp"))
        }
    }

    #[test]
    fn public_origin_strips_path() {
        assert_eq!(
            cfg("https://shop.example.com/some/path").public_origin(),
            "https://shop.example.com"
        );
        assert_eq!(
            cfg("http://localhost:5173").public_origin(),
            "http://localhost:5173"
        );
    }

    #[test]
    fn debug_redacts_secrets() {
        let mut cfg = Config::for_tests(PathBuf::from("/tmp"));
        cfg.database_url = "postgres://u:dbpass@h/db".to_string();
        cfg.smtp = Some(SmtpConfig {
            host: "smtp.example.com".to_string(),
            port: 587,
            user: Some("mailer".to_string()),
            pass: Some("mailpass".to_string()),
            from: "shop@example.com".to_string(),
        });
        let text = format!("{cfg:?}");
        for secret in [
            "dbpass",
            "mailpass",
            STAGE_AIO.1,
            STAGE_AIO.2,
            STAGE_INVOICE.1,
            STAGE_INVOICE.2,
        ] {
            assert!(!text.contains(secret), "{secret} 出現在 Debug 輸出：{text}");
        }
        assert!(text.contains("3002607"));
        assert!(text.contains("2000132"));
        assert!(text.contains("smtp.example.com"));
        assert!(text.contains("mailer"));
    }

    #[test]
    fn urls_follow_env() {
        let mut cfg = Config::for_tests(PathBuf::from("/tmp"));
        assert_eq!(
            cfg.ecpay.aio_checkout_url(),
            "https://payment-stage.ecpay.com.tw/Cashier/AioCheckOut/V5"
        );
        assert_eq!(
            cfg.ecpay.invoice_issue_url(),
            "https://einvoice-stage.ecpay.com.tw/B2CInvoice/Issue"
        );
        cfg.ecpay.env = EcpayEnv::Prod;
        assert_eq!(
            cfg.ecpay.aio_checkout_url(),
            "https://payment.ecpay.com.tw/Cashier/AioCheckOut/V5"
        );
        assert_eq!(
            cfg.ecpay.invoice_issue_url(),
            "https://einvoice.ecpay.com.tw/B2CInvoice/Issue"
        );
    }
}
```

- [ ] **Step 2: 跑測試確認失敗**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && DATABASE_URL=postgres://dog_shop:dog_shop@localhost:5435/dog_shop cargo test --manifest-path api/Cargo.toml --lib config`
Expected: 編譯錯誤（`for_tests`、`SmtpConfig`、`STAGE_AIO` 不存在）。

- [ ] **Step 3: 改寫 `api/src/config.rs`（測試模組以外的部分）**

```rust
use std::path::PathBuf;

use anyhow::Context;

/// 從環境變數讀進來的設定。測試用 `Config::for_tests` 建構；欄位都是 pub。
#[derive(Clone)]
pub struct Config {
    pub database_url: String,
    /// 對外網址，例如 http://localhost:5173 或 https://shop.example.com（結尾不帶 /）
    pub public_base_url: String,
    /// cookie 是否加 Secure。開發環境（http）設 COOKIE_SECURE=false
    pub cookie_secure: bool,
    /// 圖片存放目錄
    pub upload_dir: PathBuf,
    /// 綠界（規格 §8）
    pub ecpay: EcpayConfig,
    /// SMTP；None 表示沒設定，Email 只記 log（與規格不同之處 22）
    pub smtp: Option<SmtpConfig>,
}

/// 手動實作：database_url 含 DB 密碼，不能被 {:?} 印出來（規格 §11）。
impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("database_url", &"<redacted>")
            .field("public_base_url", &self.public_base_url)
            .field("cookie_secure", &self.cookie_secure)
            .field("upload_dir", &self.upload_dir)
            .field("ecpay", &self.ecpay)
            .field("smtp", &self.smtp)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EcpayEnv {
    Stage,
    Prod,
}

/// 一組綠界憑證。HashKey / HashIV 不能被 {:?} 印出來（規格 §11）
#[derive(Clone)]
pub struct EcpayCredentials {
    pub merchant_id: String,
    pub hash_key: String,
    pub hash_iv: String,
}

impl std::fmt::Debug for EcpayCredentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EcpayCredentials")
            .field("merchant_id", &self.merchant_id)
            .field("hash_key", &"<redacted>")
            .field("hash_iv", &"<redacted>")
            .finish()
    }
}

#[derive(Clone, Debug)]
pub struct EcpayConfig {
    pub env: EcpayEnv,
    pub aio: EcpayCredentials,
    pub invoice: EcpayCredentials,
}

/// 全方位金流測試特店（規格 §8.5；公開資料，只能用於 stage）：(MerchantID, HashKey, HashIV)
pub const STAGE_AIO: (&str, &str, &str) = ("3002607", "pwFHCqoQZGmho4w6", "EkRm7iFT261dpevs");
/// 電子發票 B2C 測試特店（developers.ecpay.com.tw「測試介接資訊」）
pub const STAGE_INVOICE: (&str, &str, &str) = ("2000132", "ejCk326UnaZWKisg", "q9jcZX8Ib9LM8wYk");

impl EcpayConfig {
    pub fn aio_checkout_url(&self) -> &'static str {
        match self.env {
            EcpayEnv::Stage => "https://payment-stage.ecpay.com.tw/Cashier/AioCheckOut/V5",
            EcpayEnv::Prod => "https://payment.ecpay.com.tw/Cashier/AioCheckOut/V5",
        }
    }

    pub fn invoice_issue_url(&self) -> &'static str {
        match self.env {
            EcpayEnv::Stage => "https://einvoice-stage.ecpay.com.tw/B2CInvoice/Issue",
            EcpayEnv::Prod => "https://einvoice.ecpay.com.tw/B2CInvoice/Issue",
        }
    }
}

#[derive(Clone)]
pub struct SmtpConfig {
    pub host: String,
    /// 465 = 一開始就 TLS；其他（587、25）= STARTTLS
    pub port: u16,
    pub user: Option<String>,
    pub pass: Option<String>,
    /// 寄件人，例如 `狗狗商店 <no-reply@example.com>` 或純地址
    pub from: String,
}

impl std::fmt::Debug for SmtpConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SmtpConfig")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("user", &self.user)
            .field("pass", &self.pass.as_ref().map(|_| "<redacted>"))
            .field("from", &self.from)
            .finish()
    }
}

/// 讀環境變數，去頭尾空白，空字串當沒設
fn env_trimmed(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// 讀一組憑證：stage 時空值退回公開測試憑證；prod 時三個都必填（與規格不同之處 24）
fn credentials(
    prefix: &str,
    env: EcpayEnv,
    stage: (&str, &str, &str),
) -> anyhow::Result<EcpayCredentials> {
    let read = |suffix: &str, fallback: &str| -> anyhow::Result<String> {
        let name = format!("{prefix}_{suffix}");
        match (env_trimmed(&name), env) {
            (Some(value), _) => Ok(value),
            (None, EcpayEnv::Stage) => Ok(fallback.to_string()),
            (None, EcpayEnv::Prod) => anyhow::bail!("ECPAY_ENV=prod 時必須設定 {name}"),
        }
    };
    Ok(EcpayCredentials {
        merchant_id: read("MERCHANT_ID", stage.0)?,
        hash_key: read("HASH_KEY", stage.1)?,
        hash_iv: read("HASH_IV", stage.2)?,
    })
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let database_url = std::env::var("DATABASE_URL").context("缺少環境變數 DATABASE_URL")?;
        let public_base_url = std::env::var("PUBLIC_BASE_URL")
            .unwrap_or_else(|_| "http://localhost:5173".to_string())
            .trim_end_matches('/')
            .to_string();
        let cookie_secure = match std::env::var("COOKIE_SECURE") {
            Ok(value) => !matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "false" | "0" | "no"
            ),
            Err(_) => true,
        };
        let upload_dir =
            PathBuf::from(std::env::var("UPLOAD_DIR").unwrap_or_else(|_| "./uploads".to_string()));

        let ecpay_env = match env_trimmed("ECPAY_ENV").as_deref() {
            None | Some("stage") => EcpayEnv::Stage,
            Some("prod") => EcpayEnv::Prod,
            Some(other) => anyhow::bail!("ECPAY_ENV 只能是 stage 或 prod，收到 {other}"),
        };
        let ecpay = EcpayConfig {
            env: ecpay_env,
            aio: credentials("ECPAY_AIO", ecpay_env, STAGE_AIO)?,
            invoice: credentials("ECPAY_INVOICE", ecpay_env, STAGE_INVOICE)?,
        };

        let smtp = match env_trimmed("SMTP_HOST") {
            None => None,
            Some(host) => Some(SmtpConfig {
                host,
                port: env_trimmed("SMTP_PORT")
                    .map(|p| p.parse::<u16>().context("SMTP_PORT 要是 1～65535 的數字"))
                    .transpose()?
                    .unwrap_or(587),
                user: env_trimmed("SMTP_USER"),
                pass: env_trimmed("SMTP_PASS"),
                from: env_trimmed("SMTP_FROM").context("有 SMTP_HOST 就必須設定 SMTP_FROM")?,
            }),
        };

        Ok(Self {
            database_url,
            public_base_url,
            cookie_secure,
            upload_dir,
            ecpay,
            smtp,
        })
    }

    /// 測試用：stage 憑證、沒有 SMTP、對外網址 http://localhost:5173、cookie 不加 Secure
    pub fn for_tests(upload_dir: PathBuf) -> Self {
        Self {
            database_url: String::new(),
            public_base_url: "http://localhost:5173".to_string(),
            cookie_secure: false,
            upload_dir,
            ecpay: EcpayConfig {
                env: EcpayEnv::Stage,
                aio: EcpayCredentials {
                    merchant_id: STAGE_AIO.0.to_string(),
                    hash_key: STAGE_AIO.1.to_string(),
                    hash_iv: STAGE_AIO.2.to_string(),
                },
                invoice: EcpayCredentials {
                    merchant_id: STAGE_INVOICE.0.to_string(),
                    hash_key: STAGE_INVOICE.1.to_string(),
                    hash_iv: STAGE_INVOICE.2.to_string(),
                },
            },
            smtp: None,
        }
    }

    /// 只留 scheme://host[:port]，用來和瀏覽器送來的 Origin header 比對
    pub fn public_origin(&self) -> &str {
        let url = &self.public_base_url;
        let Some(scheme_end) = url.find("://") else {
            return url;
        };
        let rest = &url[scheme_end + 3..];
        match rest.find('/') {
            Some(i) => &url[..scheme_end + 3 + i],
            None => url,
        }
    }
}
```

- [ ] **Step 4: 改 `api/tests/common/mod.rs` 的 `state()`**

把第 19～31 行換成：

```rust
/// 每個測試一個獨立的上傳目錄，避免互相干擾；設定用 Config::for_tests（stage 憑證、沒有 SMTP）
pub fn state(pool: PgPool) -> AppState {
    let upload_dir = std::env::temp_dir().join(format!("dog_shop_test_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&upload_dir).unwrap();
    AppState {
        db: pool,
        config: Arc::new(Config::for_tests(upload_dir)),
    }
}
```

- [ ] **Step 5: 改 `.env.example` 的綠界與 SMTP 段落**

把第 19～36 行換成：

```
# ── 綠界（規格 §8）。ECPAY_ENV=stage 用測試環境；憑證空白時 stage 會退回下面這組公開測試憑證，prod 則必填 ──
# 測試環境的回呼（ReturnURL / PaymentInfoURL）要能從網際網路打到你的機器：
#   cloudflared tunnel --url http://localhost:5173  → 把 PUBLIC_BASE_URL 改成它印出的 https 網址再重啟 api
ECPAY_ENV=stage
ECPAY_AIO_MERCHANT_ID=3002607
ECPAY_AIO_HASH_KEY=pwFHCqoQZGmho4w6
ECPAY_AIO_HASH_IV=EkRm7iFT261dpevs
# 物流（計畫 4）
ECPAY_LOGISTICS_MERCHANT_ID=
ECPAY_LOGISTICS_HASH_KEY=
ECPAY_LOGISTICS_HASH_IV=
# 電子發票 B2C（測試特店 2000132；正式憑證從綠界廠商後台取得，只放伺服器的 .env）
ECPAY_INVOICE_MERCHANT_ID=2000132
ECPAY_INVOICE_HASH_KEY=ejCk326UnaZWKisg
ECPAY_INVOICE_HASH_IV=q9jcZX8Ib9LM8wYk

# ── SMTP（規格 §12）。SMTP_HOST 空白 = 不寄信、只記 log（開發用）；正式環境必填 ──
# 465 走 TLS，587/25 走 STARTTLS。任何供應商都行（Gmail 應用程式密碼、Resend、Mailgun…）
SMTP_HOST=
SMTP_PORT=587
SMTP_USER=
SMTP_PASS=
# 寄件人，例如：狗狗商店 <no-reply@example.com>
SMTP_FROM=
```

- [ ] **Step 6: 跑全部測試、clippy、fmt**

Run（背景、timeout 600000）: `export PATH="$HOME/.cargo/bin:$PATH" && cargo fmt --all --manifest-path api/Cargo.toml && DATABASE_URL=postgres://dog_shop:dog_shop@localhost:5435/dog_shop cargo test --manifest-path api/Cargo.toml && cargo clippy --all-targets --manifest-path api/Cargo.toml -- -D warnings`
Expected: 全綠（原本的 103 個加上 config 的 2 個新測試）；clippy 乾淨。

- [ ] **Step 7: 確認 api 還能啟動**

Run（背景）: `export PATH="$HOME/.cargo/bin:$PATH" && cargo run --manifest-path api/Cargo.toml`
Expected: log 出現 `api listening on http://0.0.0.0:8080`（根目錄 `.env` 的發票憑證是空的，會退回 stage 預設，不該報錯）。看到後用 `lsof -ti :8080` 取 pid、`kill <pid>` 停掉。

- [ ] **Step 8: Commit**

```bash
git add api/src/config.rs api/tests/common/mod.rs .env.example
git commit -m "feat(api): 設定加入綠界憑證（stage 預設）、SMTP、Config::for_tests"
```

---
### Task 2: orders 領域準備：`invoices` 表、`get_detail`、`OrderDetail.invoice`、逐列歸還庫存

**Files:**
- Create: `api/migrations/0003_invoices.sql`
- Create: `api/src/domain/invoices.rs`
- Modify: `api/src/domain/mod.rs`（加 `pub mod invoices;`）
- Modify: `api/src/domain/jobs.rs:4`（加 `KIND_ISSUE_INVOICE`）
- Modify: `api/src/domain/orders.rs`（`PaymentRow` 417-426、`OrderDetail` 428-435、`create_order` 664-674 之後、`get_for_viewer` 699-746、`cancel_in_tx` 798-804）
- Test: `api/tests/orders_domain.rs`（新增兩個測試）

**Interfaces:**
- Consumes: `orders::create_order`、`cancel_in_tx`、`get_for_viewer`（計畫 2）。
- Produces：
  - `domain::invoices::{STATUS_PENDING, STATUS_ISSUED, STATUS_FAILED}`、`InvoiceRow { status, invoice_no: Option<String>, invoice_date: Option<DateTime<Utc>>, random_number: Option<String> }`（Serialize + FromRow）、`insert_pending_in_tx(tx, order_id, order_no)`、`get_by_order(db, order_id) -> Result<Option<InvoiceRow>, sqlx::Error>`
  - `domain::jobs::KIND_ISSUE_INVOICE = "issue_invoice"`
  - `orders::get_detail(db, id) -> Result<Option<OrderDetail>, ApiError>`（不看權限；worker、綠界表單、回呼用）
  - `orders::PaymentRow` 多 `#[serde(skip)] pub id: Uuid` 與 `#[serde(skip)] pub merchant_trade_no: String`
  - `orders::OrderDetail` 多 `pub invoice: Option<InvoiceRow>`（JSON 多一個 `invoice` 欄位，可能是 null）
  - `cancel_in_tx` 行為不變（回 `Ok(bool)`），內部改逐列、依 `variant_id` 排序歸還。

- [ ] **Step 1: 寫失敗的整合測試**

在 `api/tests/orders_domain.rs` 檔尾加：

```rust
#[sqlx::test(migrations = "./migrations")]
async fn create_order_inserts_pending_invoice_and_detail_has_it(pool: PgPool) {
    let (variant, _) = common::active_product(&pool, "A", 100, 5).await;
    let created = orders::create_order(&pool, input(vec![(variant, 1)], "home", None), None)
        .await
        .unwrap();
    let (status, relate): (String, String) =
        sqlx::query_as("SELECT status, relate_number FROM invoices WHERE order_id = $1")
            .bind(created.order_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        (status.as_str(), relate.as_str()),
        ("pending", created.order_no.as_str())
    );

    let detail = orders::get_detail(&pool, created.order_id)
        .await
        .unwrap()
        .expect("get_detail 不看權限");
    assert_eq!(detail.invoice.as_ref().unwrap().status, "pending");
    assert!(detail.invoice.as_ref().unwrap().invoice_no.is_none());
    assert_eq!(
        detail.payment.as_ref().unwrap().merchant_trade_no,
        format!("{}01", created.order_no)
    );
    assert!(
        orders::get_detail(&pool, Uuid::now_v7())
            .await
            .unwrap()
            .is_none()
    );

    // get_for_viewer 與 get_detail 回同一份資料（含 invoice）
    let viewed = orders::get_for_viewer(
        &pool,
        created.order_id,
        &Viewer::Guest(created.guest_token.clone()),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(viewed.invoice.unwrap().status, "pending");
}

#[sqlx::test(migrations = "./migrations")]
async fn cancel_restores_each_variant(pool: PgPool) {
    let (a, _) = common::active_product(&pool, "A", 100, 5).await;
    let (b, _) = common::active_product(&pool, "B", 100, 7).await;
    let created = orders::create_order(&pool, input(vec![(a, 2), (b, 3)], "home", None), None)
        .await
        .unwrap();
    assert_eq!((stock_of(&pool, a).await, stock_of(&pool, b).await), (3, 4));

    let mut tx = pool.begin().await.unwrap();
    assert!(
        orders::cancel_in_tx(&mut tx, created.order_id, "expired")
            .await
            .unwrap()
    );
    tx.commit().await.unwrap();
    assert_eq!((stock_of(&pool, a).await, stock_of(&pool, b).await), (5, 7));
    let (status, reason): (String, Option<String>) =
        sqlx::query_as("SELECT status, cancel_reason FROM orders WHERE id = $1")
            .bind(created.order_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        (status.as_str(), reason.as_deref()),
        ("cancelled", Some("expired"))
    );

    // 第二次取消不成功、庫存不會再加
    let mut tx = pool.begin().await.unwrap();
    assert!(
        !orders::cancel_in_tx(&mut tx, created.order_id, "expired")
            .await
            .unwrap()
    );
    tx.commit().await.unwrap();
    assert_eq!((stock_of(&pool, a).await, stock_of(&pool, b).await), (5, 7));
}
```

- [ ] **Step 2: 跑測試確認失敗**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && DATABASE_URL=postgres://dog_shop:dog_shop@localhost:5435/dog_shop cargo test --manifest-path api/Cargo.toml --test orders_domain`
Expected: 編譯錯誤（`get_detail` 不存在、`invoice` 欄位不存在、`merchant_trade_no` 不存在）。

- [ ] **Step 3: 建 migration `api/migrations/0003_invoices.sql`**

```sql
-- 電子發票（規格 §3）。下單時建 pending 一列；付款成功後由 issue_invoice job 開立
CREATE TABLE invoices (
    id            uuid PRIMARY KEY,
    order_id      uuid NOT NULL UNIQUE REFERENCES orders(id) ON DELETE CASCADE,
    status        text NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'issued', 'failed')),
    relate_number text NOT NULL UNIQUE,
    invoice_no    text,
    invoice_date  timestamptz,
    random_number text,
    request       jsonb,
    response      jsonb,
    error         text,
    created_at    timestamptz NOT NULL DEFAULT now(),
    updated_at    timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX invoices_status_idx ON invoices (status);

-- 過期掃描只看待付款訂單（規格 §5）
CREATE INDEX orders_pending_created_idx ON orders (created_at) WHERE status = 'pending_payment';
```

- [ ] **Step 4: 建 `api/src/domain/invoices.rs`**

```rust
//! invoices 表（規格 §3、§8.4）。本任務只有 pending 列與查詢；開立相關的函式在 Task 10 補
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

pub const STATUS_PENDING: &str = "pending";
pub const STATUS_ISSUED: &str = "issued";
pub const STATUS_FAILED: &str = "failed";

/// 給訂單頁看的發票狀態（規格 §3 invoices 的子集合）
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct InvoiceRow {
    pub status: String,
    pub invoice_no: Option<String>,
    pub invoice_date: Option<DateTime<Utc>>,
    pub random_number: Option<String>,
}

/// 下單交易內建一列 pending（規格 §7 第 6 點）；RelateNumber = order_no（規格 §8.4）
pub async fn insert_pending_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    order_id: Uuid,
    order_no: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO invoices (id, order_id, relate_number) VALUES ($1, $2, $3)")
        .bind(Uuid::now_v7())
        .bind(order_id)
        .bind(order_no)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

pub async fn get_by_order(db: &PgPool, order_id: Uuid) -> Result<Option<InvoiceRow>, sqlx::Error> {
    sqlx::query_as::<_, InvoiceRow>(
        "SELECT status, invoice_no, invoice_date, random_number FROM invoices WHERE order_id = $1",
    )
    .bind(order_id)
    .fetch_optional(db)
    .await
}
```

在 `api/src/domain/mod.rs` 的 `pub mod cvs_stores;` 後面加一行 `pub mod invoices;`（保持字母順序）。

- [ ] **Step 5: `api/src/domain/jobs.rs` 加 job 種類常數**

在 `pub const KIND_SEND_EMAIL: &str = "send_email";` 下面加：

```rust
pub const KIND_ISSUE_INVOICE: &str = "issue_invoice";
```

- [ ] **Step 6: 改 `api/src/domain/orders.rs`**

(a) `use` 區加一行（放在 `use crate::domain::cvs_stores::...` 後面）：

```rust
use crate::domain::invoices::{self, InvoiceRow};
```

(b) `PaymentRow`（原 417-426 行）改成：

```rust
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct PaymentRow {
    /// 給 send_email:payment_instructions 指定哪一筆；不回給前端
    #[serde(skip)]
    pub id: Uuid,
    /// 送綠界用；不回給前端
    #[serde(skip)]
    pub merchant_trade_no: String,
    pub method: String,
    pub status: String,
    pub amount: i32,
    pub atm_bank_code: Option<String>,
    pub atm_vaccount: Option<String>,
    pub cvs_payment_no: Option<String>,
    pub expire_at: Option<DateTime<Utc>>,
}
```

(c) `OrderDetail`（原 428-435 行）改成：

```rust
#[derive(Debug, Serialize)]
pub struct OrderDetail {
    #[serde(flatten)]
    pub order: OrderRow,
    pub items: Vec<OrderItemRow>,
    pub shipment: Option<ShipmentRow>,
    /// 最新一筆付款嘗試（created_at DESC, id DESC）
    pub payment: Option<PaymentRow>,
    pub invoice: Option<InvoiceRow>,
}
```

(d) `create_order` 裡 payments 的 INSERT（原 664-674 行）之後、`jobs::enqueue(` 之前加：

```rust
    // 發票先建 pending 一列（規格 §7 第 6 點）；付款成功後由 issue_invoice job 開立
    invoices::insert_pending_in_tx(&mut tx, order_id, &order_no).await?;
```

(e) 把 `get_for_viewer`（原 699-746 行）整段換成：

```rust
/// 會員看自己的、訪客用 guest_token；訪客 token 只對訪客訂單有效；都不符回 None（→ 404，不用 403 免得被猜 id）
pub async fn get_for_viewer(
    db: &PgPool,
    id: Uuid,
    viewer: &Viewer,
) -> Result<Option<OrderDetail>, ApiError> {
    let visible: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM orders
                        WHERE id = $1 AND ((user_id IS NOT NULL AND user_id = $2) OR ($3::text IS NOT NULL AND user_id IS NULL AND guest_token = $3)))",
    )
    .bind(id)
    .bind(viewer.user_id())
    .bind(viewer.token())
    .fetch_one(db)
    .await?;
    if !visible {
        return Ok(None);
    }
    get_detail(db, id).await
}

/// 不看權限的完整訂單（worker、綠界表單、回呼用）。呼叫者要自己確認能不能給人看
pub async fn get_detail(db: &PgPool, id: Uuid) -> Result<Option<OrderDetail>, ApiError> {
    let sql = format!("SELECT {ORDER_COLUMNS} FROM orders WHERE id = $1");
    let Some(order) = sqlx::query_as::<_, OrderRow>(sqlx::AssertSqlSafe(sql))
        .bind(id)
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
        "SELECT id, merchant_trade_no, method, status, amount, atm_bank_code, atm_vaccount, cvs_payment_no, expire_at
         FROM payments WHERE order_id = $1 ORDER BY created_at DESC, id DESC LIMIT 1",
    )
    .bind(id)
    .fetch_optional(db)
    .await?;
    let invoice = invoices::get_by_order(db, id).await?;
    Ok(Some(OrderDetail {
        order,
        items,
        shipment,
        payment,
        invoice,
    }))
}
```

(f) `cancel_in_tx` 裡歸還庫存的那句（原 798-804 行）換成：

```rust
    // 逐列、依 variant_id 排序加回去：和 create_order 的鎖定順序一致，
    // 批次過期取消（計畫 3）與同時下單不會互相死鎖（計畫 2 審查 Minor 4）
    let items: Vec<(Uuid, i32)> = sqlx::query_as(
        "SELECT variant_id, quantity FROM order_items WHERE order_id = $1 ORDER BY variant_id",
    )
    .bind(order_id)
    .fetch_all(&mut **tx)
    .await?;
    for (variant_id, quantity) in items {
        sqlx::query("UPDATE product_variants SET stock = stock + $2 WHERE id = $1")
            .bind(variant_id)
            .bind(quantity)
            .execute(&mut **tx)
            .await?;
    }
```

- [ ] **Step 7: 跑測試、clippy、fmt**

Run（背景）: `export PATH="$HOME/.cargo/bin:$PATH" && cargo fmt --all --manifest-path api/Cargo.toml && DATABASE_URL=postgres://dog_shop:dog_shop@localhost:5435/dog_shop cargo test --manifest-path api/Cargo.toml && cargo clippy --all-targets --manifest-path api/Cargo.toml -- -D warnings`
Expected: 全綠（含新增 2 個）。既有 `api/tests/orders.rs` 的 `guest_checkout_view_and_cancel` 仍然過（回應多了 `invoice` 欄位，沒有 `guest_token`／`user_id`／`merchant_trade_no`）。

- [ ] **Step 8: Commit**

```bash
git add api/migrations/0003_invoices.sql api/src/domain/invoices.rs api/src/domain/mod.rs api/src/domain/jobs.rs api/src/domain/orders.rs api/tests/orders_domain.rs
git commit -m "feat(api): invoices 表與下單時的 pending 列、orders::get_detail、取消時逐列歸還庫存"
```

---
### Task 3: `ecpay::mac`（CheckMacValue）與 `ecpay::time`

**Files:**
- Create: `api/src/ecpay/mod.rs`
- Create: `api/src/ecpay/mac.rs`
- Create: `api/src/ecpay/time.rs`
- Modify: `api/src/lib.rs`（加 `pub mod ecpay;`）

**Interfaces:**
- Consumes: 既有 crate `sha2`、`hex`、`chrono`。
- Produces：
  - `ecpay::mac::dotnet_url_encode(&str) -> String`（.NET `HttpUtility.UrlEncode` 規則；發票的 Data 編碼也用它）
  - `ecpay::mac::raw_string(hash_key, hash_iv, params: &[(String, String)]) -> String`（排序、串接、編碼、小寫，尚未雜湊）
  - `ecpay::mac::check_mac_value(hash_key, hash_iv, params) -> String`（大寫 SHA256 hex）
  - `ecpay::mac::verify(hash_key, hash_iv, params) -> bool`（params 內要含 `CheckMacValue`）
  - `ecpay::time::{taipei() -> FixedOffset, format_datetime(DateTime<Utc>) -> String, parse_taipei(&str, fmt) -> Option<DateTime<Utc>>, parse_datetime(&str), parse_date_end_of_day(&str), parse_expire(&str), DATETIME_FMT, DATE_FMT}`

- [ ] **Step 1: 建 `api/src/ecpay/mod.rs`**

```rust
//! 綠界整合（規格 §8）。mac：CheckMacValue；time：台北時間字串；
//! aio：全方位金流（Task 4、6）；aes／invoice：電子發票（Task 10）
pub mod mac;
pub mod time;
```

在 `api/src/lib.rs` 的 `pub mod domain;` 後面加 `pub mod ecpay;`（字母順序）。

- [ ] **Step 2: 建 `api/src/ecpay/mac.rs`（含失敗測試；先整檔建好再跑）**

```rust
//! CheckMacValue（規格 §8.1）：參數依 key 排序（不分大小寫）→ `HashKey=…&k=v&…&HashIV=…`
//! → .NET 風格 URL encode → 轉小寫 → SHA256 → 轉大寫。物流用的 MD5 版本在計畫 4。
use sha2::{Digest, Sha256};

/// .NET `HttpUtility.UrlEncode` 的規則：`A-Z a-z 0-9 - _ . ! * ( )` 原樣、空白變 `+`、
/// 其他每個 byte 變 `%xx`（小寫 hex）。綠界的替換表（`%2d→-`、`%5f→_`、`%2e→.`、`%21→!`、
/// `%2a→*`、`%28→(`、`%29→)`）是給會把這些字元編碼的語言用的，這裡本來就不編碼它們。
pub fn dotnet_url_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len() * 3);
    for b in input.bytes() {
        match b {
            b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'-'
            | b'_'
            | b'.'
            | b'!'
            | b'*'
            | b'('
            | b')' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02x}")),
        }
    }
    out
}

/// 排序、串接、編碼、轉小寫後（還沒雜湊）的字串；拆出來方便對照文件範例
pub fn raw_string(hash_key: &str, hash_iv: &str, params: &[(String, String)]) -> String {
    let mut sorted: Vec<&(String, String)> = params.iter().collect();
    sorted.sort_by_key(|(k, _)| k.to_ascii_lowercase());
    let mut joined = format!("HashKey={hash_key}");
    for (k, v) in sorted {
        joined.push('&');
        joined.push_str(k);
        joined.push('=');
        joined.push_str(v);
    }
    joined.push_str("&HashIV=");
    joined.push_str(hash_iv);
    dotnet_url_encode(&joined).to_lowercase()
}

/// SHA256（EncryptType=1）的 CheckMacValue，大寫 hex
pub fn check_mac_value(hash_key: &str, hash_iv: &str, params: &[(String, String)]) -> String {
    let digest = Sha256::digest(raw_string(hash_key, hash_iv, params).as_bytes());
    hex::encode_upper(digest)
}

/// 驗回呼：取出 `CheckMacValue`（key 不分大小寫），用其餘欄位重算再比對（值不分大小寫）
pub fn verify(hash_key: &str, hash_iv: &str, params: &[(String, String)]) -> bool {
    let Some((_, given)) = params
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("CheckMacValue"))
    else {
        return false;
    };
    let rest: Vec<(String, String)> = params
        .iter()
        .filter(|(k, _)| !k.eq_ignore_ascii_case("CheckMacValue"))
        .cloned()
        .collect();
    check_mac_value(hash_key, hash_iv, &rest).eq_ignore_ascii_case(given)
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "pwFHCqoQZGmho4w6";
    const IV: &str = "EkRm7iFT261dpevs";

    /// developers.ecpay.com.tw 全方位金流「檢查碼機制」的範例參數
    fn doc_params() -> Vec<(String, String)> {
        [
            ("TradeDesc", "促銷方案"),
            ("PaymentType", "aio"),
            ("MerchantTradeDate", "2023/03/12 15:30:23"),
            ("MerchantTradeNo", "ecpay20230312153023"),
            ("MerchantID", "3002607"),
            ("ReturnURL", "https://www.ecpay.com.tw/receive.php"),
            ("ItemName", "Apple iphone 15"),
            ("TotalAmount", "30000"),
            ("ChoosePayment", "ALL"),
            ("EncryptType", "1"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
    }

    #[test]
    fn url_encode_is_dotnet_style() {
        assert_eq!(dotnet_url_encode("Apple iphone 15"), "Apple+iphone+15");
        assert_eq!(
            dotnet_url_encode("2023/03/12 15:30:23"),
            "2023%2f03%2f12+15%3a30%3a23"
        );
        assert_eq!(dotnet_url_encode("a-b_c.d!e*f(g)"), "a-b_c.d!e*f(g)");
        assert_eq!(
            dotnet_url_encode("促銷方案"),
            "%e4%bf%83%e9%8a%b7%e6%96%b9%e6%a1%88"
        );
        assert_eq!(dotnet_url_encode("a&b=c#d"), "a%26b%3dc%23d");
    }

    #[test]
    fn raw_string_matches_doc() {
        assert_eq!(
            raw_string(KEY, IV, &doc_params()),
            "hashkey%3dpwfhcqoqzgmho4w6%26choosepayment%3dall%26encrypttype%3d1%26itemname%3dapple+iphone+15%26merchantid%3d3002607%26merchanttradedate%3d2023%2f03%2f12+15%3a30%3a23%26merchanttradeno%3decpay20230312153023%26paymenttype%3daio%26returnurl%3dhttps%3a%2f%2fwww.ecpay.com.tw%2freceive.php%26totalamount%3d30000%26tradedesc%3d%e4%bf%83%e9%8a%b7%e6%96%b9%e6%a1%88%26hashiv%3dekrm7ift261dpevs"
        );
    }

    #[test]
    fn check_mac_value_matches_doc() {
        assert_eq!(
            check_mac_value(KEY, IV, &doc_params()),
            "6C51C9E6888DE861FD62FB1DD17029FC742634498FD813DC43D4243B5685B840"
        );
    }

    #[test]
    fn verify_accepts_correct_and_rejects_tampered() {
        let mut params = doc_params();
        // 綠界回傳的值是大寫，但比對不分大小寫
        params.push((
            "CheckMacValue".to_string(),
            "6c51c9e6888de861fd62fb1dd17029fc742634498fd813dc43d4243b5685b840".to_string(),
        ));
        assert!(verify(KEY, IV, &params));

        // TotalAmount 是 doc_params 的第 8 個
        params[7].1 = "30001".to_string();
        assert!(!verify(KEY, IV, &params), "改了金額就不能過");

        assert!(!verify(KEY, IV, &doc_params()), "沒有 CheckMacValue 不能過");
        assert!(!verify("wrongkey", IV, &doc_params()), "key 不對不能過");
    }
}
```

- [ ] **Step 3: 建 `api/src/ecpay/time.rs`（含測試）**

```rust
//! 綠界的日期字串一律台北時間（UTC+8）；DB 一律 UTC（規格 §8）
use chrono::{DateTime, FixedOffset, NaiveDate, NaiveDateTime, NaiveTime, Utc};

/// `MerchantTradeDate`、`PaymentDate`、超商 `ExpireDate` 的格式
pub const DATETIME_FMT: &str = "%Y/%m/%d %H:%M:%S";
/// ATM `ExpireDate` 的格式（只有日期）
pub const DATE_FMT: &str = "%Y/%m/%d";

pub fn taipei() -> FixedOffset {
    FixedOffset::east_opt(8 * 3600).expect("UTC+8 是合法的時區位移")
}

/// UTC → `yyyy/MM/dd HH:mm:ss`（台北）
pub fn format_datetime(at: DateTime<Utc>) -> String {
    at.with_timezone(&taipei()).format(DATETIME_FMT).to_string()
}

/// 用指定格式把台北時間字串轉成 UTC；格式不符回 None。發票的 InvoiceDate 是 `%Y-%m-%d %H:%M:%S`
pub fn parse_taipei(s: &str, fmt: &str) -> Option<DateTime<Utc>> {
    let naive = NaiveDateTime::parse_from_str(s.trim(), fmt).ok()?;
    naive
        .and_local_timezone(taipei())
        .single()
        .map(|dt| dt.with_timezone(&Utc))
}

/// `yyyy/MM/dd HH:mm:ss` → UTC
pub fn parse_datetime(s: &str) -> Option<DateTime<Utc>> {
    parse_taipei(s, DATETIME_FMT)
}

/// `yyyy/MM/dd` → 該日台北時間 23:59:59（ATM 的虛擬帳號到最後一天 23:59:59 有效，規格 §5）
pub fn parse_date_end_of_day(s: &str) -> Option<DateTime<Utc>> {
    let date = NaiveDate::parse_from_str(s.trim(), DATE_FMT).ok()?;
    let naive = date.and_time(NaiveTime::from_hms_opt(23, 59, 59)?);
    naive
        .and_local_timezone(taipei())
        .single()
        .map(|dt| dt.with_timezone(&Utc))
}

/// 繳費期限：先試完整時間（超商），再試只有日期（ATM）
pub fn parse_expire(s: &str) -> Option<DateTime<Utc>> {
    parse_datetime(s).or_else(|| parse_date_end_of_day(s))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn formats_in_taipei() {
        let utc = Utc.with_ymd_and_hms(2023, 3, 12, 7, 30, 23).unwrap();
        assert_eq!(format_datetime(utc), "2023/03/12 15:30:23");
    }

    #[test]
    fn parses_back_to_utc() {
        assert_eq!(
            parse_datetime("2023/03/12 15:30:23"),
            Some(Utc.with_ymd_and_hms(2023, 3, 12, 7, 30, 23).unwrap())
        );
        assert_eq!(
            parse_date_end_of_day("2023/03/12"),
            Some(Utc.with_ymd_and_hms(2023, 3, 12, 15, 59, 59).unwrap())
        );
        assert_eq!(
            parse_expire("2023/03/12"),
            parse_date_end_of_day("2023/03/12")
        );
        assert_eq!(
            parse_expire(" 2023/03/12 08:00:00 "),
            Some(Utc.with_ymd_and_hms(2023, 3, 12, 0, 0, 0).unwrap())
        );
        assert_eq!(
            parse_taipei("2026-09-06 15:30:23", "%Y-%m-%d %H:%M:%S"),
            Some(Utc.with_ymd_and_hms(2026, 9, 6, 7, 30, 23).unwrap())
        );
        assert_eq!(parse_expire("not a date"), None);
        assert_eq!(parse_datetime("2023/03/12"), None);
    }
}
```

- [ ] **Step 4: 跑單元測試**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && DATABASE_URL=postgres://dog_shop:dog_shop@localhost:5435/dog_shop cargo test --manifest-path api/Cargo.toml --lib ecpay`
Expected: 6 個測試全過（`url_encode_is_dotnet_style`、`raw_string_matches_doc`、`check_mac_value_matches_doc`、`verify_accepts_correct_and_rejects_tampered`、`formats_in_taipei`、`parses_back_to_utc`）。若 `check_mac_value_matches_doc` 失敗，先看 `raw_string_matches_doc`：兩者都失敗是排序或編碼問題；只有前者失敗是雜湊或大小寫問題。

- [ ] **Step 5: clippy、fmt**

Run（背景）: `export PATH="$HOME/.cargo/bin:$PATH" && cargo fmt --all --manifest-path api/Cargo.toml && cargo clippy --all-targets --manifest-path api/Cargo.toml -- -D warnings`
Expected: 乾淨。

- [ ] **Step 6: Commit**

```bash
git add api/src/lib.rs api/src/ecpay/mod.rs api/src/ecpay/mac.rs api/src/ecpay/time.rs
git commit -m "feat(api): 綠界 CheckMacValue（SHA256，對照文件範例）與台北時間工具"
```

---
### Task 4: `ecpay::aio::checkout_form`；`POST /api/orders` 回 `ecpay`；新錯誤碼

**Files:**
- Modify: `api/src/error.rs`（enum 15-35、`code()` 53-65、`status()` 67-78、tests）
- Create: `api/src/ecpay/aio.rs`
- Modify: `api/src/ecpay/mod.rs`（加 `pub mod aio;`）
- Modify: `api/src/routes/orders.rs`（整檔改寫如下）
- Test: `api/tests/orders.rs`（新增兩個測試）

**Interfaces:**
- Consumes: `Config.ecpay`（Task 1）、`orders::get_detail`、`PaymentRow.merchant_trade_no`（Task 2）、`ecpay::mac`、`ecpay::time`（Task 3）、`settings::get_all(db).shop.name`。
- Produces：
  - `ApiError::OrderNotPayable`（`ORDER_NOT_PAYABLE`，400）、`ApiError::EcpayError(String)`（`ECPAY_ERROR`，502）
  - `ecpay::aio::CheckoutForm { action: String, fields: BTreeMap<String, String> }`（Serialize）
  - `ecpay::aio::checkout_form(cfg: &EcpayConfig, public_base_url: &str, shop_name: &str, order: &OrderDetail, payment: &PaymentRow, guest_token: Option<&str>, now: DateTime<Utc>) -> anyhow::Result<CheckoutForm>`
  - `ecpay::aio::choose_payment(method: &str) -> Option<&'static str>`、`ecpay::aio::item_name(&OrderDetail) -> String`
  - `POST /api/orders` → 201 `{ order_id, order_no, guest_token, ecpay: { action, fields } }`；`routes::orders::checkout_form_for(state, order_id, guest_token)`（私有，Task 7 的 repay 重用）

- [ ] **Step 1: 錯誤碼（`api/src/error.rs`）**

在 enum 的 `RateLimited,` 之後、`#[error(transparent)]` 之前加：

```rust
    /// 訂單不是待付款，不能（重新）付款（規格 §10）
    #[error("這筆訂單目前不能付款")]
    OrderNotPayable,
    /// 綠界同步呼叫失敗（規格 §10；計畫 4 的物流建單用，本計畫只定義）
    #[error("{0}")]
    EcpayError(String),
```

`code()` 的 match 加兩行（放在 `Self::RateLimited => "RATE_LIMITED",` 之後）：

```rust
            Self::OrderNotPayable => "ORDER_NOT_PAYABLE",
            Self::EcpayError(_) => "ECPAY_ERROR",
```

`status()` 的 match 加兩行（放在 `Self::RateLimited => StatusCode::TOO_MANY_REQUESTS,` 之後）：

```rust
            Self::OrderNotPayable => StatusCode::BAD_REQUEST,
            Self::EcpayError(_) => StatusCode::BAD_GATEWAY,
```

tests 模組加：

```rust
    #[tokio::test]
    async fn order_not_payable_and_ecpay_error_envelopes() {
        let response = ApiError::OrderNotPayable.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let v = body_json(response).await;
        assert_eq!(v["error"]["code"], "ORDER_NOT_PAYABLE");
        assert_eq!(v["error"]["message"], "這筆訂單目前不能付款");
        assert!(v["error"]["details"].is_null());

        let response = ApiError::EcpayError("綠界回應 0|Fail".to_string()).into_response();
        assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
        let v = body_json(response).await;
        assert_eq!(v["error"]["code"], "ECPAY_ERROR");
        assert_eq!(v["error"]["message"], "綠界回應 0|Fail");
    }
```

- [ ] **Step 2: 建 `api/src/ecpay/aio.rs`（含單元測試）**

```rust
//! 全方位金流（規格 §8.2）：建立付款表單。回呼解析在 Task 6 補在這個檔案下面。
use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::config::EcpayConfig;
use crate::domain::orders::{OrderDetail, PAYMENT_ATM, PAYMENT_CREDIT, PAYMENT_CVS_CODE, PaymentRow};
use crate::ecpay::{mac, time};

/// ATM 虛擬帳號幾天內有效（規格 §8.2）
pub const ATM_EXPIRE_DAYS: &str = "3";
/// 超商代碼幾分鐘內有效（規格 §8.2）
pub const CVS_STORE_EXPIRE_MINUTES: &str = "4320";
/// 綠界欄位長度上限
pub const TRADE_DESC_MAX: usize = 200;
pub const ITEM_NAME_MAX: usize = 400;

/// 給前端用隱藏表單 POST 的內容（規格 §7 第 6、7 點）
#[derive(Debug, Clone, Serialize)]
pub struct CheckoutForm {
    pub action: String,
    pub fields: BTreeMap<String, String>,
}

/// payments.method → ChoosePayment；cod 等不支援的回 None
pub fn choose_payment(method: &str) -> Option<&'static str> {
    match method {
        PAYMENT_CREDIT => Some("Credit"),
        PAYMENT_ATM => Some("ATM"),
        PAYMENT_CVS_CODE => Some("CVS"),
        _ => None,
    }
}

/// 取前 max 個字（不是 byte，中文不會切壞）
fn truncate_chars(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

/// 品項用 `#` 串接，超長截斷（規格 §8.2）；名稱裡的 `#` 換成空白，免得被綠界當成分隔
pub fn item_name(order: &OrderDetail) -> String {
    let joined = order
        .items
        .iter()
        .map(|i| {
            format!(
                "{}({}) x {}",
                i.product_name.replace('#', " "),
                i.variant_label.replace('#', " "),
                i.quantity
            )
        })
        .collect::<Vec<_>>()
        .join("#");
    truncate_chars(&joined, ITEM_NAME_MAX)
}

/// 組出送往 AioCheckOut/V5 的欄位（規格 §8.2 的清單，一律全帶）。`now` 由呼叫者傳入，測試才能固定。
/// `guest_token` 有值時 ClientBackURL 帶 `?t=`（訪客回到訂單頁要靠它，規格 §7 第 8 點）。
pub fn checkout_form(
    cfg: &EcpayConfig,
    public_base_url: &str,
    shop_name: &str,
    order: &OrderDetail,
    payment: &PaymentRow,
    guest_token: Option<&str>,
    now: DateTime<Utc>,
) -> anyhow::Result<CheckoutForm> {
    let choose = choose_payment(&payment.method)
        .ok_or_else(|| anyhow::anyhow!("付款方式 {} 不能送綠界", payment.method))?;
    let order_id = order.order.id;
    let mut back_url = format!("{public_base_url}/orders/{order_id}");
    if let Some(token) = guest_token {
        back_url.push_str("?t=");
        back_url.push_str(token);
    }

    let mut fields = BTreeMap::new();
    let mut put = |k: &str, v: String| {
        fields.insert(k.to_string(), v);
    };
    put("MerchantID", cfg.aio.merchant_id.clone());
    put("MerchantTradeNo", payment.merchant_trade_no.clone());
    put("MerchantTradeDate", time::format_datetime(now));
    put("PaymentType", "aio".to_string());
    put("TotalAmount", payment.amount.to_string());
    put(
        "TradeDesc",
        truncate_chars(
            &format!("{shop_name} 訂單 {}", order.order.order_no),
            TRADE_DESC_MAX,
        ),
    );
    put("ItemName", item_name(order));
    put("ReturnURL", format!("{public_base_url}/api/ecpay/payment/return"));
    put("ChoosePayment", choose.to_string());
    put("ClientBackURL", back_url);
    put(
        "PaymentInfoURL",
        format!("{public_base_url}/api/ecpay/payment/info"),
    );
    put("ExpireDate", ATM_EXPIRE_DAYS.to_string());
    put("StoreExpireDate", CVS_STORE_EXPIRE_MINUTES.to_string());
    put("NeedExtraPaidInfo", "N".to_string());
    put("EncryptType", "1".to_string());
    put("CustomField1", order_id.to_string());

    let params: Vec<(String, String)> = fields
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    let mac = mac::check_mac_value(&cfg.aio.hash_key, &cfg.aio.hash_iv, &params);
    fields.insert("CheckMacValue".to_string(), mac);
    Ok(CheckoutForm {
        action: cfg.aio_checkout_url().to_string(),
        fields,
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use chrono::TimeZone;
    use uuid::Uuid;

    use super::*;
    use crate::config::Config;
    use crate::domain::orders::{OrderItemRow, OrderRow};

    fn sample() -> (OrderDetail, PaymentRow) {
        let order = OrderRow {
            id: Uuid::parse_str("019a0000-0000-7000-8000-000000000001").unwrap(),
            order_no: "DS260906ABCD".to_string(),
            user_id: None,
            guest_token: "t".repeat(64),
            status: "pending_payment".to_string(),
            email: "a@b.co".to_string(),
            recipient_name: "王小明".to_string(),
            recipient_phone: "0912345678".to_string(),
            shipping_method: "home".to_string(),
            subtotal: 600,
            shipping_fee: 100,
            total: 700,
            note: String::new(),
            invoice_type: "personal".to_string(),
            invoice_carrier_type: Some("1".to_string()),
            invoice_carrier_num: None,
            invoice_tax_id: None,
            invoice_title: None,
            invoice_address: None,
            invoice_love_code: None,
            needs_refund: false,
            created_at: Utc::now(),
            paid_at: None,
            shipped_at: None,
            completed_at: None,
            cancelled_at: None,
            cancel_reason: None,
        };
        let items = vec![OrderItemRow {
            product_name: "雞肉狗糧 #1".to_string(),
            variant_label: "S".to_string(),
            unit_price: 300,
            quantity: 2,
            line_total: 600,
            image_path: None,
        }];
        let payment = PaymentRow {
            id: Uuid::now_v7(),
            merchant_trade_no: "DS260906ABCD01".to_string(),
            method: "credit".to_string(),
            status: "pending".to_string(),
            amount: 700,
            atm_bank_code: None,
            atm_vaccount: None,
            cvs_payment_no: None,
            expire_at: None,
        };
        (
            OrderDetail {
                order,
                items,
                shipment: None,
                payment: None,
                invoice: None,
            },
            payment,
        )
    }

    #[test]
    fn choose_payment_maps_methods() {
        assert_eq!(choose_payment("credit"), Some("Credit"));
        assert_eq!(choose_payment("atm"), Some("ATM"));
        assert_eq!(choose_payment("cvs_code"), Some("CVS"));
        assert_eq!(choose_payment("cod"), None);
    }

    #[test]
    fn item_name_joins_and_truncates() {
        let (mut order, _) = sample();
        assert_eq!(item_name(&order), "雞肉狗糧  1(S) x 2");
        order.items[0].product_name = "狗".repeat(500);
        assert_eq!(item_name(&order).chars().count(), 400);
    }

    #[test]
    fn checkout_form_has_every_field_and_valid_mac() {
        let cfg = Config::for_tests(PathBuf::from("/tmp")).ecpay;
        let (order, payment) = sample();
        let now = Utc.with_ymd_and_hms(2026, 9, 6, 7, 30, 23).unwrap();
        let form = checkout_form(
            &cfg,
            "https://shop.example.com",
            "狗狗商店",
            &order,
            &payment,
            Some("tok"),
            now,
        )
        .unwrap();
        assert_eq!(
            form.action,
            "https://payment-stage.ecpay.com.tw/Cashier/AioCheckOut/V5"
        );
        let f = &form.fields;
        assert_eq!(f["MerchantID"], "3002607");
        assert_eq!(f["MerchantTradeNo"], "DS260906ABCD01");
        assert_eq!(f["MerchantTradeDate"], "2026/09/06 15:30:23");
        assert_eq!(f["PaymentType"], "aio");
        assert_eq!(f["TotalAmount"], "700");
        assert_eq!(f["TradeDesc"], "狗狗商店 訂單 DS260906ABCD");
        assert_eq!(f["ItemName"], "雞肉狗糧  1(S) x 2");
        assert_eq!(f["ReturnURL"], "https://shop.example.com/api/ecpay/payment/return");
        assert_eq!(f["ChoosePayment"], "Credit");
        assert_eq!(
            f["ClientBackURL"],
            format!("https://shop.example.com/orders/{}?t=tok", order.order.id)
        );
        assert_eq!(f["PaymentInfoURL"], "https://shop.example.com/api/ecpay/payment/info");
        assert_eq!(f["ExpireDate"], "3");
        assert_eq!(f["StoreExpireDate"], "4320");
        assert_eq!(f["NeedExtraPaidInfo"], "N");
        assert_eq!(f["EncryptType"], "1");
        assert_eq!(f["CustomField1"], order.order.id.to_string());
        assert_eq!(f.len(), 17, "16 個欄位 + CheckMacValue");
        let params: Vec<(String, String)> =
            f.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        assert!(mac::verify(&cfg.aio.hash_key, &cfg.aio.hash_iv, &params));

        // 會員：ClientBackURL 不帶 ?t=
        let form = checkout_form(
            &cfg,
            "https://shop.example.com",
            "狗狗商店",
            &order,
            &payment,
            None,
            now,
        )
        .unwrap();
        assert_eq!(
            form.fields["ClientBackURL"],
            format!("https://shop.example.com/orders/{}", order.order.id)
        );

        // 不能送綠界的付款方式
        let (_, mut cod) = sample();
        cod.method = "cod".to_string();
        assert!(
            checkout_form(&cfg, "https://shop.example.com", "狗狗商店", &order, &cod, None, now)
                .is_err()
        );
    }
}
```

在 `api/src/ecpay/mod.rs` 加 `pub mod aio;`（放在 `pub mod mac;` 前面，字母順序）。

- [ ] **Step 3: 跑單元測試確認 aio、error 通過**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && DATABASE_URL=postgres://dog_shop:dog_shop@localhost:5435/dog_shop cargo test --manifest-path api/Cargo.toml --lib`
Expected: 全過（新增 `choose_payment_maps_methods`、`item_name_joins_and_truncates`、`checkout_form_has_every_field_and_valid_mac`、`order_not_payable_and_ecpay_error_envelopes`）。

- [ ] **Step 4: 寫失敗的整合測試（`api/tests/orders.rs` 檔尾）**

```rust
#[sqlx::test(migrations = "./migrations")]
async fn create_returns_ecpay_form_with_valid_mac(pool: PgPool) {
    let app = common::app(pool.clone());
    let (variant, _) = common::active_product(&pool, "雞肉狗糧", 300, 5).await;
    let mut body = order_body(&variant.to_string(), 2, "home", None);
    body["payment_method"] = json!("atm");
    let (status, created, _) =
        common::send(&app, common::req("POST", "/api/orders", None, Some(body))).await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let order_id = created["order_id"].as_str().unwrap();
    let order_no = created["order_no"].as_str().unwrap();
    let token = created["guest_token"].as_str().unwrap();

    assert_eq!(
        created["ecpay"]["action"],
        "https://payment-stage.ecpay.com.tw/Cashier/AioCheckOut/V5"
    );
    let fields = created["ecpay"]["fields"].as_object().unwrap();
    assert_eq!(fields["MerchantID"], "3002607");
    assert_eq!(fields["MerchantTradeNo"], format!("{order_no}01"));
    assert_eq!(fields["TotalAmount"], "700", "2 × 300 + 宅配 100");
    assert_eq!(fields["ChoosePayment"], "ATM");
    assert_eq!(fields["PaymentType"], "aio");
    assert_eq!(fields["EncryptType"], "1");
    assert_eq!(fields["ExpireDate"], "3");
    assert_eq!(fields["StoreExpireDate"], "4320");
    assert_eq!(fields["NeedExtraPaidInfo"], "N");
    assert_eq!(fields["CustomField1"], order_id);
    assert_eq!(
        fields["ReturnURL"],
        format!("{}/api/ecpay/payment/return", common::TEST_ORIGIN)
    );
    assert_eq!(
        fields["PaymentInfoURL"],
        format!("{}/api/ecpay/payment/info", common::TEST_ORIGIN)
    );
    assert_eq!(
        fields["ClientBackURL"],
        format!("{}/orders/{order_id}?t={token}", common::TEST_ORIGIN)
    );
    assert!(
        fields["ItemName"]
            .as_str()
            .unwrap()
            .starts_with("雞肉狗糧(預設) x 2")
    );
    assert_eq!(fields["MerchantTradeDate"].as_str().unwrap().len(), 19);
    assert_eq!(fields.len(), 17);
    let params: Vec<(String, String)> = fields
        .iter()
        .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_string()))
        .collect();
    assert!(dog_shop_api::ecpay::mac::verify(
        "pwFHCqoQZGmho4w6",
        "EkRm7iFT261dpevs",
        &params
    ));
}

#[sqlx::test(migrations = "./migrations")]
async fn member_checkout_back_url_has_no_token(pool: PgPool) {
    let app = common::app(pool.clone());
    let cookie = common::register_cookie(&app, "m2@test.local", "password123", "甲").await;
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
    let id = created["order_id"].as_str().unwrap();
    assert_eq!(
        created["ecpay"]["fields"]["ClientBackURL"],
        format!("{}/orders/{id}", common::TEST_ORIGIN)
    );
    assert_eq!(created["ecpay"]["fields"]["ChoosePayment"], "Credit");
}
```

Run: `export PATH="$HOME/.cargo/bin:$PATH" && DATABASE_URL=postgres://dog_shop:dog_shop@localhost:5435/dog_shop cargo test --manifest-path api/Cargo.toml --test orders`
Expected: 兩個新測試失敗（回應沒有 `ecpay`）。

- [ ] **Step 5: 改寫 `api/src/routes/orders.rs`（整檔）**

```rust
use std::{sync::Arc, time::Duration};

use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tower_governor::{
    GovernorError, GovernorLayer, governor::GovernorConfigBuilder,
    key_extractor::SmartIpKeyExtractor,
};
use uuid::Uuid;

use crate::{
    auth::extract::CurrentUser,
    domain::{
        orders::{self, OrderCreated, OrderDetail, OrderInput, Viewer},
        settings,
        users::User,
    },
    ecpay::aio::{self, CheckoutForm},
    error::{ApiError, ApiResult},
    extract::{AppJson, AppPath, AppQuery},
    state::AppState,
};

/// `POST /api/orders` 是唯一開放給匿名者的寫入端點，未登入就能扣庫存；掛一個獨立的
/// GovernorLayer 當減速帶（不與 auth 共用配額）：每個 IP 突發 10 次，之後每 12 秒補 1 次（約 5 次/分）。
/// GET /api/orders/{id}、cancel 不限（結構同 routes/auth.rs:33-58）。
pub fn router() -> Router<AppState> {
    let governor_conf = Arc::new(
        GovernorConfigBuilder::default()
            .key_extractor(SmartIpKeyExtractor)
            .per_second(12)
            .burst_size(10)
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
    let governor_layer = GovernorLayer::new(governor_conf).error_handler(|err| match err {
        GovernorError::TooManyRequests { .. } => ApiError::RateLimited.into_response(),
        other => ApiError::Internal(anyhow::anyhow!("rate limiter: {other:?}")).into_response(),
    });

    Router::new()
        .route("/api/orders", post(create).layer(governor_layer))
        .route("/api/orders/{id}", get(detail))
        .route("/api/orders/{id}/cancel", post(cancel))
}

/// 規格 §7 第 6 點的回應：訂單資料 + 送往綠界的表單（與規格不同之處 23）
#[derive(Serialize)]
pub struct CreateOrderResponse {
    #[serde(flatten)]
    pub created: OrderCreated,
    pub ecpay: CheckoutForm,
}

/// 讀完整訂單與最新一筆付款，組綠界表單。create 與 repay（Task 7）共用。
/// guest_token 有值時 ClientBackURL 帶 ?t=（訪客回到訂單頁要靠它）
async fn checkout_form_for(
    state: &AppState,
    order_id: Uuid,
    guest_token: Option<&str>,
) -> ApiResult<CheckoutForm> {
    let detail = orders::get_detail(&state.db, order_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let payment = detail.payment.as_ref().ok_or_else(|| {
        ApiError::Internal(anyhow::anyhow!("訂單 {order_id} 沒有 payments 列"))
    })?;
    let shop = settings::get_all(&state.db).await?.shop;
    aio::checkout_form(
        &state.config.ecpay,
        &state.config.public_base_url,
        &shop.name,
        &detail,
        payment,
        guest_token,
        Utc::now(),
    )
    .map_err(ApiError::Internal)
}

/// 會員或訪客都能下單（規格 §7）；登入者的訂單掛在帳號下。回應含送往綠界的表單欄位
async fn create(
    CurrentUser(user): CurrentUser,
    State(state): State<AppState>,
    AppJson(input): AppJson<OrderInput>,
) -> ApiResult<(StatusCode, Json<CreateOrderResponse>)> {
    let created = orders::create_order(&state.db, input, user.as_ref()).await?;
    let guest_token = user.is_none().then_some(created.guest_token.as_str());
    let ecpay = checkout_form_for(&state, created.order_id, guest_token).await?;
    Ok((
        StatusCode::CREATED,
        Json(CreateOrderResponse { created, ecpay }),
    ))
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

- [ ] **Step 6: 跑全部測試、clippy、fmt**

Run（背景）: `export PATH="$HOME/.cargo/bin:$PATH" && cargo fmt --all --manifest-path api/Cargo.toml && DATABASE_URL=postgres://dog_shop:dog_shop@localhost:5435/dog_shop cargo test --manifest-path api/Cargo.toml && cargo clippy --all-targets --manifest-path api/Cargo.toml -- -D warnings`
Expected: 全綠。既有 `guest_checkout_view_and_cancel`、`member_checkout_and_order_list` 只讀 `order_id`／`order_no`／`guest_token`，多出的 `ecpay` 不影響。

- [ ] **Step 7: Commit**

```bash
git add api/src/error.rs api/src/ecpay/mod.rs api/src/ecpay/aio.rs api/src/routes/orders.rs api/tests/orders.rs
git commit -m "feat(api): POST /api/orders 回傳綠界 AioCheckOut 表單欄位；新增 ORDER_NOT_PAYABLE、ECPAY_ERROR"
```

---
### Task 5: 前端結帳改 POST 綠界；Playwright 攔截綠界表單

**Files:**
- Modify: `web/src/lib/types.ts:182`
- Create: `web/src/lib/ecpay.ts`
- Modify: `web/src/routes/checkout/+page.svelte`（import 區 2-14、`submit()` 118-158）
- Modify: `web/e2e/checkout.spec.ts`（整檔改寫）

**Interfaces:**
- Consumes: `POST /api/orders` 回應的 `ecpay: { action, fields }`（Task 4）。
- Produces：
  - `types.ts`：`export type EcpayForm = { action: string; fields: Record<string, string> }`；`OrderCreated` 多 `ecpay: EcpayForm`
  - `lib/ecpay.ts`：`postToEcpay(form: EcpayForm): void`（Task 11 的重新付款也用）
  - 結帳成功後不再 `goto` 訂單頁，改成離開本站到綠界；買家經 `ClientBackURL` 回來。

- [ ] **Step 1: 型別（`web/src/lib/types.ts`）**

把第 182 行 `export type OrderCreated = { order_id: string; order_no: string; guest_token: string };` 換成：

```ts
/** 送往綠界的隱藏表單：action 是網址，fields 是欄位（含 CheckMacValue） */
export type EcpayForm = { action: string; fields: Record<string, string> };
export type OrderCreated = { order_id: string; order_no: string; guest_token: string; ecpay: EcpayForm };
```

- [ ] **Step 2: 建 `web/src/lib/ecpay.ts`**

```ts
import type { EcpayForm } from '$lib/types';

/** 用隱藏表單把欄位 POST 到綠界（頂層導頁，不用 iframe；規格 §7 第 7 點）。只能在瀏覽器呼叫 */
export function postToEcpay(form: EcpayForm): void {
	const el = document.createElement('form');
	el.method = 'POST';
	el.action = form.action;
	el.style.display = 'none';
	for (const [name, value] of Object.entries(form.fields)) {
		const input = document.createElement('input');
		input.type = 'hidden';
		input.name = name;
		input.value = value;
		el.appendChild(input);
	}
	document.body.appendChild(el);
	el.submit();
}
```

- [ ] **Step 3: 改 `web/src/routes/checkout/+page.svelte`**

(a) import 區：刪掉第 3 行 `import { goto } from '$app/navigation';`（`goto` 只有 submit 用到）；在 `import { api, ApiError } from '$lib/api';` 下面加 `import { postToEcpay } from '$lib/ecpay';`；第 12 行的型別 import 加 `EcpayForm`：

```ts
	import type { CartValidateResponse, CvsStore, CvsSubType, EcpayForm, OrderCreated, PaymentMethod } from '$lib/types';
```

(b) `submit()` 裡從 `submitting = true;` 到函式結尾換成：

```ts
		submitting = true;
		let ecpay: EcpayForm | null = null;
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
			ecpay = created.ecpay;
		} catch (err) {
			if (err instanceof ApiError && err.code === 'VALIDATION') {
				errors = err.fields();
				const first = Object.values(errors)[0];
				toast.show(first ? '請檢查紅字欄位：' + first : err.message);
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
		if (ecpay) {
			// 訂單已成立、購物車已清空；離開本頁去綠界付款（規格 §7 第 7 點）。按鈕維持停用，避免重複送出
			submitting = true;
			postToEcpay(ecpay);
		}
	}
```

- [ ] **Step 4: `pnpm -C web check` 與 `pnpm -C web build`**

Run（背景）: `pnpm -C web check && pnpm -C web build`
Expected: 0 錯誤 0 警告；build 成功。

- [ ] **Step 5: 改寫 `web/e2e/checkout.spec.ts`（整檔）**

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

test('瀏覽 → 加入購物車 → 結帳（宅配）→ 送往綠界的表單 → 訂單頁', async ({ page, request }) => {
	const product = await seedProduct(request);

	await page.goto(`/products/${product.slug}`);
	await expect(page.getByRole('heading', { name: product.name })).toBeVisible();
	// 商品頁的按鈕在 SSR 就存在（不像購物車／結帳頁要等 client-only 的 cart.loaded），
	// 第一次從整頁導覽過來時 click 可能搶在 hydration 掛上事件監聽器之前發生；重試直到成功
	await expect(async () => {
		await page.getByRole('button', { name: '加入購物車' }).click();
		await expect(page.getByText('已加入購物車')).toBeVisible({ timeout: 2_000 });
	}).toPass({ timeout: 15_000 });

	await page.goto('/cart');
	await expect(page.getByText(product.name)).toBeVisible();
	await expect(page.getByText('小計')).toBeVisible();
	await page.getByRole('link', { name: '前往結帳' }).click();
	await expect(page).toHaveURL(/\/checkout$/);

	// 'Email'、'手機' 用寬鬆比對會同時吃到發票「載具」select 的選項文字（綠界會員載具…Email、…手機條碼），改用更精準的比對
	await page.getByLabel(/^Email/).fill('e2e@test.local');
	await page.getByLabel('收件人').fill('王小明');
	await page.getByLabel('手機', { exact: true }).fill('0912345678');
	await page.getByLabel('縣市').selectOption('臺北市');
	await page.getByLabel('鄉鎮市區').selectOption('中正區');
	await expect(page.getByLabel('郵遞區號')).toHaveValue('100');
	await page.getByLabel('地址').fill('重慶南路一段 122 號');
	await expect(page.getByText('總計')).toBeVisible();

	// 攔截送往綠界的頂層表單 POST：不真的去綠界，回一頁假的；欄位用 postData 檢查（規格 §15）
	await page.route('https://payment-stage.ecpay.com.tw/**', (route) =>
		route.fulfill({
			status: 200,
			contentType: 'text/html; charset=utf-8',
			body: '<!doctype html><title>ECPay stub</title><p>ECPay stub</p>'
		})
	);
	const ecpayRequest = page.waitForRequest(
		(r) => r.url().includes('/Cashier/AioCheckOut/V5') && r.method() === 'POST'
	);
	await page.getByRole('button', { name: '送出訂單' }).click();
	const fields = new URLSearchParams((await ecpayRequest).postData() ?? '');
	expect(fields.get('MerchantID')).toBe('3002607');
	expect(fields.get('ChoosePayment')).toBe('Credit');
	expect(fields.get('PaymentType')).toBe('aio');
	expect(fields.get('EncryptType')).toBe('1');
	expect(fields.get('MerchantTradeNo')).toMatch(/^DS\d{6}[A-Z0-9]{4}01$/);
	expect(fields.get('TotalAmount')).toMatch(/^\d+$/);
	expect(fields.get('CheckMacValue')).toMatch(/^[0-9A-F]{64}$/);
	expect(fields.get('ReturnURL')).toMatch(/\/api\/ecpay\/payment\/return$/);
	expect(fields.get('PaymentInfoURL')).toMatch(/\/api\/ecpay\/payment\/info$/);
	const backUrl = fields.get('ClientBackURL') ?? '';
	expect(backUrl).toMatch(/\/orders\/[0-9a-f-]{36}\?t=[0-9a-f]{64}$/);
	await expect(page.getByText('ECPay stub')).toBeVisible();

	// 買家從綠界回來（ClientBackURL）
	await page.goto(backUrl);
	const orderNo = (fields.get('MerchantTradeNo') ?? '').slice(0, -2);
	// 純文字比對會同時吃到 SvelteKit 導覽時寫入的 #svelte-announcer（也含訂單編號），改比對標題本身
	await expect(page.getByRole('heading', { name: orderNo })).toBeVisible();
	await expect(page.getByText('待付款')).toBeVisible();
	await expect(page.getByText(product.name)).toBeVisible();
	await expect(page.getByText('重慶南路一段 122 號')).toBeVisible();

	// 取消 → 狀態變已取消
	await page.getByRole('button', { name: '取消訂單' }).click();
	await page.getByRole('button', { name: '確定取消這筆訂單' }).click();
	await expect(page.getByText('已取消', { exact: false }).first()).toBeVisible();
});
```

- [ ] **Step 6: 跑 e2e（要先起 api 與 web dev）**

Run（背景、各自一個 Bash 呼叫）:
1. `export PATH="$HOME/.cargo/bin:$PATH" && cargo run --manifest-path api/Cargo.toml`（等到 `api listening`）
2. `pnpm -C web dev`（等到 `Local: http://localhost:5173`）
3. `pnpm -C web test:e2e`

Expected: `1 passed`。失敗時看 `web/test-results/` 的截圖與 trace。做完用 `lsof -ti :8080`、`lsof -ti :5173` 取 pid `kill` 掉。

- [ ] **Step 7: vitest 仍然全過**

Run: `pnpm -C web test`
Expected: 24 個全過。

- [ ] **Step 8: Commit**

```bash
git add web/src/lib/types.ts web/src/lib/ecpay.ts web/src/routes/checkout/+page.svelte web/e2e/checkout.spec.ts
git commit -m "feat(web): 結帳送出後用隱藏表單前往綠界付款；e2e 攔截綠界表單檢查欄位"
```

---
### Task 6: 綠界回呼 `ReturnURL`／`PaymentInfoURL`；`domain/payments.rs`

**Files:**
- Modify: `api/Cargo.toml`（加 `form_urlencoded = "1"`）
- Modify: `api/src/ecpay/aio.rs`（檔尾加回呼解析；`use` 區加 `serde_json`）
- Create: `api/src/domain/payments.rs`
- Modify: `api/src/domain/mod.rs`（加 `pub mod payments;`）
- Create: `api/src/routes/ecpay_payment.rs`
- Modify: `api/src/routes/mod.rs`（加 `pub mod ecpay_payment;`）
- Modify: `api/src/app.rs:45`（`.merge(routes::ecpay_payment::router())`）
- Test: `api/tests/ecpay_payment.rs`

**Interfaces:**
- Consumes: `mac::verify`、`time::{parse_datetime, parse_expire}`（Task 3）、`jobs::{enqueue, KIND_ISSUE_INVOICE, KIND_SEND_EMAIL}`、`orders::{STATUS_PENDING_PAYMENT, STATUS_PAID}`。
- Produces：
  - `ecpay::aio::Notification { merchant_trade_no, rtn_code: i32, rtn_msg, trade_no, trade_amt: i32, payment_type, payment_date: Option<DateTime<Utc>>, simulate_paid: bool, bank_code: Option<String>, v_account: Option<String>, payment_no: Option<String>, expire_at: Option<DateTime<Utc>>, raw: serde_json::Value }`
  - `ecpay::aio::CallbackError::{BadMac, Missing(&'static str)}`；`parse_notification(cfg: &EcpayConfig, params: &[(String, String)]) -> Result<Notification, CallbackError>`
  - `domain::payments::{PAYMENT_PENDING, PAYMENT_PAID, PAYMENT_FAILED, PAYMENT_EXPIRED}`、`Payment`（全部欄位）、`get(db, id) -> Result<Option<Payment>, sqlx::Error>`
  - `payments::ReturnOutcome::{Unknown, Simulated, Duplicate, NotPaid, Late, Paid}`、`apply_return(db, &Notification) -> Result<ReturnOutcome, ApiError>`
  - `payments::InfoOutcome::{Unknown, Ignored, Stored}`、`apply_info(db, &Notification) -> Result<InfoOutcome, ApiError>`
  - `POST /api/ecpay/payment/return`、`POST /api/ecpay/payment/info`（form-urlencoded → 純文字）
  - `send_email` 的兩種新 payload：`{ "template": "payment_received", "order_id" }`（dedupe `email:payment_received:{order_id}`）、`{ "template": "payment_instructions", "order_id", "payment_id" }`（dedupe `email:payment_instructions:{payment_id}`）；`issue_invoice` payload `{ "order_id" }`（dedupe `invoice:{order_id}`）

- [ ] **Step 1: 加相依套件**

`api/Cargo.toml` 的 `[dependencies]` 加（字母順序，放在 `dotenvy` 之後）：

```toml
form_urlencoded = "1"
```

- [ ] **Step 2: 寫失敗的整合測試 `api/tests/ecpay_payment.rs`**

```rust
mod common;

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
};
use chrono::{DateTime, TimeZone, Utc};
use dog_shop_api::ecpay::mac;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

/// stage 的 AIO 憑證（Config::for_tests 用同一組）
const KEY: &str = "pwFHCqoQZGmho4w6";
const IV: &str = "EkRm7iFT261dpevs";

fn order_body(variant: &str, method: &str) -> Value {
    json!({
        "items": [{ "variant_id": variant, "qty": 2 }],
        "email": "buyer@test.local",
        "recipient_name": "王小明",
        "recipient_phone": "0912345678",
        "shipping_method": "home",
        "address": { "postal_code": "100", "city": "臺北市", "district": "中正區", "street": "重慶南路一段 122 號" },
        "invoice": { "type": "personal", "carrier_type": "1" },
        "payment_method": method,
        "note": ""
    })
}

/// 建一筆 2 × 300 + 運費 100 = 700 的訪客訂單，回 (order_id, merchant_trade_no, guest_token)
async fn place_order(app: &Router, pool: &PgPool, method: &str) -> (Uuid, String, String) {
    let (variant, _) = common::active_product(pool, "雞肉狗糧", 300, 5).await;
    let (status, created, _) = common::send(
        app,
        common::req(
            "POST",
            "/api/orders",
            None,
            Some(order_body(&variant.to_string(), method)),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    (
        Uuid::parse_str(created["order_id"].as_str().unwrap()).unwrap(),
        created["ecpay"]["fields"]["MerchantTradeNo"]
            .as_str()
            .unwrap()
            .to_string(),
        created["guest_token"].as_str().unwrap().to_string(),
    )
}

fn f(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

/// 模擬綠界伺服器：算好 CheckMacValue，用 form-urlencoded POST（沒有 Origin、沒有 X-Requested-With）
async fn ecpay_post(app: &Router, path: &str, mut fields: Vec<(String, String)>) -> (StatusCode, String) {
    let mac = mac::check_mac_value(KEY, IV, &fields);
    fields.push(("CheckMacValue".to_string(), mac));
    ecpay_post_raw(app, path, fields).await
}

async fn ecpay_post_raw(app: &Router, path: &str, fields: Vec<(String, String)>) -> (StatusCode, String) {
    let body = form_urlencoded::Serializer::new(String::new())
        .extend_pairs(fields.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .finish();
    let request = Request::builder()
        .method("POST")
        .uri(path)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from(body))
        .unwrap();
    let (status, value, _) = common::send(app, request).await;
    let text = match value {
        Value::String(s) => s,
        other => other.to_string(),
    };
    (status, text)
}

/// ReturnURL 的欄位（文件）：付款成功
fn return_fields(mtn: &str, amount: &str) -> Vec<(String, String)> {
    f(&[
        ("MerchantID", "3002607"),
        ("MerchantTradeNo", mtn),
        ("StoreID", ""),
        ("RtnCode", "1"),
        ("RtnMsg", "交易成功"),
        ("TradeNo", "2609061530000001"),
        ("TradeAmt", amount),
        ("PaymentDate", "2026/09/06 15:30:23"),
        ("PaymentType", "Credit_CreditCard"),
        ("PaymentTypeChargeFee", "20"),
        ("TradeDate", "2026/09/06 15:28:00"),
        ("SimulatePaid", "0"),
        ("CustomField1", ""),
        ("CustomField2", ""),
        ("CustomField3", ""),
        ("CustomField4", ""),
    ])
}

async fn order_row(pool: &PgPool, id: Uuid) -> (String, Option<DateTime<Utc>>, bool) {
    sqlx::query_as("SELECT status, paid_at, needs_refund FROM orders WHERE id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn payment_row(pool: &PgPool, mtn: &str) -> (String, Option<String>, Option<String>, Option<Value>) {
    sqlx::query_as(
        "SELECT status, ecpay_trade_no, payment_type, raw FROM payments WHERE merchant_trade_no = $1",
    )
    .bind(mtn)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn jobs_of(pool: &PgPool, kind: &str) -> Vec<(Value, Option<String>)> {
    sqlx::query_as("SELECT payload, dedupe_key FROM jobs WHERE kind = $1 ORDER BY id")
        .bind(kind)
        .fetch_all(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrations = "./migrations")]
async fn credit_return_marks_paid_enqueues_jobs_and_is_idempotent(pool: PgPool) {
    let app = common::app(pool.clone());
    let (order_id, mtn, token) = place_order(&app, &pool, "credit").await;

    let (status, text) = ecpay_post(&app, "/api/ecpay/payment/return", return_fields(&mtn, "700")).await;
    assert_eq!((status, text.as_str()), (StatusCode::OK, "1|OK"));

    let (o_status, paid_at, needs_refund) = order_row(&pool, order_id).await;
    assert_eq!(o_status, "paid");
    assert_eq!(paid_at, Some(Utc.with_ymd_and_hms(2026, 9, 6, 7, 30, 23).unwrap()));
    assert!(!needs_refund);
    let (p_status, trade_no, payment_type, raw) = payment_row(&pool, &mtn).await;
    assert_eq!(p_status, "paid");
    assert_eq!(trade_no.as_deref(), Some("2609061530000001"));
    assert_eq!(payment_type.as_deref(), Some("Credit_CreditCard"));
    assert_eq!(raw.unwrap()["RtnMsg"], "交易成功");

    let invoice_jobs = jobs_of(&pool, "issue_invoice").await;
    assert_eq!(invoice_jobs.len(), 1);
    assert_eq!(invoice_jobs[0].0["order_id"], order_id.to_string());
    assert_eq!(invoice_jobs[0].1.as_deref(), Some(format!("invoice:{order_id}").as_str()));
    let emails = jobs_of(&pool, "send_email").await;
    assert_eq!(emails.len(), 2, "order_created + payment_received");
    assert_eq!(emails[1].0["template"], "payment_received");

    // 綠界重送同一筆：回 1|OK、不重做
    let (status, text) = ecpay_post(&app, "/api/ecpay/payment/return", return_fields(&mtn, "700")).await;
    assert_eq!((status, text.as_str()), (StatusCode::OK, "1|OK"));
    assert_eq!(jobs_of(&pool, "issue_invoice").await.len(), 1);
    assert_eq!(jobs_of(&pool, "send_email").await.len(), 2);

    // 訂單頁看得到已付款
    let (_, detail, _) = common::send(
        &app,
        common::req("GET", &format!("/api/orders/{order_id}?t={token}"), None, None),
    )
    .await;
    assert_eq!(detail["status"], "paid");
    assert_eq!(detail["payment"]["status"], "paid");
}

#[sqlx::test(migrations = "./migrations")]
async fn bad_mac_is_400_and_changes_nothing(pool: PgPool) {
    let app = common::app(pool.clone());
    let (order_id, mtn, _) = place_order(&app, &pool, "credit").await;

    let mut fields = return_fields(&mtn, "700");
    fields.push(("CheckMacValue".to_string(), "0".repeat(64)));
    let (status, text) = ecpay_post_raw(&app, "/api/ecpay/payment/return", fields).await;
    assert_eq!((status, text.as_str()), (StatusCode::BAD_REQUEST, "0|CheckMacValue Error"));

    // 沒帶 CheckMacValue 也是 400
    let (status, _) = ecpay_post_raw(&app, "/api/ecpay/payment/return", return_fields(&mtn, "700")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // 金額被改過（簽章是用原本欄位算的）
    let mut fields = return_fields(&mtn, "700");
    let mac = mac::check_mac_value(KEY, IV, &fields);
    fields.push(("CheckMacValue".to_string(), mac));
    fields.iter_mut().find(|(k, _)| k == "TradeAmt").unwrap().1 = "1".to_string();
    let (status, _) = ecpay_post_raw(&app, "/api/ecpay/payment/return", fields).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (o_status, _, _) = order_row(&pool, order_id).await;
    assert_eq!(o_status, "pending_payment");
    assert_eq!(payment_row(&pool, &mtn).await.0, "pending");
    assert!(jobs_of(&pool, "issue_invoice").await.is_empty());
}

#[sqlx::test(migrations = "./migrations")]
async fn unknown_merchant_trade_no_and_missing_fields(pool: PgPool) {
    let app = common::app(pool.clone());
    let (status, text) = ecpay_post(&app, "/api/ecpay/payment/return", return_fields("DS000000XXXX01", "700")).await;
    assert_eq!((status, text.as_str()), (StatusCode::OK, "0|Unknown MerchantTradeNo"));

    let (status, text) = ecpay_post(&app, "/api/ecpay/payment/return", f(&[("MerchantID", "3002607"), ("RtnCode", "1")])).await;
    assert_eq!((status, text.as_str()), (StatusCode::BAD_REQUEST, "0|Missing Field"));
}

#[sqlx::test(migrations = "./migrations")]
async fn simulate_paid_only_logs(pool: PgPool) {
    let app = common::app(pool.clone());
    let (order_id, mtn, _) = place_order(&app, &pool, "credit").await;
    let mut fields = return_fields(&mtn, "700");
    fields.iter_mut().find(|(k, _)| k == "SimulatePaid").unwrap().1 = "1".to_string();
    let (status, text) = ecpay_post(&app, "/api/ecpay/payment/return", fields).await;
    assert_eq!((status, text.as_str()), (StatusCode::OK, "1|OK"));
    assert_eq!(order_row(&pool, order_id).await.0, "pending_payment");
    assert_eq!(payment_row(&pool, &mtn).await.0, "pending");
    assert!(jobs_of(&pool, "issue_invoice").await.is_empty());
}

#[sqlx::test(migrations = "./migrations")]
async fn failed_rtn_code_marks_payment_failed_only(pool: PgPool) {
    let app = common::app(pool.clone());
    let (order_id, mtn, _) = place_order(&app, &pool, "credit").await;
    let mut fields = return_fields(&mtn, "700");
    fields.iter_mut().find(|(k, _)| k == "RtnCode").unwrap().1 = "10100058".to_string();
    fields.iter_mut().find(|(k, _)| k == "RtnMsg").unwrap().1 = "付款失敗".to_string();
    let (status, text) = ecpay_post(&app, "/api/ecpay/payment/return", fields).await;
    assert_eq!((status, text.as_str()), (StatusCode::OK, "1|OK"));
    assert_eq!(order_row(&pool, order_id).await.0, "pending_payment");
    let (p_status, _, _, raw) = payment_row(&pool, &mtn).await;
    assert_eq!(p_status, "failed");
    assert_eq!(raw.unwrap()["RtnCode"], "10100058");
    assert!(jobs_of(&pool, "issue_invoice").await.is_empty());

    // 同一個 MerchantTradeNo 之後成功（買家在綠界頁重刷）→ 正常付款
    let (status, text) = ecpay_post(&app, "/api/ecpay/payment/return", return_fields(&mtn, "700")).await;
    assert_eq!((status, text.as_str()), (StatusCode::OK, "1|OK"));
    assert_eq!(order_row(&pool, order_id).await.0, "paid");
    assert_eq!(payment_row(&pool, &mtn).await.0, "paid");
}

#[sqlx::test(migrations = "./migrations")]
async fn late_payment_after_cancel_sets_needs_refund(pool: PgPool) {
    let app = common::app(pool.clone());
    let (order_id, mtn, token) = place_order(&app, &pool, "credit").await;
    let (status, _, _) = common::send(
        &app,
        common::req("POST", &format!("/api/orders/{order_id}/cancel?t={token}"), None, None),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, text) = ecpay_post(&app, "/api/ecpay/payment/return", return_fields(&mtn, "700")).await;
    assert_eq!((status, text.as_str()), (StatusCode::OK, "1|OK"));
    let (o_status, paid_at, needs_refund) = order_row(&pool, order_id).await;
    assert_eq!(o_status, "cancelled", "訂單狀態不動");
    assert!(paid_at.is_none());
    assert!(needs_refund, "遲到的付款要標 needs_refund（規格 §4）");
    assert_eq!(payment_row(&pool, &mtn).await.0, "paid");
    assert!(jobs_of(&pool, "issue_invoice").await.is_empty(), "不開發票");
    // 庫存不動（取消時已歸還）
    let stock: i32 = sqlx::query_scalar("SELECT stock FROM product_variants LIMIT 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(stock, 5);
}

#[sqlx::test(migrations = "./migrations")]
async fn amount_mismatch_is_treated_as_late(pool: PgPool) {
    let app = common::app(pool.clone());
    let (order_id, mtn, _) = place_order(&app, &pool, "credit").await;
    let (status, text) = ecpay_post(&app, "/api/ecpay/payment/return", return_fields(&mtn, "1")).await;
    assert_eq!((status, text.as_str()), (StatusCode::OK, "1|OK"));
    let (o_status, _, needs_refund) = order_row(&pool, order_id).await;
    assert_eq!(o_status, "pending_payment");
    assert!(needs_refund);
    assert_eq!(payment_row(&pool, &mtn).await.0, "paid");
    assert!(jobs_of(&pool, "issue_invoice").await.is_empty());
}

fn info_fields(mtn: &str, extra: &[(&str, &str)]) -> Vec<(String, String)> {
    let mut fields = f(&[
        ("MerchantID", "3002607"),
        ("MerchantTradeNo", mtn),
        ("StoreID", ""),
        ("RtnMsg", "Get VirtualAccount Succeeded"),
        ("TradeNo", "2609061530000002"),
        ("TradeAmt", "700"),
        ("TradeDate", "2026/09/06 15:28:00"),
        ("CustomField1", ""),
        ("CustomField2", ""),
        ("CustomField3", ""),
        ("CustomField4", ""),
    ]);
    fields.extend(f(extra));
    fields
}

#[sqlx::test(migrations = "./migrations")]
async fn atm_info_is_stored_and_emails_instructions(pool: PgPool) {
    let app = common::app(pool.clone());
    let (order_id, mtn, token) = place_order(&app, &pool, "atm").await;
    let fields = info_fields(
        &mtn,
        &[
            ("RtnCode", "2"),
            ("PaymentType", "ATM_TAISHIN"),
            ("BankCode", "812"),
            ("vAccount", "1234567890123456"),
            ("ExpireDate", "2026/09/09"),
        ],
    );
    let (status, text) = ecpay_post(&app, "/api/ecpay/payment/info", fields).await;
    assert_eq!((status, text.as_str()), (StatusCode::OK, "1|OK"));

    let (bank, vaccount, expire_at, p_status): (Option<String>, Option<String>, Option<DateTime<Utc>>, String) =
        sqlx::query_as("SELECT atm_bank_code, atm_vaccount, expire_at, status FROM payments WHERE merchant_trade_no = $1")
            .bind(&mtn)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(bank.as_deref(), Some("812"));
    assert_eq!(vaccount.as_deref(), Some("1234567890123456"));
    // 只有日期 → 該日台北 23:59:59 = UTC 15:59:59
    assert_eq!(expire_at, Some(Utc.with_ymd_and_hms(2026, 9, 9, 15, 59, 59).unwrap()));
    assert_eq!(p_status, "pending", "拿到帳號還沒付款");
    assert_eq!(order_row(&pool, order_id).await.0, "pending_payment");

    let emails = jobs_of(&pool, "send_email").await;
    assert_eq!(emails.len(), 2);
    assert_eq!(emails[1].0["template"], "payment_instructions");
    assert_eq!(emails[1].0["order_id"], order_id.to_string());
    let payment_id: Uuid = sqlx::query_scalar("SELECT id FROM payments WHERE merchant_trade_no = $1")
        .bind(&mtn)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(emails[1].0["payment_id"], payment_id.to_string());
    assert_eq!(emails[1].1.as_deref(), Some(format!("email:payment_instructions:{payment_id}").as_str()));

    // 訂單頁看得到帳號
    let (_, detail, _) = common::send(
        &app,
        common::req("GET", &format!("/api/orders/{order_id}?t={token}"), None, None),
    )
    .await;
    assert_eq!(detail["payment"]["atm_vaccount"], "1234567890123456");
    assert_eq!(detail["payment"]["atm_bank_code"], "812");
    assert_eq!(detail["payment"]["expire_at"], "2026-09-09T15:59:59Z");

    // 綠界重送同一筆 → 1|OK、不再排信
    let fields = info_fields(&mtn, &[("RtnCode", "2"), ("PaymentType", "ATM_TAISHIN"), ("BankCode", "812"), ("vAccount", "1234567890123456"), ("ExpireDate", "2026/09/09")]);
    let (status, text) = ecpay_post(&app, "/api/ecpay/payment/info", fields).await;
    assert_eq!((status, text.as_str()), (StatusCode::OK, "1|OK"));
    assert_eq!(jobs_of(&pool, "send_email").await.len(), 2);
}

#[sqlx::test(migrations = "./migrations")]
async fn cvs_info_is_stored(pool: PgPool) {
    let app = common::app(pool.clone());
    let (_, mtn, _) = place_order(&app, &pool, "cvs_code").await;
    let fields = info_fields(
        &mtn,
        &[
            ("RtnCode", "10100073"),
            ("PaymentType", "CVS_CVS"),
            ("PaymentNo", "LLL26090612345"),
            ("ExpireDate", "2026/09/09 15:30:23"),
        ],
    );
    let (status, text) = ecpay_post(&app, "/api/ecpay/payment/info", fields).await;
    assert_eq!((status, text.as_str()), (StatusCode::OK, "1|OK"));
    let (payment_no, expire_at): (Option<String>, Option<DateTime<Utc>>) =
        sqlx::query_as("SELECT cvs_payment_no, expire_at FROM payments WHERE merchant_trade_no = $1")
            .bind(&mtn)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(payment_no.as_deref(), Some("LLL26090612345"));
    assert_eq!(expire_at, Some(Utc.with_ymd_and_hms(2026, 9, 9, 7, 30, 23).unwrap()));
}

#[sqlx::test(migrations = "./migrations")]
async fn info_after_paid_or_unknown_is_harmless(pool: PgPool) {
    let app = common::app(pool.clone());
    let (_, mtn, _) = place_order(&app, &pool, "atm").await;
    let (status, text) = ecpay_post(&app, "/api/ecpay/payment/return", return_fields(&mtn, "700")).await;
    assert_eq!((status, text.as_str()), (StatusCode::OK, "1|OK"));

    let fields = info_fields(&mtn, &[("RtnCode", "2"), ("PaymentType", "ATM_TAISHIN"), ("BankCode", "812"), ("vAccount", "1234567890123456"), ("ExpireDate", "2026/09/09")]);
    let (status, text) = ecpay_post(&app, "/api/ecpay/payment/info", fields).await;
    assert_eq!((status, text.as_str()), (StatusCode::OK, "1|OK"));
    let vaccount: Option<String> = sqlx::query_scalar("SELECT atm_vaccount FROM payments WHERE merchant_trade_no = $1")
        .bind(&mtn)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(vaccount.is_none(), "已付款的不再覆寫");

    let (status, text) = ecpay_post(&app, "/api/ecpay/payment/info", info_fields("DS000000XXXX01", &[("RtnCode", "2")])).await;
    assert_eq!((status, text.as_str()), (StatusCode::OK, "0|Unknown MerchantTradeNo"));
}
```

Run: `export PATH="$HOME/.cargo/bin:$PATH" && DATABASE_URL=postgres://dog_shop:dog_shop@localhost:5435/dog_shop cargo test --manifest-path api/Cargo.toml --test ecpay_payment`
Expected: 編譯過（只用到既有 crate）但每個測試失敗於 404（路由不存在，`common::send` 回的 body 不是 `1|OK`）。

- [ ] **Step 3: `api/src/ecpay/aio.rs` 檔尾（tests 模組之前）加回呼解析**

`use` 區加：

```rust
use serde_json::{Map, Value};
```

在 `checkout_form` 函式之後、`#[cfg(test)]` 之前加：

```rust
/// 綠界回呼的共同欄位（ReturnURL 與 PaymentInfoURL 都有；規格 §8.2）。其餘原樣留在 `raw`
#[derive(Debug, Clone)]
pub struct Notification {
    pub merchant_trade_no: String,
    pub rtn_code: i32,
    pub rtn_msg: String,
    pub trade_no: String,
    pub trade_amt: i32,
    pub payment_type: String,
    /// `yyyy/MM/dd HH:mm:ss`（台北）；ReturnURL 才有
    pub payment_date: Option<DateTime<Utc>>,
    /// 測試環境「模擬付款」會帶 1（規格 §8.2）
    pub simulate_paid: bool,
    /// 以下 PaymentInfoURL 才有
    pub bank_code: Option<String>,
    pub v_account: Option<String>,
    pub payment_no: Option<String>,
    /// ATM 是 `yyyy/MM/dd`（當天 23:59:59），超商是 `yyyy/MM/dd HH:mm:ss`
    pub expire_at: Option<DateTime<Utc>>,
    /// 所有欄位原樣（含 CheckMacValue），存進 payments.raw 對帳用
    pub raw: Value,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CallbackError {
    #[error("CheckMacValue Error")]
    BadMac,
    #[error("缺少欄位 {0}")]
    Missing(&'static str),
}

/// 取欄位值（key 不分大小寫）；空字串當沒有
fn get<'a>(params: &'a [(String, String)], key: &str) -> Option<&'a str> {
    params
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(key))
        .map(|(_, v)| v.as_str())
        .filter(|v| !v.is_empty())
}

/// 先驗簽章再解析。順序重要：沒過簽章什麼都不信
pub fn parse_notification(
    cfg: &EcpayConfig,
    params: &[(String, String)],
) -> Result<Notification, CallbackError> {
    if !mac::verify(&cfg.aio.hash_key, &cfg.aio.hash_iv, params) {
        return Err(CallbackError::BadMac);
    }
    let raw: Map<String, Value> = params
        .iter()
        .map(|(k, v)| (k.clone(), Value::String(v.clone())))
        .collect();
    Ok(Notification {
        merchant_trade_no: get(params, "MerchantTradeNo")
            .ok_or(CallbackError::Missing("MerchantTradeNo"))?
            .to_string(),
        rtn_code: get(params, "RtnCode")
            .and_then(|v| v.parse().ok())
            .ok_or(CallbackError::Missing("RtnCode"))?,
        rtn_msg: get(params, "RtnMsg").unwrap_or("").to_string(),
        trade_no: get(params, "TradeNo").unwrap_or("").to_string(),
        trade_amt: get(params, "TradeAmt")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0),
        payment_type: get(params, "PaymentType").unwrap_or("").to_string(),
        payment_date: get(params, "PaymentDate").and_then(time::parse_datetime),
        simulate_paid: get(params, "SimulatePaid") == Some("1"),
        bank_code: get(params, "BankCode").map(str::to_string),
        v_account: get(params, "vAccount").map(str::to_string),
        payment_no: get(params, "PaymentNo").map(str::to_string),
        expire_at: get(params, "ExpireDate").and_then(time::parse_expire),
        raw: Value::Object(raw),
    })
}
```

tests 模組加：

```rust
    #[test]
    fn parse_notification_requires_mac_then_reads_fields() {
        let cfg = Config::for_tests(PathBuf::from("/tmp")).ecpay;
        let mut params: Vec<(String, String)> = [
            ("MerchantID", "3002607"),
            ("MerchantTradeNo", "DS260906ABCD01"),
            ("RtnCode", "2"),
            ("RtnMsg", "Get VirtualAccount Succeeded"),
            ("TradeNo", "2609061530000002"),
            ("TradeAmt", "700"),
            ("PaymentType", "ATM_TAISHIN"),
            ("BankCode", "812"),
            ("vAccount", "1234567890123456"),
            ("ExpireDate", "2026/09/09"),
            ("SimulatePaid", "0"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
        assert_eq!(
            parse_notification(&cfg, &params).unwrap_err(),
            CallbackError::BadMac
        );
        let mac = mac::check_mac_value(&cfg.aio.hash_key, &cfg.aio.hash_iv, &params);
        params.push(("CheckMacValue".to_string(), mac));
        let n = parse_notification(&cfg, &params).unwrap();
        assert_eq!(n.merchant_trade_no, "DS260906ABCD01");
        assert_eq!(n.rtn_code, 2);
        assert_eq!(n.trade_amt, 700);
        assert_eq!(n.bank_code.as_deref(), Some("812"));
        assert_eq!(n.v_account.as_deref(), Some("1234567890123456"));
        assert_eq!(
            n.expire_at,
            Some(Utc.with_ymd_and_hms(2026, 9, 9, 15, 59, 59).unwrap())
        );
        assert!(n.payment_date.is_none());
        assert!(!n.simulate_paid);
        assert_eq!(n.raw["vAccount"], "1234567890123456");
        assert!(n.raw.get("CheckMacValue").is_some());

        // 缺 MerchantTradeNo（簽章對）
        let mut short: Vec<(String, String)> = vec![("RtnCode".to_string(), "1".to_string())];
        let mac = mac::check_mac_value(&cfg.aio.hash_key, &cfg.aio.hash_iv, &short);
        short.push(("CheckMacValue".to_string(), mac));
        assert_eq!(
            parse_notification(&cfg, &short).unwrap_err(),
            CallbackError::Missing("MerchantTradeNo")
        );
    }
```

- [ ] **Step 4: 建 `api/src/domain/payments.rs`**

```rust
//! payments 表（規格 §3、§4、§7 第 8、9 點）：回呼寫入。重新付款在 Task 7 補在這個檔案下面
use chrono::{DateTime, Utc};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::jobs;
use crate::domain::orders::{STATUS_PAID, STATUS_PENDING_PAYMENT};
use crate::ecpay::aio::Notification;
use crate::error::ApiError;

pub const PAYMENT_PENDING: &str = "pending";
pub const PAYMENT_PAID: &str = "paid";
pub const PAYMENT_FAILED: &str = "failed";
pub const PAYMENT_EXPIRED: &str = "expired";

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Payment {
    pub id: Uuid,
    pub order_id: Uuid,
    pub merchant_trade_no: String,
    pub method: String,
    pub status: String,
    pub amount: i32,
    pub ecpay_trade_no: Option<String>,
    pub payment_type: Option<String>,
    pub payment_date: Option<DateTime<Utc>>,
    pub atm_bank_code: Option<String>,
    pub atm_vaccount: Option<String>,
    pub cvs_payment_no: Option<String>,
    pub expire_at: Option<DateTime<Utc>>,
}

const PAYMENT_COLUMNS: &str = "id, order_id, merchant_trade_no, method, status, amount, ecpay_trade_no, payment_type,
     payment_date, atm_bank_code, atm_vaccount, cvs_payment_no, expire_at";

pub async fn get(db: &PgPool, id: Uuid) -> Result<Option<Payment>, sqlx::Error> {
    let sql = format!("SELECT {PAYMENT_COLUMNS} FROM payments WHERE id = $1");
    sqlx::query_as::<_, Payment>(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .fetch_optional(db)
        .await
}

/// ReturnURL 的處理結果；除了 Unknown 都回綠界 `1|OK`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReturnOutcome {
    /// 找不到 MerchantTradeNo → `0|Unknown MerchantTradeNo`
    Unknown,
    /// 測試環境的模擬付款：只記 log
    Simulated,
    /// 這筆 payment 早就 paid：重複通知，不重做
    Duplicate,
    /// RtnCode ≠ 1：payment 標 failed，訂單不動
    NotPaid,
    /// 訂單已不是待付款、或金額不符：payment 標 paid、訂單 needs_refund（規格 §4）
    Late,
    /// 正常付款成功：訂單 paid，排發票與通知信
    Paid,
}

/// ReturnURL（付款結果）。一個交易內鎖 payments 與 orders 列；重複通知是 no-op（規格 §7 第 8 點、§14）
pub async fn apply_return(db: &PgPool, n: &Notification) -> Result<ReturnOutcome, ApiError> {
    if n.simulate_paid {
        tracing::info!(merchant_trade_no = %n.merchant_trade_no, rtn_code = n.rtn_code, "綠界模擬付款通知，只記 log 不改狀態");
        return Ok(ReturnOutcome::Simulated);
    }
    let mut tx = db.begin().await?;
    let sql = format!("SELECT {PAYMENT_COLUMNS} FROM payments WHERE merchant_trade_no = $1 FOR UPDATE");
    let Some(payment) = sqlx::query_as::<_, Payment>(sqlx::AssertSqlSafe(sql))
        .bind(&n.merchant_trade_no)
        .fetch_optional(&mut *tx)
        .await?
    else {
        return Ok(ReturnOutcome::Unknown);
    };
    if payment.status == PAYMENT_PAID {
        return Ok(ReturnOutcome::Duplicate);
    }
    if n.rtn_code != 1 {
        // 失敗或待確認（信用卡 10300066）：記下來，訂單不動；買家可以重新付款
        sqlx::query(
            "UPDATE payments SET status = $2, ecpay_trade_no = $3, payment_type = $4, raw = $5, updated_at = now() WHERE id = $1",
        )
        .bind(payment.id)
        .bind(PAYMENT_FAILED)
        .bind(&n.trade_no)
        .bind(&n.payment_type)
        .bind(&n.raw)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        return Ok(ReturnOutcome::NotPaid);
    }

    // 成功：先鎖訂單列再決定是正常付款還是遲到（規格 §4）
    let order_status: String = sqlx::query_scalar("SELECT status FROM orders WHERE id = $1 FOR UPDATE")
        .bind(payment.order_id)
        .fetch_one(&mut *tx)
        .await?;
    let paid_at = n.payment_date.unwrap_or_else(Utc::now);
    sqlx::query(
        "UPDATE payments SET status = $2, ecpay_trade_no = $3, payment_type = $4, payment_date = $5, raw = $6, updated_at = now() WHERE id = $1",
    )
    .bind(payment.id)
    .bind(PAYMENT_PAID)
    .bind(&n.trade_no)
    .bind(&n.payment_type)
    .bind(paid_at)
    .bind(&n.raw)
    .execute(&mut *tx)
    .await?;

    let on_time = order_status == STATUS_PENDING_PAYMENT && n.trade_amt == payment.amount;
    if !on_time {
        tracing::warn!(
            order_id = %payment.order_id,
            merchant_trade_no = %n.merchant_trade_no,
            order_status = %order_status,
            trade_amt = n.trade_amt,
            expected = payment.amount,
            "遲到或金額不符的付款：只標 payment paid 並記 needs_refund（規格 §4）"
        );
        sqlx::query("UPDATE orders SET needs_refund = true WHERE id = $1")
            .bind(payment.order_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        return Ok(ReturnOutcome::Late);
    }

    let order_id = payment.order_id;
    sqlx::query("UPDATE orders SET status = $2, paid_at = $3 WHERE id = $1")
        .bind(order_id)
        .bind(STATUS_PAID)
        .bind(paid_at)
        .execute(&mut *tx)
        .await?;
    jobs::enqueue(
        &mut tx,
        jobs::KIND_ISSUE_INVOICE,
        json!({ "order_id": order_id }),
        Some(&format!("invoice:{order_id}")),
    )
    .await?;
    jobs::enqueue(
        &mut tx,
        jobs::KIND_SEND_EMAIL,
        json!({ "template": "payment_received", "order_id": order_id }),
        Some(&format!("email:payment_received:{order_id}")),
    )
    .await?;
    tx.commit().await?;
    Ok(ReturnOutcome::Paid)
}

/// PaymentInfoURL 的處理結果；除了 Unknown 都回綠界 `1|OK`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InfoOutcome {
    Unknown,
    /// payment 已不是 pending，或這次通知沒帶繳費資訊：只存 raw（或不動）
    Ignored,
    /// 存好帳號／代碼與期限，排 payment_instructions 信
    Stored,
}

/// PaymentInfoURL（ATM 虛擬帳號／超商代碼；規格 §7 第 8 點）。只在 payment 仍 pending 時寫入
pub async fn apply_info(db: &PgPool, n: &Notification) -> Result<InfoOutcome, ApiError> {
    let mut tx = db.begin().await?;
    let sql = format!("SELECT {PAYMENT_COLUMNS} FROM payments WHERE merchant_trade_no = $1 FOR UPDATE");
    let Some(payment) = sqlx::query_as::<_, Payment>(sqlx::AssertSqlSafe(sql))
        .bind(&n.merchant_trade_no)
        .fetch_optional(&mut *tx)
        .await?
    else {
        return Ok(InfoOutcome::Unknown);
    };
    if payment.status != PAYMENT_PENDING {
        return Ok(InfoOutcome::Ignored);
    }
    // RtnCode 2 = ATM 取號成功、10100073 = 超商代碼取號成功（綠界文件）；其他碼只存 raw
    let got_info = matches!(n.rtn_code, 2 | 10100073) && (n.v_account.is_some() || n.payment_no.is_some());
    if !got_info {
        tracing::warn!(merchant_trade_no = %n.merchant_trade_no, rtn_code = n.rtn_code, rtn_msg = %n.rtn_msg, "PaymentInfoURL 沒帶繳費資訊");
        sqlx::query("UPDATE payments SET raw = $2, updated_at = now() WHERE id = $1")
            .bind(payment.id)
            .bind(&n.raw)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        return Ok(InfoOutcome::Ignored);
    }
    sqlx::query(
        "UPDATE payments SET ecpay_trade_no = $2, payment_type = $3, atm_bank_code = $4, atm_vaccount = $5,
                cvs_payment_no = $6, expire_at = $7, raw = $8, updated_at = now()
         WHERE id = $1",
    )
    .bind(payment.id)
    .bind(&n.trade_no)
    .bind(&n.payment_type)
    .bind(&n.bank_code)
    .bind(&n.v_account)
    .bind(&n.payment_no)
    .bind(n.expire_at)
    .bind(&n.raw)
    .execute(&mut *tx)
    .await?;
    jobs::enqueue(
        &mut tx,
        jobs::KIND_SEND_EMAIL,
        json!({ "template": "payment_instructions", "order_id": payment.order_id, "payment_id": payment.id }),
        Some(&format!("email:payment_instructions:{}", payment.id)),
    )
    .await?;
    tx.commit().await?;
    Ok(InfoOutcome::Stored)
}
```

在 `api/src/domain/mod.rs` 加 `pub mod payments;`（放在 `pub mod password_resets;` 之後）。

- [ ] **Step 5: 建 `api/src/routes/ecpay_payment.rs`**

```rust
//! 綠界付款回呼（規格 §7 第 8 點、§8.2、§14）。回純文字：`1|OK`、`0|CheckMacValue Error`（400）、
//! `0|Missing Field`（400）、`0|Unknown MerchantTradeNo`（200）、`0|Server Error`（500，讓綠界重送）。
//! 路徑前綴 `/api/ecpay/` 已在 auth/csrf.rs 的 EXEMPT_PREFIXES 內（規格 §11：靠 CheckMacValue 驗）。
use axum::{
    Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
};

use crate::{
    domain::payments::{self, InfoOutcome, ReturnOutcome},
    ecpay::aio::{self, CallbackError},
    error::ApiError,
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/ecpay/payment/return", post(payment_return))
        .route("/api/ecpay/payment/info", post(payment_info))
}

/// 綠界送 application/x-www-form-urlencoded；不用 axum::Form，因為要拿到所有欄位重算簽章
fn parse_form(body: &str) -> Vec<(String, String)> {
    form_urlencoded::parse(body.as_bytes())
        .into_owned()
        .collect()
}

fn text(status: StatusCode, body: &'static str) -> Response {
    (status, body).into_response()
}

fn callback_error(path: &'static str, err: CallbackError) -> Response {
    tracing::warn!(path, error = %err, "綠界回呼簽章或欄位錯誤");
    match err {
        CallbackError::BadMac => text(StatusCode::BAD_REQUEST, "0|CheckMacValue Error"),
        CallbackError::Missing(_) => text(StatusCode::BAD_REQUEST, "0|Missing Field"),
    }
}

fn server_error(path: &'static str, err: ApiError) -> Response {
    tracing::error!(path, error = %err, "綠界回呼處理失敗，回 500 讓綠界重送");
    text(StatusCode::INTERNAL_SERVER_ERROR, "0|Server Error")
}

/// 付款結果
async fn payment_return(State(state): State<AppState>, body: String) -> Response {
    let params = parse_form(&body);
    let notification = match aio::parse_notification(&state.config.ecpay, &params) {
        Ok(n) => n,
        Err(e) => return callback_error("return", e),
    };
    match payments::apply_return(&state.db, &notification).await {
        Ok(ReturnOutcome::Unknown) => {
            tracing::warn!(merchant_trade_no = %notification.merchant_trade_no, "ReturnURL 找不到 MerchantTradeNo");
            text(StatusCode::OK, "0|Unknown MerchantTradeNo")
        }
        Ok(outcome) => {
            tracing::info!(merchant_trade_no = %notification.merchant_trade_no, rtn_code = notification.rtn_code, ?outcome, "ReturnURL 處理完成");
            text(StatusCode::OK, "1|OK")
        }
        Err(e) => server_error("return", e),
    }
}

/// ATM 虛擬帳號／超商代碼
async fn payment_info(State(state): State<AppState>, body: String) -> Response {
    let params = parse_form(&body);
    let notification = match aio::parse_notification(&state.config.ecpay, &params) {
        Ok(n) => n,
        Err(e) => return callback_error("info", e),
    };
    match payments::apply_info(&state.db, &notification).await {
        Ok(InfoOutcome::Unknown) => {
            tracing::warn!(merchant_trade_no = %notification.merchant_trade_no, "PaymentInfoURL 找不到 MerchantTradeNo");
            text(StatusCode::OK, "0|Unknown MerchantTradeNo")
        }
        Ok(outcome) => {
            tracing::info!(merchant_trade_no = %notification.merchant_trade_no, rtn_code = notification.rtn_code, ?outcome, "PaymentInfoURL 處理完成");
            text(StatusCode::OK, "1|OK")
        }
        Err(e) => server_error("info", e),
    }
}
```

`api/src/routes/mod.rs` 加 `pub mod ecpay_payment;`（放在 `pub mod checkout;` 之後）。`api/src/app.rs` 在 `.merge(routes::orders::router())` 之後加一行 `.merge(routes::ecpay_payment::router())`。

- [ ] **Step 6: 跑測試、clippy、fmt**

Run（背景）: `export PATH="$HOME/.cargo/bin:$PATH" && cargo fmt --all --manifest-path api/Cargo.toml && DATABASE_URL=postgres://dog_shop:dog_shop@localhost:5435/dog_shop cargo test --manifest-path api/Cargo.toml && cargo clippy --all-targets --manifest-path api/Cargo.toml -- -D warnings`
Expected: 全綠（`ecpay_payment` 10 個 + aio 單元測試 1 個新增）。

- [ ] **Step 7: Commit**

```bash
git add api/Cargo.toml api/Cargo.lock api/src/ecpay/aio.rs api/src/domain/payments.rs api/src/domain/mod.rs api/src/routes/ecpay_payment.rs api/src/routes/mod.rs api/src/app.rs api/tests/ecpay_payment.rs
git commit -m "feat(api): 綠界 ReturnURL／PaymentInfoURL 回呼（付款成功、重複、模擬、遲到、繳費資訊）"
```

---
### Task 7: `POST /api/orders/{id}/repay` 重新付款

**Files:**
- Modify: `api/src/domain/payments.rs`（檔尾加 `create_repayment`）
- Modify: `api/src/routes/orders.rs`（router 加一條、加 `RepayBody`／`RepayResponse`／`repay`；`use` 區加 `payments`）
- Test: `api/tests/repay.rs`

**Interfaces:**
- Consumes: `routes::orders::{find_order, checkout_form_for}`（Task 4）、`ApiError::OrderNotPayable`、`settings::get_all(db).payment_methods`。
- Produces：
  - `payments::create_repayment(db, order_id, method: &str) -> Result<Payment, ApiError>`（新列、`merchant_trade_no = order_no + 兩碼流水`；非待付款回 `OrderNotPayable`；訂單不存在回 `NotFound`）
  - `POST /api/orders/{id}/repay?t=`，body `{ "payment_method"?: "credit" | "atm" | "cvs_code" }` → 200 `{ ecpay: { action, fields } }`；錯誤：404（看不到）、400 `ORDER_NOT_PAYABLE`、400 `VALIDATION`（`fields.payment_method`）

- [ ] **Step 1: 寫失敗的整合測試 `api/tests/repay.rs`**

```rust
mod common;

use axum::http::StatusCode;
use serde_json::{Value, json};
use sqlx::PgPool;

fn order_body(variant: &str, method: &str) -> Value {
    json!({
        "items": [{ "variant_id": variant, "qty": 2 }],
        "email": "buyer@test.local",
        "recipient_name": "王小明",
        "recipient_phone": "0912345678",
        "shipping_method": "home",
        "address": { "postal_code": "100", "city": "臺北市", "district": "中正區", "street": "重慶南路一段 122 號" },
        "invoice": { "type": "personal", "carrier_type": "1" },
        "payment_method": method,
        "note": ""
    })
}

/// 回 (order_id, order_no, guest_token)
async fn place_order(app: &axum::Router, pool: &PgPool, cookie: Option<&str>) -> (String, String, String) {
    let (variant, _) = common::active_product(pool, "雞肉狗糧", 300, 5).await;
    let (status, created, _) = common::send(
        app,
        common::req(
            "POST",
            "/api/orders",
            cookie,
            Some(order_body(&variant.to_string(), "credit")),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    (
        created["order_id"].as_str().unwrap().to_string(),
        created["order_no"].as_str().unwrap().to_string(),
        created["guest_token"].as_str().unwrap().to_string(),
    )
}

fn uuid(s: &str) -> uuid::Uuid {
    uuid::Uuid::parse_str(s).unwrap()
}

async fn payment_count(pool: &PgPool, order_id: &str) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM payments WHERE order_id = $1")
        .bind(uuid(order_id))
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrations = "./migrations")]
async fn repay_creates_new_payment_and_form(pool: PgPool) {
    let app = common::app(pool.clone());
    let (id, order_no, token) = place_order(&app, &pool, None).await;

    let (status, body, _) = common::send(
        &app,
        common::req(
            "POST",
            &format!("/api/orders/{id}/repay?t={token}"),
            None,
            Some(json!({ "payment_method": "atm" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let fields = &body["ecpay"]["fields"];
    assert_eq!(fields["MerchantTradeNo"], format!("{order_no}02"));
    assert_eq!(fields["ChoosePayment"], "ATM");
    assert_eq!(fields["TotalAmount"], "700");
    assert_eq!(
        fields["ClientBackURL"],
        format!("{}/orders/{id}?t={token}", common::TEST_ORIGIN)
    );
    assert_eq!(payment_count(&pool, &id).await, 2);

    // 訂單頁的 payment 是最新那筆
    let (_, detail, _) = common::send(
        &app,
        common::req("GET", &format!("/api/orders/{id}?t={token}"), None, None),
    )
    .await;
    assert_eq!(detail["payment"]["method"], "atm");
    assert_eq!(detail["payment"]["status"], "pending");

    // 沒帶 payment_method → 沿用最近一筆（atm）；流水 03
    let (status, body, _) = common::send(
        &app,
        common::req(
            "POST",
            &format!("/api/orders/{id}/repay?t={token}"),
            None,
            Some(json!({})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["ecpay"]["fields"]["MerchantTradeNo"], format!("{order_no}03"));
    assert_eq!(body["ecpay"]["fields"]["ChoosePayment"], "ATM");
    assert_eq!(payment_count(&pool, &id).await, 3);
}

#[sqlx::test(migrations = "./migrations")]
async fn repay_rejects_non_pending_and_disabled_method(pool: PgPool) {
    let app = common::app(pool.clone());
    let (id, _, token) = place_order(&app, &pool, None).await;

    // 關掉 ATM
    sqlx::query("UPDATE settings SET value = '{\"credit\": true, \"atm\": false, \"cvs_code\": true}' WHERE key = 'payment_methods'")
        .execute(&pool)
        .await
        .unwrap();
    let (status, body, _) = common::send(
        &app,
        common::req(
            "POST",
            &format!("/api/orders/{id}/repay?t={token}"),
            None,
            Some(json!({ "payment_method": "atm" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "VALIDATION");
    assert_eq!(body["error"]["details"]["fields"]["payment_method"], "這個付款方式目前沒有開放");

    // 已付款（直接改 DB 模擬）→ ORDER_NOT_PAYABLE
    sqlx::query("UPDATE orders SET status = 'paid', paid_at = now() WHERE id = $1")
        .bind(uuid(&id))
        .execute(&pool)
        .await
        .unwrap();
    let (status, body, _) = common::send(
        &app,
        common::req(
            "POST",
            &format!("/api/orders/{id}/repay?t={token}"),
            None,
            Some(json!({})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "ORDER_NOT_PAYABLE");
    assert_eq!(payment_count(&pool, &id).await, 1);

    // 取消的也不行
    sqlx::query("UPDATE orders SET status = 'cancelled', paid_at = NULL WHERE id = $1")
        .bind(uuid(&id))
        .execute(&pool)
        .await
        .unwrap();
    let (status, body, _) = common::send(
        &app,
        common::req(
            "POST",
            &format!("/api/orders/{id}/repay?t={token}"),
            None,
            Some(json!({})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["error"]["code"], "ORDER_NOT_PAYABLE");
}

#[sqlx::test(migrations = "./migrations")]
async fn repay_needs_viewer(pool: PgPool) {
    let app = common::app(pool.clone());
    let (id, _, _) = place_order(&app, &pool, None).await;
    let (status, _, _) = common::send(
        &app,
        common::req("POST", &format!("/api/orders/{id}/repay"), None, Some(json!({}))),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _, _) = common::send(
        &app,
        common::req("POST", &format!("/api/orders/{id}/repay?t=wrong"), None, Some(json!({}))),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(payment_count(&pool, &id).await, 1);

    // 會員用 cookie；ClientBackURL 不帶 ?t=
    let cookie = common::register_cookie(&app, "m@test.local", "password123", "甲").await;
    let (mid, order_no, _) = place_order(&app, &pool, Some(&cookie)).await;
    let (status, body, _) = common::send(
        &app,
        common::req("POST", &format!("/api/orders/{mid}/repay"), Some(&cookie), Some(json!({}))),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["ecpay"]["fields"]["MerchantTradeNo"], format!("{order_no}02"));
    assert_eq!(
        body["ecpay"]["fields"]["ClientBackURL"],
        format!("{}/orders/{mid}", common::TEST_ORIGIN)
    );
}
```

Run: `export PATH="$HOME/.cargo/bin:$PATH" && DATABASE_URL=postgres://dog_shop:dog_shop@localhost:5435/dog_shop cargo test --manifest-path api/Cargo.toml --test repay`
Expected: 全部失敗於 404（路由不存在）。

- [ ] **Step 2: `api/src/domain/payments.rs` 檔尾加**

```rust
/// 重新付款（規格 §7 第 9 點）：新列、新 merchant_trade_no（order_no + 兩碼流水，規格 §3）。
/// 只有 pending_payment 能重付（ORDER_NOT_PAYABLE）。鎖訂單列，兩個同時重付不會拿到同一個流水
pub async fn create_repayment(db: &PgPool, order_id: Uuid, method: &str) -> Result<Payment, ApiError> {
    let mut tx = db.begin().await?;
    let row: Option<(String, String, i32)> =
        sqlx::query_as("SELECT order_no, status, total FROM orders WHERE id = $1 FOR UPDATE")
            .bind(order_id)
            .fetch_optional(&mut *tx)
            .await?;
    let Some((order_no, status, total)) = row else {
        return Err(ApiError::NotFound);
    };
    if status != STATUS_PENDING_PAYMENT {
        return Err(ApiError::OrderNotPayable);
    }
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM payments WHERE order_id = $1")
        .bind(order_id)
        .fetch_one(&mut *tx)
        .await?;
    let seq = count + 1;
    if seq > 99 {
        return Err(ApiError::OrderNotPayable);
    }
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO payments (id, order_id, merchant_trade_no, method, amount) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(id)
    .bind(order_id)
    .bind(format!("{order_no}{seq:02}"))
    .bind(method)
    .bind(total)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    get(db, id)
        .await?
        .ok_or_else(|| ApiError::Internal(anyhow::anyhow!("剛建立的 payment {id} 不見了")))
}
```

- [ ] **Step 3: `api/src/routes/orders.rs`**

(a) `use crate::{ domain::{ orders::{...}, settings, users::User }, ...}` 改成也引入 `payments`：

```rust
    domain::{
        orders::{self, OrderCreated, OrderDetail, OrderInput, Viewer},
        payments, settings,
        users::User,
    },
```

(b) `router()` 的 `Router::new()` 加一條（放在 cancel 那條之後）：

```rust
        .route("/api/orders/{id}/repay", post(repay))
```

(c) 檔尾加：

```rust
#[derive(Deserialize, Default)]
pub struct RepayBody {
    /// 沒帶就沿用最近一筆付款的方式
    #[serde(default)]
    pub payment_method: Option<String>,
}

#[derive(Serialize)]
pub struct RepayResponse {
    pub ecpay: CheckoutForm,
}

/// 重新付款（規格 §7 第 9 點、§10）：新 payments 列與新 merchant_trade_no，回新的綠界表單。
/// body 至少要是 `{}`（與規格不同之處 23）
async fn repay(
    CurrentUser(user): CurrentUser,
    State(state): State<AppState>,
    AppPath(id): AppPath<Uuid>,
    AppQuery(query): AppQuery<ViewerQuery>,
    AppJson(body): AppJson<RepayBody>,
) -> ApiResult<Json<RepayResponse>> {
    let (detail, viewer) = find_order(&state, id, user.as_ref(), query.t.as_deref()).await?;
    if detail.order.status != orders::STATUS_PENDING_PAYMENT {
        return Err(ApiError::OrderNotPayable);
    }
    let method = body
        .payment_method
        .as_deref()
        .map(str::trim)
        .filter(|m| !m.is_empty())
        .map(str::to_string)
        .or_else(|| detail.payment.as_ref().map(|p| p.method.clone()))
        .ok_or_else(|| ApiError::field("payment_method", "請選擇付款方式"))?;
    let enabled = settings::get_all(&state.db).await?.payment_methods;
    let allowed = match method.as_str() {
        orders::PAYMENT_CREDIT => enabled.credit,
        orders::PAYMENT_ATM => enabled.atm,
        orders::PAYMENT_CVS_CODE => enabled.cvs_code,
        _ => false,
    };
    if !allowed {
        return Err(ApiError::field("payment_method", "這個付款方式目前沒有開放"));
    }
    payments::create_repayment(&state.db, id, &method).await?;
    let guest_token = match &viewer {
        Viewer::Guest(token) => Some(token.as_str()),
        Viewer::User(_) => None,
    };
    let ecpay = checkout_form_for(&state, id, guest_token).await?;
    Ok(Json(RepayResponse { ecpay }))
}
```

- [ ] **Step 4: 跑測試、clippy、fmt**

Run（背景）: `export PATH="$HOME/.cargo/bin:$PATH" && cargo fmt --all --manifest-path api/Cargo.toml && DATABASE_URL=postgres://dog_shop:dog_shop@localhost:5435/dog_shop cargo test --manifest-path api/Cargo.toml && cargo clippy --all-targets --manifest-path api/Cargo.toml -- -D warnings`
Expected: 全綠（repay 3 個新增）。

- [ ] **Step 5: Commit**

```bash
git add api/src/domain/payments.rs api/src/routes/orders.rs api/tests/repay.rs
git commit -m "feat(api): POST /api/orders/{id}/repay 重新付款（新 merchant_trade_no、可換付款方式）"
```

---
### Task 8: jobs worker 核心 + 排程工作（過期未付款、自動完成、清理）

**Files:**
- Create: `api/src/jobs/mod.rs`
- Create: `api/src/jobs/worker.rs`
- Create: `api/src/jobs/scheduled.rs`
- Create: `api/src/jobs/handlers.rs`（骨架；Task 9、10 填內容）
- Modify: `api/src/lib.rs`（加 `pub mod jobs;`）
- Test: `api/tests/jobs_worker.rs`

**Interfaces:**
- Consumes: `jobs` 表（計畫 2）、`orders::cancel_in_tx`（Task 2）、`payments::{PAYMENT_PENDING, PAYMENT_EXPIRED}`（Task 6）。
- Produces：
  - `jobs::start(state: AppState)`（在 main 呼叫一次；Task 9 才接上）、`jobs::{POLL_INTERVAL, EXPIRE_INTERVAL, AUTO_COMPLETE_INTERVAL, PURGE_INTERVAL}`
  - `jobs::worker::Job { id: i64, kind: String, payload: Value, attempts: i32, max_attempts: i32 }` + `is_last_attempt(&self) -> bool`
  - `jobs::worker::{claim(db, limit), backoff_minutes(attempts) -> i32, run_once_with(db, handler), run_once(state), run(state), requeue_stale(db), BATCH, STALE_RUNNING_MINUTES}`
  - `jobs::scheduled::{run_every(every, name, state, task), expire_unpaid_orders(db), auto_complete_shipped(db), purge_expired(db)}`（都回 `anyhow::Result<usize>`）
  - `jobs::handlers::run(state, &Job) -> anyhow::Result<()>`（本任務對所有 kind 回錯誤；Task 9、10 填）

- [ ] **Step 1: 寫失敗的整合測試 `api/tests/jobs_worker.rs`**

```rust
mod common;

use std::sync::{Arc, Mutex};

use chrono::{DateTime, Duration, Utc};
use dog_shop_api::domain::{jobs, orders};
use dog_shop_api::domain::orders::{HomeAddress, InvoiceInput, OrderInput, OrderItemInput};
use dog_shop_api::jobs::{scheduled, worker};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

async fn enqueue(pool: &PgPool, kind: &str, payload: Value) -> i64 {
    let mut tx = pool.begin().await.unwrap();
    jobs::enqueue(&mut tx, kind, payload, None).await.unwrap();
    tx.commit().await.unwrap();
    sqlx::query_scalar("SELECT max(id) FROM jobs")
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn job_row(pool: &PgPool, id: i64) -> (String, i32, Option<String>, Value, DateTime<Utc>) {
    sqlx::query_as("SELECT status, attempts, last_error, payload, run_at FROM jobs WHERE id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrations = "./migrations")]
async fn success_marks_done_and_clears_payload(pool: PgPool) {
    let id = enqueue(&pool, "test", json!({ "secret": "x" })).await;
    let seen = Arc::new(Mutex::new(Vec::<worker::Job>::new()));
    let sink = seen.clone();
    let n = worker::run_once_with(&pool, |job| {
        let sink = sink.clone();
        async move {
            sink.lock().unwrap().push(job);
            Ok(())
        }
    })
    .await
    .unwrap();
    assert_eq!(n, 1);
    {
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].id, id);
        assert_eq!(seen[0].kind, "test");
        assert_eq!(seen[0].attempts, 1, "認領時 +1");
        assert_eq!(seen[0].max_attempts, 5);
        assert_eq!(seen[0].payload["secret"], "x");
        assert!(!seen[0].is_last_attempt());
    }
    let (status, attempts, err, payload, _) = job_row(&pool, id).await;
    assert_eq!((status.as_str(), attempts, err), ("done", 1, None));
    assert_eq!(payload, json!({}), "做完不留個資");

    // 第二輪沒東西
    let n = worker::run_once_with(&pool, |_| async { Ok(()) }).await.unwrap();
    assert_eq!(n, 0);
}

#[sqlx::test(migrations = "./migrations")]
async fn failure_backs_off_then_fails_for_good(pool: PgPool) {
    let id = enqueue(&pool, "test", json!({ "keep": 1 })).await;
    sqlx::query("UPDATE jobs SET max_attempts = 2")
        .execute(&pool)
        .await
        .unwrap();
    let before = Utc::now();
    let n = worker::run_once_with(&pool, |_| async { Err(anyhow::anyhow!("boom")) })
        .await
        .unwrap();
    assert_eq!(n, 1);
    let (status, attempts, err, payload, run_at) = job_row(&pool, id).await;
    assert_eq!((status.as_str(), attempts), ("queued", 1));
    assert!(err.unwrap().contains("boom"));
    assert_eq!(payload, json!({ "keep": 1 }));
    // 2^1 = 2 分鐘後
    assert!(
        run_at >= before + Duration::minutes(1) && run_at <= Utc::now() + Duration::minutes(3),
        "{run_at}"
    );

    // run_at 還沒到 → 不會被認領
    let n = worker::run_once_with(&pool, |_| async { Ok(()) }).await.unwrap();
    assert_eq!(n, 0);
    sqlx::query("UPDATE jobs SET run_at = now()")
        .execute(&pool)
        .await
        .unwrap();

    // 第二次（= max_attempts）再失敗 → failed，payload 保留
    let n = worker::run_once_with(&pool, |job| async move {
        assert!(job.is_last_attempt());
        Err(anyhow::anyhow!("boom again"))
    })
    .await
    .unwrap();
    assert_eq!(n, 1);
    let (status, attempts, err, payload, _) = job_row(&pool, id).await;
    assert_eq!((status.as_str(), attempts), ("failed", 2));
    assert!(err.unwrap().contains("boom again"));
    assert_eq!(payload, json!({ "keep": 1 }));
    let n = worker::run_once_with(&pool, |_| async { Ok(()) }).await.unwrap();
    assert_eq!(n, 0, "failed 不會再被認領");
}

#[sqlx::test(migrations = "./migrations")]
async fn claims_in_id_order_up_to_batch(pool: PgPool) {
    for i in 0..12 {
        enqueue(&pool, "test", json!({ "i": i })).await;
    }
    let seen = Arc::new(Mutex::new(Vec::<i64>::new()));
    let sink = seen.clone();
    let n = worker::run_once_with(&pool, |job| {
        let sink = sink.clone();
        async move {
            sink.lock().unwrap().push(job.payload["i"].as_i64().unwrap());
            Ok(())
        }
    })
    .await
    .unwrap();
    assert_eq!(n, 10);
    assert_eq!(*seen.lock().unwrap(), (0..10).collect::<Vec<i64>>());
    let n = worker::run_once_with(&pool, |_| async { Ok(()) }).await.unwrap();
    assert_eq!(n, 2);
}

#[sqlx::test(migrations = "./migrations")]
async fn stale_running_is_requeued(pool: PgPool) {
    enqueue(&pool, "test", json!({})).await;
    sqlx::query("UPDATE jobs SET status = 'running', updated_at = now() - interval '11 minutes'")
        .execute(&pool)
        .await
        .unwrap();
    let n = worker::run_once_with(&pool, |_| async { Ok(()) }).await.unwrap();
    assert_eq!(n, 0, "running 不會被認領");
    assert_eq!(worker::requeue_stale(&pool).await.unwrap(), 1);
    let n = worker::run_once_with(&pool, |_| async { Ok(()) }).await.unwrap();
    assert_eq!(n, 1);

    // 剛開始跑的 running 不會被撿
    enqueue(&pool, "test", json!({})).await;
    sqlx::query("UPDATE jobs SET status = 'running', updated_at = now() WHERE status = 'queued'")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(worker::requeue_stale(&pool).await.unwrap(), 0);
}

#[test]
fn backoff_doubles() {
    assert_eq!(worker::backoff_minutes(1), 2);
    assert_eq!(worker::backoff_minutes(2), 4);
    assert_eq!(worker::backoff_minutes(5), 32);
    assert_eq!(worker::backoff_minutes(99), 1024, "有上限");
}

// ───── 排程工作 ─────

fn input(items: Vec<(Uuid, i32)>) -> OrderInput {
    OrderInput {
        items: items
            .into_iter()
            .map(|(variant_id, qty)| OrderItemInput { variant_id, qty })
            .collect(),
        email: "buyer@test.local".to_string(),
        recipient_name: "王小明".to_string(),
        recipient_phone: "0912345678".to_string(),
        shipping_method: "home".to_string(),
        cvs_store_token: None,
        address: Some(HomeAddress {
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
        payment_method: "atm".to_string(),
        note: String::new(),
    }
}

async fn stock_of(pool: &PgPool, variant_id: Uuid) -> i32 {
    sqlx::query_scalar("SELECT stock FROM product_variants WHERE id = $1")
        .bind(variant_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn order_status(pool: &PgPool, id: Uuid) -> (String, Option<String>) {
    sqlx::query_as("SELECT status, cancel_reason FROM orders WHERE id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrations = "./migrations")]
async fn expires_orders_by_created_at_or_payment_expiry(pool: PgPool) {
    let (variant, _) = common::active_product(&pool, "A", 100, 10).await;
    let old = orders::create_order(&pool, input(vec![(variant, 2)]), None).await.unwrap();
    let fresh = orders::create_order(&pool, input(vec![(variant, 1)]), None).await.unwrap();
    let atm_expired = orders::create_order(&pool, input(vec![(variant, 1)]), None).await.unwrap();
    let atm_alive = orders::create_order(&pool, input(vec![(variant, 1)]), None).await.unwrap();
    assert_eq!(stock_of(&pool, variant).await, 5);

    // 沒有繳費期限、3 天多 → 過期
    sqlx::query("UPDATE orders SET created_at = now() - interval '3 days 1 minute' WHERE id = $1")
        .bind(old.order_id)
        .execute(&pool)
        .await
        .unwrap();
    // 繳費期限 3 小時前 → 過了 2 小時緩衝
    sqlx::query("UPDATE payments SET expire_at = now() - interval '3 hours' WHERE order_id = $1")
        .bind(atm_expired.order_id)
        .execute(&pool)
        .await
        .unwrap();
    // 繳費期限 1 小時前 → 還在緩衝內；就算 created_at 很久以前也以 expire_at 為準
    sqlx::query("UPDATE payments SET expire_at = now() - interval '1 hour' WHERE order_id = $1")
        .bind(atm_alive.order_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE orders SET created_at = now() - interval '10 days' WHERE id = $1")
        .bind(atm_alive.order_id)
        .execute(&pool)
        .await
        .unwrap();

    assert_eq!(scheduled::expire_unpaid_orders(&pool).await.unwrap(), 2);
    assert_eq!(
        order_status(&pool, old.order_id).await,
        ("cancelled".to_string(), Some("expired".to_string()))
    );
    assert_eq!(
        order_status(&pool, atm_expired.order_id).await,
        ("cancelled".to_string(), Some("expired".to_string()))
    );
    assert_eq!(order_status(&pool, fresh.order_id).await.0, "pending_payment");
    assert_eq!(order_status(&pool, atm_alive.order_id).await.0, "pending_payment");
    assert_eq!(stock_of(&pool, variant).await, 8, "5 + 2 + 1");
    let payment_status: String = sqlx::query_scalar("SELECT status FROM payments WHERE order_id = $1")
        .bind(old.order_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(payment_status, "expired");
    let alive_payment: String = sqlx::query_scalar("SELECT status FROM payments WHERE order_id = $1")
        .bind(atm_alive.order_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(alive_payment, "pending");

    // 再跑一次沒有東西
    assert_eq!(scheduled::expire_unpaid_orders(&pool).await.unwrap(), 0);
    assert_eq!(stock_of(&pool, variant).await, 8);
}

#[sqlx::test(migrations = "./migrations")]
async fn auto_completes_shipped_after_14_days_unless_returned(pool: PgPool) {
    let (variant, _) = common::active_product(&pool, "A", 100, 10).await;
    let a = orders::create_order(&pool, input(vec![(variant, 1)]), None).await.unwrap();
    let b = orders::create_order(&pool, input(vec![(variant, 1)]), None).await.unwrap();
    let c = orders::create_order(&pool, input(vec![(variant, 1)]), None).await.unwrap();
    for (order, days, shipment_status) in [(&a, 15, "created"), (&b, 15, "returned"), (&c, 1, "created")] {
        sqlx::query("UPDATE orders SET status = 'shipped', paid_at = now(), shipped_at = now() - make_interval(days => $2) WHERE id = $1")
            .bind(order.order_id)
            .bind(days)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("UPDATE shipments SET status = $2 WHERE order_id = $1")
            .bind(order.order_id)
            .bind(shipment_status)
            .execute(&pool)
            .await
            .unwrap();
    }
    assert_eq!(scheduled::auto_complete_shipped(&pool).await.unwrap(), 1);
    assert_eq!(order_status(&pool, a.order_id).await.0, "completed");
    assert_eq!(order_status(&pool, b.order_id).await.0, "shipped", "退回的不自動完成");
    assert_eq!(order_status(&pool, c.order_id).await.0, "shipped", "還沒 14 天");
    let completed_at: Option<DateTime<Utc>> =
        sqlx::query_scalar("SELECT completed_at FROM orders WHERE id = $1")
            .bind(a.order_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(completed_at.is_some());
}

#[sqlx::test(migrations = "./migrations")]
async fn purge_removes_expired_rows(pool: PgPool) {
    let hash = dog_shop_api::auth::password::hash_password("password123").unwrap();
    let user = dog_shop_api::domain::users::create(&pool, "p@test.local", &hash, "小美", "customer")
        .await
        .unwrap();
    // 兩個 session：一個過期
    let expired_sid = dog_shop_api::auth::session::create(&pool, user.id).await.unwrap();
    dog_shop_api::auth::session::create(&pool, user.id).await.unwrap();
    sqlx::query("UPDATE sessions SET expires_at = now() - interval '1 minute' WHERE id = $1")
        .bind(expired_sid)
        .execute(&pool)
        .await
        .unwrap();
    // 重設 token：一個過期、一個用過、一個有效
    dog_shop_api::domain::password_resets::create(&pool, user.id).await.unwrap();
    sqlx::query("UPDATE password_resets SET expires_at = now() - interval '1 minute'")
        .execute(&pool)
        .await
        .unwrap();
    let used = dog_shop_api::domain::password_resets::create(&pool, user.id).await.unwrap();
    dog_shop_api::domain::password_resets::consume(&pool, &used).await.unwrap();
    dog_shop_api::domain::password_resets::create(&pool, user.id).await.unwrap();
    // 門市選擇：過期
    common::cvs_store_token(&pool).await;
    sqlx::query("UPDATE cvs_store_selections SET expires_at = now() - interval '1 minute'")
        .execute(&pool)
        .await
        .unwrap();
    // jobs：一個 31 天前做完、一個剛做完、一個 queued
    let old_done = enqueue(&pool, "test", json!({})).await;
    sqlx::query("UPDATE jobs SET status = 'done', updated_at = now() - interval '31 days' WHERE id = $1")
        .bind(old_done)
        .execute(&pool)
        .await
        .unwrap();
    let recent_done = enqueue(&pool, "test", json!({})).await;
    sqlx::query("UPDATE jobs SET status = 'done' WHERE id = $1")
        .bind(recent_done)
        .execute(&pool)
        .await
        .unwrap();
    enqueue(&pool, "test", json!({})).await;

    let n = scheduled::purge_expired(&pool).await.unwrap();
    assert_eq!(n, 5, "1 session + 2 resets + 1 store + 1 job");
    let sessions: i64 = sqlx::query_scalar("SELECT count(*) FROM sessions").fetch_one(&pool).await.unwrap();
    let resets: i64 = sqlx::query_scalar("SELECT count(*) FROM password_resets").fetch_one(&pool).await.unwrap();
    let stores: i64 = sqlx::query_scalar("SELECT count(*) FROM cvs_store_selections").fetch_one(&pool).await.unwrap();
    let jobs_left: i64 = sqlx::query_scalar("SELECT count(*) FROM jobs").fetch_one(&pool).await.unwrap();
    assert_eq!((sessions, resets, stores, jobs_left), (1, 1, 0, 2));
}
```

Run: `export PATH="$HOME/.cargo/bin:$PATH" && DATABASE_URL=postgres://dog_shop:dog_shop@localhost:5435/dog_shop cargo test --manifest-path api/Cargo.toml --test jobs_worker`
Expected: 編譯錯誤（`dog_shop_api::jobs` 不存在）。

- [ ] **Step 2: 建 `api/src/jobs/mod.rs`**

```rust
//! 背景工作（規格 §9）：worker 認領 jobs 表的工作並執行；scheduled 是定時掃描（不走 jobs 表）
pub mod handlers;
pub mod scheduled;
pub mod worker;

use std::time::Duration;

use crate::state::AppState;

/// worker 每 2 秒看一次 jobs 表（規格 §9）
pub const POLL_INTERVAL: Duration = Duration::from_secs(2);
/// 過期未付款每 10 分鐘（規格 §9）
pub const EXPIRE_INTERVAL: Duration = Duration::from_secs(10 * 60);
/// 出貨 14 天自動完成每小時（規格 §9）
pub const AUTO_COMPLETE_INTERVAL: Duration = Duration::from_secs(60 * 60);
/// 清理每天（規格 §9 purge_expired_sessions，擴大到其他過期資料）
pub const PURGE_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);

/// 在 main 裡呼叫一次：開四個 tokio task，各自無限迴圈；錯誤只記 log 不會停
pub fn start(state: AppState) {
    tokio::spawn(worker::run(state.clone()));
    tokio::spawn(scheduled::run_every(
        EXPIRE_INTERVAL,
        "expire_unpaid_orders",
        state.clone(),
        |s| async move {
            // 順便把當機殘留的 running job 撿回來（與規格不同之處 27）
            worker::requeue_stale(&s.db).await?;
            scheduled::expire_unpaid_orders(&s.db).await
        },
    ));
    tokio::spawn(scheduled::run_every(
        AUTO_COMPLETE_INTERVAL,
        "auto_complete_shipped",
        state.clone(),
        |s| async move { scheduled::auto_complete_shipped(&s.db).await },
    ));
    tokio::spawn(scheduled::run_every(
        PURGE_INTERVAL,
        "purge_expired",
        state,
        |s| async move { scheduled::purge_expired(&s.db).await },
    ));
    tracing::info!("jobs worker 與排程工作已啟動");
}
```

- [ ] **Step 3: 建 `api/src/jobs/worker.rs`**

```rust
//! 認領 → 執行 → 標記。認領用一句 UPDATE … FROM (SELECT … FOR UPDATE SKIP LOCKED LIMIT 10)，
//! 多個 worker 行程也不會搶到同一筆；當機留下的 running 由 requeue_stale 撿回來（與規格不同之處 27）
use std::future::Future;

use serde_json::Value;
use sqlx::PgPool;

use crate::{
    jobs::{POLL_INTERVAL, handlers},
    state::AppState,
};

/// 一輪最多認領幾筆（規格 §9 LIMIT 10）
pub const BATCH: i64 = 10;
/// running 超過這麼久當作當機殘留
pub const STALE_RUNNING_MINUTES: i32 = 10;
/// 退避上限：2^attempts 分鐘，最多 2^10（max_attempts 預設 5，實際到不了）
const MAX_BACKOFF_EXP: i32 = 10;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Job {
    pub id: i64,
    pub kind: String,
    pub payload: Value,
    /// 認領時已 +1：第一次執行是 1
    pub attempts: i32,
    pub max_attempts: i32,
}

impl Job {
    /// 這次再失敗就會被標 failed；handler 用它決定要不要做「最後一次」的收尾（例如把發票標 failed）
    pub fn is_last_attempt(&self) -> bool {
        self.attempts >= self.max_attempts
    }
}

/// 認領一批：queued 且 run_at 到了，依 id 排序，跳過被別人鎖住的；認領同時標 running、attempts + 1
pub async fn claim(db: &PgPool, limit: i64) -> Result<Vec<Job>, sqlx::Error> {
    sqlx::query_as::<_, Job>(
        "WITH picked AS (
             SELECT id FROM jobs
             WHERE status = 'queued' AND run_at <= now()
             ORDER BY id
             FOR UPDATE SKIP LOCKED
             LIMIT $1
         )
         UPDATE jobs j SET status = 'running', attempts = j.attempts + 1, updated_at = now()
         FROM picked WHERE j.id = picked.id
         RETURNING j.id, j.kind, j.payload, j.attempts, j.max_attempts",
    )
    .bind(limit)
    .fetch_all(db)
    .await
}

/// 規格 §9：`run_at = now() + 2^attempts 分鐘`
pub fn backoff_minutes(attempts: i32) -> i32 {
    2_i32.pow(attempts.clamp(0, MAX_BACKOFF_EXP) as u32)
}

async fn mark_done(db: &PgPool, id: i64) -> Result<(), sqlx::Error> {
    // payload 清成 {} 不留個資（計畫 2 交接 2）
    sqlx::query(
        "UPDATE jobs SET status = 'done', payload = '{}'::jsonb, last_error = NULL, updated_at = now() WHERE id = $1",
    )
    .bind(id)
    .execute(db)
    .await?;
    Ok(())
}

async fn mark_failed_attempt(db: &PgPool, job: &Job, error: &str) -> Result<(), sqlx::Error> {
    let error: String = error.chars().take(1000).collect();
    if job.is_last_attempt() {
        sqlx::query("UPDATE jobs SET status = 'failed', last_error = $2, updated_at = now() WHERE id = $1")
            .bind(job.id)
            .bind(error)
            .execute(db)
            .await?;
    } else {
        sqlx::query(
            "UPDATE jobs SET status = 'queued', run_at = now() + make_interval(mins => $3), last_error = $2, updated_at = now()
             WHERE id = $1",
        )
        .bind(job.id)
        .bind(error)
        .bind(backoff_minutes(job.attempts))
        .execute(db)
        .await?;
    }
    Ok(())
}

/// 跑一輪：認領、逐筆執行、標記。回處理的筆數。handler 可注入（測試用假的）
pub async fn run_once_with<F, Fut>(db: &PgPool, handler: F) -> anyhow::Result<usize>
where
    F: Fn(Job) -> Fut,
    Fut: Future<Output = anyhow::Result<()>>,
{
    let jobs = claim(db, BATCH).await?;
    let n = jobs.len();
    for job in jobs {
        match handler(job.clone()).await {
            Ok(()) => mark_done(db, job.id).await?,
            Err(e) => {
                let msg = format!("{e:#}");
                tracing::warn!(
                    job_id = job.id,
                    kind = %job.kind,
                    attempts = job.attempts,
                    failed_for_good = job.is_last_attempt(),
                    error = %msg,
                    "job 失敗"
                );
                mark_failed_attempt(db, &job, &msg).await?;
            }
        }
    }
    Ok(n)
}

/// 正式的一輪：用 handlers::run
pub async fn run_once(state: &AppState) -> anyhow::Result<usize> {
    run_once_with(&state.db, |job| async move { handlers::run(state, &job).await }).await
}

/// 無限迴圈：每 2 秒跑一輪；一輪認領滿 BATCH 就馬上再跑（佇列長時不用等）
pub async fn run(state: AppState) {
    if let Err(e) = requeue_stale(&state.db).await {
        tracing::error!(error = %format!("{e:#}"), "啟動時重排 stale job 失敗");
    }
    loop {
        match run_once(&state).await {
            Ok(n) if n >= BATCH as usize => continue,
            Ok(_) => {}
            Err(e) => tracing::error!(error = %format!("{e:#}"), "worker 這一輪失敗"),
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

/// running 超過 STALE_RUNNING_MINUTES 的重新排隊（不改 attempts；認領時會再 +1）
pub async fn requeue_stale(db: &PgPool) -> anyhow::Result<usize> {
    let n = sqlx::query(
        "UPDATE jobs SET status = 'queued', run_at = now(), updated_at = now()
         WHERE status = 'running' AND updated_at < now() - make_interval(mins => $1)",
    )
    .bind(STALE_RUNNING_MINUTES)
    .execute(db)
    .await?
    .rows_affected();
    if n > 0 {
        tracing::warn!(count = n, "重新排隊當機殘留的 running job");
    }
    Ok(n as usize)
}
```

- [ ] **Step 4: 建 `api/src/jobs/scheduled.rs`**

```rust
//! 排程型工作（規格 §9）：不走 jobs 表，直接定時掃
use std::{future::Future, time::Duration};

use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    domain::{orders, payments},
    state::AppState,
};

/// 每 `every` 跑一次 `task`（第一次立刻跑）；錯誤記 log 不中斷
pub async fn run_every<F, Fut>(every: Duration, name: &'static str, state: AppState, task: F)
where
    F: Fn(AppState) -> Fut + Send + 'static,
    Fut: Future<Output = anyhow::Result<usize>> + Send,
{
    let mut tick = tokio::time::interval(every);
    loop {
        tick.tick().await;
        match task(state.clone()).await {
            Ok(0) => {}
            Ok(n) => tracing::info!(task = name, affected = n, "排程工作完成"),
            Err(e) => tracing::error!(task = name, error = %format!("{e:#}"), "排程工作失敗"),
        }
    }
}

/// 過期未付款（規格 §5）：到期時間 = 該訂單所有 payments.expire_at 的最大值 + 2 小時；
/// 沒有任何繳費期限就 created_at + 3 天。到期 → cancelled(expired)、歸還庫存、pending 的 payments 標 expired。
/// 每筆一個交易（一筆失敗不影響其他）；一輪最多 500 筆，下一輪再繼續
pub async fn expire_unpaid_orders(db: &PgPool) -> anyhow::Result<usize> {
    let ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT o.id FROM orders o
         WHERE o.status = 'pending_payment'
           AND COALESCE(
                 (SELECT max(p.expire_at) FROM payments p WHERE p.order_id = o.id) + interval '2 hours',
                 o.created_at + interval '3 days'
               ) <= now()
         ORDER BY o.created_at
         LIMIT 500",
    )
    .fetch_all(db)
    .await?;
    let mut n = 0;
    for id in ids {
        let mut tx = db.begin().await?;
        if orders::cancel_in_tx(&mut tx, id, "expired").await? {
            sqlx::query("UPDATE payments SET status = $2, updated_at = now() WHERE order_id = $1 AND status = $3")
                .bind(id)
                .bind(payments::PAYMENT_EXPIRED)
                .bind(payments::PAYMENT_PENDING)
                .execute(&mut *tx)
                .await?;
            n += 1;
        }
        tx.commit().await?;
    }
    Ok(n)
}

/// 出貨超過 14 天且沒被退回 → completed（規格 §9）。shipped_at 由計畫 4 的出貨寫入
pub async fn auto_complete_shipped(db: &PgPool) -> anyhow::Result<usize> {
    let n = sqlx::query(
        "UPDATE orders o SET status = 'completed', completed_at = now()
         FROM shipments s
         WHERE s.order_id = o.id
           AND o.status = 'shipped'
           AND o.shipped_at <= now() - interval '14 days'
           AND s.status <> 'returned'",
    )
    .execute(db)
    .await?
    .rows_affected();
    Ok(n as usize)
}

/// 每天清理：過期 session、過期或用過的重設 token、過期門市選擇、30 天前做完的 job
pub async fn purge_expired(db: &PgPool) -> anyhow::Result<usize> {
    let mut n = 0u64;
    n += sqlx::query("DELETE FROM sessions WHERE expires_at <= now()")
        .execute(db)
        .await?
        .rows_affected();
    n += sqlx::query("DELETE FROM password_resets WHERE expires_at <= now() OR used_at IS NOT NULL")
        .execute(db)
        .await?
        .rows_affected();
    n += sqlx::query("DELETE FROM cvs_store_selections WHERE expires_at <= now()")
        .execute(db)
        .await?
        .rows_affected();
    n += sqlx::query("DELETE FROM jobs WHERE status = 'done' AND updated_at <= now() - interval '30 days'")
        .execute(db)
        .await?
        .rows_affected();
    Ok(n as usize)
}
```

- [ ] **Step 5: 建 `api/src/jobs/handlers.rs`（骨架）**

```rust
//! job 種類對應的執行函式。Task 9 加 send_email、Task 10 加 issue_invoice
use crate::{
    domain::jobs::{KIND_ISSUE_INVOICE, KIND_SEND_EMAIL},
    jobs::worker::Job,
    state::AppState,
};

pub async fn run(_state: &AppState, job: &Job) -> anyhow::Result<()> {
    match job.kind.as_str() {
        KIND_SEND_EMAIL => anyhow::bail!("send_email handler 尚未實作（Task 9）"),
        KIND_ISSUE_INVOICE => anyhow::bail!("issue_invoice handler 尚未實作（Task 10）"),
        other => anyhow::bail!("未知的 job kind：{other}"),
    }
}
```

`api/src/lib.rs` 在 `pub mod extract;` 之後加 `pub mod jobs;`。本任務**不**在 `main.rs` 呼叫 `jobs::start`（Task 9 接上，那時 send_email 才有東西可做）。

- [ ] **Step 6: 跑測試、clippy、fmt**

Run（背景）: `export PATH="$HOME/.cargo/bin:$PATH" && cargo fmt --all --manifest-path api/Cargo.toml && DATABASE_URL=postgres://dog_shop:dog_shop@localhost:5435/dog_shop cargo test --manifest-path api/Cargo.toml && cargo clippy --all-targets --manifest-path api/Cargo.toml -- -D warnings`
Expected: 全綠（jobs_worker 8 個新增）。clippy 可能對 `jobs::start` 說 `run`／`handlers` 未使用？不會：`start` 是 pub，pub 項目不算 dead code。

- [ ] **Step 7: Commit**

```bash
git add api/src/lib.rs api/src/jobs/mod.rs api/src/jobs/worker.rs api/src/jobs/scheduled.rs api/src/jobs/handlers.rs api/tests/jobs_worker.rs
git commit -m "feat(api): jobs worker（認領、退避、failed、stale 重排）與排程工作（過期未付款、自動完成、清理）"
```

---
### Task 9: Email：`Mailer`、askama 模板、`send_email` handler、worker 隨 api 啟動

**Files:**
- Modify: `api/Cargo.toml`（加 `askama`、`lettre`）
- Create: `api/src/mail/mod.rs`
- Create: `api/src/mail/templates.rs`
- Create: `api/templates/mail/layout.html`、`order_created.{txt,html}`、`payment_instructions.{txt,html}`、`payment_received.{txt,html}`、`order_shipped.{txt,html}`、`invoice_issued.{txt,html}`、`password_reset.{txt,html}`（13 個）
- Modify: `api/src/jobs/handlers.rs`（整檔改寫）
- Modify: `api/src/state.rs`（加 `mailer`）
- Modify: `api/src/main.rs`（建 mailer、`jobs::start`）
- Modify: `api/src/lib.rs`（加 `pub mod mail;`）
- Modify: `api/tests/common/mod.rs`（`state()` 加 mailer；新增 `app_with_state`、`sent_emails`）
- Test: `api/tests/mail_jobs.rs`

**Interfaces:**
- Consumes: `Config.smtp`（Task 1）、`orders::get_detail`（Task 2）、`payments::get`（Task 6）、`worker::Job`（Task 8）、`password_resets::{create, RESET_TTL_MINUTES}`、`auth::tokens::sha256_hex`、`users::find_by_id`、`settings::get_all`。
- Produces：
  - `mail::Email { to, subject, text, html }`（Clone、PartialEq）
  - `mail::Mailer::{Smtp { transport, from }, Log, Capture(Arc<Mutex<Vec<Email>>>)}`；`Mailer::from_config(&Config) -> anyhow::Result<Mailer>`、`Mailer::capture() -> (Mailer, Arc<Mutex<Vec<Email>>>)`、`async fn send(&self, Email) -> anyhow::Result<()>`
  - `mail::templates::{MailItem, OrderCreatedMail, PaymentInstructionsMail, PaymentReceivedMail, OrderShippedMail, InvoiceIssuedMail, PasswordResetMail}` 各有 `render(&self) -> askama::Result<(String, String, String)>`（主旨、純文字、HTML）；`templates::twd(i32) -> String`（`NT$ 1,234`）
  - `AppState.mailer: Arc<Mailer>`
  - `jobs::handlers::run` 的 `send_email` 分支支援六種 template；payload 形狀：`order_created { order_id }`、`payment_instructions { order_id, payment_id }`、`payment_received { order_id }`、`order_shipped { order_id }`（計畫 4 觸發）、`invoice_issued { order_id }`（Task 10 觸發）、`password_reset { user_id }`
  - `jobs::handlers::RESET_THROTTLE_MINUTES = 10`
  - 測試：`common::app_with_state(pool) -> (Router, AppState)`、`common::sent_emails(&AppState) -> Vec<Email>`

- [ ] **Step 1: 加相依套件**

`api/Cargo.toml` 的 `[dependencies]` 加（字母順序）：

```toml
askama = "0.16"
lettre = { version = "0.11", default-features = false, features = [
  "builder",
  "hostname",
  "pool",
  "smtp-transport",
  "tokio1",
  "tokio1-rustls-tls",
] }
```

（`tokio1-rustls-tls` = tokio + rustls + ring + webpki-roots；不用預設的 native-tls，避免 OpenSSL。）

- [ ] **Step 2: 寫失敗的整合測試 `api/tests/mail_jobs.rs`**

```rust
mod common;

use dog_shop_api::domain::orders::{self, HomeAddress, InvoiceInput, OrderInput, OrderItemInput};
use dog_shop_api::domain::{jobs, password_resets, users};
use dog_shop_api::jobs::worker;
use dog_shop_api::state::AppState;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

fn home_input(items: Vec<(Uuid, i32)>, method: &str) -> OrderInput {
    OrderInput {
        items: items
            .into_iter()
            .map(|(variant_id, qty)| OrderItemInput { variant_id, qty })
            .collect(),
        email: "buyer@test.local".to_string(),
        recipient_name: "王小明".to_string(),
        recipient_phone: "0912345678".to_string(),
        shipping_method: "home".to_string(),
        cvs_store_token: None,
        address: Some(HomeAddress {
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
        payment_method: method.to_string(),
        note: String::new(),
    }
}

fn cvs_input(items: Vec<(Uuid, i32)>, token: String) -> OrderInput {
    OrderInput {
        shipping_method: "cvs".to_string(),
        cvs_store_token: Some(token),
        address: None,
        ..home_input(items, "credit")
    }
}

/// 一直跑到沒有可認領的 job
async fn run_all(state: &AppState) {
    while worker::run_once(state).await.unwrap() > 0 {}
}

async fn enqueue(pool: &PgPool, payload: Value) {
    let mut tx = pool.begin().await.unwrap();
    jobs::enqueue(&mut tx, jobs::KIND_SEND_EMAIL, payload, None)
        .await
        .unwrap();
    tx.commit().await.unwrap();
}

async fn job_rows(pool: &PgPool) -> Vec<(String, i32, Value, Option<String>)> {
    sqlx::query_as("SELECT status, attempts, payload, last_error FROM jobs ORDER BY id")
        .fetch_all(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrations = "./migrations")]
async fn order_created_email_goes_to_guest_with_token_link(pool: PgPool) {
    let state = common::state(pool.clone());
    let (variant, _) = common::active_product(&pool, "雞肉狗糧", 300, 5).await;
    let created = orders::create_order(&pool, home_input(vec![(variant, 2)], "credit"), None)
        .await
        .unwrap();
    run_all(&state).await;

    let emails = common::sent_emails(&state);
    assert_eq!(emails.len(), 1);
    let m = &emails[0];
    assert_eq!(m.to, "buyer@test.local");
    assert!(m.subject.contains(&created.order_no) && m.subject.contains("已成立"), "{}", m.subject);
    let link = format!(
        "http://localhost:5173/orders/{}?t={}",
        created.order_id, created.guest_token
    );
    assert!(m.text.contains(&link), "{}", m.text);
    assert!(m.html.contains(&link));
    assert!(m.text.contains("雞肉狗糧（預設）× 2"), "{}", m.text);
    assert!(m.text.contains("NT$ 600") && m.text.contains("NT$ 100") && m.text.contains("NT$ 700"));
    assert!(m.text.contains("信用卡"));
    assert!(m.text.contains("宅配：100臺北市中正區重慶南路一段 122 號"), "{}", m.text);
    assert!(m.html.contains("<!doctype html>") && m.html.contains("<strong>"));

    let jobs = job_rows(&pool).await;
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].0, "done");
    assert_eq!(jobs[0].2, json!({}), "做完不留個資");
}

#[sqlx::test(migrations = "./migrations")]
async fn member_order_link_has_no_token(pool: PgPool) {
    let state = common::state(pool.clone());
    let hash = dog_shop_api::auth::password::hash_password("password123").unwrap();
    let user = users::create(&pool, "m@test.local", &hash, "甲", "customer")
        .await
        .unwrap();
    let (variant, _) = common::active_product(&pool, "A", 100, 5).await;
    let created = orders::create_order(&pool, home_input(vec![(variant, 1)], "credit"), Some(&user))
        .await
        .unwrap();
    run_all(&state).await;
    let emails = common::sent_emails(&state);
    assert_eq!(emails.len(), 1);
    assert_eq!(emails[0].to, "buyer@test.local", "寄到訂單填的 Email");
    assert!(emails[0].text.contains(&format!("http://localhost:5173/orders/{}\n", created.order_id)));
    assert!(!emails[0].text.contains("?t="));
}

#[sqlx::test(migrations = "./migrations")]
async fn payment_instructions_email_has_atm_or_cvs_details(pool: PgPool) {
    let state = common::state(pool.clone());
    let (variant, _) = common::active_product(&pool, "A", 300, 5).await;
    let created = orders::create_order(&pool, home_input(vec![(variant, 2)], "atm"), None)
        .await
        .unwrap();
    let payment_id: Uuid = sqlx::query_scalar("SELECT id FROM payments WHERE order_id = $1")
        .bind(created.order_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE payments SET atm_bank_code = '812', atm_vaccount = '1234567890123456', expire_at = '2026-09-09T15:59:59Z' WHERE id = $1",
    )
    .bind(payment_id)
    .execute(&pool)
    .await
    .unwrap();
    enqueue(
        &pool,
        json!({ "template": "payment_instructions", "order_id": created.order_id, "payment_id": payment_id }),
    )
    .await;
    run_all(&state).await;
    let emails = common::sent_emails(&state);
    assert_eq!(emails.len(), 2, "order_created + payment_instructions");
    let m = &emails[1];
    assert!(m.subject.contains("繳費資訊"), "{}", m.subject);
    for needle in ["ATM 轉帳", "812", "1234567890123456", "2026/09/09 23:59:59", "NT$ 700"] {
        assert!(m.text.contains(needle), "純文字缺 {needle}：{}", m.text);
        assert!(m.html.contains(needle), "HTML 缺 {needle}");
    }
    assert!(!m.text.contains("超商繳費代碼"));

    // 超商代碼
    sqlx::query(
        "UPDATE payments SET method = 'cvs_code', atm_bank_code = NULL, atm_vaccount = NULL, cvs_payment_no = 'LLL26090612345', expire_at = '2026-09-09T07:30:23Z' WHERE id = $1",
    )
    .bind(payment_id)
    .execute(&pool)
    .await
    .unwrap();
    enqueue(
        &pool,
        json!({ "template": "payment_instructions", "order_id": created.order_id, "payment_id": payment_id }),
    )
    .await;
    run_all(&state).await;
    let emails = common::sent_emails(&state);
    assert_eq!(emails.len(), 3);
    let m = &emails[2];
    assert!(m.text.contains("超商代碼繳費") && m.text.contains("LLL26090612345") && m.text.contains("2026/09/09 15:30:23"), "{}", m.text);
    assert!(!m.text.contains("虛擬帳號"));
}

#[sqlx::test(migrations = "./migrations")]
async fn status_emails_render(pool: PgPool) {
    let state = common::state(pool.clone());
    let (variant, _) = common::active_product(&pool, "A", 300, 10).await;
    let created = orders::create_order(&pool, home_input(vec![(variant, 2)], "credit"), None)
        .await
        .unwrap();
    let id = created.order_id;

    sqlx::query("UPDATE orders SET status = 'paid', paid_at = '2026-09-06T07:30:23Z' WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    enqueue(&pool, json!({ "template": "payment_received", "order_id": id })).await;
    run_all(&state).await;
    let m = common::sent_emails(&state).into_iter().nth(1).unwrap();
    assert!(m.subject.contains("已收到款項"), "{}", m.subject);
    assert!(m.text.contains("2026/09/06 15:30:23") && m.text.contains("NT$ 700"), "{}", m.text);

    sqlx::query(
        "UPDATE invoices SET status = 'issued', invoice_no = 'AB12345678', random_number = '1234', invoice_date = '2026-09-06T07:31:00Z' WHERE order_id = $1",
    )
    .bind(id)
    .execute(&pool)
    .await
    .unwrap();
    enqueue(&pool, json!({ "template": "invoice_issued", "order_id": id })).await;
    run_all(&state).await;
    let m = common::sent_emails(&state).into_iter().nth(2).unwrap();
    assert!(m.subject.contains("電子發票"), "{}", m.subject);
    assert!(m.text.contains("AB12345678") && m.text.contains("1234") && m.text.contains("2026/09/06 15:31:00"), "{}", m.text);

    sqlx::query("UPDATE orders SET status = 'shipped', shipped_at = now() WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE shipments SET status = 'shipped', carrier = '黑貓宅急便', tracking_no = '9001234567' WHERE order_id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    enqueue(&pool, json!({ "template": "order_shipped", "order_id": id })).await;
    run_all(&state).await;
    let m = common::sent_emails(&state).into_iter().nth(3).unwrap();
    assert!(m.subject.contains("已出貨"), "{}", m.subject);
    assert!(m.text.contains("黑貓宅急便") && m.text.contains("9001234567"), "{}", m.text);

    // 超商取貨的出貨信：門市名稱與地址
    let token = common::cvs_store_token(&pool).await;
    let cvs = orders::create_order(&pool, cvs_input(vec![(variant, 1)], token), None)
        .await
        .unwrap();
    enqueue(&pool, json!({ "template": "order_shipped", "order_id": cvs.order_id })).await;
    run_all(&state).await;
    let m = common::sent_emails(&state).last().unwrap().clone();
    assert!(m.subject.contains("已出貨"));
    assert!(m.text.contains("測試門市") && m.text.contains("台北市中正區重慶南路一段 122 號"), "{}", m.text);
    assert!(m.text.contains("超商簡訊"));
}

#[sqlx::test(migrations = "./migrations")]
async fn password_reset_creates_token_throttles_and_skips_unknown_user(pool: PgPool) {
    let state = common::state(pool.clone());
    let hash = dog_shop_api::auth::password::hash_password("password123").unwrap();
    let user = users::create(&pool, "reset@test.local", &hash, "小美", "customer")
        .await
        .unwrap();

    enqueue(&pool, json!({ "template": "password_reset", "user_id": user.id })).await;
    run_all(&state).await;
    let emails = common::sent_emails(&state);
    assert_eq!(emails.len(), 1);
    let m = &emails[0];
    assert_eq!(m.to, "reset@test.local");
    assert!(m.subject.contains("重設密碼"), "{}", m.subject);
    assert!(m.text.contains("小美") && m.text.contains("60 分鐘"), "{}", m.text);
    let start = m.text.find("http://localhost:5173/reset/").expect("有重設連結") + "http://localhost:5173/reset/".len();
    let token = &m.text[start..start + 64];
    assert!(token.chars().all(|c| c.is_ascii_hexdigit()), "{token}");
    assert!(m.html.contains(token));
    // token 真的能用
    assert_eq!(password_resets::consume(&pool, token).await.unwrap(), Some(user.id));

    // 10 分鐘內再要一次：job done、沒有新信、沒有新 token（與規格不同之處 25）
    enqueue(&pool, json!({ "template": "password_reset", "user_id": user.id })).await;
    run_all(&state).await;
    assert_eq!(common::sent_emails(&state).len(), 1);
    let tokens: i64 = sqlx::query_scalar("SELECT count(*) FROM password_resets WHERE user_id = $1")
        .bind(user.id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(tokens, 1);

    // 使用者不存在：done、沒信
    enqueue(&pool, json!({ "template": "password_reset", "user_id": Uuid::now_v7() })).await;
    run_all(&state).await;
    assert_eq!(common::sent_emails(&state).len(), 1);
    let jobs = job_rows(&pool).await;
    assert_eq!(jobs.len(), 3);
    assert!(jobs.iter().all(|j| j.0 == "done"), "{jobs:?}");
}

#[sqlx::test(migrations = "./migrations")]
async fn bad_jobs_fail_and_retry(pool: PgPool) {
    let state = common::state(pool.clone());
    let (variant, _) = common::active_product(&pool, "A", 300, 10).await;
    let created = orders::create_order(&pool, home_input(vec![(variant, 1)], "credit"), None)
        .await
        .unwrap();
    // 發票還沒開就要寄 invoice_issued → 失敗重試
    enqueue(&pool, json!({ "template": "invoice_issued", "order_id": created.order_id })).await;
    // 不認識的模板
    enqueue(&pool, json!({ "template": "nope", "order_id": created.order_id })).await;
    // 訂單不存在 → 略過（done）
    enqueue(&pool, json!({ "template": "order_created", "order_id": Uuid::now_v7() })).await;
    // 不認識的 kind
    let mut tx = pool.begin().await.unwrap();
    jobs::enqueue(&mut tx, "weird", json!({}), None).await.unwrap();
    tx.commit().await.unwrap();

    run_all(&state).await;
    let jobs = job_rows(&pool).await;
    assert_eq!(jobs.len(), 5);
    assert_eq!(jobs[0].0, "done", "order_created");
    assert_eq!((jobs[1].0.as_str(), jobs[1].1), ("queued", 1));
    assert!(jobs[1].3.as_deref().unwrap().contains("還沒開立"));
    assert_eq!((jobs[2].0.as_str(), jobs[2].1), ("queued", 1));
    assert!(jobs[2].3.as_deref().unwrap().contains("未知的 email 模板"));
    assert_eq!(jobs[3].0, "done");
    assert_eq!((jobs[4].0.as_str(), jobs[4].1), ("queued", 1));
    assert!(jobs[4].3.as_deref().unwrap().contains("未知的 job kind"));
    assert_eq!(common::sent_emails(&state).len(), 1, "只有 order_created 寄出");
}
```

Run: `export PATH="$HOME/.cargo/bin:$PATH" && DATABASE_URL=postgres://dog_shop:dog_shop@localhost:5435/dog_shop cargo test --manifest-path api/Cargo.toml --test mail_jobs`
Expected: 編譯錯誤（`common::sent_emails` 不存在）。

- [ ] **Step 3: 建 `api/src/mail/mod.rs`**

```rust
//! Email 寄送（規格 §12）。Mailer 有三種：Smtp（正式）、Log（沒設 SMTP，與規格不同之處 22）、Capture（測試）
pub mod templates;

use std::sync::{Arc, Mutex};

use anyhow::Context;
use lettre::{
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
    message::{Mailbox, MultiPart},
    transport::smtp::authentication::Credentials,
};

use crate::config::Config;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Email {
    pub to: String,
    pub subject: String,
    pub text: String,
    pub html: String,
}

pub enum Mailer {
    Smtp {
        transport: AsyncSmtpTransport<Tokio1Executor>,
        from: Mailbox,
    },
    /// 只記 to／subject（info）；內文只在 RUST_LOG 開 `mail_body=debug` 時輸出（含重設連結，正式環境不要開）
    Log,
    /// 測試用：全部收進 Vec
    Capture(Arc<Mutex<Vec<Email>>>),
}

impl Mailer {
    pub fn from_config(cfg: &Config) -> anyhow::Result<Self> {
        let Some(smtp) = &cfg.smtp else {
            tracing::warn!("SMTP 未設定（SMTP_HOST 空白）：Email 只會記 log，不會真的寄出");
            return Ok(Self::Log);
        };
        // 465 = 一開始就是 TLS；其他（587、25）= STARTTLS
        let mut builder = if smtp.port == 465 {
            AsyncSmtpTransport::<Tokio1Executor>::relay(&smtp.host)?
        } else {
            AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&smtp.host)?
        };
        builder = builder.port(smtp.port);
        if let (Some(user), Some(pass)) = (&smtp.user, &smtp.pass) {
            builder = builder.credentials(Credentials::new(user.clone(), pass.clone()));
        }
        let from: Mailbox = smtp
            .from
            .parse()
            .with_context(|| format!("SMTP_FROM 不是合法的寄件人：{}", smtp.from))?;
        Ok(Self::Smtp {
            transport: builder.build(),
            from,
        })
    }

    pub fn capture() -> (Self, Arc<Mutex<Vec<Email>>>) {
        let sink = Arc::new(Mutex::new(Vec::new()));
        (Self::Capture(sink.clone()), sink)
    }

    pub async fn send(&self, email: Email) -> anyhow::Result<()> {
        match self {
            Self::Smtp { transport, from } => {
                let to: Mailbox = email
                    .to
                    .parse()
                    .with_context(|| format!("收件人格式錯誤：{}", email.to))?;
                let message = Message::builder()
                    .from(from.clone())
                    .to(to)
                    .subject(email.subject)
                    .multipart(MultiPart::alternative_plain_html(email.text, email.html))?;
                transport.send(message).await.context("SMTP 寄送失敗")?;
                Ok(())
            }
            Self::Log => {
                tracing::info!(to = %email.to, subject = %email.subject, "Email（未設定 SMTP，只記 log）");
                tracing::debug!(target: "mail_body", to = %email.to, "{}", email.text);
                Ok(())
            }
            Self::Capture(sink) => {
                sink.lock().expect("mail sink").push(email);
                Ok(())
            }
        }
    }
}
```

在 `api/src/lib.rs` 的 `pub mod jobs;` 之後加 `pub mod mail;`。

- [ ] **Step 4: 建 `api/src/mail/templates.rs`**

```rust
//! askama 模板（`api/templates/mail/*.txt|html`）。每種信一個資料 struct、一對 Txt／Html 包裝（欄位 `m`），
//! `render()` 回 (主旨, 純文字, HTML)。HTML 都套 `layout.html`，需要 `m.shop_name`、`m.contact`
use askama::Template;

pub struct MailItem {
    pub name: String,
    pub label: String,
    pub quantity: i32,
    pub line_total: String,
}

/// `NT$ 1,234`
pub fn twd(n: i32) -> String {
    let digits = n.abs().to_string();
    let mut out = String::with_capacity(digits.len() + 4);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    format!("{}NT$ {out}", if n < 0 { "-" } else { "" })
}

fn pair<T: Template, H: Template>(
    subject: String,
    txt: T,
    html: H,
) -> askama::Result<(String, String, String)> {
    Ok((subject, txt.render()?, html.render()?))
}

// ───── order_created ─────

pub struct OrderCreatedMail {
    pub shop_name: String,
    pub order_no: String,
    pub order_url: String,
    pub items: Vec<MailItem>,
    pub subtotal: String,
    /// 已格式化；免運時是「免運」
    pub shipping_fee: String,
    pub total: String,
    pub payment_label: String,
    /// 「超商取貨：門市 地址」或「宅配：郵遞區號縣市鄉鎮地址」
    pub shipping_desc: String,
    pub contact: String,
}

#[derive(Template)]
#[template(path = "mail/order_created.txt")]
struct OrderCreatedTxt<'a> {
    m: &'a OrderCreatedMail,
}

#[derive(Template)]
#[template(path = "mail/order_created.html")]
struct OrderCreatedHtml<'a> {
    m: &'a OrderCreatedMail,
}

impl OrderCreatedMail {
    pub fn render(&self) -> askama::Result<(String, String, String)> {
        pair(
            format!("【{}】訂單 {} 已成立", self.shop_name, self.order_no),
            OrderCreatedTxt { m: self },
            OrderCreatedHtml { m: self },
        )
    }
}

// ───── payment_instructions ─────

pub struct PaymentInstructionsMail {
    pub shop_name: String,
    pub order_no: String,
    pub order_url: String,
    pub total: String,
    pub method_label: String,
    pub atm_bank_code: Option<String>,
    pub atm_vaccount: Option<String>,
    pub cvs_payment_no: Option<String>,
    /// 台北時間 `yyyy/MM/dd HH:mm:ss`；沒有就空字串
    pub expire_at: String,
    pub contact: String,
}

#[derive(Template)]
#[template(path = "mail/payment_instructions.txt")]
struct PaymentInstructionsTxt<'a> {
    m: &'a PaymentInstructionsMail,
}

#[derive(Template)]
#[template(path = "mail/payment_instructions.html")]
struct PaymentInstructionsHtml<'a> {
    m: &'a PaymentInstructionsMail,
}

impl PaymentInstructionsMail {
    pub fn render(&self) -> askama::Result<(String, String, String)> {
        pair(
            format!("【{}】訂單 {} 繳費資訊", self.shop_name, self.order_no),
            PaymentInstructionsTxt { m: self },
            PaymentInstructionsHtml { m: self },
        )
    }
}

// ───── payment_received ─────

pub struct PaymentReceivedMail {
    pub shop_name: String,
    pub order_no: String,
    pub order_url: String,
    pub total: String,
    pub paid_at: String,
    pub contact: String,
}

#[derive(Template)]
#[template(path = "mail/payment_received.txt")]
struct PaymentReceivedTxt<'a> {
    m: &'a PaymentReceivedMail,
}

#[derive(Template)]
#[template(path = "mail/payment_received.html")]
struct PaymentReceivedHtml<'a> {
    m: &'a PaymentReceivedMail,
}

impl PaymentReceivedMail {
    pub fn render(&self) -> askama::Result<(String, String, String)> {
        pair(
            format!("【{}】訂單 {} 已收到款項", self.shop_name, self.order_no),
            PaymentReceivedTxt { m: self },
            PaymentReceivedHtml { m: self },
        )
    }
}

// ───── order_shipped ─────

pub struct OrderShippedMail {
    pub shop_name: String,
    pub order_no: String,
    pub order_url: String,
    /// true = 超商取貨（顯示門市）；false = 宅配（顯示貨運公司與單號）
    pub cvs: bool,
    pub store_name: String,
    pub store_address: String,
    pub carrier: String,
    pub tracking_no: String,
    pub contact: String,
}

#[derive(Template)]
#[template(path = "mail/order_shipped.txt")]
struct OrderShippedTxt<'a> {
    m: &'a OrderShippedMail,
}

#[derive(Template)]
#[template(path = "mail/order_shipped.html")]
struct OrderShippedHtml<'a> {
    m: &'a OrderShippedMail,
}

impl OrderShippedMail {
    pub fn render(&self) -> askama::Result<(String, String, String)> {
        pair(
            format!("【{}】訂單 {} 已出貨", self.shop_name, self.order_no),
            OrderShippedTxt { m: self },
            OrderShippedHtml { m: self },
        )
    }
}

// ───── invoice_issued ─────

pub struct InvoiceIssuedMail {
    pub shop_name: String,
    pub order_no: String,
    pub order_url: String,
    pub invoice_no: String,
    pub invoice_date: String,
    pub random_number: String,
    pub total: String,
    pub contact: String,
}

#[derive(Template)]
#[template(path = "mail/invoice_issued.txt")]
struct InvoiceIssuedTxt<'a> {
    m: &'a InvoiceIssuedMail,
}

#[derive(Template)]
#[template(path = "mail/invoice_issued.html")]
struct InvoiceIssuedHtml<'a> {
    m: &'a InvoiceIssuedMail,
}

impl InvoiceIssuedMail {
    pub fn render(&self) -> askama::Result<(String, String, String)> {
        pair(
            format!("【{}】訂單 {} 電子發票已開立", self.shop_name, self.order_no),
            InvoiceIssuedTxt { m: self },
            InvoiceIssuedHtml { m: self },
        )
    }
}

// ───── password_reset ─────

pub struct PasswordResetMail {
    pub shop_name: String,
    pub user_name: String,
    pub reset_url: String,
    pub ttl_minutes: i64,
    pub contact: String,
}

#[derive(Template)]
#[template(path = "mail/password_reset.txt")]
struct PasswordResetTxt<'a> {
    m: &'a PasswordResetMail,
}

#[derive(Template)]
#[template(path = "mail/password_reset.html")]
struct PasswordResetHtml<'a> {
    m: &'a PasswordResetMail,
}

impl PasswordResetMail {
    pub fn render(&self) -> askama::Result<(String, String, String)> {
        pair(
            format!("【{}】重設密碼", self.shop_name),
            PasswordResetTxt { m: self },
            PasswordResetHtml { m: self },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn twd_groups_thousands() {
        assert_eq!(twd(0), "NT$ 0");
        assert_eq!(twd(700), "NT$ 700");
        assert_eq!(twd(1234), "NT$ 1,234");
        assert_eq!(twd(1_234_567), "NT$ 1,234,567");
        assert_eq!(twd(-50), "-NT$ 50");
    }

    #[test]
    fn password_reset_renders_link_in_both_parts() {
        let mail = PasswordResetMail {
            shop_name: "狗狗商店".to_string(),
            user_name: "小美".to_string(),
            reset_url: "https://shop.example.com/reset/abc".to_string(),
            ttl_minutes: 60,
            contact: "狗狗商店（hi@example.com）".to_string(),
        };
        let (subject, text, html) = mail.render().unwrap();
        assert_eq!(subject, "【狗狗商店】重設密碼");
        assert!(text.contains("小美") && text.contains("60 分鐘"));
        assert!(text.contains("https://shop.example.com/reset/abc"));
        assert!(html.contains("href=\"https://shop.example.com/reset/abc\""));
        assert!(html.contains("hi@example.com"));
    }

    #[test]
    fn html_escapes_user_text_but_txt_does_not() {
        let mail = PaymentReceivedMail {
            shop_name: "A&B <shop>".to_string(),
            order_no: "DS260906ABCD".to_string(),
            order_url: "https://shop.example.com/orders/x?t=y".to_string(),
            total: twd(700),
            paid_at: "2026/09/06 15:30:23".to_string(),
            contact: "A&B".to_string(),
        };
        let (_, text, html) = mail.render().unwrap();
        assert!(text.contains("A&B <shop>"));
        assert!(html.contains("A&amp;B &lt;shop&gt;"));
        assert!(!html.contains("<shop>"));
    }
}
```

- [ ] **Step 5: 建 13 個模板檔（目錄 `api/templates/mail/`）**

`layout.html`：

```html
<!doctype html>
<html lang="zh-Hant">
<head>
<meta charset="utf-8">
<title>{{ m.shop_name }}</title>
</head>
<body style="margin:0;padding:24px;background:#f5f5f5;font-family:-apple-system,BlinkMacSystemFont,'Noto Sans TC','PingFang TC',sans-serif;color:#111;font-size:15px;line-height:1.6;">
<div style="max-width:560px;margin:0 auto;background:#fff;border:1px solid #e5e5e5;border-radius:8px;padding:24px;">
<h1 style="font-size:18px;margin:0 0 16px;">{{ m.shop_name }}</h1>
{% block content %}{% endblock %}
<hr style="border:0;border-top:1px solid #e5e5e5;margin:24px 0 12px;">
<p style="font-size:12px;color:#666;margin:0;">這封信由系統自動寄出。如有問題請聯絡 {{ m.contact }}。</p>
</div>
</body>
</html>
```

`order_created.txt`：

```
{{ m.shop_name }}｜訂單 {{ m.order_no }} 已成立

感謝您的購買，訂單內容如下：
{% for item in m.items %}
- {{ item.name }}（{{ item.label }}）× {{ item.quantity }}　{{ item.line_total }}
{%- endfor %}

商品小計 {{ m.subtotal }}
運費 {{ m.shipping_fee }}
總計 {{ m.total }}
付款方式：{{ m.payment_label }}
{{ m.shipping_desc }}

查看訂單：{{ m.order_url }}

這封信由系統自動寄出。如有問題請聯絡 {{ m.contact }}。
```

`order_created.html`：

```html
{% extends "mail/layout.html" %}
{% block content %}
<p>您的訂單 <strong>{{ m.order_no }}</strong> 已成立，感謝您的購買。</p>
<table style="width:100%;border-collapse:collapse;">
{% for item in m.items %}
<tr><td style="padding:4px 0;">{{ item.name }}（{{ item.label }}）× {{ item.quantity }}</td><td style="text-align:right;">{{ item.line_total }}</td></tr>
{% endfor %}
<tr><td style="padding-top:8px;border-top:1px solid #eee;">商品小計</td><td style="text-align:right;padding-top:8px;border-top:1px solid #eee;">{{ m.subtotal }}</td></tr>
<tr><td>運費</td><td style="text-align:right;">{{ m.shipping_fee }}</td></tr>
<tr><td style="font-weight:bold;">總計</td><td style="text-align:right;font-weight:bold;">{{ m.total }}</td></tr>
</table>
<p>付款方式：{{ m.payment_label }}<br>{{ m.shipping_desc }}</p>
<p><a href="{{ m.order_url }}">查看訂單</a></p>
{% endblock %}
```

`payment_instructions.txt`：

```
{{ m.shop_name }}｜訂單 {{ m.order_no }} 的繳費資訊

請在期限內完成付款，訂單才會成立出貨。
付款方式：{{ m.method_label }}
金額：{{ m.total }}
{% if let Some(bank) = m.atm_bank_code %}銀行代碼：{{ bank }}
{% endif %}{% if let Some(account) = m.atm_vaccount %}虛擬帳號：{{ account }}
{% endif %}{% if let Some(code) = m.cvs_payment_no %}超商繳費代碼：{{ code }}（到超商多媒體機台輸入）
{% endif %}繳費期限：{{ m.expire_at }}

查看訂單：{{ m.order_url }}

這封信由系統自動寄出。如有問題請聯絡 {{ m.contact }}。
```

`payment_instructions.html`：

```html
{% extends "mail/layout.html" %}
{% block content %}
<p>訂單 <strong>{{ m.order_no }}</strong> 請在期限內完成付款，訂單才會成立出貨。</p>
<p>付款方式：{{ m.method_label }}<br>金額：<strong>{{ m.total }}</strong></p>
{% if let Some(bank) = m.atm_bank_code %}<p>銀行代碼：<strong>{{ bank }}</strong></p>{% endif %}
{% if let Some(account) = m.atm_vaccount %}<p>虛擬帳號：<strong style="font-size:18px;">{{ account }}</strong></p>{% endif %}
{% if let Some(code) = m.cvs_payment_no %}<p>超商繳費代碼：<strong style="font-size:18px;">{{ code }}</strong>（到超商多媒體機台輸入）</p>{% endif %}
<p>繳費期限：{{ m.expire_at }}</p>
<p><a href="{{ m.order_url }}">查看訂單</a></p>
{% endblock %}
```

`payment_received.txt`：

```
{{ m.shop_name }}｜訂單 {{ m.order_no }} 已收到款項

我們已收到您的付款（{{ m.paid_at }}），金額 {{ m.total }}。商品準備好會再通知您出貨。

查看訂單：{{ m.order_url }}

這封信由系統自動寄出。如有問題請聯絡 {{ m.contact }}。
```

`payment_received.html`：

```html
{% extends "mail/layout.html" %}
{% block content %}
<p>訂單 <strong>{{ m.order_no }}</strong> 已收到您的付款（{{ m.paid_at }}），金額 {{ m.total }}。</p>
<p>商品準備好會再通知您出貨。</p>
<p><a href="{{ m.order_url }}">查看訂單</a></p>
{% endblock %}
```

`order_shipped.txt`：

```
{{ m.shop_name }}｜訂單 {{ m.order_no }} 已出貨

{% if m.cvs %}您的包裹已送往 {{ m.store_name }}（{{ m.store_address }}）。到店後會收到超商簡訊，請憑手機末三碼取貨。
{% else %}您的包裹已由 {{ m.carrier }} 寄出，單號 {{ m.tracking_no }}。
{% endif %}
查看訂單：{{ m.order_url }}

這封信由系統自動寄出。如有問題請聯絡 {{ m.contact }}。
```

`order_shipped.html`：

```html
{% extends "mail/layout.html" %}
{% block content %}
<p>訂單 <strong>{{ m.order_no }}</strong> 已出貨。</p>
{% if m.cvs %}
<p>您的包裹已送往 <strong>{{ m.store_name }}</strong>（{{ m.store_address }}）。到店後會收到超商簡訊，請憑手機末三碼取貨。</p>
{% else %}
<p>您的包裹已由 {{ m.carrier }} 寄出，單號 <strong>{{ m.tracking_no }}</strong>。</p>
{% endif %}
<p><a href="{{ m.order_url }}">查看訂單</a></p>
{% endblock %}
```

`invoice_issued.txt`：

```
{{ m.shop_name }}｜訂單 {{ m.order_no }} 的電子發票已開立

發票號碼：{{ m.invoice_no }}
隨機碼：{{ m.random_number }}
開立時間：{{ m.invoice_date }}
金額：{{ m.total }}

發票已依您選擇的方式（載具、統編或捐贈）處理；綠界也會另寄通知。

查看訂單：{{ m.order_url }}

這封信由系統自動寄出。如有問題請聯絡 {{ m.contact }}。
```

`invoice_issued.html`：

```html
{% extends "mail/layout.html" %}
{% block content %}
<p>訂單 <strong>{{ m.order_no }}</strong> 的電子發票已開立。</p>
<p>發票號碼：<strong>{{ m.invoice_no }}</strong><br>隨機碼：{{ m.random_number }}<br>開立時間：{{ m.invoice_date }}<br>金額：{{ m.total }}</p>
<p>發票已依您選擇的方式（載具、統編或捐贈）處理；綠界也會另寄通知。</p>
<p><a href="{{ m.order_url }}">查看訂單</a></p>
{% endblock %}
```

`password_reset.txt`：

```
{{ m.shop_name }}｜重設密碼

{{ m.user_name }} 您好，

有人（希望是您）要求重設密碼。請在 {{ m.ttl_minutes }} 分鐘內點下面的連結：
{{ m.reset_url }}

如果不是您本人操作，請忽略這封信，密碼不會被更改。

這封信由系統自動寄出。如有問題請聯絡 {{ m.contact }}。
```

`password_reset.html`：

```html
{% extends "mail/layout.html" %}
{% block content %}
<p>{{ m.user_name }} 您好，</p>
<p>有人（希望是您）要求重設密碼。請在 {{ m.ttl_minutes }} 分鐘內點下面的連結：</p>
<p><a href="{{ m.reset_url }}">{{ m.reset_url }}</a></p>
<p>如果不是您本人操作，請忽略這封信，密碼不會被更改。</p>
{% endblock %}
```

- [ ] **Step 6: 改寫 `api/src/jobs/handlers.rs`（整檔）**

```rust
//! job 種類對應的執行函式：send_email（本任務）、issue_invoice（Task 10）
use anyhow::Context;
use serde_json::Value;
use uuid::Uuid;

use crate::{
    auth::tokens::sha256_hex,
    domain::{
        jobs::{KIND_ISSUE_INVOICE, KIND_SEND_EMAIL},
        orders::{self, OrderDetail, PAYMENT_ATM, PAYMENT_CREDIT, PAYMENT_CVS_CODE, SHIPPING_CVS},
        password_resets, payments,
        settings::{self, ShopSettings},
        users,
    },
    ecpay::time,
    jobs::worker::Job,
    mail::{
        Email,
        templates::{self, MailItem},
    },
    state::AppState,
};

/// 同一使用者 10 分鐘內只寄一封重設信（與規格不同之處 25）
pub const RESET_THROTTLE_MINUTES: i32 = 10;

pub async fn run(state: &AppState, job: &Job) -> anyhow::Result<()> {
    match job.kind.as_str() {
        KIND_SEND_EMAIL => send_email(state, &job.payload).await,
        KIND_ISSUE_INVOICE => anyhow::bail!("issue_invoice handler 尚未實作（Task 10）"),
        other => anyhow::bail!("未知的 job kind：{other}"),
    }
}

fn payload_uuid(payload: &Value, key: &str) -> anyhow::Result<Uuid> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .and_then(|s| Uuid::parse_str(s).ok())
        .with_context(|| format!("payload 缺少 {key}"))
}

/// 信尾的聯絡方式：商店名稱（＋聯絡 Email）
fn contact_line(shop: &ShopSettings) -> String {
    if shop.contact_email.is_empty() {
        shop.name.clone()
    } else {
        format!("{}（{}）", shop.name, shop.contact_email)
    }
}

/// 訂單頁網址；訪客訂單帶 guest_token（規格 §11：只出現在訂單頁網址與 Email）
fn order_url(base: &str, detail: &OrderDetail) -> String {
    match detail.order.user_id {
        Some(_) => format!("{base}/orders/{}", detail.order.id),
        None => format!(
            "{base}/orders/{}?t={}",
            detail.order.id, detail.order.guest_token
        ),
    }
}

fn payment_label(method: &str) -> &'static str {
    match method {
        PAYMENT_CREDIT => "信用卡",
        PAYMENT_ATM => "ATM 轉帳",
        PAYMENT_CVS_CODE => "超商代碼繳費",
        _ => "－",
    }
}

fn mail_items(detail: &OrderDetail) -> Vec<MailItem> {
    detail
        .items
        .iter()
        .map(|i| MailItem {
            name: i.product_name.clone(),
            label: i.variant_label.clone(),
            quantity: i.quantity,
            line_total: templates::twd(i.line_total),
        })
        .collect()
}

fn shipping_desc(detail: &OrderDetail) -> String {
    let Some(s) = detail.shipment.as_ref() else {
        return String::new();
    };
    let or_empty = |v: &Option<String>| v.clone().unwrap_or_default();
    if s.method == SHIPPING_CVS {
        format!(
            "超商取貨：{} {}",
            or_empty(&s.cvs_store_name),
            or_empty(&s.cvs_store_address)
        )
    } else {
        format!(
            "宅配：{}{}{}{}",
            or_empty(&s.home_postal_code),
            or_empty(&s.home_city),
            or_empty(&s.home_district),
            or_empty(&s.home_street)
        )
    }
}

async fn send_email(state: &AppState, payload: &Value) -> anyhow::Result<()> {
    let template = payload
        .get("template")
        .and_then(Value::as_str)
        .context("payload 缺少 template")?
        .to_string();
    let shop = settings::get_all(&state.db).await?.shop;
    let base = state.config.public_base_url.as_str();
    let contact = contact_line(&shop);

    if template == "password_reset" {
        return send_password_reset(
            state,
            payload_uuid(payload, "user_id")?,
            &shop.name,
            base,
            &contact,
        )
        .await;
    }

    let order_id = payload_uuid(payload, "order_id")?;
    let Some(detail) = orders::get_detail(&state.db, order_id).await? else {
        tracing::warn!(%order_id, template = %template, "訂單不存在，略過寄信");
        return Ok(());
    };
    let order_url = order_url(base, &detail);
    let order_no = detail.order.order_no.clone();
    let total = templates::twd(detail.order.total);
    let (subject, text, html) = match template.as_str() {
        "order_created" => templates::OrderCreatedMail {
            shop_name: shop.name.clone(),
            order_no,
            order_url,
            items: mail_items(&detail),
            subtotal: templates::twd(detail.order.subtotal),
            shipping_fee: if detail.order.shipping_fee == 0 {
                "免運".to_string()
            } else {
                templates::twd(detail.order.shipping_fee)
            },
            total,
            payment_label: payment_label(
                detail
                    .payment
                    .as_ref()
                    .map(|p| p.method.as_str())
                    .unwrap_or(""),
            )
            .to_string(),
            shipping_desc: shipping_desc(&detail),
            contact,
        }
        .render()?,
        "payment_instructions" => {
            let payment = payments::get(&state.db, payload_uuid(payload, "payment_id")?)
                .await?
                .context("payment_instructions 找不到 payment")?;
            templates::PaymentInstructionsMail {
                shop_name: shop.name.clone(),
                order_no,
                order_url,
                total: templates::twd(payment.amount),
                method_label: payment_label(&payment.method).to_string(),
                atm_bank_code: payment.atm_bank_code.clone(),
                atm_vaccount: payment.atm_vaccount.clone(),
                cvs_payment_no: payment.cvs_payment_no.clone(),
                expire_at: payment
                    .expire_at
                    .map(time::format_datetime)
                    .unwrap_or_default(),
                contact,
            }
            .render()?
        }
        "payment_received" => templates::PaymentReceivedMail {
            shop_name: shop.name.clone(),
            order_no,
            order_url,
            total,
            paid_at: detail
                .order
                .paid_at
                .map(time::format_datetime)
                .unwrap_or_default(),
            contact,
        }
        .render()?,
        "order_shipped" => {
            let s = detail.shipment.as_ref();
            let or_empty = |v: Option<&String>| v.cloned().unwrap_or_default();
            templates::OrderShippedMail {
                shop_name: shop.name.clone(),
                order_no,
                order_url,
                cvs: detail.order.shipping_method == SHIPPING_CVS,
                store_name: or_empty(s.and_then(|s| s.cvs_store_name.as_ref())),
                store_address: or_empty(s.and_then(|s| s.cvs_store_address.as_ref())),
                carrier: or_empty(s.and_then(|s| s.carrier.as_ref())),
                tracking_no: or_empty(s.and_then(|s| s.tracking_no.as_ref())),
                contact,
            }
            .render()?
        }
        "invoice_issued" => {
            let Some(inv) = detail
                .invoice
                .as_ref()
                .filter(|i| i.invoice_no.is_some())
            else {
                anyhow::bail!("invoice_issued 但發票還沒開立");
            };
            templates::InvoiceIssuedMail {
                shop_name: shop.name.clone(),
                order_no,
                order_url,
                invoice_no: inv.invoice_no.clone().unwrap_or_default(),
                invoice_date: inv
                    .invoice_date
                    .map(time::format_datetime)
                    .unwrap_or_default(),
                random_number: inv.random_number.clone().unwrap_or_default(),
                total,
                contact,
            }
            .render()?
        }
        other => anyhow::bail!("未知的 email 模板：{other}"),
    };
    state
        .mailer
        .send(Email {
            to: detail.order.email.clone(),
            subject,
            text,
            html,
        })
        .await
}

async fn send_password_reset(
    state: &AppState,
    user_id: Uuid,
    shop_name: &str,
    base: &str,
    contact: &str,
) -> anyhow::Result<()> {
    let Some(user) = users::find_by_id(&state.db, user_id).await? else {
        tracing::warn!(%user_id, "使用者不存在，略過重設信");
        return Ok(());
    };
    let recent: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM password_resets
                        WHERE user_id = $1 AND created_at > now() - make_interval(mins => $2))",
    )
    .bind(user_id)
    .bind(RESET_THROTTLE_MINUTES)
    .fetch_one(&state.db)
    .await?;
    if recent {
        tracing::info!(%user_id, "{RESET_THROTTLE_MINUTES} 分鐘內已寄過重設信，略過");
        return Ok(());
    }
    // token 在寄信當下才產生（與規格不同之處 11）；DB 只有 SHA-256
    let raw = password_resets::create(&state.db, user_id).await?;
    let (subject, text, html) = templates::PasswordResetMail {
        shop_name: shop_name.to_string(),
        user_name: user.name.clone(),
        reset_url: format!("{base}/reset/{raw}"),
        ttl_minutes: password_resets::RESET_TTL_MINUTES,
        contact: contact.to_string(),
    }
    .render()?;
    if let Err(e) = state
        .mailer
        .send(Email {
            to: user.email.clone(),
            subject,
            text,
            html,
        })
        .await
    {
        // 寄失敗就把剛建的 token 作廢，重試時才能再產生（不然會被 10 分鐘節流擋住）
        sqlx::query("DELETE FROM password_resets WHERE token_hash = $1")
            .bind(sha256_hex(&raw))
            .execute(&state.db)
            .await?;
        return Err(e);
    }
    Ok(())
}
```

- [ ] **Step 7: `AppState`、`main.rs`、`tests/common`**

`api/src/state.rs` 整檔：

```rust
use std::sync::Arc;

use sqlx::PgPool;

use crate::{config::Config, mail::Mailer};

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub config: Arc<Config>,
    /// Email 出口（SMTP／只記 log／測試擷取）
    pub mailer: Arc<Mailer>,
}
```

`api/src/main.rs`：第 4 行的 `use dog_shop_api::{app, config::Config, state::AppState};` 改成 `use dog_shop_api::{app, config::Config, jobs, mail, state::AppState};`；把

```rust
    let state = AppState {
        db,
        config: config.clone(),
    };
    let app = app::router(state);
```

換成：

```rust
    let mailer = Arc::new(mail::Mailer::from_config(&config)?);
    let state = AppState {
        db,
        config: config.clone(),
        mailer,
    };
    // 背景工作：jobs worker 與排程掃描（規格 §9），和 api 同一個行程、同一個連線池
    jobs::start(state.clone());
    let app = app::router(state);
```

`api/tests/common/mod.rs`：`use dog_shop_api::{app, config::Config, state::AppState};` 改成 `use dog_shop_api::{app, config::Config, mail::{Email, Mailer}, state::AppState};`；`state()` 換成：

```rust
/// 每個測試一個獨立的上傳目錄；設定用 Config::for_tests；Email 用 Mailer::Capture（用 sent_emails 讀）
pub fn state(pool: PgPool) -> AppState {
    let upload_dir = std::env::temp_dir().join(format!("dog_shop_test_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&upload_dir).unwrap();
    let (mailer, _) = Mailer::capture();
    AppState {
        db: pool,
        config: Arc::new(Config::for_tests(upload_dir)),
        mailer: Arc::new(mailer),
    }
}

/// 同時要打 API 又要看 job／信件的測試用這個
pub fn app_with_state(pool: PgPool) -> (Router, AppState) {
    let state = state(pool);
    (app::router(state.clone()), state)
}

/// 測試裡寄出的信（Mailer::Capture）
pub fn sent_emails(state: &AppState) -> Vec<Email> {
    match &*state.mailer {
        Mailer::Capture(sink) => sink.lock().unwrap().clone(),
        _ => Vec::new(),
    }
}
```

- [ ] **Step 8: 跑測試、clippy、fmt**

Run（背景、timeout 600000；lettre 與 askama 第一次編譯很慢）: `export PATH="$HOME/.cargo/bin:$PATH" && cargo fmt --all --manifest-path api/Cargo.toml && DATABASE_URL=postgres://dog_shop:dog_shop@localhost:5435/dog_shop cargo test --manifest-path api/Cargo.toml && cargo clippy --all-targets --manifest-path api/Cargo.toml -- -D warnings`
Expected: 全綠（mail_jobs 6 個 + templates 3 個新增）。askama 模板錯誤會在編譯期出現，錯誤訊息會指出模板檔與行號。

- [ ] **Step 9: 手動確認 worker 隨 api 啟動**

Run（背景）: `export PATH="$HOME/.cargo/bin:$PATH" && cargo run --manifest-path api/Cargo.toml`
Expected: log 有 `SMTP 未設定` 的 warn 與 `jobs worker 與排程工作已啟動`；幾秒內開發 DB 裡計畫 2 留下的 `send_email` job 會被處理：`Email（未設定 SMTP，只記 log）` 的 info（有 `to`、`subject`，**沒有內文**）。停掉 api（`lsof -ti :8080` → `kill`）。

- [ ] **Step 10: Commit**

```bash
git add api/Cargo.toml api/Cargo.lock api/src/lib.rs api/src/state.rs api/src/main.rs api/src/mail/mod.rs api/src/mail/templates.rs api/templates/mail api/src/jobs/handlers.rs api/tests/common/mod.rs api/tests/mail_jobs.rs
git commit -m "feat(api): Email（lettre + askama 六種模板）、send_email job、worker 隨 api 啟動"
```

---
### Task 10: 電子發票：`aes.rs`、`invoice.rs`、`issue_invoice` handler

**Files:**
- Modify: `api/Cargo.toml`（加 `aes`、`base64`、`cbc`、`percent-encoding`、`reqwest`、`rustls`）
- Create: `api/src/ecpay/aes.rs`
- Create: `api/src/ecpay/invoice.rs`
- Modify: `api/src/ecpay/mod.rs`（加 `pub mod aes; pub mod invoice;`）
- Modify: `api/src/domain/invoices.rs`（檔尾加 `record_request`、`mark_issued`、`record_failure`）
- Modify: `api/src/jobs/handlers.rs`（`run` 的 `KIND_ISSUE_INVOICE` 分支、新增 `issue_invoice`）
- Modify: `api/src/state.rs`（加 `invoices`）
- Modify: `api/src/main.rs`（rustls provider、建發票閘道）
- Modify: `api/tests/common/mod.rs`（`state()` 加 `invoices`；新增 `fake_invoices`）
- Test: `api/tests/invoice_job.rs`

**Interfaces:**
- Consumes: `Config.ecpay.invoice`、`invoice_issue_url()`（Task 1）、`mac::dotnet_url_encode`、`time::parse_taipei`（Task 3）、`orders::get_detail`、`invoices::{InvoiceRow, STATUS_*}`（Task 2）、`worker::Job::is_last_attempt`（Task 8）、`send_email` 的 `invoice_issued`（Task 9）。
- Produces：
  - `ecpay::aes::{key16(&str) -> anyhow::Result<[u8; 16]>, encrypt(key, iv, plain) -> Vec<u8>, decrypt(key, iv, cipher) -> anyhow::Result<Vec<u8>>, url_decode(&str) -> anyhow::Result<String>, encode_data(key, iv, &Value) -> String, decode_data(key, iv, &str) -> anyhow::Result<Value>}`
  - `ecpay::invoice::{IssueItem, IssueRequest, IssueResponse { rtn_code, rtn_msg, invoice_no, invoice_date, random_number, raw } + is_ok(), build_issue_request(merchant_id, &OrderDetail) -> IssueRequest, parse_issue_response(Value) -> IssueResponse, EcpayInvoiceClient::new(&EcpayConfig) + async issue(&IssueRequest), InvoiceGateway::{Ecpay(EcpayInvoiceClient), Fake(FakeInvoiceGateway)} + ecpay(&EcpayConfig) + merchant_id() + async issue(), FakeInvoiceGateway::{default(), calls(), fail_next_with_error(msg), fail_next_with_rtn(code, msg)}`
  - `domain::invoices::{record_request(db, order_id, &Value), mark_issued(db, order_id, invoice_no, invoice_date: Option<DateTime<Utc>>, random_number, response: &Value), record_failure(db, order_id, response: Option<&Value>, error: &str, final_attempt: bool)}`
  - `AppState.invoices: Arc<InvoiceGateway>`
  - `jobs::handlers::run` 支援 `issue_invoice { order_id }`；成功後排 `send_email { template: "invoice_issued", order_id }`（dedupe `email:invoice_issued:{order_id}`）
  - 測試：`common::fake_invoices(&AppState) -> &FakeInvoiceGateway`

- [ ] **Step 1: 加相依套件**

`api/Cargo.toml` 的 `[dependencies]` 加（字母順序）：

```toml
aes = "0.8"
base64 = "0.22"
cbc = { version = "0.1", features = ["alloc"] }
percent-encoding = "2"
reqwest = { version = "0.13", default-features = false, features = ["charset", "http2", "json", "rustls-no-provider"] }
rustls = { version = "0.23", default-features = false, features = ["logging", "ring", "std", "tls12"] }
```

（`rustls-no-provider` + 明確安裝 ring provider：與規格不同之處 32。reqwest、lettre、sqlx 都綁 rustls 0.23，`install_default` 裝的就是它們用的那個全域。）

- [ ] **Step 2: 建 `api/src/ecpay/aes.rs`（含單元測試）**

```rust
//! 電子發票的 Data 加解密（規格 §8.4）：JSON → URL encode（.NET 風格）→ AES-128-CBC/PKCS7（HashKey 為 key、HashIV 為 iv）→ Base64
use aes::cipher::{BlockDecryptMut, BlockEncryptMut, KeyIvInit, block_padding::Pkcs7};
use anyhow::Context;
use base64::{Engine, engine::general_purpose::STANDARD};
use percent_encoding::percent_decode_str;
use serde_json::Value;

use crate::ecpay::mac::dotnet_url_encode;

type Enc = cbc::Encryptor<aes::Aes128>;
type Dec = cbc::Decryptor<aes::Aes128>;

/// HashKey／HashIV 都必須剛好 16 bytes
pub fn key16(s: &str) -> anyhow::Result<[u8; 16]> {
    s.as_bytes().try_into().map_err(|_| {
        anyhow::anyhow!(
            "綠界發票 HashKey/HashIV 長度必須是 16 bytes（收到 {} bytes）",
            s.len()
        )
    })
}

pub fn encrypt(key: &[u8; 16], iv: &[u8; 16], plain: &[u8]) -> Vec<u8> {
    Enc::new(key.into(), iv.into()).encrypt_padded_vec_mut::<Pkcs7>(plain)
}

pub fn decrypt(key: &[u8; 16], iv: &[u8; 16], cipher: &[u8]) -> anyhow::Result<Vec<u8>> {
    Dec::new(key.into(), iv.into())
        .decrypt_padded_vec_mut::<Pkcs7>(cipher)
        .map_err(|e| anyhow::anyhow!("AES 解密失敗：{e}"))
}

/// .NET UrlEncode 的反向：`+` 是空白，`%xx` 解碼
pub fn url_decode(s: &str) -> anyhow::Result<String> {
    let plus_fixed = s.replace('+', "%20");
    Ok(percent_decode_str(&plus_fixed)
        .decode_utf8()
        .context("URL decode 的結果不是 UTF-8")?
        .into_owned())
}

/// 送出去的 Data
pub fn encode_data(key: &[u8; 16], iv: &[u8; 16], data: &Value) -> String {
    let encoded = dotnet_url_encode(&data.to_string());
    STANDARD.encode(encrypt(key, iv, encoded.as_bytes()))
}

/// 收回來的 Data
pub fn decode_data(key: &[u8; 16], iv: &[u8; 16], b64: &str) -> anyhow::Result<Value> {
    let cipher = STANDARD.decode(b64.trim()).context("Data 不是 Base64")?;
    let plain = decrypt(key, iv, &cipher)?;
    let text = url_decode(std::str::from_utf8(&plain).context("解密結果不是 UTF-8")?)?;
    serde_json::from_str(&text).context("解密結果不是 JSON")
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    const KEY: &[u8; 16] = b"ejCk326UnaZWKisg";
    const IV: &[u8; 16] = b"q9jcZX8Ib9LM8wYk";

    #[test]
    fn known_vector_from_openssl() {
        // printf '%s' '%7B%22Name%22%3A%22Test%22%2C%22ID%22%3A%2211%22%7D' \
        //   | openssl enc -aes-128-cbc -K 656a436b333236556e615a574b697367 -iv 71396a635a58384962394c4d3877596b -nosalt -base64 -A
        let plain = b"%7B%22Name%22%3A%22Test%22%2C%22ID%22%3A%2211%22%7D";
        let cipher = encrypt(KEY, IV, plain);
        assert_eq!(cipher.len(), 64, "50 bytes 補到 64");
        assert_eq!(
            STANDARD.encode(&cipher),
            "Uehplzw/uQEriJmjo9d+fxhAho1x5yWz6mJQ3usJzsl1IVtcKBO7pSktbc8kKFGxDdNBK2IUl7ErBTWwpUNIBg=="
        );
        assert_eq!(decrypt(KEY, IV, &cipher).unwrap(), plain.to_vec());
    }

    #[test]
    fn round_trip_json_with_chinese() {
        let v = json!({ "Name": "Test 測試", "ID": "11", "Items": [{ "a": 1 }] });
        let b64 = encode_data(KEY, IV, &v);
        assert_eq!(decode_data(KEY, IV, &b64).unwrap(), v);
    }

    #[test]
    fn url_decode_handles_plus_and_percent() {
        assert_eq!(url_decode("a+b%2c%e6%b8%ac").unwrap(), "a b,測");
        assert_eq!(url_decode("%7B%22x%22%3A1%7D").unwrap(), "{\"x\":1}");
    }

    #[test]
    fn key_must_be_16_bytes() {
        assert!(key16("short").is_err());
        assert!(key16("ejCk326UnaZWKisg!").is_err());
        assert_eq!(key16("ejCk326UnaZWKisg").unwrap(), *KEY);
    }

    #[test]
    fn wrong_key_does_not_decrypt() {
        let cipher = encrypt(KEY, IV, b"hello");
        let other = b"0000000000000000";
        let result = decrypt(other, IV, &cipher);
        assert!(result.map(|p| p != b"hello").unwrap_or(true));
        assert!(decode_data(KEY, IV, "not base64!!").is_err());
    }
}
```

- [ ] **Step 3: 建 `api/src/ecpay/invoice.rs`（含單元測試）**

```rust
//! 電子發票 B2C 開立（規格 §8.4）：組 Data、加密送 /B2CInvoice/Issue、解回應。
//! `InvoiceGateway` 讓測試換成 Fake，不打網路
use std::sync::Mutex;

use anyhow::Context;
use chrono::Utc;
use serde::Serialize;
use serde_json::{Value, json};

use crate::config::EcpayConfig;
use crate::domain::orders::{INVOICE_COMPANY, INVOICE_DONATION, OrderDetail};
use crate::ecpay::aes;

/// 綠界欄位長度上限
pub const ITEM_NAME_MAX: usize = 100;
pub const CUSTOMER_NAME_MAX: usize = 60;
pub const CUSTOMER_ADDR_MAX: usize = 100;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct IssueItem {
    #[serde(rename = "ItemSeq")]
    pub item_seq: i32,
    #[serde(rename = "ItemName")]
    pub item_name: String,
    #[serde(rename = "ItemCount")]
    pub item_count: i32,
    #[serde(rename = "ItemWord")]
    pub item_word: String,
    #[serde(rename = "ItemPrice")]
    pub item_price: i32,
    #[serde(rename = "ItemTaxType")]
    pub item_tax_type: String,
    #[serde(rename = "ItemAmount")]
    pub item_amount: i32,
    #[serde(rename = "ItemRemark")]
    pub item_remark: String,
}

/// 內層 Data（欄位名照綠界文件）
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct IssueRequest {
    #[serde(rename = "MerchantID")]
    pub merchant_id: String,
    #[serde(rename = "RelateNumber")]
    pub relate_number: String,
    #[serde(rename = "CustomerID")]
    pub customer_id: String,
    #[serde(rename = "CustomerIdentifier")]
    pub customer_identifier: String,
    #[serde(rename = "CustomerName")]
    pub customer_name: String,
    #[serde(rename = "CustomerAddr")]
    pub customer_addr: String,
    #[serde(rename = "CustomerPhone")]
    pub customer_phone: String,
    #[serde(rename = "CustomerEmail")]
    pub customer_email: String,
    #[serde(rename = "ClearanceMark")]
    pub clearance_mark: String,
    #[serde(rename = "Print")]
    pub print: String,
    #[serde(rename = "Donation")]
    pub donation: String,
    #[serde(rename = "LoveCode")]
    pub love_code: String,
    #[serde(rename = "CarrierType")]
    pub carrier_type: String,
    #[serde(rename = "CarrierNum")]
    pub carrier_num: String,
    #[serde(rename = "TaxType")]
    pub tax_type: String,
    #[serde(rename = "SalesAmount")]
    pub sales_amount: i32,
    #[serde(rename = "InvoiceRemark")]
    pub invoice_remark: String,
    #[serde(rename = "Items")]
    pub items: Vec<IssueItem>,
    #[serde(rename = "InvType")]
    pub inv_type: String,
    #[serde(rename = "vat")]
    pub vat: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssueResponse {
    pub rtn_code: i32,
    pub rtn_msg: String,
    pub invoice_no: String,
    /// `yyyy-MM-dd HH:mm:ss`（台北）
    pub invoice_date: String,
    pub random_number: String,
    /// 解密後的整個 Data，存進 invoices.response
    pub raw: Value,
}

impl IssueResponse {
    pub fn is_ok(&self) -> bool {
        self.rtn_code == 1
    }
}

fn truncate_chars(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

/// 依訂單的發票資料組 Data（規格 §8.4、與規格不同之處 29）。
/// 個人：CarrierType 1／2／3（1 時 CarrierNum 空）；公司：統編 + Print=1 + 抬頭 + 地址、不帶載具；
/// 捐贈：Donation=1 + 愛心碼。Items 每個 order_item 一列，運費 > 0 多一列「運費」；SalesAmount = total
pub fn build_issue_request(merchant_id: &str, order: &OrderDetail) -> IssueRequest {
    let o = &order.order;
    let mut items: Vec<IssueItem> = order
        .items
        .iter()
        .enumerate()
        .map(|(i, it)| IssueItem {
            item_seq: i as i32 + 1,
            item_name: truncate_chars(
                &format!("{}（{}）", it.product_name, it.variant_label),
                ITEM_NAME_MAX,
            ),
            item_count: it.quantity,
            item_word: "件".to_string(),
            item_price: it.unit_price,
            item_tax_type: "1".to_string(),
            item_amount: it.line_total,
            item_remark: String::new(),
        })
        .collect();
    if o.shipping_fee > 0 {
        items.push(IssueItem {
            item_seq: items.len() as i32 + 1,
            item_name: "運費".to_string(),
            item_count: 1,
            item_word: "式".to_string(),
            item_price: o.shipping_fee,
            item_tax_type: "1".to_string(),
            item_amount: o.shipping_fee,
            item_remark: String::new(),
        });
    }
    let empty = String::new;
    let (customer_identifier, customer_name, customer_addr, print, donation, love_code, carrier_type, carrier_num) =
        match o.invoice_type.as_str() {
            INVOICE_COMPANY => (
                o.invoice_tax_id.clone().unwrap_or_default(),
                o.invoice_title.clone().unwrap_or_default(),
                o.invoice_address.clone().unwrap_or_default(),
                "1",
                "0",
                empty(),
                empty(),
                empty(),
            ),
            INVOICE_DONATION => (
                empty(),
                o.recipient_name.clone(),
                empty(),
                "0",
                "1",
                o.invoice_love_code.clone().unwrap_or_default(),
                empty(),
                empty(),
            ),
            _ => (
                empty(),
                o.recipient_name.clone(),
                empty(),
                "0",
                "0",
                empty(),
                o.invoice_carrier_type.clone().unwrap_or_else(|| "1".to_string()),
                o.invoice_carrier_num.clone().unwrap_or_default(),
            ),
        };
    IssueRequest {
        merchant_id: merchant_id.to_string(),
        relate_number: o.order_no.clone(),
        customer_id: empty(),
        customer_identifier,
        customer_name: truncate_chars(&customer_name, CUSTOMER_NAME_MAX),
        customer_addr: truncate_chars(&customer_addr, CUSTOMER_ADDR_MAX),
        customer_phone: o.recipient_phone.clone(),
        customer_email: o.email.clone(),
        clearance_mark: empty(),
        print: print.to_string(),
        donation: donation.to_string(),
        love_code,
        carrier_type,
        carrier_num,
        tax_type: "1".to_string(),
        sales_amount: o.total,
        invoice_remark: empty(),
        items,
        inv_type: "07".to_string(),
        vat: "1".to_string(),
    }
}

/// 解密後的 Data → IssueResponse
pub fn parse_issue_response(inner: Value) -> IssueResponse {
    let text = |k: &str| inner.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let rtn_code = inner.get("RtnCode").and_then(Value::as_i64).unwrap_or(0) as i32;
    let rtn_msg = text("RtnMsg");
    let invoice_no = text("InvoiceNo");
    let invoice_date = text("InvoiceDate");
    let random_number = text("RandomNumber");
    IssueResponse {
        rtn_code,
        rtn_msg,
        invoice_no,
        invoice_date,
        random_number,
        raw: inner,
    }
}

/// 真的打綠界
pub struct EcpayInvoiceClient {
    client: reqwest::Client,
    url: String,
    merchant_id: String,
    key: [u8; 16],
    iv: [u8; 16],
}

impl EcpayInvoiceClient {
    pub fn new(cfg: &EcpayConfig) -> anyhow::Result<Self> {
        Ok(Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(20))
                .build()
                .context("建立 HTTP client")?,
            url: cfg.invoice_issue_url().to_string(),
            merchant_id: cfg.invoice.merchant_id.clone(),
            key: aes::key16(&cfg.invoice.hash_key)?,
            iv: aes::key16(&cfg.invoice.hash_iv)?,
        })
    }

    pub fn merchant_id(&self) -> &str {
        &self.merchant_id
    }

    pub async fn issue(&self, req: &IssueRequest) -> anyhow::Result<IssueResponse> {
        let data = aes::encode_data(&self.key, &self.iv, &serde_json::to_value(req)?);
        // Timestamp 是 Unix epoch 秒，沒有時區（規格 §8）
        let envelope = json!({
            "MerchantID": self.merchant_id,
            "RqHeader": { "Timestamp": Utc::now().timestamp() },
            "Data": data,
        });
        let response: Value = self
            .client
            .post(&self.url)
            .json(&envelope)
            .send()
            .await
            .context("連線綠界發票 API")?
            .error_for_status()
            .context("綠界發票 API HTTP 錯誤")?
            .json()
            .await
            .context("綠界發票回應不是 JSON")?;
        let trans_code = response.get("TransCode").and_then(Value::as_i64).unwrap_or(0);
        if trans_code != 1 {
            anyhow::bail!(
                "綠界發票 TransCode {trans_code}：{}",
                response.get("TransMsg").and_then(Value::as_str).unwrap_or("")
            );
        }
        let encoded = response
            .get("Data")
            .and_then(Value::as_str)
            .context("綠界發票回應缺 Data")?;
        Ok(parse_issue_response(aes::decode_data(&self.key, &self.iv, encoded)?))
    }
}

/// 測試用：記錄請求、回預設或指定的結果
#[derive(Default)]
pub struct FakeInvoiceGateway {
    calls: Mutex<Vec<IssueRequest>>,
    next_error: Mutex<Option<String>>,
    next_rtn: Mutex<Option<(i32, String)>>,
}

impl FakeInvoiceGateway {
    pub fn calls(&self) -> Vec<IssueRequest> {
        self.calls.lock().expect("fake invoice calls").clone()
    }

    /// 下一次 issue 回連線層錯誤（Err）
    pub fn fail_next_with_error(&self, msg: &str) {
        *self.next_error.lock().expect("fake invoice next_error") = Some(msg.to_string());
    }

    /// 下一次 issue 回綠界的錯誤碼（Ok 但 RtnCode ≠ 1）
    pub fn fail_next_with_rtn(&self, code: i32, msg: &str) {
        *self.next_rtn.lock().expect("fake invoice next_rtn") = Some((code, msg.to_string()));
    }
}

/// 可替換的閘道：正式打綠界；測試用 Fake
pub enum InvoiceGateway {
    Ecpay(EcpayInvoiceClient),
    Fake(FakeInvoiceGateway),
}

impl InvoiceGateway {
    pub fn ecpay(cfg: &EcpayConfig) -> anyhow::Result<Self> {
        Ok(Self::Ecpay(EcpayInvoiceClient::new(cfg)?))
    }

    pub fn merchant_id(&self) -> &str {
        match self {
            Self::Ecpay(client) => client.merchant_id(),
            Self::Fake(_) => "2000132",
        }
    }

    pub async fn issue(&self, req: &IssueRequest) -> anyhow::Result<IssueResponse> {
        match self {
            Self::Ecpay(client) => client.issue(req).await,
            Self::Fake(fake) => {
                fake.calls.lock().expect("fake invoice calls").push(req.clone());
                let error = fake.next_error.lock().expect("fake invoice next_error").take();
                if let Some(msg) = error {
                    anyhow::bail!("{msg}");
                }
                let rtn = fake.next_rtn.lock().expect("fake invoice next_rtn").take();
                if let Some((code, msg)) = rtn {
                    return Ok(IssueResponse {
                        rtn_code: code,
                        rtn_msg: msg.clone(),
                        invoice_no: String::new(),
                        invoice_date: String::new(),
                        random_number: String::new(),
                        raw: json!({ "RtnCode": code, "RtnMsg": msg }),
                    });
                }
                Ok(parse_issue_response(json!({
                    "RtnCode": 1,
                    "RtnMsg": "開立發票成功",
                    "InvoiceNo": "AB12345678",
                    "InvoiceDate": "2026-09-06 15:30:23",
                    "RandomNumber": "1234"
                })))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use uuid::Uuid;

    use super::*;
    use crate::domain::orders::{OrderItemRow, OrderRow};

    fn sample(invoice_type: &str) -> OrderDetail {
        let (carrier_type, carrier_num, tax_id, title, address, love_code) = match invoice_type {
            "company" => (None, None, Some("04595257"), Some("測試公司"), Some("台北市信義區市府路 1 號"), None),
            "donation" => (None, None, None, None, None, Some("168")),
            "mobile" => (Some("3"), Some("/ABC+123"), None, None, None, None),
            _ => (Some("1"), None, None, None, None, None),
        };
        let s = |v: Option<&str>| v.map(str::to_string);
        OrderDetail {
            order: OrderRow {
                id: Uuid::now_v7(),
                order_no: "DS260906ABCD".to_string(),
                user_id: None,
                guest_token: "t".repeat(64),
                status: "paid".to_string(),
                email: "a@b.co".to_string(),
                recipient_name: "王小明".to_string(),
                recipient_phone: "0912345678".to_string(),
                shipping_method: "home".to_string(),
                subtotal: 600,
                shipping_fee: 100,
                total: 700,
                note: String::new(),
                invoice_type: if invoice_type == "mobile" { "personal".to_string() } else { invoice_type.to_string() },
                invoice_carrier_type: s(carrier_type),
                invoice_carrier_num: s(carrier_num),
                invoice_tax_id: s(tax_id),
                invoice_title: s(title),
                invoice_address: s(address),
                invoice_love_code: s(love_code),
                needs_refund: false,
                created_at: Utc::now(),
                paid_at: Some(Utc::now()),
                shipped_at: None,
                completed_at: None,
                cancelled_at: None,
                cancel_reason: None,
            },
            items: vec![
                OrderItemRow {
                    product_name: "雞肉狗糧".to_string(),
                    variant_label: "S".to_string(),
                    unit_price: 200,
                    quantity: 2,
                    line_total: 400,
                    image_path: None,
                },
                OrderItemRow {
                    product_name: "牛肉狗糧".to_string(),
                    variant_label: "預設".to_string(),
                    unit_price: 200,
                    quantity: 1,
                    line_total: 200,
                    image_path: None,
                },
            ],
            shipment: None,
            payment: None,
            invoice: None,
        }
    }

    #[test]
    fn personal_with_ecpay_carrier() {
        let req = build_issue_request("2000132", &sample("personal"));
        assert_eq!(req.merchant_id, "2000132");
        assert_eq!(req.relate_number, "DS260906ABCD");
        assert_eq!((req.carrier_type.as_str(), req.carrier_num.as_str()), ("1", ""));
        assert_eq!((req.print.as_str(), req.donation.as_str()), ("0", "0"));
        assert_eq!(req.customer_identifier, "");
        assert_eq!(req.customer_id, "");
        assert_eq!(req.customer_name, "王小明");
        assert_eq!(req.customer_email, "a@b.co");
        assert_eq!(req.customer_phone, "0912345678");
        assert_eq!(req.sales_amount, 700);
        assert_eq!(req.items.len(), 3, "兩個品項 + 運費");
        assert_eq!(req.items.iter().map(|i| i.item_seq).collect::<Vec<_>>(), vec![1, 2, 3]);
        assert_eq!(req.items.iter().map(|i| i.item_amount).sum::<i32>(), 700);
        assert_eq!(req.items[0].item_name, "雞肉狗糧（S）");
        assert_eq!((req.items[0].item_count, req.items[0].item_price), (2, 200));
        assert_eq!((req.items[2].item_name.as_str(), req.items[2].item_price, req.items[2].item_word.as_str()), ("運費", 100, "式"));
        assert_eq!((req.tax_type.as_str(), req.inv_type.as_str(), req.vat.as_str()), ("1", "07", "1"));
    }

    #[test]
    fn mobile_barcode_company_and_donation() {
        let req = build_issue_request("2000132", &sample("mobile"));
        assert_eq!((req.carrier_type.as_str(), req.carrier_num.as_str()), ("3", "/ABC+123"));

        let req = build_issue_request("2000132", &sample("company"));
        assert_eq!(req.customer_identifier, "04595257");
        assert_eq!(req.customer_name, "測試公司");
        assert_eq!(req.customer_addr, "台北市信義區市府路 1 號");
        assert_eq!((req.print.as_str(), req.donation.as_str()), ("1", "0"));
        assert_eq!((req.carrier_type.as_str(), req.carrier_num.as_str()), ("", ""));

        let req = build_issue_request("2000132", &sample("donation"));
        assert_eq!((req.print.as_str(), req.donation.as_str(), req.love_code.as_str()), ("0", "1", "168"));
        assert_eq!(req.carrier_type, "");
        assert_eq!(req.customer_identifier, "");
    }

    #[test]
    fn no_shipping_fee_means_no_fee_line() {
        let mut order = sample("personal");
        order.order.shipping_fee = 0;
        order.order.total = 600;
        let req = build_issue_request("2000132", &order);
        assert_eq!(req.items.len(), 2);
        assert_eq!(req.sales_amount, 600);
    }

    #[test]
    fn serializes_with_ecpay_field_names() {
        let v = serde_json::to_value(build_issue_request("2000132", &sample("company"))).unwrap();
        assert_eq!(v["MerchantID"], "2000132");
        assert_eq!(v["RelateNumber"], "DS260906ABCD");
        assert_eq!(v["CustomerIdentifier"], "04595257");
        assert_eq!(v["Print"], "1");
        assert_eq!(v["SalesAmount"], 700);
        assert_eq!(v["Items"][0]["ItemSeq"], 1);
        assert_eq!(v["Items"][2]["ItemName"], "運費");
        assert_eq!(v["InvType"], "07");
        assert_eq!(v["vat"], "1");
        assert!(v.get("merchant_id").is_none(), "不能出現 snake_case");
    }

    #[test]
    fn parses_issue_response() {
        let resp = parse_issue_response(json!({
            "RtnCode": 1, "RtnMsg": "開立發票成功", "InvoiceNo": "AB12345678",
            "InvoiceDate": "2026-09-06 15:30:23", "RandomNumber": "1234"
        }));
        assert!(resp.is_ok());
        assert_eq!(resp.invoice_no, "AB12345678");
        assert_eq!(resp.random_number, "1234");
        assert_eq!(resp.raw["RtnMsg"], "開立發票成功");
        let bad = parse_issue_response(json!({ "RtnCode": 1000007, "RtnMsg": "RelateNumber 重複" }));
        assert!(!bad.is_ok());
        assert_eq!(bad.invoice_no, "");
    }

    #[tokio::test]
    async fn fake_gateway_records_and_fails_on_demand() {
        let gateway = InvoiceGateway::Fake(FakeInvoiceGateway::default());
        let req = build_issue_request("2000132", &sample("personal"));
        let ok = gateway.issue(&req).await.unwrap();
        assert!(ok.is_ok());
        assert_eq!(ok.invoice_no, "AB12345678");
        let InvoiceGateway::Fake(fake) = &gateway else { unreachable!() };
        fake.fail_next_with_rtn(1000007, "RelateNumber 重複");
        let bad = gateway.issue(&req).await.unwrap();
        assert_eq!((bad.rtn_code, bad.rtn_msg.as_str()), (1000007, "RelateNumber 重複"));
        fake.fail_next_with_error("connect timeout");
        assert!(gateway.issue(&req).await.unwrap_err().to_string().contains("connect timeout"));
        assert!(gateway.issue(&req).await.unwrap().is_ok(), "錯誤只影響下一次");
        assert_eq!(fake.calls().len(), 4);
    }
}
```

在 `api/src/ecpay/mod.rs` 加 `pub mod aes;`（最前面）與 `pub mod invoice;`（`pub mod aio;` 之後、`pub mod mac;` 之前）。

- [ ] **Step 4: 跑單元測試（aes、invoice）**

Run（背景、timeout 600000；reqwest 第一次編譯慢）: `export PATH="$HOME/.cargo/bin:$PATH" && DATABASE_URL=postgres://dog_shop:dog_shop@localhost:5435/dog_shop cargo test --manifest-path api/Cargo.toml --lib ecpay`
Expected: aes 5 個、invoice 6 個新測試全過。`known_vector_from_openssl` 失敗表示 padding 或模式不對（要 CBC + PKCS7）。

- [ ] **Step 5: 寫失敗的整合測試 `api/tests/invoice_job.rs`**

```rust
mod common;

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
};
use chrono::{DateTime, TimeZone, Utc};
use dog_shop_api::domain::orders::{self, HomeAddress, InvoiceInput, OrderInput, OrderItemInput, Viewer};
use dog_shop_api::domain::jobs;
use dog_shop_api::ecpay::mac;
use dog_shop_api::jobs::worker;
use dog_shop_api::state::AppState;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

fn company_invoice() -> InvoiceInput {
    InvoiceInput {
        kind: "company".to_string(),
        tax_id: "04595257".to_string(),
        title: "測試公司".to_string(),
        address: "台北市信義區市府路 1 號".to_string(),
        ..Default::default()
    }
}

fn personal_invoice() -> InvoiceInput {
    InvoiceInput {
        kind: "personal".to_string(),
        carrier_type: "1".to_string(),
        ..Default::default()
    }
}

fn input(items: Vec<(Uuid, i32)>, invoice: InvoiceInput) -> OrderInput {
    OrderInput {
        items: items
            .into_iter()
            .map(|(variant_id, qty)| OrderItemInput { variant_id, qty })
            .collect(),
        email: "buyer@test.local".to_string(),
        recipient_name: "王小明".to_string(),
        recipient_phone: "0912345678".to_string(),
        shipping_method: "home".to_string(),
        cvs_store_token: None,
        address: Some(HomeAddress {
            postal_code: "100".to_string(),
            city: "臺北市".to_string(),
            district: "中正區".to_string(),
            street: "重慶南路一段 122 號".to_string(),
        }),
        invoice,
        payment_method: "credit".to_string(),
        note: String::new(),
    }
}

async fn run_all(state: &AppState) {
    while worker::run_once(state).await.unwrap() > 0 {}
}

async fn mark_paid(pool: &PgPool, order_id: Uuid) {
    sqlx::query("UPDATE orders SET status = 'paid', paid_at = now() WHERE id = $1")
        .bind(order_id)
        .execute(pool)
        .await
        .unwrap();
}

async fn enqueue_invoice(pool: &PgPool, order_id: Uuid, dedupe: Option<&str>) {
    let mut tx = pool.begin().await.unwrap();
    jobs::enqueue(&mut tx, jobs::KIND_ISSUE_INVOICE, json!({ "order_id": order_id }), dedupe)
        .await
        .unwrap();
    tx.commit().await.unwrap();
}

type InvoiceRow = (String, Option<String>, Option<DateTime<Utc>>, Option<String>, Option<Value>, Option<Value>, Option<String>);

async fn invoice_row(pool: &PgPool, order_id: Uuid) -> InvoiceRow {
    sqlx::query_as(
        "SELECT status, invoice_no, invoice_date, random_number, request, response, error FROM invoices WHERE order_id = $1",
    )
    .bind(order_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn job_rows(pool: &PgPool, kind: &str) -> Vec<(String, i32, Option<String>)> {
    sqlx::query_as("SELECT status, attempts, last_error FROM jobs WHERE kind = $1 ORDER BY id")
        .bind(kind)
        .fetch_all(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrations = "./migrations")]
async fn paid_order_gets_invoice_and_email(pool: PgPool) {
    let (app, state) = common::app_with_state(pool.clone());
    let (variant, _) = common::active_product(&pool, "雞肉狗糧", 300, 5).await;
    let created = orders::create_order(&pool, input(vec![(variant, 2)], company_invoice()), None)
        .await
        .unwrap();
    mark_paid(&pool, created.order_id).await;
    enqueue_invoice(&pool, created.order_id, Some(&format!("invoice:{}", created.order_id))).await;
    run_all(&state).await;

    let (status, invoice_no, invoice_date, random_number, request, response, error) =
        invoice_row(&pool, created.order_id).await;
    assert_eq!(status, "issued");
    assert_eq!(invoice_no.as_deref(), Some("AB12345678"));
    assert_eq!(random_number.as_deref(), Some("1234"));
    assert_eq!(invoice_date, Some(Utc.with_ymd_and_hms(2026, 9, 6, 7, 30, 23).unwrap()), "台北 15:30:23");
    assert!(error.is_none());
    let request = request.expect("先存請求再送（規格 §14）");
    assert_eq!(request["MerchantID"], "2000132");
    assert_eq!(request["RelateNumber"], created.order_no);
    assert_eq!(request["CustomerIdentifier"], "04595257");
    assert_eq!(request["CustomerName"], "測試公司");
    assert_eq!(request["Print"], "1");
    assert_eq!(request["SalesAmount"], 700);
    assert_eq!(request["Items"].as_array().unwrap().len(), 2);
    assert_eq!(request["Items"][1]["ItemName"], "運費");
    assert_eq!(response.unwrap()["RtnCode"], 1);

    let calls = common::fake_invoices(&state).calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].relate_number, created.order_no);

    let emails = common::sent_emails(&state);
    assert_eq!(emails.len(), 2, "order_created + invoice_issued");
    assert!(emails[1].subject.contains("電子發票"), "{}", emails[1].subject);
    assert!(emails[1].text.contains("AB12345678") && emails[1].text.contains("1234"));

    // 再排一次（不同 dedupe）→ 已開立就略過，不再打綠界、不再寄信
    enqueue_invoice(&pool, created.order_id, None).await;
    run_all(&state).await;
    assert_eq!(common::fake_invoices(&state).calls().len(), 1);
    assert_eq!(common::sent_emails(&state).len(), 2);
    let jobs = job_rows(&pool, "issue_invoice").await;
    assert_eq!(jobs.len(), 2);
    assert!(jobs.iter().all(|j| j.0 == "done"), "{jobs:?}");

    // 訂單頁看得到發票
    let (status, detail, _) = common::send(
        &app,
        common::req("GET", &format!("/api/orders/{}?t={}", created.order_id, created.guest_token), None, None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(detail["invoice"]["status"], "issued");
    assert_eq!(detail["invoice"]["invoice_no"], "AB12345678");
    assert_eq!(detail["invoice"]["random_number"], "1234");
}

#[sqlx::test(migrations = "./migrations")]
async fn ecpay_errors_are_recorded_retried_then_marked_failed(pool: PgPool) {
    let state = common::state(pool.clone());
    let (variant, _) = common::active_product(&pool, "A", 300, 5).await;
    let created = orders::create_order(&pool, input(vec![(variant, 1)], personal_invoice()), None)
        .await
        .unwrap();
    mark_paid(&pool, created.order_id).await;
    let fake = common::fake_invoices(&state);

    fake.fail_next_with_rtn(1000007, "RelateNumber 重複");
    enqueue_invoice(&pool, created.order_id, Some(&format!("invoice:{}", created.order_id))).await;
    assert_eq!(worker::run_once(&state).await.unwrap(), 2, "order_created 信 + issue_invoice");
    let (status, invoice_no, _, _, _, response, error) = invoice_row(&pool, created.order_id).await;
    assert_eq!(status, "pending", "還會重試");
    assert!(invoice_no.is_none());
    assert!(error.unwrap().contains("1000007"));
    assert_eq!(response.unwrap()["RtnMsg"], "RelateNumber 重複");
    let jobs = job_rows(&pool, "issue_invoice").await;
    assert_eq!((jobs[0].0.as_str(), jobs[0].1), ("queued", 1));

    // 讓下一次成為最後一次；這次連線層失敗 → invoices 標 failed
    sqlx::query("UPDATE jobs SET run_at = now(), max_attempts = 2 WHERE kind = 'issue_invoice'")
        .execute(&pool)
        .await
        .unwrap();
    fake.fail_next_with_error("connect timeout");
    assert_eq!(worker::run_once(&state).await.unwrap(), 1);
    let (status, _, _, _, _, _, error) = invoice_row(&pool, created.order_id).await;
    assert_eq!(status, "failed");
    assert!(error.unwrap().contains("connect timeout"));
    let jobs = job_rows(&pool, "issue_invoice").await;
    assert_eq!((jobs[0].0.as_str(), jobs[0].1), ("failed", 2));
    assert_eq!(fake.calls().len(), 2);
    assert_eq!(common::sent_emails(&state).len(), 1, "沒有 invoice_issued 信");
}

#[sqlx::test(migrations = "./migrations")]
async fn unpaid_or_cancelled_order_is_skipped(pool: PgPool) {
    let state = common::state(pool.clone());
    let (variant, _) = common::active_product(&pool, "A", 300, 5).await;
    let created = orders::create_order(&pool, input(vec![(variant, 1)], personal_invoice()), None)
        .await
        .unwrap();

    enqueue_invoice(&pool, created.order_id, Some(&format!("invoice:{}", created.order_id))).await;
    run_all(&state).await;
    assert_eq!(common::fake_invoices(&state).calls().len(), 0, "待付款不開");
    assert_eq!(invoice_row(&pool, created.order_id).await.0, "pending");

    orders::cancel(&pool, created.order_id, &Viewer::Guest(created.guest_token.clone()), "buyer")
        .await
        .unwrap();
    enqueue_invoice(&pool, created.order_id, None).await;
    run_all(&state).await;
    assert_eq!(common::fake_invoices(&state).calls().len(), 0, "取消的不開");
    let jobs = job_rows(&pool, "issue_invoice").await;
    assert!(jobs.iter().all(|j| j.0 == "done"), "略過算完成：{jobs:?}");
}

/// 模擬綠界伺服器的 ReturnURL（同 tests/ecpay_payment.rs）
async fn ecpay_return(app: &Router, mtn: &str) -> (StatusCode, String) {
    let mut fields: Vec<(String, String)> = [
        ("MerchantID", "3002607"),
        ("MerchantTradeNo", mtn),
        ("RtnCode", "1"),
        ("RtnMsg", "交易成功"),
        ("TradeNo", "2609061530000001"),
        ("TradeAmt", "700"),
        ("PaymentDate", "2026/09/06 15:30:23"),
        ("PaymentType", "Credit_CreditCard"),
        ("TradeDate", "2026/09/06 15:28:00"),
        ("SimulatePaid", "0"),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v.to_string()))
    .collect();
    let mac = mac::check_mac_value("pwFHCqoQZGmho4w6", "EkRm7iFT261dpevs", &fields);
    fields.push(("CheckMacValue".to_string(), mac));
    let body = form_urlencoded::Serializer::new(String::new())
        .extend_pairs(fields.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .finish();
    let request = Request::builder()
        .method("POST")
        .uri("/api/ecpay/payment/return")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from(body))
        .unwrap();
    let (status, value, _) = common::send(app, request).await;
    let text = match value {
        Value::String(s) => s,
        other => other.to_string(),
    };
    (status, text)
}

#[sqlx::test(migrations = "./migrations")]
async fn full_flow_from_return_callback(pool: PgPool) {
    let (app, state) = common::app_with_state(pool.clone());
    let (variant, _) = common::active_product(&pool, "雞肉狗糧", 300, 5).await;
    let (status, created, _) = common::send(
        &app,
        common::req(
            "POST",
            "/api/orders",
            None,
            Some(json!({
                "items": [{ "variant_id": variant, "qty": 2 }],
                "email": "buyer@test.local",
                "recipient_name": "王小明",
                "recipient_phone": "0912345678",
                "shipping_method": "home",
                "address": { "postal_code": "100", "city": "臺北市", "district": "中正區", "street": "重慶南路一段 122 號" },
                "invoice": { "type": "company", "tax_id": "04595257", "title": "測試公司", "address": "台北市信義區市府路 1 號" },
                "payment_method": "credit",
                "note": ""
            })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let order_id = Uuid::parse_str(created["order_id"].as_str().unwrap()).unwrap();
    let mtn = created["ecpay"]["fields"]["MerchantTradeNo"].as_str().unwrap().to_string();

    let (status, text) = ecpay_return(&app, &mtn).await;
    assert_eq!((status, text.as_str()), (StatusCode::OK, "1|OK"));
    run_all(&state).await;

    let (inv_status, invoice_no, ..) = invoice_row(&pool, order_id).await;
    assert_eq!(inv_status, "issued");
    assert_eq!(invoice_no.as_deref(), Some("AB12345678"));
    assert_eq!(common::fake_invoices(&state).calls().len(), 1);
    let subjects: Vec<String> = common::sent_emails(&state).into_iter().map(|m| m.subject).collect();
    assert_eq!(subjects.len(), 3, "{subjects:?}");
    assert!(subjects[0].contains("已成立"));
    assert!(subjects[1].contains("已收到款項"));
    assert!(subjects[2].contains("電子發票"));
    let all_jobs: Vec<(String, String)> = sqlx::query_as("SELECT kind, status FROM jobs ORDER BY id")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert!(all_jobs.iter().all(|(_, s)| s == "done"), "{all_jobs:?}");
    assert_eq!(all_jobs.len(), 4, "order_created、issue_invoice、payment_received、invoice_issued");
}
```

Run: `export PATH="$HOME/.cargo/bin:$PATH" && DATABASE_URL=postgres://dog_shop:dog_shop@localhost:5435/dog_shop cargo test --manifest-path api/Cargo.toml --test invoice_job`
Expected: 編譯錯誤（`common::fake_invoices` 不存在）。

- [ ] **Step 6: `api/src/domain/invoices.rs` 檔尾加**

```rust
/// 先存請求再送（規格 §14）
pub async fn record_request(db: &PgPool, order_id: Uuid, request: &Value) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE invoices SET request = $2, updated_at = now() WHERE order_id = $1")
        .bind(order_id)
        .bind(request)
        .execute(db)
        .await?;
    Ok(())
}

pub async fn mark_issued(
    db: &PgPool,
    order_id: Uuid,
    invoice_no: &str,
    invoice_date: Option<DateTime<Utc>>,
    random_number: &str,
    response: &Value,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE invoices SET status = $2, invoice_no = $3, invoice_date = $4, random_number = $5, response = $6,
                error = NULL, updated_at = now()
         WHERE order_id = $1",
    )
    .bind(order_id)
    .bind(STATUS_ISSUED)
    .bind(invoice_no)
    .bind(invoice_date)
    .bind(random_number)
    .bind(response)
    .execute(db)
    .await?;
    Ok(())
}

/// 記錄一次失敗；最後一次嘗試時把狀態標 failed（後台顯示、重試在計畫 4）
pub async fn record_failure(
    db: &PgPool,
    order_id: Uuid,
    response: Option<&Value>,
    error: &str,
    final_attempt: bool,
) -> Result<(), sqlx::Error> {
    let error: String = error.chars().take(1000).collect();
    sqlx::query(
        "UPDATE invoices SET status = CASE WHEN $4 THEN $5 ELSE status END, response = COALESCE($2, response),
                error = $3, updated_at = now()
         WHERE order_id = $1",
    )
    .bind(order_id)
    .bind(response)
    .bind(error)
    .bind(final_attempt)
    .bind(STATUS_FAILED)
    .execute(db)
    .await?;
    Ok(())
}
```

`use` 區加 `use serde_json::Value;`。

- [ ] **Step 7: `api/src/jobs/handlers.rs`**

(a) `use` 區：`domain::{ jobs::{...}, orders::{...}, password_resets, payments, settings::{...}, users }` 改成也引入 `invoices`，並且 `orders` 的清單加 `STATUS_COMPLETED, STATUS_PAID, STATUS_SHIPPED`：

```rust
    domain::{
        invoices,
        jobs::{self, KIND_ISSUE_INVOICE, KIND_SEND_EMAIL},
        orders::{
            self, OrderDetail, PAYMENT_ATM, PAYMENT_CREDIT, PAYMENT_CVS_CODE, SHIPPING_CVS,
            STATUS_COMPLETED, STATUS_PAID, STATUS_SHIPPED,
        },
        password_resets, payments,
        settings::{self, ShopSettings},
        users,
    },
    ecpay::{invoice, time},
```

並加 `use serde_json::json;`（放在 `use serde_json::Value;` 旁：改成 `use serde_json::{Value, json};`）。

(b) `run` 的分支改成：

```rust
        KIND_ISSUE_INVOICE => issue_invoice(state, job).await,
```

(c) 檔尾加：

```rust
/// 開立電子發票（規格 §8.4、與規格不同之處 29）。已開立就略過；訂單不是已付款狀態也略過（done）。
/// 綠界回錯或連不上 → 記在 invoices.error 並回 Err 讓 worker 重試；最後一次失敗把 invoices 標 failed
async fn issue_invoice(state: &AppState, job: &Job) -> anyhow::Result<()> {
    let order_id = payload_uuid(&job.payload, "order_id")?;
    let Some(detail) = orders::get_detail(&state.db, order_id).await? else {
        tracing::warn!(%order_id, "訂單不存在，略過開發票");
        return Ok(());
    };
    let Some(invoice) = detail.invoice.as_ref() else {
        anyhow::bail!("訂單 {order_id} 沒有 invoices 列");
    };
    if invoice.status == invoices::STATUS_ISSUED {
        tracing::info!(%order_id, "發票已開立，略過");
        return Ok(());
    }
    if !matches!(
        detail.order.status.as_str(),
        STATUS_PAID | STATUS_SHIPPED | STATUS_COMPLETED
    ) {
        tracing::warn!(%order_id, status = %detail.order.status, "訂單不是已付款狀態，不開發票");
        return Ok(());
    }

    let request = invoice::build_issue_request(state.invoices.merchant_id(), &detail);
    invoices::record_request(&state.db, order_id, &serde_json::to_value(&request)?).await?;
    match state.invoices.issue(&request).await {
        Ok(resp) if resp.is_ok() => {
            let invoice_date = time::parse_taipei(&resp.invoice_date, "%Y-%m-%d %H:%M:%S");
            invoices::mark_issued(
                &state.db,
                order_id,
                &resp.invoice_no,
                invoice_date,
                &resp.random_number,
                &resp.raw,
            )
            .await?;
            let mut tx = state.db.begin().await?;
            jobs::enqueue(
                &mut tx,
                KIND_SEND_EMAIL,
                json!({ "template": "invoice_issued", "order_id": order_id }),
                Some(&format!("email:invoice_issued:{order_id}")),
            )
            .await?;
            tx.commit().await?;
            tracing::info!(%order_id, invoice_no = %resp.invoice_no, "發票開立成功");
            Ok(())
        }
        Ok(resp) => {
            let msg = format!("綠界 RtnCode {}：{}", resp.rtn_code, resp.rtn_msg);
            invoices::record_failure(&state.db, order_id, Some(&resp.raw), &msg, job.is_last_attempt())
                .await?;
            anyhow::bail!("{msg}")
        }
        Err(e) => {
            let msg = format!("{e:#}");
            invoices::record_failure(&state.db, order_id, None, &msg, job.is_last_attempt()).await?;
            Err(e)
        }
    }
}
```

- [ ] **Step 8: `AppState`、`main.rs`、`tests/common`**

`api/src/state.rs`：`use crate::{config::Config, mail::Mailer};` 改成 `use crate::{config::Config, ecpay::invoice::InvoiceGateway, mail::Mailer};`；struct 加：

```rust
    /// 電子發票出口（綠界／測試 Fake）
    pub invoices: Arc<InvoiceGateway>,
```

`api/src/main.rs`：`use dog_shop_api::{app, config::Config, jobs, mail, state::AppState};` 改成 `use dog_shop_api::{app, config::Config, ecpay, jobs, mail, state::AppState};`；`main()` 的第一行（`dotenvy::dotenv().ok();` 之前）加：

```rust
    // reqwest 用 rustls-no-provider，整個行程要先裝好 crypto provider（與規格不同之處 32）；重複安裝會回 Err，忽略
    let _ = rustls::crypto::ring::default_provider().install_default();
```

`let mailer = …` 之後加 `let invoices = Arc::new(ecpay::invoice::InvoiceGateway::ecpay(&config)?);`，`AppState { … }` 加 `invoices,`。

`api/tests/common/mod.rs`：`use dog_shop_api::{app, config::Config, mail::{Email, Mailer}, state::AppState};` 改成

```rust
use dog_shop_api::{
    app,
    config::Config,
    ecpay::invoice::{FakeInvoiceGateway, InvoiceGateway},
    mail::{Email, Mailer},
    state::AppState,
};
```

`state()` 的 `AppState { … }` 加 `invoices: Arc::new(InvoiceGateway::Fake(FakeInvoiceGateway::default())),`；檔尾加：

```rust
/// 測試裡的假發票閘道（看 calls()、設定下一次失敗）
pub fn fake_invoices(state: &AppState) -> &FakeInvoiceGateway {
    match &*state.invoices {
        InvoiceGateway::Fake(fake) => fake,
        InvoiceGateway::Ecpay(_) => panic!("測試的 AppState 要用 InvoiceGateway::Fake"),
    }
}
```

- [ ] **Step 9: 跑測試、clippy、fmt**

Run（背景、timeout 600000）: `export PATH="$HOME/.cargo/bin:$PATH" && cargo fmt --all --manifest-path api/Cargo.toml && DATABASE_URL=postgres://dog_shop:dog_shop@localhost:5435/dog_shop cargo test --manifest-path api/Cargo.toml && cargo clippy --all-targets --manifest-path api/Cargo.toml -- -D warnings`
Expected: 全綠（invoice_job 4 個新增）。

- [ ] **Step 10: 確認 api 啟動（rustls provider、發票閘道）**

Run（背景）: `export PATH="$HOME/.cargo/bin:$PATH" && cargo run --manifest-path api/Cargo.toml`
Expected: `api listening` 正常，沒有 rustls 的 panic。停掉。

- [ ] **Step 11: Commit**

```bash
git add api/Cargo.toml api/Cargo.lock api/src/ecpay/mod.rs api/src/ecpay/aes.rs api/src/ecpay/invoice.rs api/src/domain/invoices.rs api/src/jobs/handlers.rs api/src/state.rs api/src/main.rs api/tests/common/mod.rs api/tests/invoice_job.rs
git commit -m "feat(api): 電子發票（AES 加解密、B2C Issue 客戶端、issue_invoice job、失敗標記）"
```

---
### Task 11: 訂單頁：繳費資訊、付款輪詢、重新付款、已付款、發票號碼

**Files:**
- Modify: `web/src/lib/types.ts`（`OrderDetail` 加 `invoice`；新增 `OrderInvoice`、`RepayResponse`）
- Create: `web/src/lib/orderPage.ts`
- Create: `web/src/lib/orderPage.test.ts`
- Modify: `web/src/routes/orders/[id]/+page.svelte`（整檔改寫）

**Interfaces:**
- Consumes: `GET /api/orders/{id}` 的 `payment`（Task 6 之後有 `atm_bank_code`／`atm_vaccount`／`cvs_payment_no`／`expire_at`）與 `invoice`（Task 2）；`POST /api/orders/{id}/repay`（Task 7）；`postToEcpay`（Task 5）；layout data 的 `settings.payment_methods`。
- Produces：
  - `types.ts`：`OrderInvoice = { status: 'pending' | 'issued' | 'failed'; invoice_no: string | null; invoice_date: string | null; random_number: string | null }`；`OrderDetail.invoice: OrderInvoice | null`；`RepayResponse = { ecpay: EcpayForm }`
  - `lib/orderPage.ts`：`POLL_INTERVAL_MS = 3_000`、`POLL_MAX_MS = 120_000`、`hasPaymentInfo(order)`、`needsPolling(order)`、`paymentExpired(order, now?)`

- [ ] **Step 1: 型別（`web/src/lib/types.ts`）**

在 `OrderPayment` 型別之後、`OrderDetail` 之前加：

```ts
export type OrderInvoice = {
	status: 'pending' | 'issued' | 'failed';
	invoice_no: string | null;
	invoice_date: string | null;
	random_number: string | null;
};
```

`OrderDetail` 的 `payment: OrderPayment | null;` 之後加一行 `invoice: OrderInvoice | null;`。

檔尾加：

```ts
export type RepayResponse = { ecpay: EcpayForm };
```

- [ ] **Step 2: 寫失敗的 vitest `web/src/lib/orderPage.test.ts`**

```ts
import { describe, expect, it } from 'vitest';
import { hasPaymentInfo, needsPolling, paymentExpired } from './orderPage';
import type { OrderDetail, OrderPayment } from './types';

function order(status: OrderDetail['status'], payment: Partial<OrderPayment> | null): OrderDetail {
	const base: OrderDetail = {
		id: 'o',
		order_no: 'DS260906ABCD',
		status,
		email: 'a@b.co',
		recipient_name: '王小明',
		recipient_phone: '0912345678',
		shipping_method: 'home',
		subtotal: 300,
		shipping_fee: 100,
		total: 400,
		note: '',
		invoice_type: 'personal',
		invoice_carrier_type: '1',
		invoice_carrier_num: null,
		invoice_tax_id: null,
		invoice_title: null,
		invoice_address: null,
		invoice_love_code: null,
		created_at: '2026-09-06T00:00:00Z',
		paid_at: null,
		shipped_at: null,
		completed_at: null,
		cancelled_at: null,
		cancel_reason: null,
		items: [],
		shipment: null,
		payment: null,
		invoice: null
	};
	return {
		...base,
		payment: payment
			? {
					method: 'credit',
					status: 'pending',
					amount: 400,
					atm_bank_code: null,
					atm_vaccount: null,
					cvs_payment_no: null,
					expire_at: null,
					...payment
				}
			: null
	};
}

describe('orderPage', () => {
	it('hasPaymentInfo 看 ATM 帳號或超商代碼', () => {
		expect(hasPaymentInfo(order('pending_payment', null))).toBe(false);
		expect(hasPaymentInfo(order('pending_payment', {}))).toBe(false);
		expect(hasPaymentInfo(order('pending_payment', { atm_vaccount: '1234567890123456' }))).toBe(true);
		expect(hasPaymentInfo(order('pending_payment', { cvs_payment_no: 'LLL26090612345' }))).toBe(true);
	});

	it('待付款且沒繳費資訊才輪詢', () => {
		expect(needsPolling(order('pending_payment', null))).toBe(true);
		expect(needsPolling(order('pending_payment', {}))).toBe(true);
		expect(needsPolling(order('pending_payment', { atm_vaccount: '123' }))).toBe(false);
		expect(needsPolling(order('paid', {}))).toBe(false);
		expect(needsPolling(order('cancelled', null))).toBe(false);
	});

	it('paymentExpired 用期限比現在', () => {
		const now = Date.parse('2026-09-06T00:00:00Z');
		expect(paymentExpired(order('pending_payment', { expire_at: '2026-09-05T23:59:59Z' }), now)).toBe(true);
		expect(paymentExpired(order('pending_payment', { expire_at: '2026-09-06T00:00:01Z' }), now)).toBe(false);
		expect(paymentExpired(order('pending_payment', {}), now)).toBe(false);
		expect(paymentExpired(order('pending_payment', null), now)).toBe(false);
	});
});
```

Run: `pnpm -C web test`
Expected: 失敗（`./orderPage` 不存在）。

- [ ] **Step 3: 建 `web/src/lib/orderPage.ts`**

```ts
import type { OrderDetail } from '$lib/types';

/** 付款中每 3 秒重新載入，最多 2 分鐘（規格 §6.1） */
export const POLL_INTERVAL_MS = 3_000;
export const POLL_MAX_MS = 120_000;

/** ATM 帳號或超商代碼已經拿到了（PaymentInfoURL 已回） */
export function hasPaymentInfo(order: OrderDetail): boolean {
	const p = order.payment;
	return !!p && (!!p.atm_vaccount || !!p.cvs_payment_no);
}

/** 還在等綠界回呼：待付款、而且還沒有繳費資訊（規格 §7 第 8 點） */
export function needsPolling(order: OrderDetail): boolean {
	return order.status === 'pending_payment' && !hasPaymentInfo(order);
}

/** 繳費期限過了（用瀏覽器時間比） */
export function paymentExpired(order: OrderDetail, now: number = Date.now()): boolean {
	const e = order.payment?.expire_at;
	return !!e && new Date(e).getTime() < now;
}
```

Run: `pnpm -C web test`
Expected: 27 個全過（24 + 3）。

- [ ] **Step 4: 改寫 `web/src/routes/orders/[id]/+page.svelte`（整檔）**

```svelte
<script lang="ts">
	import { onMount, untrack } from 'svelte';
	import { invalidateAll } from '$app/navigation';
	import { api, ApiError } from '$lib/api';
	import { postToEcpay } from '$lib/ecpay';
	import { formatDate, twd } from '$lib/format';
	import { CVS_LABELS, INVOICE_LABELS, ORDER_STATUS_LABELS, PAYMENT_LABELS } from '$lib/labels';
	import { POLL_INTERVAL_MS, POLL_MAX_MS, hasPaymentInfo, needsPolling, paymentExpired } from '$lib/orderPage';
	import { toast } from '$lib/toast.svelte';
	import type { PaymentMethod, RepayResponse } from '$lib/types';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
	const o = $derived(data.order);
	const qs = $derived(data.token ? `?t=${encodeURIComponent(data.token)}` : '');
	const enabledPayments = $derived(
		(['credit', 'atm', 'cvs_code'] as PaymentMethod[]).filter((m) => data.settings.payment_methods[m])
	);
	let confirming = $state(false);
	let cancelling = $state(false);
	let repaying = $state(false);
	let pollTimedOut = $state(false);
	let repayMethod = $state<PaymentMethod>(untrack(() => data.order.payment?.method ?? 'credit'));

	// 付款中每 3 秒重新載入，最多 2 分鐘（規格 §6.1）；拿到繳費資訊或狀態改變就停
	onMount(() => {
		if (!needsPolling(untrack(() => o))) return;
		const started = Date.now();
		const id = setInterval(async () => {
			if (!needsPolling(o)) {
				clearInterval(id);
				return;
			}
			if (Date.now() - started >= POLL_MAX_MS) {
				clearInterval(id);
				pollTimedOut = true;
				return;
			}
			await invalidateAll();
		}, POLL_INTERVAL_MS);
		return () => clearInterval(id);
	});

	async function cancel() {
		if (cancelling) return;
		cancelling = true;
		try {
			await api(`/api/orders/${o.id}/cancel${qs}`, { method: 'POST' });
			confirming = false;
			await invalidateAll();
			toast.show('訂單已取消');
		} catch (err) {
			toast.show(err instanceof ApiError ? err.message : '取消失敗');
			confirming = false;
			await invalidateAll();
		} finally {
			cancelling = false;
		}
	}

	/** 重新付款（規格 §7 第 9 點）：拿新表單後離開本頁去綠界 */
	async function repay() {
		if (repaying) return;
		repaying = true;
		try {
			const res = await api<RepayResponse>(`/api/orders/${o.id}/repay${qs}`, {
				method: 'POST',
				body: JSON.stringify({ payment_method: repayMethod })
			});
			postToEcpay(res.ecpay);
		} catch (err) {
			toast.show(err instanceof ApiError ? err.message : '重新付款失敗');
			repaying = false;
			await invalidateAll();
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
		{#if o.payment && hasPaymentInfo(o)}
			{#if o.payment.atm_vaccount}
				<p class="mt-2">請在期限內轉帳到下面的帳號：</p>
				<p class="mt-1 text-lg font-bold">銀行代碼 {o.payment.atm_bank_code}　帳號 {o.payment.atm_vaccount}</p>
			{:else}
				<p class="mt-2">請到超商多媒體機台輸入繳費代碼：</p>
				<p class="mt-1 text-lg font-bold">{o.payment.cvs_payment_no}</p>
			{/if}
			<p class="mt-1 text-gray-700">
				金額 {twd(o.payment.amount)}{#if o.payment.expire_at}　繳費期限 {formatDate(o.payment.expire_at)}{/if}
			</p>
			{#if paymentExpired(o)}
				<p class="mt-2 text-red-700">繳費期限已過，請重新付款。</p>
			{:else}
				<p class="mt-2 text-gray-700">繳費後幾分鐘內會收到付款成功的 Email；重新整理這一頁也會更新。</p>
			{/if}
		{:else if pollTimedOut}
			<p class="mt-1 text-gray-700">還沒收到付款結果。請重新整理這一頁；若已付款卻沒更新，請聯絡我們。</p>
		{:else}
			<p class="mt-1 text-gray-700">等候綠界付款結果中…（每 3 秒自動更新）</p>
		{/if}
		<div class="mt-3 flex flex-wrap items-center gap-2">
			<label class="text-gray-700" for="repay-method">重新付款：</label>
			<select id="repay-method" bind:value={repayMethod} class="rounded border border-gray-300 px-2 py-1">
				{#each enabledPayments as m (m)}
					<option value={m}>{PAYMENT_LABELS[m]}</option>
				{/each}
			</select>
			<button type="button" onclick={repay} disabled={repaying} class="rounded bg-gray-900 px-3 py-1 text-white disabled:opacity-50">
				{repaying ? '前往付款…' : '前往付款'}
			</button>
		</div>
	</div>
{:else if o.status === 'paid' || o.status === 'shipped' || o.status === 'completed'}
	<div class="mt-4 rounded border border-green-300 bg-green-50 p-4 text-sm">
		<p class="font-medium">已付款{#if o.paid_at}（{formatDate(o.paid_at)}）{/if}，目前狀態：{ORDER_STATUS_LABELS[o.status]}</p>
	</div>
{:else if o.status === 'refunded'}
	<div class="mt-4 rounded border border-gray-300 bg-gray-50 p-4 text-sm">這筆訂單已退款。</div>
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
		{#if o.invoice?.status === 'issued'}
			<p class="mt-2">
				發票號碼 {o.invoice.invoice_no}　隨機碼 {o.invoice.random_number}{#if o.invoice.invoice_date}　{formatDate(o.invoice.invoice_date)}{/if}
			</p>
		{:else if o.status === 'paid' || o.status === 'shipped' || o.status === 'completed'}
			<p class="mt-2 text-gray-500">發票開立中，開好會通知您。</p>
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

- [ ] **Step 5: `pnpm -C web check`、`pnpm -C web test`、`pnpm -C web build`**

Run（背景）: `pnpm -C web check && pnpm -C web test && pnpm -C web build`
Expected: 0 錯誤 0 警告、27 個測試過、build 成功。`data.settings` 來自 `+layout.server.ts`（`PageProps['data']` 含 layout 資料）；若 svelte-check 說沒有 `settings`，先跑 `pnpm -C web prepare` 讓 `$types` 重新產生。

- [ ] **Step 6: 手動走一次（api、web dev 都在跑）**

1. 下單（信用卡）→ 到綠界 stage 頁 → 直接按瀏覽器「上一頁」回結帳頁 → 手動輸入訂單頁網址（api log 有 `ClientBackURL`，或從 `/account/orders`）：看到「等候綠界付款結果中…（每 3 秒自動更新）」，Network 面板每 3 秒一次 `__data.json`。
2. 用 SQL 模擬拿到 ATM 帳號：`docker exec -i dog_shop-db-1 psql -U dog_shop -d dog_shop -c "UPDATE payments SET atm_bank_code='812', atm_vaccount='1234567890123456', expire_at=now()+interval '3 days' WHERE merchant_trade_no='<剛才的 MerchantTradeNo>'"` → 3 秒內頁面顯示銀行代碼與帳號、輪詢停止。
3. 選「ATM 轉帳」按「前往付款」→ 送到綠界 stage（新的 MerchantTradeNo 結尾 02）。
4. 用 SQL 把訂單改成 paid、invoices 改成 issued（`UPDATE orders SET status='paid', paid_at=now() …; UPDATE invoices SET status='issued', invoice_no='AB12345678', random_number='1234', invoice_date=now() …`）→ 重新整理：綠色「已付款」區塊、發票號碼與隨機碼。

- [ ] **Step 7: e2e 仍過**

Run: `pnpm -C web test:e2e`
Expected: `1 passed`（訂單頁仍顯示「待付款」與取消流程）。

- [ ] **Step 8: Commit**

```bash
git add web/src/lib/types.ts web/src/lib/orderPage.ts web/src/lib/orderPage.test.ts web/src/routes/orders/[id]/+page.svelte
git commit -m "feat(web): 訂單頁付款狀態（ATM／超商繳費資訊、輪詢、重新付款、已付款與發票號碼）"
```

---
### Task 12: 文件與整體驗證

**Files:**
- Create: `docs/dev/ecpay-stage.md`

**Interfaces:** 無程式碼變更；這個任務把手動測試步驟寫下來並跑一次全套自動測試。

- [ ] **Step 1: 建 `docs/dev/ecpay-stage.md`**

```markdown
# 綠界 stage 手動測試

自動測試不會真的打綠界；付款回呼、繳費資訊、發票開立要用綠界的測試環境走一次。

## 前置

1. `docker compose -f deploy/docker-compose.dev.yml up -d db`
2. 讓綠界打得到你的機器：`cloudflared tunnel --url http://localhost:5173`（沒有就 `brew install cloudflared`），記下它印出的 `https://xxxx.trycloudflare.com`。
3. 根目錄 `.env`：`PUBLIC_BASE_URL=https://xxxx.trycloudflare.com`、`ECPAY_ENV=stage`（AIO／發票憑證留空會用公開測試憑證）。要看信件內容再加 `RUST_LOG=info,tower_http=info,mail_body=debug`（只在開發機；內文含重設連結）。
4. 啟動：`export PATH="$HOME/.cargo/bin:$PATH" && cargo run --manifest-path api/Cargo.toml`、`pnpm -C web dev`。Vite 會把 `/api` 轉到 :8080，所以 cloudflared 只要指到 :5173。

## 信用卡

1. 瀏覽器開 `https://xxxx.trycloudflare.com`，加入購物車 → 結帳 → 付款方式「信用卡」→ 送出。
2. 綠界 stage 頁：卡號 `4311-9511-1111-1111`、有效期任意未來月年、安全碼 `222`；OTP `1234`。
3. 回到訂單頁（綠界的「返回商店」= `ClientBackURL`）：幾秒內狀態變「已付款」。
4. api log：`ReturnURL 處理完成 … outcome=Paid`，接著 `發票開立成功 invoice_no=…`、兩封 Email（`已收到款項`、`電子發票已開立`）的 info。
5. DB：`SELECT status, invoice_no, random_number FROM invoices` 有值；`SELECT status, ecpay_trade_no, raw->>'RtnMsg' FROM payments`。
6. 綠界發票 stage 後台 `https://einvoice-stage.ecpay.com.tw`（Stagetest1234 / test1234）→ 發票查詢，能看到同一張。

## 模擬付款（SimulatePaid）

綠界廠商後台 `https://vendor-stage.ecpay.com.tw`（stagetest3 / test1234）→ 一般訂單查詢 → 找到 `MerchantTradeNo` → 「模擬付款」。api log 應該只有 `綠界模擬付款通知，只記 log 不改狀態`，訂單維持待付款，回 `1|OK`。

## ATM 與超商代碼

1. 結帳選「ATM 轉帳」→ 綠界頁取得虛擬帳號 → 「返回商店」。訂單頁 3 秒內顯示銀行代碼、帳號、期限（該日 23:59:59）；Email log 有 `繳費資訊`。
2. 「超商代碼繳費」同理，顯示繳費代碼與期限。
3. 在廠商後台對這筆「模擬付款」是 SimulatePaid，不會變已付款；真的付款要用綠界 stage 提供的方式，或直接用 `tests/ecpay_payment.rs` 的整合測試驗證 ReturnURL 邏輯。

## 過期與遲到付款

1. `UPDATE orders SET created_at = now() - interval '4 days' WHERE order_no = 'DS…'` → 最多等 10 分鐘（或重啟 api，排程第一次立刻跑）→ 訂單 `cancelled`、`cancel_reason = expired`、庫存回來、payments `expired`。
2. 對這筆已取消的訂單用廠商後台「模擬付款」不會改狀態（SimulatePaid）；要驗證遲到付款請看 `tests/ecpay_payment.rs::late_payment_after_cancel_sets_needs_refund`。

## 重新付款

訂單頁選付款方式按「前往付款」→ 綠界頁的 `MerchantTradeNo` 結尾從 `01` 變 `02`。

## 忘記密碼

`/forgot-password` 送出 → api log 有 `Email（未設定 SMTP，只記 log）subject=【…】重設密碼`；開 `mail_body=debug` 才看得到連結，貼到瀏覽器可以重設。10 分鐘內再送一次不會再寄（log：`已寄過重設信，略過`）。

## 收工

`lsof -ti :8080`、`lsof -ti :5173` 取 pid 後 `kill`；把 `.env` 的 `PUBLIC_BASE_URL` 改回 `http://localhost:5173`。
```

- [ ] **Step 2: 全套自動測試**

Run（背景、timeout 600000）: `export PATH="$HOME/.cargo/bin:$PATH" && cargo fmt --all --check --manifest-path api/Cargo.toml && cargo clippy --all-targets --manifest-path api/Cargo.toml -- -D warnings && DATABASE_URL=postgres://dog_shop:dog_shop@localhost:5435/dog_shop cargo test --manifest-path api/Cargo.toml`
Expected: 全綠。整合測試檔 21 個（計畫 2 的 16 個 + `ecpay_payment`、`repay`、`jobs_worker`、`mail_jobs`、`invoice_job`）。

Run（背景）: `pnpm -C web check && pnpm -C web test && pnpm -C web build`
Expected: 0 錯誤 0 警告、27 個測試、build 成功。

Run（api 與 web dev 起來後）: `pnpm -C web test:e2e`
Expected: `1 passed`。

- [ ] **Step 3: Commit**

```bash
git add docs/dev/ecpay-stage.md
git commit -m "docs: 綠界 stage 手動測試步驟"
```

---

## 計畫 3 完成時的驗收清單

全部做完後，從乾淨狀態走一次（每一行都要成立）：

1. `docker compose -f deploy/docker-compose.dev.yml up -d db` → healthy；`cargo run --manifest-path api/Cargo.toml` 啟動時跑完 `0003_invoices.sql`，log 有 `SMTP 未設定` 的 warn 與 `jobs worker 與排程工作已啟動`，沒有 rustls panic。
2. `DATABASE_URL=… cargo test --manifest-path api/Cargo.toml` 全綠（單元 + 21 個整合測試檔），含：CheckMacValue 對照綠界文件範例、AES 對照 openssl 向量、回呼重複通知冪等、遲到付款 `needs_refund`、過期歸還庫存、worker 退避與 failed、密碼重設節流、發票開立與失敗標記。`cargo fmt --all --check`、`cargo clippy --all-targets -- -D warnings` 乾淨。
3. `pnpm -C web test`（27 個）、`pnpm -C web check`（0 錯誤 0 警告）、`pnpm -C web build` 全過。
4. `pnpm -C web test:e2e` → 1 passed：送出訂單後攔到送往 `payment-stage.ecpay.com.tw/Cashier/AioCheckOut/V5` 的表單，`MerchantID=3002607`、`ChoosePayment=Credit`、`CheckMacValue` 64 碼；經 `ClientBackURL` 回到訂單頁、取消成功。
5. 依 `docs/dev/ecpay-stage.md` 用 cloudflared 走綠界 stage：信用卡付款 → 訂單頁「已付款」、`invoices.status = issued`、log 有兩封 Email；ATM → 訂單頁顯示帳號與期限；廠商後台「模擬付款」只記 log。
6. 訂單頁待付款時每 3 秒輪詢、2 分鐘後顯示「請重新整理」；「前往付款」送到綠界且 `MerchantTradeNo` 結尾 `02`。
7. `/forgot-password` 送出後 log 有重設信的 subject；`RUST_LOG=…,mail_body=debug` 時看得到連結且能用；10 分鐘內第二次不寄。
8. `git log --oneline` 看到本計畫 12 個 commit 都在 `worktree-mvp-design`；`git status` 乾淨；沒有 push。

## 交給計畫 4 的事項（寫計畫 4 時必看）

1. **物流憑證與 MD5**：`Config::EcpayConfig` 加 `logistics: EcpayCredentials`（`credentials("ECPAY_LOGISTICS", env, STAGE_LOGISTICS)`，stage 預設值到 developers.ecpay.com.tw 物流「測試介接資訊」頁確認後填進 `config.rs` 與 `.env.example`）；`ecpay::mac` 加 MD5 版本（`md-5 = "0.10"`，與 `sha2` 同一代 digest），簽名建議 `check_mac_value_md5(key, iv, params)`，用物流文件範例當測試向量（與規格不同之處 30）。
2. **物流模組**：`ecpay/logistics.rs`（地圖表單、建單、列印、狀態碼對應）；`routes/checkout.rs` 加 `POST /api/checkout/cvs-map`；新 `routes/ecpay_logistics.rs` 放 `map-reply`（寫 `cvs_stores::insert`，303 到 `/checkout?store=<token>`）與 `status`（驗 MAC）；`/api/ecpay/` 前綴已 CSRF 豁免。結帳頁「選擇門市」按鈕改成呼叫 `cvs-map` 並 `postToEcpay`（`web/src/lib/ecpay.ts` 已有）。
3. **出貨信**：後台出貨（`ship-cvs`、`ship-home`）成功後在同一交易 `jobs::enqueue(KIND_SEND_EMAIL, { "template": "order_shipped", "order_id" }, Some("email:order_shipped:{order_id}"))`；模板已在本計畫，宅配要先寫 `shipments.carrier`、`tracking_no`，超商要有 `cvs_store_name`、`cvs_store_address`。記得寫 `orders.shipped_at`（`auto_complete_shipped` 靠它）。
4. **後台重開發票 `POST /api/admin/orders/{id}/retry-invoice`**：`dedupe_key = "invoice:{order_id}"` 已被第一次的 job（done 或 failed）占用，`ON CONFLICT DO NOTHING` 會讓重試排不進去 → 重試時用 `Some(&format!("invoice:{order_id}:retry:{}", Utc::now().timestamp()))` 或 `None`，並先 `UPDATE invoices SET status = 'pending', error = NULL`。handler 對 `issued` 會略過，所以重複重試安全。
5. **後台取消與退款**：`orders::cancel_in_tx(tx, id, "admin")` 可直接用（已逐列歸還）；`mark-refunded`：已付未出貨時歸還庫存（同 `cancel_in_tx` 的逐列迴圈，抽成共用函式）、`payments` 不動、顯示「請至綠界作廢發票」。取消已付款訂單時綠界的錢要老闆手動退。
6. **儀表板「需退款」**：`orders.needs_refund = true` 的列（遲到付款、金額不符），加 `POST /api/admin/orders/{id}/clear-refund` 之類的「已處理」清除；`payments.raw` 有綠界原始 payload 可對帳。
7. **`OrderDetail.payment` 是最新一筆**；後台訂單頁若要列出所有付款嘗試，寫 `payments::list_for_order(db, order_id)`（`PAYMENT_COLUMNS` 已抽出）。
8. **Email 只記 log 的限制**：`Mailer::Log` 是開發用；計畫 5 部署文件要求正式環境設 `SMTP_HOST`／`SMTP_FROM`，且不要設 `MAIL_LOG_BODY`（信件內文只在本機開發用 `MAIL_LOG_BODY=1` 打開；codex 審查修正 `4e7d017` 之後不再由 `RUST_LOG` 把關）。
9. **Docker 映像**（計畫 5）：`reqwest` 用 `rustls-platform-verifier`，debian-slim 要 `apt-get install -y ca-certificates`，否則發票 API 會 TLS 失敗。
10. **產品決定（交給使用者）**：規格 §5 的「沒有繳費期限就 `created_at` + 3 天」對從未到過綠界頁的訂單也適用；若要縮短（例如 2 小時），改 `jobs/scheduled.rs::expire_unpaid_orders` 的 SQL 與 `tests/jobs_worker.rs` 對應測試即可（與規格不同之處 33）。
11. **發票重複 RelateNumber**：開立成功但沒存到時，重試會拿到綠界錯誤並在 5 次後標 failed；計畫 5 可加 `POST /B2CInvoice/GetIssue`（查 `RelateNumber`）讓重試先查再開。
12. **計畫 2 審查留下的小項**：Minor 1、2、5、6（Email ≤ 254、密碼 ≤ 128、改密碼登出其他裝置、重設後作廢同使用者其他 token）與計畫 1 的 Minor 1、4、8、9、11～14 仍未做；計畫 5 上線前挑。

<!-- PLAN3-END -->
