# dog_shop 計畫 4／5：綠界物流、後台訂單管理、儀表板 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 讓已付款的訂單真的能出貨、能收尾：買家結帳時用綠界電子地圖選超商門市；老闆在後台看訂單、建立綠界 C2C 物流單、列印託運單、填宅配單號、取消、標記已退款、重開發票、清除「需退款」；綠界物流狀態回呼更新出貨狀態（取件完成自動把訂單標完成、未取退回標紅）；後台首頁變成儀表板。

**Architecture:** 後端 `api/` 新增 `ecpay/logistics.rs`（電子地圖表單、建單欄位與回應解析、列印表單、狀態通知解析、貨態代碼對照；MD5 CheckMacValue 加在 `ecpay/mac.rs`）、`LogisticsGateway`（真打綠界／測試 Fake，掛在 `AppState.logistics`，與計畫 3 的 `InvoiceGateway` 同一個形狀）、`domain/shipments.rs`（完整 shipments 列、出貨寫入、狀態套用）、`domain/admin_orders.rs`（後台列表、明細、儀表板、退款）、`routes/ecpay_logistics.rs`（三條綠界回呼：`map-reply`、`status`、`store-update`）、`routes/admin_orders.rs`（後台訂單 API）。建立物流單是後台按鈕的同步 HTTP 呼叫（不走 job）：先認領（一句 UPDATE，把這次的 `MerchantTradeNo` 與請求存進 `shipments.raw`）→ 打綠界 → 先存綠界單號 → 一個交易內轉 `shipped`、寫 `shipped_at`、排出貨信。前端 `web/` 接上結帳頁的「選擇門市」（頂層導頁到綠界地圖、回來還原草稿）、新增 `/admin/orders`、`/admin/orders/[id]`、儀表板 `/admin`。

**Tech Stack:** 同計畫 3（Rust 1.98 / axum 0.8 / sqlx 0.9 / PostgreSQL 17 / reqwest 0.13；SvelteKit 2 / Svelte 5 runes / Tailwind 4 / vitest / Playwright 1.63）。新增 Rust crate：`md-5` 0.10（與 `sha2` 同一代 digest）。

**Spec:** `docs/superpowers/specs/2026-09-06-dog-shop-mvp-design.md`（本計畫負責 §3 `cvs_store_selections` 的寫入、§4 出貨／完成／退回／退款／後台取消、§6.1 `/checkout` 超商電子地圖與 `/admin`、`/admin/orders`、`/admin/orders/[id]`、§7 步驟 2、§8.1 MD5、§8.3 物流、§10 `checkout/cvs-map`、`ecpay/logistics/*`、`admin/orders/*`、`admin/dashboard`、§11 門市回傳的 token 規則、§12 `order_shipped` 的觸發點、§14、§15 對應測試）。計畫 3 的交接事項在 `docs/superpowers/plans/2026-09-08-dog-shop-plan-3-payments-jobs-email.md` 的「交給計畫 4 的事項」（7087–7101 行）；計畫 3 最終審查的補充交接在 `docs/superpowers/reviews/2026-09-08-plan-3-final-review.md` 的「交給計畫 4 的事項」（203–216 行）。綠界物流文件查證結果整理在本計畫的「環境事實」。

## Global Constraints

- 前端一律用最新版 SvelteKit 2 / Svelte 5 runes（`$state`、`$derived`、`$props`、`$effect`、`$bindable`），不用 legacy `export let` / store 寫法（規格 §1.1）。所有變更請求走瀏覽器端 `api()`（自動帶 `X-Requested-With: fetch`），不用 SvelteKit form actions。
- 後端 Rust 1.98 stable、axum 0.8、sqlx 0.9、tokio；edition 2024（規格 §1.1）。`cargo clippy --all-targets -- -D warnings` 與 `cargo fmt --all --check` 每個任務結束都要乾淨。
- 資料庫 PostgreSQL 17；主鍵 UUID v7（`Uuid::now_v7()`；session id 例外用 v4）；金額 `integer` 新台幣；時間 `timestamptz` 存 UTC（規格 §3）。
- 錯誤回應格式固定 `{ "error": { "code", "message", "details" } }`；本計畫用到的 code：`VALIDATION`（狀態不允許的操作，帶 `details.fields`）、`NOT_FOUND`、`UNAUTHORIZED`、`FORBIDDEN`、`ECPAY_ERROR`（502，綠界同步呼叫失敗）（規格 §10、§14）。`ApiError::EcpayError(String)` 的字串會直接當成給使用者看的 `message`：**只放固定文案，綠界的原始訊息記 log 並存進 `shipments.last_status_msg`**（計畫 3 審查交接 2）。
- **訂單狀態機（規格 §4 原文）：** `pending_payment ──付款成功回呼──► paid ──後台出貨──► shipped ──取貨完成 / 後台標記 / 14 天自動──► completed`；`pending_payment ──3 天未付 / 買家取消 / 後台取消──► cancelled（庫存歸還）`；`paid ──後台取消（先在綠界後台手動退款）──► refunded（後台顯示「請至綠界作廢發票」提醒）`。出貨的細部狀態在 `shipments.status`：`pending → created → in_transit → arrived → picked_up`（超商），或 `pending → shipped`（宅配）。超商狀態由綠界物流狀態回呼更新，代碼對應表寫在 `ecpay/logistics.rs`。超商未取件被退回：`shipments.status = returned`，訂單維持 `shipped`，後台列表標紅提醒老闆處理（在綠界後台退款後按「標記已退款」）。已付款但尚未出貨的訂單被標記 `refunded` 時，庫存加回去（同取消）。已出貨的不加回。遲到的付款…把 `orders.needs_refund` 設為 true。後台儀表板列出「需退款」，老闆在綠界後台退款後按「已處理」清除。
- **超商門市（規格 §7 步驟 2 原文）：** 選 7-11 / 全家 / 萊爾富，按「選擇門市」。前端先把表單狀態存到 sessionStorage，再呼叫 `POST /api/checkout/cvs-map` 拿綠界電子地圖表單欄位，用隱藏表單 POST 到綠界（頂層導頁，不用 iframe）。買家在綠界地圖選完，綠界用買家的瀏覽器 POST 門市資料到我們的 `POST /api/ecpay/logistics/map-reply`。我們存進 `cvs_store_selections`，回 303 轉到 `/checkout?store=<token>`。頁面還原表單、用 token 取回門市顯示。
- **CheckMacValue（規格 §8.1 原文）：** 參數依 key 排序（不分大小寫）→ 串成 `HashKey=...&k1=v1&...&HashIV=...` → 依綠界規定做 URL encode（.NET 風格，再套用綠界的字元替換表）→ 轉小寫 → 雜湊 → 轉大寫。全方位金流用 SHA256（`EncryptType=1`）；物流 API 用 MD5。實作時以綠界文件上的範例做單元測試。
- **物流（規格 §8.3 原文）：** 電子地圖：POST 表單到 `/Express/map`，欄位 `MerchantID`、`MerchantTradeNo`、`LogisticsType=CVS`、`LogisticsSubType`、`IsCollection=N`、`ServerReplyURL`、`ExtraData=<token>`、`Device`（手機 1）。不需 `CheckMacValue`。回傳到 `ServerReplyURL` 的欄位：`CVSStoreID`、`CVSStoreName`、`CVSAddress`、`CVSTelephone`、`CVSOutSide`、`ExtraData`。測試環境不會顯示真地圖，會直接回固定門市。建立物流單（後台按「建立物流單」時同步呼叫，不用 job，錯誤直接顯示）：POST `/Express/Create`。欄位含 `MerchantTradeNo`（新號：`order_no` + `L` + 流水）、`MerchantTradeDate`、`LogisticsType=CVS`、`LogisticsSubType`、`GoodsAmount`（1～20000）、`GoodsName`、`SenderName`、`SenderCellPhone`、`ReceiverName`、`ReceiverCellPhone`、`ReceiverEmail`、`ReceiverStoreID`、`ReturnStoreID`（設定裡的退貨門市）、`ServerReplyURL=/api/ecpay/logistics/status`、`IsCollection=N`、`CheckMacValue`。成功回 `1|MerchantID=...&AllPayLogisticsID=...&CVSPaymentNo=...&CVSValidationNo=...`，失敗回 `0|訊息`。存進 `shipments`，訂單轉 `shipped`，排 `send_email:order_shipped`。列印託運單：後台按「列印」，api 回一組帶 `CheckMacValue` 的表單欄位，前端在新分頁 POST 到對應的 `Express/Print{UniMart,FAMI,HILIFE}C2COrderInfo`。狀態回呼：綠界 POST 到 `/api/ecpay/logistics/status`，驗 `CheckMacValue`，依 `RtnCode` 對應到 `shipments.status`，`picked_up` 時訂單轉 `completed`。回 `1|OK`。收件人姓名、手機格式限制（姓名 2～5 個中文字、手機 09 開頭 10 碼）在結帳表單就先驗證。
- **後台頁面（規格 §6.1 原文）：** `/admin`：今日訂單、待出貨、發票開立失敗、需退款（遲到付款）、超商退回。`/admin/orders`、`/admin/orders/[id]`：訂單列表（依狀態篩選）、建物流單、列印託運單、填宅配單號、取消、標記已退款、重開發票。
- **後台 API（規格 §10 原文）：** `GET /api/admin/orders`、`GET /api/admin/orders/{id}`；`POST /api/admin/orders/{id}/ship-cvs`、`POST /api/admin/orders/{id}/print-label`、`POST /api/admin/orders/{id}/ship-home`、`POST /api/admin/orders/{id}/cancel`、`POST /api/admin/orders/{id}/mark-refunded`、`POST /api/admin/orders/{id}/complete`、`POST /api/admin/orders/{id}/retry-invoice`；`GET /api/admin/dashboard`。公開：`POST /api/checkout/cvs-map`、`GET /api/checkout/cvs-store/{token}`、`POST /api/ecpay/logistics/map-reply`、`POST /api/ecpay/logistics/status`（綠界用，回純文字）。
- **回呼安全（規格 §11 原文）：** 綠界回呼路徑例外：付款與物流狀態回呼靠 `CheckMacValue` 簽章驗證；門市回傳（`map-reply`）沒有簽章，靠 `ExtraData` 裡的隨機 token 必須存在且未過期，且它只影響該次結帳的門市選擇。物流回呼與付款回呼一樣掛 `DefaultBodyLimit::max(64 * 1024)` 與欄位數上限 100（計畫 3 審查交接 1）。
- **綠界回呼錯誤處理（規格 §14 原文）：** 簽章錯誤回 400；已處理過的重複通知回 `1|OK` 不重做；暫時性失敗（DB 連不上）回 500 讓綠界重送。任何寫入綠界的呼叫都先把請求存 DB 再送，回應也存，方便對帳與重試。
- **測試（規格 §15）：** Rust 單元 `mac.rs` MD5 對照綠界文件範例、狀態轉移合法性；整合 `後台建物流單狀態轉移`、`admin 權限`（未登入 401、非 admin 403）；Playwright 攔截送往綠界的表單。
- **鎖序：** 付款那邊維持計畫 3 的 `payments → orders → product_variants`（`expire_one`、`apply_return`）；本計畫新增 `orders → shipments`（狀態回呼、出貨收尾）；標記退款走 `payments → orders → product_variants`（先 UPDATE payments、再 `FOR UPDATE` orders、最後歸還庫存）。
- `merchant_trade_no`（付款）= `order_no` + 兩碼流水；物流的 `MerchantTradeNo` = `order_no` + `L` + 兩碼流水，每次嘗試換新號（規格 §3、§8.3）。
- 樣式 Tailwind CSS 4，自寫元件；語言繁體中文（規格 §6.2）。
- 所有 commit 只在本機分支 `worktree-mvp-design`。**不要 `git push`、不要開 PR**（使用者的 CLAUDE.md 第 5 條）。
- 秘密（HashKey、HashIV、SMTP 密碼、DB 連線字串、重設 token、guest_token）不進 log、不進 git（規格 §11）。**`mac::raw_string` 的輸出含 HashKey／HashIV，永遠不要印進 log**；MAC 不符時只記收到的與重算的 MAC。綠界原始 payload 存進各表的 `raw` 欄位。stage 憑證是公開測試資料，只能用於 stage；正式憑證只放伺服器的 `.env`。後台明細不回 `guest_token`。

---

## 五份計畫的分工（規格涵蓋表，本計畫更新版）

計畫 1、2、3 已完成（`docs/superpowers/reviews/2026-09-06-plan-1-final-review.md`、`2026-09-07-plan-2-final-review.md`、`2026-09-08-plan-3-final-review.md`）。粗體是本計畫。

| 規格章節／需求 | 計畫 |
|---|---|
| §3 `cvs_store_selections` 的寫入（map-reply）；新增 `cvs_map_requests` | **4** |
| §4 出貨、完成、退回、退款、後台取消、需退款清除 | **4** |
| §6.1 `/checkout` 超商電子地圖；`/admin` 儀表板；`/admin/orders`、`/admin/orders/[id]` | **4** |
| §7 步驟 2 超商門市（`cvs-map`、`map-reply`） | **4** |
| §8.1 MD5 版 CheckMacValue、§8.3 物流 | **4** |
| §8.5 物流 C2C 測試憑證 | **4** |
| §10 API：`checkout/cvs-map`、`ecpay/logistics/*`、`admin/orders/*`、`admin/dashboard` | **4** |
| §10 API：`admin/import/*` | 5 |
| §12 `order_shipped` 的觸發點 | **4** |
| §15 MD5 單元測試、建物流單狀態轉移、admin 權限、Playwright 超商路徑 | **4** |
| §13 匯入、§16 部署 | 5 |

## 與規格不同之處（已決定，執行時照這裡做）

前 33 條是計畫 1、2、3 的決定，原文見計畫 3 的同名章節；34 起是本計畫的決定。

34. **多一張 `cvs_map_requests` 表（token、sub_type、expires_at）。** 按「選擇門市」時先登記一筆（1 小時有效），綠界回傳時「token 必須存在且未過期」就是查這張表，核對成功後刪掉它（單次使用），再寫 `cvs_store_selections`（同一個 token，1 小時有效）；買家要換門市就再按一次「選擇門市」（新 token）。原因：`cvs_store_selections` 的門市欄位都是 `NOT NULL`，登記階段還沒有門市資料。token 20 碼英數（綠界 `ExtraData` 上限 20 字元；62^20 ≈ 2^119 猜不到），同時當地圖表單的 `MerchantTradeNo`。
35. **第三條回呼 `POST /api/ecpay/logistics/store-update`。** 7-ELEVEN C2C 建單必填 `LogisticsC2CReplyURL`（門市關轉店等「更新門市通知」的 Server 端網址），它的欄位形狀與狀態通知不同（沒有 `MerchantTradeNo`、`RtnCode`，改用 `AllPayLogisticsID`、`StoreType`、`Status`、`StoreID`），所以獨立一條：驗 MD5 簽章、把通知存進 `shipments.raw.store_updates`、`last_status_msg` 寫一句中文（例如「取件門市異動：門市關轉店（991182）」），不改 `status`，回 `1|OK`。三種超商都送這個網址（其他超商不會打）。
36. **`GoodsAmount` 用商品小計 `subtotal`（不含運費）**，夾在 1～20000；結帳已限制超商小計 ≤ 20,000，所以不會超過（總計含運費可能超過）。`GoodsName` = 第一個品項名稱（多品項加「等 N 件」），去掉綠界禁用符號 `^ ' \` ! @ # % & * + \ " < > | _ [ ]`，依寬度截到 50（中文算 2），空的話用「商品」。`SenderName` 去掉空白與禁用符號、寬度 ≤ 10；`ReceiverName` 已在結帳限制 2～5 個中文字。`ReceiverEmail` 超過 50 字元就不帶。
37. **`map-reply` 失敗不回 400 純文字，改 303 回 `/checkout?store_error=<原因>`**（`expired`：token 不存在或過期；`invalid`：欄位不全或超商種類與登記的不符；`server`：資料庫錯誤），結帳頁用 toast 顯示中文並讓買家重選。原因：這條是買家的瀏覽器在 POST，400 純文字會讓買家卡在一頁錯誤。
38. **買家取消（`orders::cancel`）也把 `pending` 的付款嘗試標 `expired`**：抽出 `orders::cancel_with_payments_in_tx`（先 UPDATE payments、再 `cancel_in_tx`），買家取消與後台取消共用；`expire_one` 維持原樣（它在兩步之間要重查到期條件）。
39. **`ReturnStoreID` 只在設定的退貨門市 `sub_type` 等於這筆訂單的超商種類、且 `store_id` 非空時才帶**（綠界文件：僅 C2C 適用、7-11 C2C 才生效；沒設就退回原寄件門市）。
40. **建單回應嚴格驗 MD5 CheckMacValue**（綠界文件：合作特店必須檢查）。不符就當失敗（`ECPAY_ERROR`），`last_status_msg` 寫「回應簽章不符」加回應前 200 字，讓老闆能到綠界後台核對是否已建單；stage 走查要確認真實回應能通過驗證（驗收清單第 6 點）。
41. **建單的認領與冪等重試。** 打綠界前先用一句 UPDATE 認領：`status = 'pending' AND ecpay_logistics_id IS NULL AND (last_status_code IS DISTINCT FROM 'creating' OR updated_at < now() - interval '2 分鐘')`，同時寫入這次的 `MerchantTradeNo` 與請求欄位（`raw.create_request`；規格 §14 先存再送）；認領不到就回 `VALIDATION`「物流單建立中或已建立」。綠界成功後**先單獨一句 UPDATE 存綠界單號**（`ecpay_logistics_id`、`cvs_payment_no`、`cvs_validation_no`、`raw.create_response`），再開交易做收尾（`shipments.status = created`、`orders.status = shipped`、`shipped_at`、排出貨信）。收尾失敗時下次按鈕看到「有綠界單號但 shipments 還是 pending」就只補做收尾、不再打綠界（避免重複建單、重複扣運費）。綠界拒絕（`0|訊息`）：`last_status_code = 'create_failed'`、`last_status_msg = 綠界訊息（≤ 200 字）`，訂單維持 `paid`，下次重試流水號 +1。連線失敗：`last_status_code = 'create_error'`。
42. **貨態代碼對照與不倒退規則。** `created`：300、310、2001、2024；`in_transit`：2068、3032（賣家已交寄）、2030、3024（物流中心）、3001、3006（轉運／配送中）；`arrived`：2073、2063、3018、2098（重新配達）、3029（轉換店）；`picked_up`：2067、3022；`returned`：2074、2076、2077、2078～2093、2069、2070、2072、2075、2099、3019、3020、3021、3023、3025、7011。其他代碼只記 `last_status_code`／`last_status_msg`／`raw.last_notification`，不改 `status`。rank：`pending` 0 < `created` 1 < `in_transit` 2 < `arrived` = `returned` 3 < `picked_up` = `shipped` 4；新 rank ≥ 目前 rank 且不同才套用（退回後重新配達可回 `arrived`；`picked_up` 是終態）。`picked_up` 且訂單是 `shipped` → 訂單 `completed`。狀態通知找不到 `MerchantTradeNo` 時再用 `AllPayLogisticsID` 找一次，都沒有回 `0|Unknown MerchantTradeNo`（200）。
43. **後台取消只允許 `pending_payment`**（→ `cancelled`、`cancel_reason = admin`、歸還庫存、pending 付款標 expired）；已付款的一律用「標記已退款」：`paid` 或 `shipped` → `refunded`，`paid` 時歸還庫存、`shipped` 不歸還，pending 付款標 expired，`needs_refund = false`，並寫 `cancelled_at = now()`、`cancel_reason = 'refunded'`（記錄結束時間）。錢由老闆在綠界後台退；發票已開立時後台顯示「請至綠界作廢發票」。
44. **重開發票**：只有 `invoices.status = failed` 能重試；`UPDATE invoices SET status = 'pending', error = NULL` 後排 `issue_invoice`，`dedupe_key = invoice:{order_id}:retry:{unix 秒}`（第一次的 `invoice:{order_id}` 已被用掉）；`invoices::record_failure` 加 `AND status <> 'issued'` 守衛。計畫 3 審查交接 6 提到的「已開立就略過的分支也要排通知信」在 codex 修正 `9ed2522`（標記與排信同一交易）之後已不需要。
45. **後台列表的搜尋 `q` 比對 `order_no`、`email`、`recipient_name`（ILIKE 部分相符）；`flag` 篩選 `needs_refund`、`cvs_returned`（`shipped` 且 `shipments.status = returned`）、`invoice_failed`。** 儀表板的「今日」用台北日期（`date_trunc('day', now() AT TIME ZONE 'Asia/Taipei') AT TIME ZONE 'Asia/Taipei'`）。
46. **列印託運單在新分頁**：`postToEcpay(form, '_blank')`（綠界文件：請勿用 iframe）。`print-label` 依規格是 POST，回 `{ action, fields }`。
47. **`complete` 允許 `shipped` 的訂單不論 `shipments.status`**（規格 §4「後台標記」）。
48. **後台明細回 `user_id`（可為 null）與 `needs_refund`，不回 `guest_token`**；`payments` 回全部嘗試（新到舊，含 `created_at`），`shipment` 回完整欄位（`raw` 除外），`invoice` 多回 `error` 與 `updated_at`。

## 環境事實（每個任務開始前都要知道）

- 這台機器：macOS、zsh、Node 24、pnpm 10、Docker（OrbStack）。Rust 1.98.1（rustup，在 `~/.cargo`）。**這個 sandbox 的新 shell 找不到 `cargo`，而且拒絕 `source`、heredoc、`$(...)`、`for` 迴圈**：每個含 cargo 的指令一律寫成 `export PATH="$HOME/.cargo/bin:$PATH" && cargo <子指令> --manifest-path api/Cargo.toml`（不要 `cd api`）。`#[sqlx::test(migrations = "./migrations")]` 與 askama 的 `templates/` 都相對於 `CARGO_MANIFEST_DIR`（= `api/`），用 `--manifest-path` 沒問題；`cargo run` 用 `dotenvy` 讀**工作目錄**的 `.env`（repo 根目錄，已存在）。
- 工作目錄是 git worktree：`/Users/wilson08/IdeaProjects/dog_shop/.claude/worktrees/mvp-design`，分支 `worktree-mvp-design`。所有指令都從這裡跑。**不要 `cd web`**：前端指令一律寫 `pnpm -C web <script>`。
- commit 步驟一律寫成純指令：`git add <檔案...>` 然後 `git commit -m "..."`。建檔用 Write 工具、改檔用 Edit 工具。`rm -rf` 會被攔（用 `trash`）。停伺服器用 `lsof -ti :8080`／`lsof -ti :5173` 取 pid 再 `kill`（`pkill -f` 對 vite 無效）。zsh 裡 `echo =====` 會被當成 `=cmd` 展開，分隔線用 `echo '---'`。
- **開發資料庫在 `localhost:5435`**（Docker 容器 `dog_shop-db-1`，PostgreSQL 17）。`#[sqlx::test]` 從**行程環境變數** `DATABASE_URL` 讀連線（不讀 `.env`）：測試指令一律 `DATABASE_URL=postgres://dog_shop:dog_shop@localhost:5435/dog_shop cargo test --manifest-path api/Cargo.toml`。它會為每個測試建獨立臨時資料庫並跑 `migrations/`。CI 用 5432（容器內），不要改。
- 根目錄 `.env`（不進 git）含 `ECPAY_ENV=stage` 與 AIO 測試憑證；`ECPAY_LOGISTICS_*` 三個是空的（Task 1 之後 stage 會退回公開測試憑證）；`SMTP_HOST` 空（Email 只記 log）；`PUBLIC_BASE_URL=http://localhost:5173`。開發用管理員：`admin@example.com` / `admin12345`。
- 計畫 3 結束時的狀態（HEAD `ccb4c88`，程式碼到 `ecd2871`）：`cargo test` 179 個全綠（單元 + 21 個整合測試檔）、`cargo fmt --all --check` 與 `cargo clippy --all-targets -- -D warnings` 乾淨；`pnpm -C web test` 27 個、`pnpm -C web check` 0 錯誤 0 警告、`pnpm -C web build` 成功；`pnpm -C web test:e2e` 1 passed（要 api 與 web dev 都在跑）。每個任務結束時這些都要維持（**svelte-check 的警告也算錯**）。
- **新相依套件的第一次編譯很慢**：`cargo test`、`cargo clippy`、`cargo run` 一律 `run_in_background: true`、timeout 拉到 600000，等通知，不要輪詢。
- sqlx 0.9：`query` / `query_as` / `query_scalar` 只接受 `&'static str`，動態組的 SQL 要包 `sqlx::AssertSqlSafe(sql)` 且只能包沒有使用者輸入的字串。交易：`let mut tx = db.begin().await?;`，在 `fn(tx: &mut Transaction<'_, Postgres>)` 裡執行用 `&mut **tx`，在擁有 `tx` 的函式裡用 `&mut *tx`。tuple 也能當 `query_as` 的目標（`(String, i32)`）。jsonb 合併用 `COALESCE(raw, '{}'::jsonb) || $n::jsonb`（同 key 覆蓋）。
- axum 0.8 路徑參數寫法 `/api/orders/{id}`；handler 用 `AppJson` / `AppQuery` / `AppPath`（`api/src/extract.rs`）。綠界回呼不是 JSON，用 `body: String` 擷取器（要放在參數最後）自己解 form-urlencoded。`/api/ecpay/` 前綴已在 `api/src/auth/csrf.rs:11` 的 `EXEMPT_PREFIXES` 內。後台權限靠 handler 參數 `_admin: AdminUser`（`api/src/auth/extract.rs`；未登入 401、非 admin 403），沒有 middleware。
- 現有可直接用的東西：`shipments` 表（`api/migrations/0002_members_orders.sql:100-126`）已有 `ecpay_logistics_id`、`ecpay_merchant_trade_no`、`cvs_payment_no`、`cvs_validation_no`、`carrier`、`tracking_no`、`last_status_code`、`last_status_msg`、`raw`，狀態 CHECK `('pending','created','in_transit','arrived','picked_up','returned','shipped')`；`cvs_store_selections` 表與 `domain/cvs_stores.rs`（`CvsStore`、`get_valid`、`insert`）；`GET /api/checkout/cvs-store/{token}`（`routes/checkout.rs`）；`settings.sender`（`SenderSettings { name, phone }`）與 `settings.return_store`（`ReturnStore { sub_type, store_id, store_name }`）已在後台設定頁；`mail::templates::OrderShippedMail` 與 `jobs/handlers.rs` 的 `"order_shipped"` 分支已完成，只差排 job；`orders::cancel_in_tx(tx, id, reason)`（只允許 pending_payment，逐列歸還庫存）；`orders::get_detail`（`OrderDetail { order: OrderRow, items, shipment: Option<ShipmentRow>, payment: 最新一筆, invoice }`）；`products::Page<T>`、`products::clamp_paging(page, per_page, default, max)`、`products::clean(&Option<String>)`；`ecpay::aio::CheckoutForm { action, fields: BTreeMap }`、`CallbackError { BadMac, Missing(&'static str) }`；`ecpay::time::format_datetime`／`parse_datetime`；`auth::tokens::generate_token()`（64 hex，太長不能當 ExtraData）；前端 `postToEcpay(form)`（`web/src/lib/ecpay.ts`）、`CVS_LABELS`／`ORDER_STATUS_LABELS`（`web/src/lib/labels.ts`）、結帳頁 `?store=<token>` 的還原（`web/src/routes/checkout/+page.server.ts`）、草稿存 sessionStorage 的 `$effect`。
- 測試共用工具 `api/tests/common/mod.rs`：`state(pool)`／`app(pool)`／`app_with_state(pool)`、`req(method, uri, cookie, body)`（帶 Origin 與 X-Requested-With）、`send(app, request) -> (StatusCode, Value, HeaderMap)`（body 不是 JSON 時包成 `Value::String`）、`admin_cookie(app, pool)`、`customer_cookie(app, pool)`、`active_product(pool, name, price, stock) -> (variant_id, slug)`、`cvs_store_token(pool)`（UNIMARTC2C、門市 `131386`「測試門市」）、`sent_emails(state)`、`fake_invoices(state)`。`api/tests/ecpay_payment.rs` 的 `ecpay_post`／`ecpay_post_raw`（form-urlencoded、沒有 Origin）是物流回呼測試的範本。worker 在測試裡用 `worker::run_once(&state)` 跑一輪。
- **綠界物流文件查證（2026-09-08，developers.ecpay.com.tw 7398／7424／8795／8809／7420、物流整合 API 技術文件 PDF V2.3.19、官方貨態代碼 Excel）：**
  - 物流 C2C 測試特店：MerchantID `2000933`、HashKey `XBERn1YOvpM9nfZc`、HashIV `h1ONHk4P4yqbl5LK`；廠商後台 `https://vendor-stage.ecpay.com.tw`（`LogisticsC2CTest` / `test1234`）。B2C（2000132）不能混用。stage 網址 `https://logistics-stage.ecpay.com.tw`、正式 `https://logistics.ecpay.com.tw`。測試門市代號：7-ELEVEN `991182`、全家 `006598`、萊爾富 `2001`。測試環境電子地圖是固定門市；**測試環境不提供模擬物流狀態通知**（回呼只能靠整合測試）。
  - MD5 CheckMacValue 文件範例（已用 python 重算驗證）：HashKey `XBERn1YOvpM9nfZc`、HashIV `h1ONHk4P4yqbl5LK`，參數 `GoodsAmount=1000`、`IsCollection=N`、`LogisticsSubType=FAMIC2C`、`LogisticsType=CVS`、`MerchantID=2000933`、`MerchantTradeDate=2013/03/12 15:30:23`、`MerchantTradeNo=A20130312153023`、`ReceiverName=收件者姓名`、`ReceiverStoreID=001779`、`SenderName=寄件者姓名`、`ServerReplyURL=https://www.ecpay.com.tw/ServerReplyURL` → `692FD6E2CDB539CCDB7206C76DC239AD`。編碼規則與 AIO 完全相同，只有雜湊換成 MD5。
  - 電子地圖 `/Express/map`：`MerchantID`、`MerchantTradeNo`（String(20) 英數）、`LogisticsType=CVS`、`LogisticsSubType`、`IsCollection=N`、`ServerReplyURL`、`ExtraData`（String(20)，原值回傳）、`Device`（0 PC／1 手機）；不需 CheckMacValue。回傳 `MerchantID`、`MerchantTradeNo`、`LogisticsSubType`、`CVSStoreID`（String(9)）、`CVSStoreName`（String(10)）、`CVSAddress`（String(60)）、`CVSTelephone`（7-11 不回）、`CVSOutSide`（0 本島／1 離島）、`ExtraData`；沒有簽章。
  - 建單 `/Express/Create`：必填 `MerchantID`、`MerchantTradeDate`（`yyyy/MM/dd HH:mm:ss` 台北）、`LogisticsType=CVS`、`LogisticsSubType`、`GoodsAmount`（1～20000，超過回 10500040）、`SenderName`（≤ 10 字元、中文 ≤ 5、無符號無空白）、`ReceiverName`（4～10 字元、中文 2～5）、`ReceiverCellPhone`（09 開頭 10 碼）、`ServerReplyURL`、`ReceiverStoreID`（String(6)）、`CheckMacValue`；`GoodsName`（UNIMARTC2C／HILIFEC2C 必填，≤ 50 寬度）、`SenderCellPhone`（UNIMARTC2C／HILIFEC2C 必填）、`LogisticsC2CReplyURL`（UNIMARTC2C 必填）；選填 `MerchantTradeNo`（String(20) 唯一）、`ReceiverEmail`（String(50)）、`ReturnStoreID`（String(6)）、`IsCollection`（預設 N）、`ClientReplyURL`（幕後建單不填）。成功 `1|MerchantID=…&MerchantTradeNo=…&RtnCode=…&BookingNote=&RtnMsg=…&AllPayLogisticsID=…&LogisticsType=…&LogisticsSubType=…&GoodsAmount=…&UpdateStatusDate=…&ReceiverName=…&ReceiverPhone=…&ReceiverCellPhone=…&ReceiverEmail=…&ReceiverAddress=…&CVSPaymentNo=…&CVSValidationNo=…&CheckMacValue=…`（`RtnCode` 建單成功通常 300；`CVSValidationNo` 只有 7-11 才回）；失敗 `0|ErrorMessage`。常見錯誤碼：10500035 寄件人姓名規則、10500036 收件人姓名規則、10500038 商品名稱規則、10500040 商品金額、10500041 收件人手機、10500047／48 手機必填、10500049 綠界帳戶餘額不足、10500052／53 Email。
  - 列印：`/Express/PrintUniMartC2COrderInfo`（`MerchantID`、`AllPayLogisticsID`、`CVSPaymentNo`、`CVSValidationNo`、`CheckMacValue`）、`/Express/PrintFAMIC2COrderInfo` 與 `/Express/PrintHILIFEC2COrderInfo`（`MerchantID`、`AllPayLogisticsID`、`CVSPaymentNo`、`CheckMacValue`）；表單 POST、新分頁、禁止 iframe。
  - 狀態通知（綠界 Server POST 到 `ServerReplyURL`，form-urlencoded）：`MerchantID`、`MerchantTradeNo`、`RtnCode`（Int）、`RtnMsg`（≤ 200）、`AllPayLogisticsID`、`LogisticsType`、`LogisticsSubType`、`GoodsAmount`、`UpdateStatusDate`、`ReceiverName`、`ReceiverPhone`、`ReceiverCellPhone`、`ReceiverEmail`、`ReceiverAddress`、`CVSPaymentNo`、`CVSValidationNo`、`BookingNote`、`CheckMacValue`。回純文字 `1|OK`，不是就重送 3 次、延到隔天、維持 3 天。狀態非即時（物流中心批次）。
  - 更新門市通知（POST 到 `LogisticsC2CReplyURL`）：`MerchantID`、`AllPayLogisticsID`、`GoodsName`、`GoodsAmount`、`StoreType`（01 取件門市／02 退件門市）、`Status`（01 門市關轉店／02 門市舊店號更新／03 退件門市為原寄件門市但無寄件門市資料／04 取(退)件門市臨時關轉店）、`StoreID`、`CheckMacValue`。回 `1|OK`。
  - 貨態代碼（官方 Excel；全家與萊爾富 B2C/C2C 共用）：常用 — 已送至物流中心 7-11 `2030`、全家 `3024`、萊爾富 `2030`／`3024`；已送達門市 7-11 `2073`、全家 `3018`、萊爾富 `2063`／`3018`；消費者成功取件 7-11 `2067`、全家 `3022`、萊爾富 `2067`／`3022`；七天未取 7-11 `2074`、全家 `3020`、萊爾富 `2074`／`3020`；重新配達取件門市 `2098`、重新配達寄件門市 `2099`。7-11 C2C 其餘：`2068` 交貨便收件（賣家交寄）、`2069` 退貨便收件、`2076`／`2077` 退回大智通、`2078`～`2093` 買家未取貨退回物流中心（各種原因）、`2101`～`2105` 門市關轉店／變更、`7019`／`7020` 異常。全家：`300`、`310`、`3019` 退件到店、`3021`、`3023` 賣家已取買家未取貨、`3025`、`3029` 商品已轉換店、`3032` 賣家已到門市寄件、`4001`／`4002` 退貨、`7006`～`7032` 異常。萊爾富另有 `2001` 檔案傳送成功、`2024` 超商接受資料中、`2070` 退回原寄件門市且已取件、`2072` 配達賣家取退貨門市、`2075`、`3001` 轉運中、`3006` 配送中。訂單有效日（C2C）：全家 6 天、7-11／萊爾富 7 天。
  - C2C 店到店未取件退回原寄件門市時要出示身分證件領取，`SenderName` 不要填公司名稱。

## 檔案結構

```
api/Cargo.toml                                        Modify（Task 1：md-5）
.env.example                                          Modify（Task 1：物流測試憑證）
api/src/config.rs                                     Modify（Task 1：logistics 憑證、STAGE_LOGISTICS、logistics_base_url、for_tests）
api/src/ecpay/mac.rs                                  Modify（Task 1：check_mac_value_md5、verify_md5、verify_with）
api/src/ecpay/mod.rs                                  Modify（Task 2：pub mod logistics）
api/src/ecpay/logistics.rs                            Create（Task 2：地圖表單、建單欄位／回應、列印、通知解析、代碼對照、閘道）
api/src/domain/shipments.rs                           Create（Task 2：狀態常數與 rank；Task 4：Shipment 列、apply_status、apply_store_update；Task 6：出貨寫入）
api/src/domain/mod.rs                                 Modify（Task 2 shipments、Task 5 admin_orders）
api/src/state.rs                                      Modify（Task 2：logistics 閘道）
api/src/main.rs                                       Modify（Task 2：建立閘道）
api/tests/common/mod.rs                               Modify（Task 2：Fake 閘道與 fake_logistics；Task 5：訂單 fixture）
api/migrations/0004_cvs_map_requests.sql              Create（Task 3）
api/src/auth/tokens.rs                                Modify（Task 3：generate_short_token）
api/src/domain/cvs_stores.rs                          Modify（Task 3：map request、upsert、TTL 常數）
api/src/routes/ecpay_callback.rs                      Create（Task 3：回呼共用的 parse_form／text／callback_error／server_error）
api/src/routes/ecpay_payment.rs                       Modify（Task 3：改用 ecpay_callback 的共用函式）
api/src/routes/checkout.rs                            Modify（Task 3：POST /api/checkout/cvs-map）
api/src/routes/ecpay_logistics.rs                     Create（Task 3：map-reply；Task 4：status、store-update）
api/src/routes/mod.rs                                 Modify（Task 3、5）
api/src/app.rs                                        Modify（Task 3、5：merge 新 router）
api/src/jobs/scheduled.rs                             Modify（Task 3：purge cvs_map_requests）
api/tests/ecpay_logistics.rs                          Create（Task 3：cvs-map、map-reply；Task 4：status、store-update）
web/src/lib/checkout.ts (+ checkout.test.ts)          Modify（Task 3：isMobileDevice、STORE_ERROR_MESSAGES）
web/src/routes/checkout/+page.server.ts               Modify（Task 3：store_error）
web/src/routes/checkout/+page.svelte                  Modify（Task 3：選擇門市按鈕、換超商清門市、錯誤 toast）
web/e2e/checkout.spec.ts                              Modify（Task 3：超商取貨路徑）
api/src/domain/payments.rs                            Modify（Task 5：Payment 加 created_at 與 Serialize、list_for_order）
api/src/domain/admin_orders.rs                        Create（Task 5：列表、明細；Task 7：mark_refunded、clear_refund、complete；Task 8：dashboard）
api/src/routes/admin_orders.rs                        Create（Task 5：GET 列表、明細；Task 6：ship-cvs、ship-home、print-label、complete；Task 7：cancel、mark-refunded、retry-invoice、clear-refund；Task 8：dashboard）
api/tests/admin_orders.rs                             Create（Task 5～8）
api/src/domain/orders.rs                              Modify（Task 7：restore_stock_in_tx、cancel_with_payments_in_tx、cancel 改用）
api/src/domain/invoices.rs                            Modify（Task 7：record_failure 守衛、reset_for_retry_in_tx）
web/src/lib/types.ts                                  Modify（Task 9：後台型別）
web/src/lib/labels.ts                                 Modify（Task 9：SHIPMENT_STATUS_LABELS、FLAG_LABELS）
web/src/lib/ecpay.ts                                  Modify（Task 9：target 參數）
web/src/lib/adminOrders.ts (+ adminOrders.test.ts)    Create（Task 9：可用動作、警示文案）
web/src/routes/admin/+layout.svelte                   Modify（Task 9：「訂單」連結）
web/src/routes/admin/+page.server.ts                  Create（Task 9：儀表板 load）
web/src/routes/admin/+page.svelte                     Modify（Task 9：儀表板）
web/src/routes/admin/orders/+page.server.ts           Create（Task 9）
web/src/routes/admin/orders/+page.svelte              Create（Task 9）
web/src/routes/admin/orders/[id]/+page.server.ts      Create（Task 9）
web/src/routes/admin/orders/[id]/+page.svelte         Create（Task 9）
docs/dev/ecpay-stage.md                               Modify（Task 10：物流走查、舊訂單 backfill）
```

---

### Task 1: 物流憑證、網址與 MD5 CheckMacValue

**Files:**
- Modify: `api/Cargo.toml`（`[dependencies]` 加 `md-5 = "0.10"`，放在 `lettre` 之後、`percent-encoding` 之前，維持字母順序）
- Modify: `api/src/config.rs:64-89`（`EcpayConfig` 加 `logistics`、`STAGE_LOGISTICS`、`logistics_base_url`）、`:173-177`（`from_env`）、`:205-227`（`for_tests`）、`:244-`（tests）
- Modify: `api/src/ecpay/mac.rs`（MD5 版本、`verify_with`）
- Modify: `.env.example:26-29`
- Test: `api/src/ecpay/mac.rs`（單元）、`api/src/config.rs`（單元）

**Interfaces:**
- Consumes: `config::credentials(prefix, env, stage)`（`api/src/config.rs:131`）、`mac::raw_string`（`api/src/ecpay/mac.rs:30`）。
- Produces: `config::STAGE_LOGISTICS: (&str, &str, &str)`；`EcpayConfig.logistics: EcpayCredentials`；`EcpayConfig::logistics_base_url(&self) -> &'static str`（stage `https://logistics-stage.ecpay.com.tw`／prod `https://logistics.ecpay.com.tw`，不含結尾斜線）；`mac::check_mac_value_md5(hash_key: &str, hash_iv: &str, params: &[(String, String)]) -> String`（大寫 hex，32 字）；`mac::verify_md5(hash_key, hash_iv, params: &[(String, String)]) -> bool`（取出 `CheckMacValue` 用其餘欄位重算，值不分大小寫）。

- [ ] **Step 1: 寫 mac.rs 的失敗測試**

在 `api/src/ecpay/mac.rs` 的 `mod tests` 最後（`verify_accepts_correct_and_rejects_tampered` 之後）加：

```rust
    /// 物流 C2C 測試特店（config::STAGE_LOGISTICS）
    const LKEY: &str = "XBERn1YOvpM9nfZc";
    const LIV: &str = "h1ONHk4P4yqbl5LK";

    /// developers.ecpay.com.tw 物流「檢查碼機制」（/7424/）的範例參數
    fn logistics_doc_params() -> Vec<(String, String)> {
        [
            ("GoodsAmount", "1000"),
            ("IsCollection", "N"),
            ("LogisticsSubType", "FAMIC2C"),
            ("LogisticsType", "CVS"),
            ("MerchantID", "2000933"),
            ("MerchantTradeDate", "2013/03/12 15:30:23"),
            ("MerchantTradeNo", "A20130312153023"),
            ("ReceiverName", "收件者姓名"),
            ("ReceiverStoreID", "001779"),
            ("SenderName", "寄件者姓名"),
            ("ServerReplyURL", "https://www.ecpay.com.tw/ServerReplyURL"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
    }

    #[test]
    fn md5_check_mac_value_matches_logistics_doc() {
        assert_eq!(
            check_mac_value_md5(LKEY, LIV, &logistics_doc_params()),
            "692FD6E2CDB539CCDB7206C76DC239AD"
        );
    }

    #[test]
    fn verify_md5_accepts_real_mac_and_rejects_tampered_or_wrong_key() {
        let mut params = logistics_doc_params();
        // 綠界回傳大寫，比對不分大小寫
        params.push((
            "CheckMacValue".to_string(),
            "692fd6e2cdb539ccdb7206c76dc239ad".to_string(),
        ));
        assert!(verify_md5(LKEY, LIV, &params));
        assert!(!verify(LKEY, LIV, &params), "SHA256 版不能拿 MD5 的簽章過關");

        // GoodsAmount 是第 1 個
        params[0].1 = "1001".to_string();
        assert!(!verify_md5(LKEY, LIV, &params), "改了金額就不能過");

        assert!(!verify_md5(LKEY, LIV, &logistics_doc_params()), "沒有 CheckMacValue 不能過");

        let mut params2 = logistics_doc_params();
        params2.push((
            "CheckMacValue".to_string(),
            check_mac_value_md5(LKEY, LIV, &logistics_doc_params()),
        ));
        assert!(!verify_md5("wrongkey", LIV, &params2), "key 不對不能過");
    }
```

- [ ] **Step 2: 跑測試確認失敗**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && cargo test --manifest-path api/Cargo.toml --lib ecpay::mac`（`run_in_background: true`）
Expected: 編譯失敗，`cannot find function check_mac_value_md5`。

- [ ] **Step 3: 加 md-5 crate、實作 MD5 與 verify_with**

`api/Cargo.toml` 的 `[dependencies]` 加一行（放在 `lettre = …` 區塊之後）：

```toml
md-5 = "0.10"
```

`api/src/ecpay/mac.rs` 開頭的說明與 import 改成：

```rust
//! CheckMacValue（規格 §8.1）：參數依 key 排序（不分大小寫）→ `HashKey=…&k=v&…&HashIV=…`
//! → .NET 風格 URL encode → 轉小寫 → 雜湊 → 轉大寫。全方位金流用 SHA256、物流用 MD5。
//! `raw_string` 的輸出含 HashKey／HashIV：不要印進 log（規格 §11）
use md5::Md5;
use sha2::{Digest, Sha256};
```

把 `verify` 改成共用一個 `verify_with`，並加 MD5 版本（取代原本的 `check_mac_value`／`verify` 兩個函式所在區段）：

```rust
/// SHA256（EncryptType=1）的 CheckMacValue，大寫 hex
pub fn check_mac_value(hash_key: &str, hash_iv: &str, params: &[(String, String)]) -> String {
    let digest = Sha256::digest(raw_string(hash_key, hash_iv, params).as_bytes());
    hex::encode_upper(digest)
}

/// MD5 的 CheckMacValue（物流 API；規格 §8.1），大寫 hex（32 字）
pub fn check_mac_value_md5(hash_key: &str, hash_iv: &str, params: &[(String, String)]) -> String {
    let digest = Md5::digest(raw_string(hash_key, hash_iv, params).as_bytes());
    hex::encode_upper(digest)
}

/// 驗回呼：取出 `CheckMacValue`（key 不分大小寫），用其餘欄位重算再比對（值不分大小寫）
fn verify_with(
    hash_key: &str,
    hash_iv: &str,
    params: &[(String, String)],
    compute: fn(&str, &str, &[(String, String)]) -> String,
) -> bool {
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
    compute(hash_key, hash_iv, &rest).eq_ignore_ascii_case(given)
}

/// 驗付款回呼（SHA256）
pub fn verify(hash_key: &str, hash_iv: &str, params: &[(String, String)]) -> bool {
    verify_with(hash_key, hash_iv, params, check_mac_value)
}

/// 驗物流回呼與建單回應（MD5）
pub fn verify_md5(hash_key: &str, hash_iv: &str, params: &[(String, String)]) -> bool {
    verify_with(hash_key, hash_iv, params, check_mac_value_md5)
}
```

- [ ] **Step 4: 跑 mac 測試確認通過**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && cargo test --manifest-path api/Cargo.toml --lib ecpay::mac`（`run_in_background: true`；第一次會下載編譯 md-5）
Expected: 6 個測試 PASS（原本 4 個 + 新 2 個）。

- [ ] **Step 5: 寫 config.rs 的失敗測試**

在 `api/src/config.rs` 的 `mod tests` 最後加：

```rust
    #[test]
    fn for_tests_uses_stage_logistics_credentials() {
        let cfg = Config::for_tests(std::path::PathBuf::from("/tmp/dog_shop_cfg_test"));
        assert_eq!(cfg.ecpay.logistics.merchant_id, "2000933");
        assert_eq!(cfg.ecpay.logistics.hash_key, "XBERn1YOvpM9nfZc");
        assert_eq!(
            cfg.ecpay.logistics_base_url(),
            "https://logistics-stage.ecpay.com.tw"
        );
    }
```

- [ ] **Step 6: 跑 config 測試確認失敗**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && cargo test --manifest-path api/Cargo.toml --lib config`（`run_in_background: true`）
Expected: 編譯失敗，`no field logistics`。

- [ ] **Step 7: 實作 config.rs**

`EcpayConfig` 加欄位、加常數、加網址（`api/src/config.rs:64-89` 區段改成）：

```rust
#[derive(Clone, Debug)]
pub struct EcpayConfig {
    pub env: EcpayEnv,
    pub aio: EcpayCredentials,
    pub invoice: EcpayCredentials,
    /// 物流 C2C（計畫 4）
    pub logistics: EcpayCredentials,
}

/// 全方位金流測試特店（規格 §8.5；公開資料，只能用於 stage）：(MerchantID, HashKey, HashIV)
pub const STAGE_AIO: (&str, &str, &str) = ("3002607", "pwFHCqoQZGmho4w6", "EkRm7iFT261dpevs");
/// 電子發票 B2C 測試特店（developers.ecpay.com.tw「測試介接資訊」）
pub const STAGE_INVOICE: (&str, &str, &str) = ("2000132", "ejCk326UnaZWKisg", "q9jcZX8Ib9LM8wYk");
/// 物流 C2C 測試特店（developers.ecpay.com.tw/7398/「測試介接資訊」；B2C 的 2000132 不能混用）
pub const STAGE_LOGISTICS: (&str, &str, &str) =
    ("2000933", "XBERn1YOvpM9nfZc", "h1ONHk4P4yqbl5LK");

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

    /// 物流 API 主機（規格 §8.3 的 `/Express/*` 都接在後面），不含結尾斜線
    pub fn logistics_base_url(&self) -> &'static str {
        match self.env {
            EcpayEnv::Stage => "https://logistics-stage.ecpay.com.tw",
            EcpayEnv::Prod => "https://logistics.ecpay.com.tw",
        }
    }
}
```

`from_env` 的 `EcpayConfig { … }` 改成：

```rust
        let ecpay = EcpayConfig {
            env: ecpay_env,
            aio: credentials("ECPAY_AIO", ecpay_env, STAGE_AIO)?,
            invoice: credentials("ECPAY_INVOICE", ecpay_env, STAGE_INVOICE)?,
            logistics: credentials("ECPAY_LOGISTICS", ecpay_env, STAGE_LOGISTICS)?,
        };
```

`for_tests` 的 `ecpay: EcpayConfig { … }` 在 `invoice` 之後加：

```rust
                logistics: EcpayCredentials {
                    merchant_id: STAGE_LOGISTICS.0.to_string(),
                    hash_key: STAGE_LOGISTICS.1.to_string(),
                    hash_iv: STAGE_LOGISTICS.2.to_string(),
                },
```

- [ ] **Step 8: 更新 .env.example**

把 `.env.example` 的物流四行改成：

```
# 物流 C2C 超商取貨（計畫 4）。stage 憑證是公開測試資料（測試特店 2000933，廠商後台 LogisticsC2CTest / test1234）；
# 正式憑證從綠界廠商後台「物流」取得，只放伺服器的 .env
ECPAY_LOGISTICS_MERCHANT_ID=2000933
ECPAY_LOGISTICS_HASH_KEY=XBERn1YOvpM9nfZc
ECPAY_LOGISTICS_HASH_IV=h1ONHk4P4yqbl5LK
```

- [ ] **Step 9: 跑全部單元測試、fmt、clippy**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && cargo test --manifest-path api/Cargo.toml --lib`（`run_in_background: true`）
Expected: 全綠（原本 67 個單元測試 + 3 個新的）。
Run: `export PATH="$HOME/.cargo/bin:$PATH" && cargo fmt --all --manifest-path api/Cargo.toml --check && cargo clippy --manifest-path api/Cargo.toml --all-targets -- -D warnings`（`run_in_background: true`）
Expected: 沒有輸出、exit 0。

- [ ] **Step 10: Commit**

```bash
git add api/Cargo.toml api/Cargo.lock api/src/config.rs api/src/ecpay/mac.rs .env.example
git commit -m "feat(api): 物流 C2C 憑證、網址與 MD5 CheckMacValue（對照綠界文件範例）"
```

---

### Task 2: `ecpay/logistics.rs`：純函式與可替換的物流閘道

**Files:**
- Create: `api/src/ecpay/logistics.rs`
- Create: `api/src/domain/shipments.rs`（本任務只放狀態常數與 rank；Task 4、6 再擴充）
- Modify: `api/src/ecpay/mod.rs`（`pub mod logistics;`）、`api/src/domain/mod.rs`（`pub mod shipments;`）
- Modify: `api/src/state.rs`（`logistics: Arc<LogisticsGateway>`）、`api/src/main.rs:39-46`
- Modify: `api/tests/common/mod.rs:25-35`（Fake 閘道）、末尾（`fake_logistics`）
- Test: `api/src/ecpay/logistics.rs`（單元）、`api/src/domain/shipments.rs`（單元）

**Interfaces:**
- Consumes: `EcpayConfig.logistics`、`logistics_base_url()`（Task 1）；`mac::check_mac_value_md5`／`verify_md5`（Task 1）；`aio::CheckoutForm`、`aio::CallbackError`；`time::format_datetime`／`parse_datetime`；`orders::OrderDetail`。
- Produces（給 Task 3～6 用，簽名固定）：
  - `domain::shipments`：`SHIPMENT_PENDING`、`SHIPMENT_CREATED`、`SHIPMENT_IN_TRANSIT`、`SHIPMENT_ARRIVED`、`SHIPMENT_PICKED_UP`、`SHIPMENT_RETURNED`、`SHIPMENT_SHIPPED: &str`；`status_rank(status: &str) -> u8`；`should_apply(current: &str, next: &str) -> bool`。
  - `ecpay::logistics`：`SUB_TYPES`、`is_sub_type(&str) -> bool`；`map_form(cfg: &EcpayConfig, public_base_url: &str, token: &str, sub_type: &str, device_mobile: bool) -> CheckoutForm`；`MapReply { token, sub_type, store_id, store_name, store_address, store_phone: String, outside: bool }` 與 `parse_map_reply(params: &[(String, String)]) -> Result<MapReply, CallbackError>`；`CreateRequest { merchant_trade_no, sub_type, goods_amount: i32, goods_name, sender_name, sender_phone, receiver_name, receiver_phone, receiver_email, receiver_store_id: String, return_store_id: Option<String> }`；`create_fields(cfg, public_base_url, req: &CreateRequest, now: DateTime<Utc>) -> BTreeMap<String, String>`（含 `CheckMacValue`）；`create_url(cfg) -> String`；`CreateOk { logistics_id, rtn_code: i32, rtn_msg, cvs_payment_no, cvs_validation_no: String, raw: Value }`；`CreateError::{Rejected(String), Malformed(String), BadMac}`；`parse_create_response(cfg, body: &str) -> Result<CreateOk, CreateError>`；`print_form(cfg, sub_type, logistics_id, cvs_payment_no, cvs_validation_no) -> anyhow::Result<CheckoutForm>`；`StatusNotification { merchant_trade_no, logistics_id, rtn_code: i32, rtn_msg: String, update_at: Option<DateTime<Utc>>, raw: Value }` 與 `parse_status(cfg, params) -> Result<StatusNotification, CallbackError>`；`StoreUpdate { logistics_id, store_type, status, store_id: String, raw: Value }` 與 `parse_store_update(cfg, params) -> Result<StoreUpdate, CallbackError>`、`store_update_message(&StoreUpdate) -> String`；`shipment_status_for(rtn_code: i32) -> Option<&'static str>`；`sanitize_goods_name(&str) -> String`、`sanitize_name(&str) -> String`、`goods_name(&OrderDetail) -> String`；`fake_success_body(fields: &BTreeMap<String, String>) -> String`；`LogisticsGateway::{Ecpay(EcpayLogisticsClient), Fake(FakeLogisticsGateway)}`、`LogisticsGateway::ecpay() -> anyhow::Result<Self>`、`LogisticsGateway::post_form(&self, url: &str, fields: &BTreeMap<String, String>) -> anyhow::Result<String>`；`FakeLogisticsGateway::calls() -> Vec<(String, BTreeMap<String, String>)>`、`respond_with(&self, body: &str)`、`fail_next(&self, msg: &str)`。
  - `AppState.logistics: Arc<LogisticsGateway>`；測試 `common::fake_logistics(&state) -> &FakeLogisticsGateway`。

- [ ] **Step 1: 建 `domain/shipments.rs`（常數與 rank）與它的測試**

`api/src/domain/shipments.rs`：

```rust
//! shipments 表（規格 §3、§4）。本任務只有狀態常數與不倒退規則；Task 4 加完整列與狀態套用，Task 6 加出貨寫入
pub const SHIPMENT_PENDING: &str = "pending";
pub const SHIPMENT_CREATED: &str = "created";
pub const SHIPMENT_IN_TRANSIT: &str = "in_transit";
pub const SHIPMENT_ARRIVED: &str = "arrived";
pub const SHIPMENT_PICKED_UP: &str = "picked_up";
pub const SHIPMENT_RETURNED: &str = "returned";
/// 宅配：老闆填單號就是 shipped
pub const SHIPMENT_SHIPPED: &str = "shipped";

/// 狀態的先後（與規格不同之處 42）：arrived 與 returned 同一階（退回後可能重新配達）；
/// picked_up 與宅配的 shipped 是終態
pub fn status_rank(status: &str) -> u8 {
    match status {
        SHIPMENT_PENDING => 0,
        SHIPMENT_CREATED => 1,
        SHIPMENT_IN_TRANSIT => 2,
        SHIPMENT_ARRIVED | SHIPMENT_RETURNED => 3,
        SHIPMENT_PICKED_UP | SHIPMENT_SHIPPED => 4,
        _ => 0,
    }
}

/// 綠界通知晚到、重複、亂序都不能讓狀態倒退：新狀態的階要 >= 目前的、而且不同才套用；終態不再改
pub fn should_apply(current: &str, next: &str) -> bool {
    current != next
        && current != SHIPMENT_PICKED_UP
        && current != SHIPMENT_SHIPPED
        && status_rank(next) >= status_rank(current)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forward_moves_apply_backward_moves_do_not() {
        assert!(should_apply(SHIPMENT_PENDING, SHIPMENT_CREATED));
        assert!(should_apply(SHIPMENT_CREATED, SHIPMENT_IN_TRANSIT));
        assert!(should_apply(SHIPMENT_IN_TRANSIT, SHIPMENT_ARRIVED));
        assert!(should_apply(SHIPMENT_ARRIVED, SHIPMENT_PICKED_UP));
        assert!(!should_apply(SHIPMENT_ARRIVED, SHIPMENT_IN_TRANSIT), "晚到的物流中心通知不能倒退");
        assert!(!should_apply(SHIPMENT_IN_TRANSIT, SHIPMENT_CREATED));
        assert!(!should_apply(SHIPMENT_ARRIVED, SHIPMENT_ARRIVED), "同狀態不算變更");
    }

    #[test]
    fn returned_and_arrived_can_swap_but_picked_up_is_final() {
        assert!(should_apply(SHIPMENT_ARRIVED, SHIPMENT_RETURNED));
        assert!(should_apply(SHIPMENT_RETURNED, SHIPMENT_ARRIVED), "退回後重新配達");
        assert!(!should_apply(SHIPMENT_PICKED_UP, SHIPMENT_RETURNED));
        assert!(!should_apply(SHIPMENT_PICKED_UP, SHIPMENT_ARRIVED));
        assert!(!should_apply(SHIPMENT_SHIPPED, SHIPMENT_ARRIVED), "宅配不會收到超商通知");
    }
}
```

`api/src/domain/mod.rs` 加 `pub mod shipments;`（字母順序，放在 `settings` 之後）。

- [ ] **Step 2: 寫 logistics.rs 的測試（先寫，會編譯失敗）**

建 `api/src/ecpay/logistics.rs`，先只放測試模組（實作在 Step 4 補在測試之前）：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, STAGE_LOGISTICS};
    use crate::domain::orders::{OrderDetail, OrderItemRow, OrderRow};
    use crate::domain::shipments::*;
    use chrono::TimeZone;
    use uuid::Uuid;

    fn cfg() -> Config {
        Config::for_tests(std::path::PathBuf::from("/tmp/dog_shop_logistics_test"))
    }

    fn params(fields: &BTreeMap<String, String>) -> Vec<(String, String)> {
        fields.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
    }

    fn order(items: &[(&str, i32)], subtotal: i32) -> OrderDetail {
        OrderDetail {
            order: OrderRow {
                id: Uuid::now_v7(),
                order_no: "DS260908ABCD".to_string(),
                user_id: None,
                guest_token: "t".to_string(),
                status: "paid".to_string(),
                email: "buyer@test.local".to_string(),
                recipient_name: "王小明".to_string(),
                recipient_phone: "0912345678".to_string(),
                shipping_method: "cvs".to_string(),
                subtotal,
                shipping_fee: 60,
                total: subtotal + 60,
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
                paid_at: Some(Utc::now()),
                shipped_at: None,
                completed_at: None,
                cancelled_at: None,
                cancel_reason: None,
            },
            items: items
                .iter()
                .map(|(name, qty)| OrderItemRow {
                    product_name: name.to_string(),
                    variant_label: "預設".to_string(),
                    unit_price: 100,
                    quantity: *qty,
                    line_total: 100 * qty,
                    image_path: None,
                })
                .collect(),
            shipment: None,
            payment: None,
            invoice: None,
        }
    }

    fn request() -> CreateRequest {
        CreateRequest {
            merchant_trade_no: "DS260908ABCDL01".to_string(),
            sub_type: "UNIMARTC2C".to_string(),
            goods_amount: 700,
            goods_name: "雞肉狗糧".to_string(),
            sender_name: "狗狗商店".to_string(),
            sender_phone: "0987654321".to_string(),
            receiver_name: "王小明".to_string(),
            receiver_phone: "0912345678".to_string(),
            receiver_email: "buyer@test.local".to_string(),
            receiver_store_id: "131386".to_string(),
            return_store_id: None,
        }
    }

    #[test]
    fn map_form_has_spec_fields_and_no_mac() {
        let cfg = cfg();
        let form = map_form(&cfg.ecpay, "http://localhost:5173", "AbCdEfGhIjKlMnOpQrSt", "FAMIC2C", true);
        assert_eq!(form.action, "https://logistics-stage.ecpay.com.tw/Express/map");
        let f = &form.fields;
        assert_eq!(f["MerchantID"], "2000933");
        assert_eq!(f["MerchantTradeNo"], "AbCdEfGhIjKlMnOpQrSt");
        assert_eq!(f["ExtraData"], "AbCdEfGhIjKlMnOpQrSt");
        assert_eq!(f["LogisticsType"], "CVS");
        assert_eq!(f["LogisticsSubType"], "FAMIC2C");
        assert_eq!(f["IsCollection"], "N");
        assert_eq!(f["ServerReplyURL"], "http://localhost:5173/api/ecpay/logistics/map-reply");
        assert_eq!(f["Device"], "1");
        assert!(!f.contains_key("CheckMacValue"), "電子地圖不需簽章");
        assert_eq!(map_form(&cfg.ecpay, "http://x", "t", "UNIMARTC2C", false).fields["Device"], "0");
    }

    #[test]
    fn parse_map_reply_reads_store_and_tolerates_missing_phone() {
        let p = vec![
            ("MerchantID".to_string(), "2000933".to_string()),
            ("MerchantTradeNo".to_string(), "tok".to_string()),
            ("LogisticsSubType".to_string(), "UNIMARTC2C".to_string()),
            ("CVSStoreID".to_string(), " 991182 ".to_string()),
            ("CVSStoreName".to_string(), "測試門市".to_string()),
            ("CVSAddress".to_string(), "台北市中正區重慶南路一段 122 號".to_string()),
            ("CVSOutSide".to_string(), "0".to_string()),
            ("ExtraData".to_string(), "tok".to_string()),
        ];
        let r = parse_map_reply(&p).unwrap();
        assert_eq!(r.token, "tok");
        assert_eq!(r.sub_type, "UNIMARTC2C");
        assert_eq!(r.store_id, "991182");
        assert_eq!(r.store_name, "測試門市");
        assert_eq!(r.store_phone, "", "7-11 不回電話");
        assert!(!r.outside);

        let mut missing = p.clone();
        missing.retain(|(k, _)| k != "CVSStoreID");
        assert_eq!(parse_map_reply(&missing).unwrap_err(), CallbackError::Missing("CVSStoreID"));
        let mut bad = p.clone();
        bad[2].1 = "TCAT".to_string();
        assert_eq!(parse_map_reply(&bad).unwrap_err(), CallbackError::Missing("LogisticsSubType"));
        let mut no_token = p;
        no_token.retain(|(k, _)| k != "ExtraData");
        assert_eq!(parse_map_reply(&no_token).unwrap_err(), CallbackError::Missing("ExtraData"));
    }

    #[test]
    fn create_fields_match_spec_and_mac_verifies() {
        let cfg = cfg();
        let now = Utc.with_ymd_and_hms(2026, 9, 8, 4, 5, 6).unwrap(); // 台北 12:05:06
        let f = create_fields(&cfg.ecpay, "https://shop.example", &request(), now);
        assert_eq!(f["MerchantID"], "2000933");
        assert_eq!(f["MerchantTradeNo"], "DS260908ABCDL01");
        assert_eq!(f["MerchantTradeDate"], "2026/09/08 12:05:06");
        assert_eq!(f["LogisticsType"], "CVS");
        assert_eq!(f["LogisticsSubType"], "UNIMARTC2C");
        assert_eq!(f["GoodsAmount"], "700");
        assert_eq!(f["GoodsName"], "雞肉狗糧");
        assert_eq!(f["SenderName"], "狗狗商店");
        assert_eq!(f["SenderCellPhone"], "0987654321");
        assert_eq!(f["ReceiverName"], "王小明");
        assert_eq!(f["ReceiverCellPhone"], "0912345678");
        assert_eq!(f["ReceiverEmail"], "buyer@test.local");
        assert_eq!(f["ReceiverStoreID"], "131386");
        assert_eq!(f["ServerReplyURL"], "https://shop.example/api/ecpay/logistics/status");
        assert_eq!(f["LogisticsC2CReplyURL"], "https://shop.example/api/ecpay/logistics/store-update");
        assert_eq!(f["IsCollection"], "N");
        assert!(!f.contains_key("ReturnStoreID"), "沒設退貨門市就不帶");
        assert!(!f.contains_key("ClientReplyURL"), "幕後建單不帶");
        assert_eq!(f["CheckMacValue"].len(), 32);
        assert!(mac::verify_md5(STAGE_LOGISTICS.1, STAGE_LOGISTICS.2, &params(&f)));

        let mut req = request();
        req.return_store_id = Some("991182".to_string());
        req.goods_amount = 25_000;
        req.receiver_email = "a".repeat(46) + "@x.tw"; // 51 字
        let f = create_fields(&cfg.ecpay, "https://shop.example", &req, now);
        assert_eq!(f["ReturnStoreID"], "991182");
        assert_eq!(f["GoodsAmount"], "20000", "夾在 1～20000");
        assert!(!f.contains_key("ReceiverEmail"), "Email 超過 50 字不帶");
        assert_eq!(create_url(&cfg.ecpay), "https://logistics-stage.ecpay.com.tw/Express/Create");
    }

    #[test]
    fn parse_create_response_success_rejected_bad_mac_malformed() {
        let cfg = cfg();
        let f = create_fields(&cfg.ecpay, "https://shop.example", &request(), Utc::now());
        let body = fake_success_body(&f);
        assert!(body.starts_with("1|"));
        let ok = parse_create_response(&cfg.ecpay, &body).unwrap();
        assert_eq!(ok.logistics_id, "FAKEDS260908ABCDL01");
        assert_eq!(ok.rtn_code, 300);
        assert_eq!(ok.rtn_msg, "訂單處理中(已收到訂單資料)");
        assert_eq!(ok.cvs_payment_no, "F0001234");
        assert_eq!(ok.cvs_validation_no, "1234", "7-11 才有驗證碼");
        assert_eq!(ok.raw["MerchantTradeNo"], "DS260908ABCDL01");

        assert_eq!(
            parse_create_response(&cfg.ecpay, "0|收件人姓名格式錯誤").unwrap_err(),
            CreateError::Rejected("收件人姓名格式錯誤".to_string())
        );
        let tampered = body.replace("RtnCode=300", "RtnCode=301");
        assert_eq!(parse_create_response(&cfg.ecpay, &tampered).unwrap_err(), CreateError::BadMac);
        assert!(matches!(parse_create_response(&cfg.ecpay, "<html>500</html>").unwrap_err(), CreateError::Malformed(_)));
        assert!(matches!(parse_create_response(&cfg.ecpay, "1|MerchantID=2000933").unwrap_err(), CreateError::BadMac));

        let mut fam = request();
        fam.sub_type = "FAMIC2C".to_string();
        let f = create_fields(&cfg.ecpay, "https://shop.example", &fam, Utc::now());
        let ok = parse_create_response(&cfg.ecpay, &fake_success_body(&f)).unwrap();
        assert_eq!(ok.cvs_validation_no, "", "全家沒有驗證碼");
    }

    #[test]
    fn print_form_per_sub_type() {
        let cfg = cfg();
        let f = print_form(&cfg.ecpay, "UNIMARTC2C", "10035", "F0001234", "1234").unwrap();
        assert_eq!(f.action, "https://logistics-stage.ecpay.com.tw/Express/PrintUniMartC2COrderInfo");
        assert_eq!(f.fields["AllPayLogisticsID"], "10035");
        assert_eq!(f.fields["CVSPaymentNo"], "F0001234");
        assert_eq!(f.fields["CVSValidationNo"], "1234");
        assert!(mac::verify_md5(STAGE_LOGISTICS.1, STAGE_LOGISTICS.2, &params(&f.fields)));

        let f = print_form(&cfg.ecpay, "FAMIC2C", "10035", "F0001234", "").unwrap();
        assert_eq!(f.action, "https://logistics-stage.ecpay.com.tw/Express/PrintFAMIC2COrderInfo");
        assert!(!f.fields.contains_key("CVSValidationNo"));
        let f = print_form(&cfg.ecpay, "HILIFEC2C", "10035", "F0001234", "").unwrap();
        assert_eq!(f.action, "https://logistics-stage.ecpay.com.tw/Express/PrintHILIFEC2COrderInfo");
        assert!(print_form(&cfg.ecpay, "TCAT", "1", "2", "3").is_err());
    }

    #[test]
    fn parse_status_requires_valid_mac_and_fields() {
        let cfg = cfg();
        let mut p = vec![
            ("MerchantID".to_string(), "2000933".to_string()),
            ("MerchantTradeNo".to_string(), "DS260908ABCDL01".to_string()),
            ("RtnCode".to_string(), "2067".to_string()),
            ("RtnMsg".to_string(), "消費者成功取件".to_string()),
            ("AllPayLogisticsID".to_string(), "10035".to_string()),
            ("LogisticsType".to_string(), "CVS".to_string()),
            ("LogisticsSubType".to_string(), "UNIMARTC2C".to_string()),
            ("GoodsAmount".to_string(), "700".to_string()),
            ("UpdateStatusDate".to_string(), "2026/09/10 18:30:00".to_string()),
        ];
        let macv = mac::check_mac_value_md5(STAGE_LOGISTICS.1, STAGE_LOGISTICS.2, &p);
        p.push(("CheckMacValue".to_string(), macv));
        let n = parse_status(&cfg.ecpay, &p).unwrap();
        assert_eq!(n.merchant_trade_no, "DS260908ABCDL01");
        assert_eq!(n.logistics_id, "10035");
        assert_eq!(n.rtn_code, 2067);
        assert_eq!(n.rtn_msg, "消費者成功取件");
        assert_eq!(n.update_at, Some(Utc.with_ymd_and_hms(2026, 9, 10, 10, 30, 0).unwrap()));
        assert_eq!(n.raw["RtnCode"], "2067");

        let mut bad = p.clone();
        bad[2].1 = "2074".to_string();
        assert_eq!(parse_status(&cfg.ecpay, &bad).unwrap_err(), CallbackError::BadMac);
        let mut no_code: Vec<_> = p.iter().filter(|(k, _)| k != "RtnCode").cloned().collect();
        let macv = mac::check_mac_value_md5(STAGE_LOGISTICS.1, STAGE_LOGISTICS.2, &no_code[..no_code.len() - 1]);
        no_code.last_mut().unwrap().1 = macv;
        assert_eq!(parse_status(&cfg.ecpay, &no_code).unwrap_err(), CallbackError::Missing("RtnCode"));
    }

    #[test]
    fn parse_store_update_and_message() {
        let cfg = cfg();
        let mut p = vec![
            ("MerchantID".to_string(), "2000933".to_string()),
            ("AllPayLogisticsID".to_string(), "10035".to_string()),
            ("GoodsName".to_string(), "雞肉狗糧".to_string()),
            ("GoodsAmount".to_string(), "700".to_string()),
            ("StoreType".to_string(), "01".to_string()),
            ("Status".to_string(), "01".to_string()),
            ("StoreID".to_string(), "991182".to_string()),
        ];
        let macv = mac::check_mac_value_md5(STAGE_LOGISTICS.1, STAGE_LOGISTICS.2, &p);
        p.push(("CheckMacValue".to_string(), macv));
        let u = parse_store_update(&cfg.ecpay, &p).unwrap();
        assert_eq!(u.logistics_id, "10035");
        assert_eq!(store_update_message(&u), "取件門市異動：門市關轉店（991182）");
        p[6].1 = "000001".to_string();
        assert_eq!(parse_store_update(&cfg.ecpay, &p).unwrap_err(), CallbackError::BadMac);
    }

    #[test]
    fn status_code_table() {
        assert_eq!(shipment_status_for(300), Some(SHIPMENT_CREATED));
        assert_eq!(shipment_status_for(2030), Some(SHIPMENT_IN_TRANSIT));
        assert_eq!(shipment_status_for(3024), Some(SHIPMENT_IN_TRANSIT));
        assert_eq!(shipment_status_for(2068), Some(SHIPMENT_IN_TRANSIT));
        assert_eq!(shipment_status_for(2073), Some(SHIPMENT_ARRIVED));
        assert_eq!(shipment_status_for(2063), Some(SHIPMENT_ARRIVED));
        assert_eq!(shipment_status_for(3018), Some(SHIPMENT_ARRIVED));
        assert_eq!(shipment_status_for(2098), Some(SHIPMENT_ARRIVED));
        assert_eq!(shipment_status_for(2067), Some(SHIPMENT_PICKED_UP));
        assert_eq!(shipment_status_for(3022), Some(SHIPMENT_PICKED_UP));
        assert_eq!(shipment_status_for(2074), Some(SHIPMENT_RETURNED));
        assert_eq!(shipment_status_for(3020), Some(SHIPMENT_RETURNED));
        assert_eq!(shipment_status_for(2088), Some(SHIPMENT_RETURNED));
        assert_eq!(shipment_status_for(2101), None, "門市關轉店只記錄");
        assert_eq!(shipment_status_for(9999), None);
    }

    #[test]
    fn goods_name_and_names_are_sanitized() {
        assert_eq!(sanitize_goods_name("雞肉狗糧 #1 [大包] <特價>"), "雞肉狗糧 1 大包 特價");
        assert_eq!(sanitize_goods_name(""), "商品");
        assert_eq!(sanitize_goods_name("^'`!@#%&*+\\\"<>|_[]"), "商品");
        let long = "狗".repeat(40);
        assert_eq!(sanitize_goods_name(&long).chars().count(), 25, "中文算 2，寬度上限 50");
        let mixed = "abc".to_string() + &"狗".repeat(30);
        assert_eq!(sanitize_goods_name(&mixed).chars().count(), 3 + 23);
        assert_eq!(goods_name(&order(&[("雞肉狗糧", 1)], 300)), "雞肉狗糧");
        assert_eq!(goods_name(&order(&[("雞肉狗糧", 2), ("潔牙骨", 1)], 700)), "雞肉狗糧 等3件");
        assert_eq!(sanitize_name(" 狗狗 商店! "), "狗狗商店");
        assert_eq!(sanitize_name("王小明"), "王小明");
        assert_eq!(sanitize_name(&"商".repeat(8)), "商".repeat(5), "寄件人寬度上限 10");
        assert!(is_sub_type("FAMIC2C") && !is_sub_type("TCAT"));
    }
}
```

- [ ] **Step 3: 跑測試確認失敗**

`api/src/ecpay/mod.rs` 加 `pub mod logistics;`（字母順序，放在 `invoice` 之後）。

Run: `export PATH="$HOME/.cargo/bin:$PATH" && cargo test --manifest-path api/Cargo.toml --lib ecpay::logistics`（`run_in_background: true`）
Expected: 編譯失敗（`map_form` 等不存在）。

- [ ] **Step 4: 實作 logistics.rs（放在測試模組之前）**

```rust
//! 綠界物流 C2C 超商取貨（規格 §8.3）：電子地圖表單、建立物流單、列印託運單、狀態回呼解析、
//! 貨態代碼對照。網路呼叫只在 `LogisticsGateway`；其餘都是純函式，測試不打綠界。
//! 綠界文件的查證結果見計畫 4 的「環境事實」。
use std::collections::{BTreeMap, VecDeque};
use std::sync::Mutex;

use anyhow::Context;
use chrono::{DateTime, Utc};
use serde_json::{Map, Value};

use crate::config::EcpayConfig;
use crate::domain::orders::OrderDetail;
use crate::domain::shipments::{
    SHIPMENT_ARRIVED, SHIPMENT_CREATED, SHIPMENT_IN_TRANSIT, SHIPMENT_PICKED_UP, SHIPMENT_RETURNED,
};
use crate::ecpay::aio::{CallbackError, CheckoutForm};
use crate::ecpay::{mac, time};

/// 綠界 C2C 超商代碼（規格 §3）
pub const SUB_TYPES: &[&str] = &["UNIMARTC2C", "FAMIC2C", "HILIFEC2C"];
/// GoodsAmount 範圍（規格 §8.3）
pub const GOODS_AMOUNT_MIN: i32 = 1;
pub const GOODS_AMOUNT_MAX: i32 = 20_000;
/// GoodsName 上限：50 個「寬度」，中文等非 ASCII 算 2（綠界文件）
pub const GOODS_NAME_WIDTH_MAX: usize = 50;
/// SenderName 上限：10 個寬度（中文 5 字）
pub const SENDER_NAME_WIDTH_MAX: usize = 10;
/// ReceiverEmail 上限（綠界 String(50)）
pub const RECEIVER_EMAIL_MAX: usize = 50;
/// 綠界 GoodsName／姓名不得含的符號
pub const FORBIDDEN_CHARS: &[char] = &[
    '^', '\'', '`', '!', '@', '#', '%', '&', '*', '+', '\\', '"', '<', '>', '|', '_', '[', ']',
];
/// 建單的 HTTP 逾時（同發票 API）
pub const HTTP_TIMEOUT_SECS: u64 = 20;

pub fn is_sub_type(s: &str) -> bool {
    SUB_TYPES.contains(&s)
}

fn truncate_chars(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

fn field<'a>(params: &'a [(String, String)], name: &str) -> Option<&'a str> {
    params
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}

fn required<'a>(
    params: &'a [(String, String)],
    name: &'static str,
) -> Result<&'a str, CallbackError> {
    field(params, name)
        .filter(|v| !v.trim().is_empty())
        .ok_or(CallbackError::Missing(name))
}

fn params_to_json(params: &[(String, String)]) -> Value {
    Value::Object(
        params
            .iter()
            .map(|(k, v)| (k.clone(), Value::String(v.clone())))
            .collect::<Map<String, Value>>(),
    )
}

fn form_params(fields: &BTreeMap<String, String>) -> Vec<(String, String)> {
    fields.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
}

fn sign(cfg: &EcpayConfig, fields: &mut BTreeMap<String, String>) {
    let mac = mac::check_mac_value_md5(
        &cfg.logistics.hash_key,
        &cfg.logistics.hash_iv,
        &form_params(fields),
    );
    fields.insert("CheckMacValue".to_string(), mac);
}

// ───── 電子地圖 ─────

/// 電子地圖表單（規格 §8.3；不需 CheckMacValue）。token 同時當 MerchantTradeNo 與 ExtraData（都 ≤ 20 字）
pub fn map_form(
    cfg: &EcpayConfig,
    public_base_url: &str,
    token: &str,
    sub_type: &str,
    device_mobile: bool,
) -> CheckoutForm {
    let mut fields = BTreeMap::new();
    let mut put = |k: &str, v: String| {
        fields.insert(k.to_string(), v);
    };
    put("MerchantID", cfg.logistics.merchant_id.clone());
    put("MerchantTradeNo", token.to_string());
    put("LogisticsType", "CVS".to_string());
    put("LogisticsSubType", sub_type.to_string());
    put("IsCollection", "N".to_string());
    put(
        "ServerReplyURL",
        format!("{public_base_url}/api/ecpay/logistics/map-reply"),
    );
    put("ExtraData", token.to_string());
    put("Device", if device_mobile { "1" } else { "0" }.to_string());
    CheckoutForm {
        action: format!("{}/Express/map", cfg.logistics_base_url()),
        fields,
    }
}

/// 綠界地圖用買家瀏覽器 POST 回 map-reply 的欄位（規格 §8.3）；7-11 沒有電話。沒有簽章
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapReply {
    pub token: String,
    pub sub_type: String,
    pub store_id: String,
    pub store_name: String,
    pub store_address: String,
    pub store_phone: String,
    /// CVSOutSide = 1（離島）
    pub outside: bool,
}

pub fn parse_map_reply(params: &[(String, String)]) -> Result<MapReply, CallbackError> {
    let token = truncate_chars(required(params, "ExtraData")?.trim(), 20);
    let sub_type = required(params, "LogisticsSubType")?.trim().to_string();
    if !is_sub_type(&sub_type) {
        return Err(CallbackError::Missing("LogisticsSubType"));
    }
    Ok(MapReply {
        token,
        sub_type,
        store_id: truncate_chars(required(params, "CVSStoreID")?.trim(), 20),
        store_name: truncate_chars(required(params, "CVSStoreName")?.trim(), 40),
        store_address: truncate_chars(required(params, "CVSAddress")?.trim(), 120),
        store_phone: truncate_chars(field(params, "CVSTelephone").unwrap_or("").trim(), 20),
        outside: field(params, "CVSOutSide").is_some_and(|v| v.trim() == "1"),
    })
}

// ───── 建立物流單 ─────

/// 建單要的資料（routes/admin_orders 從訂單與設定湊出來）
#[derive(Debug, Clone)]
pub struct CreateRequest {
    pub merchant_trade_no: String,
    pub sub_type: String,
    /// 商品小計（與規格不同之處 36）
    pub goods_amount: i32,
    pub goods_name: String,
    pub sender_name: String,
    pub sender_phone: String,
    pub receiver_name: String,
    pub receiver_phone: String,
    pub receiver_email: String,
    pub receiver_store_id: String,
    pub return_store_id: Option<String>,
}

pub fn create_url(cfg: &EcpayConfig) -> String {
    format!("{}/Express/Create", cfg.logistics_base_url())
}

/// 組出 POST /Express/Create 的欄位（規格 §8.3 的清單，含 MD5 CheckMacValue）。
/// `now` 由呼叫者傳入，測試才能固定。幕後建單不帶 ClientReplyURL
pub fn create_fields(
    cfg: &EcpayConfig,
    public_base_url: &str,
    req: &CreateRequest,
    now: DateTime<Utc>,
) -> BTreeMap<String, String> {
    let mut fields = BTreeMap::new();
    let mut put = |k: &str, v: String| {
        fields.insert(k.to_string(), v);
    };
    put("MerchantID", cfg.logistics.merchant_id.clone());
    put("MerchantTradeNo", req.merchant_trade_no.clone());
    put("MerchantTradeDate", time::format_datetime(now));
    put("LogisticsType", "CVS".to_string());
    put("LogisticsSubType", req.sub_type.clone());
    put(
        "GoodsAmount",
        req.goods_amount
            .clamp(GOODS_AMOUNT_MIN, GOODS_AMOUNT_MAX)
            .to_string(),
    );
    put("GoodsName", req.goods_name.clone());
    put("SenderName", req.sender_name.clone());
    put("SenderCellPhone", req.sender_phone.clone());
    put("ReceiverName", req.receiver_name.clone());
    put("ReceiverCellPhone", req.receiver_phone.clone());
    if !req.receiver_email.is_empty() && req.receiver_email.chars().count() <= RECEIVER_EMAIL_MAX {
        put("ReceiverEmail", req.receiver_email.clone());
    }
    put("ReceiverStoreID", req.receiver_store_id.clone());
    if let Some(id) = req.return_store_id.as_ref().filter(|s| !s.is_empty()) {
        put("ReturnStoreID", id.clone());
    }
    put(
        "ServerReplyURL",
        format!("{public_base_url}/api/ecpay/logistics/status"),
    );
    // 7-11 C2C 必填；門市關轉店等通知會打這裡（與規格不同之處 35）
    put(
        "LogisticsC2CReplyURL",
        format!("{public_base_url}/api/ecpay/logistics/store-update"),
    );
    put("IsCollection", "N".to_string());
    sign(cfg, &mut fields);
    fields
}

/// `/Express/Create` 成功回應（`1|k=v&…`）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateOk {
    pub logistics_id: String,
    pub rtn_code: i32,
    pub rtn_msg: String,
    pub cvs_payment_no: String,
    /// 7-11 才有
    pub cvs_validation_no: String,
    pub raw: Value,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CreateError {
    /// 綠界回 `0|訊息`
    #[error("綠界拒絕建單：{0}")]
    Rejected(String),
    /// 不是 `1|…`／`0|…`，或成功回應缺欄位
    #[error("綠界回應格式不符：{0}")]
    Malformed(String),
    /// 成功回應的 CheckMacValue 不符（與規格不同之處 40）
    #[error("綠界回應的 CheckMacValue 不符")]
    BadMac,
}

/// 解析建單回應並驗 MD5 MAC。回應本身不含秘密可以存進 raw；不要把 mac::raw_string 印進 log
pub fn parse_create_response(cfg: &EcpayConfig, body: &str) -> Result<CreateOk, CreateError> {
    let body = body.trim();
    let Some((flag, rest)) = body.split_once('|') else {
        return Err(CreateError::Malformed(truncate_chars(body, 200)));
    };
    match flag.trim() {
        "1" => {}
        "0" => return Err(CreateError::Rejected(truncate_chars(rest.trim(), 200))),
        other => return Err(CreateError::Malformed(format!("開頭是 {other}"))),
    }
    let params: Vec<(String, String)> = form_urlencoded::parse(rest.as_bytes())
        .into_owned()
        .collect();
    if !mac::verify_md5(&cfg.logistics.hash_key, &cfg.logistics.hash_iv, &params) {
        return Err(CreateError::BadMac);
    }
    let get = |name: &str| field(&params, name).unwrap_or("").trim().to_string();
    let logistics_id = get("AllPayLogisticsID");
    if logistics_id.is_empty() {
        return Err(CreateError::Malformed("缺 AllPayLogisticsID".to_string()));
    }
    Ok(CreateOk {
        logistics_id,
        rtn_code: get("RtnCode").parse().unwrap_or(0),
        rtn_msg: truncate_chars(&get("RtnMsg"), 200),
        cvs_payment_no: get("CVSPaymentNo"),
        cvs_validation_no: get("CVSValidationNo"),
        raw: params_to_json(&params),
    })
}

// ───── 列印託運單 ─────

/// 列印託運單的表單（規格 §8.3）：前端在新分頁 POST；只有 7-11 帶 CVSValidationNo
pub fn print_form(
    cfg: &EcpayConfig,
    sub_type: &str,
    logistics_id: &str,
    cvs_payment_no: &str,
    cvs_validation_no: &str,
) -> anyhow::Result<CheckoutForm> {
    let path = match sub_type {
        "UNIMARTC2C" => "/Express/PrintUniMartC2COrderInfo",
        "FAMIC2C" => "/Express/PrintFAMIC2COrderInfo",
        "HILIFEC2C" => "/Express/PrintHILIFEC2COrderInfo",
        other => anyhow::bail!("不支援的超商類型 {other}"),
    };
    let mut fields = BTreeMap::new();
    fields.insert("MerchantID".to_string(), cfg.logistics.merchant_id.clone());
    fields.insert("AllPayLogisticsID".to_string(), logistics_id.to_string());
    fields.insert("CVSPaymentNo".to_string(), cvs_payment_no.to_string());
    if sub_type == "UNIMARTC2C" {
        fields.insert("CVSValidationNo".to_string(), cvs_validation_no.to_string());
    }
    sign(cfg, &mut fields);
    Ok(CheckoutForm {
        action: format!("{}{path}", cfg.logistics_base_url()),
        fields,
    })
}

// ───── 狀態通知 ─────

/// 物流狀態通知（ServerReplyURL；規格 §8.3）。其餘欄位原樣留在 raw
#[derive(Debug, Clone)]
pub struct StatusNotification {
    pub merchant_trade_no: String,
    pub logistics_id: String,
    pub rtn_code: i32,
    pub rtn_msg: String,
    pub update_at: Option<DateTime<Utc>>,
    pub raw: Value,
}

pub fn parse_status(
    cfg: &EcpayConfig,
    params: &[(String, String)],
) -> Result<StatusNotification, CallbackError> {
    if !mac::verify_md5(&cfg.logistics.hash_key, &cfg.logistics.hash_iv, params) {
        return Err(CallbackError::BadMac);
    }
    let merchant_trade_no = required(params, "MerchantTradeNo")?.trim().to_string();
    let rtn_code = required(params, "RtnCode")?
        .trim()
        .parse::<i32>()
        .map_err(|_| CallbackError::Missing("RtnCode"))?;
    Ok(StatusNotification {
        merchant_trade_no,
        logistics_id: field(params, "AllPayLogisticsID")
            .unwrap_or("")
            .trim()
            .to_string(),
        rtn_code,
        rtn_msg: truncate_chars(field(params, "RtnMsg").unwrap_or("").trim(), 200),
        update_at: field(params, "UpdateStatusDate").and_then(time::parse_datetime),
        raw: params_to_json(params),
    })
}

/// 更新門市通知（LogisticsC2CReplyURL；7-11 C2C 門市關轉店等）。欄位形狀與狀態通知不同：
/// 沒有 MerchantTradeNo，用 AllPayLogisticsID 找單（與規格不同之處 35）
#[derive(Debug, Clone)]
pub struct StoreUpdate {
    pub logistics_id: String,
    /// 01 取件門市／02 退件門市
    pub store_type: String,
    /// 01 門市關轉店／02 門市舊店號更新／03 退件門市為原寄件門市但無寄件門市資料／04 門市臨時關轉店
    pub status: String,
    pub store_id: String,
    pub raw: Value,
}

pub fn parse_store_update(
    cfg: &EcpayConfig,
    params: &[(String, String)],
) -> Result<StoreUpdate, CallbackError> {
    if !mac::verify_md5(&cfg.logistics.hash_key, &cfg.logistics.hash_iv, params) {
        return Err(CallbackError::BadMac);
    }
    let get = |name: &str| truncate_chars(field(params, name).unwrap_or("").trim(), 20);
    Ok(StoreUpdate {
        logistics_id: required(params, "AllPayLogisticsID")?.trim().to_string(),
        store_type: get("StoreType"),
        status: get("Status"),
        store_id: get("StoreID"),
        raw: params_to_json(params),
    })
}

/// 給老闆看的一句話，存 shipments.last_status_msg
pub fn store_update_message(u: &StoreUpdate) -> String {
    let which = match u.store_type.as_str() {
        "01" => "取件門市",
        "02" => "退件門市",
        other => other,
    };
    let what = match u.status.as_str() {
        "01" => "門市關轉店",
        "02" => "門市舊店號更新",
        "03" => "退件門市無寄件門市資料",
        "04" => "門市臨時關轉店",
        other => other,
    };
    format!("{which}異動：{what}（{}）", u.store_id)
}

/// 貨態代碼 → shipments.status（官方貨態代碼表；全家與萊爾富 B2C/C2C 共用一組；與規格不同之處 42）。
/// 不在表上的代碼回 None：只記 last_status_code／last_status_msg，不改狀態
pub fn shipment_status_for(rtn_code: i32) -> Option<&'static str> {
    Some(match rtn_code {
        // 已建檔／上傳處理中／檔案傳送成功／超商接受資料中
        300 | 310 | 2001 | 2024 => SHIPMENT_CREATED,
        // 賣家已交寄（2068 交貨便收件、3032 賣家已到門市寄件）、物流中心驗收（2030、3024）、轉運／配送中
        2068 | 3032 | 2030 | 3024 | 3001 | 3006 => SHIPMENT_IN_TRANSIT,
        // 到店（2073 配達買家門市、2063 門市配達、3018 到店尚未取貨）、重新配達取件門市、轉換店送達
        2073 | 2063 | 3018 | 2098 | 3029 => SHIPMENT_ARRIVED,
        // 消費者成功取件
        2067 | 3022 => SHIPMENT_PICKED_UP,
        // 七天未取離開門市、退回物流中心／寄件門市的各種原因（2078～2093 是 7-11 的「買家未取貨退回」細分碼）
        2074 | 2076 | 2077 | 2078..=2093 | 2069 | 2070 | 2072 | 2075 | 2099 | 3019 | 3020
        | 3021 | 3023 | 3025 | 7011 => SHIPMENT_RETURNED,
        _ => return None,
    })
}

// ───── 名稱清理 ─────

/// 中文等非 ASCII 算 2、其餘算 1（綠界的算法）
fn width(c: char) -> usize {
    if c.is_ascii() { 1 } else { 2 }
}

fn strip_and_fit(raw: &str, width_max: usize, drop_spaces: bool) -> String {
    let mut out = String::new();
    let mut used = 0;
    for c in raw.chars() {
        if FORBIDDEN_CHARS.contains(&c) || c.is_control() || (drop_spaces && c.is_whitespace()) {
            continue;
        }
        let w = width(c);
        if used + w > width_max {
            break;
        }
        used += w;
        out.push(c);
    }
    out.trim().to_string()
}

/// 去掉綠界禁用符號、依寬度截到 50；空的話回「商品」
pub fn sanitize_goods_name(raw: &str) -> String {
    let out = strip_and_fit(raw, GOODS_NAME_WIDTH_MAX, false);
    if out.is_empty() { "商品".to_string() } else { out }
}

/// 寄件人／收件人姓名：去掉空白與禁用符號、寬度 ≤ 10（綠界會自己去空白，先做免得長度算錯）
pub fn sanitize_name(raw: &str) -> String {
    strip_and_fit(raw, SENDER_NAME_WIDTH_MAX, true)
}

/// GoodsName：第一個品項名稱，多品項加「等 N 件」（N = 總數量）
pub fn goods_name(order: &OrderDetail) -> String {
    let first = order
        .items
        .first()
        .map(|i| i.product_name.as_str())
        .unwrap_or("商品");
    let raw = if order.items.len() > 1 {
        let count: i32 = order.items.iter().map(|i| i.quantity).sum();
        format!("{first} 等{count}件")
    } else {
        first.to_string()
    };
    sanitize_goods_name(&raw)
}

// ───── 閘道 ─────

/// 真的打綠界（表單 POST，回純文字）
pub struct EcpayLogisticsClient {
    client: reqwest::Client,
}

impl EcpayLogisticsClient {
    pub fn new() -> anyhow::Result<Self> {
        Ok(Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(HTTP_TIMEOUT_SECS))
                .build()
                .context("建立 HTTP client")?,
        })
    }

    pub async fn post_form(
        &self,
        url: &str,
        fields: &BTreeMap<String, String>,
    ) -> anyhow::Result<String> {
        let body = form_urlencoded::Serializer::new(String::new())
            .extend_pairs(fields.iter())
            .finish();
        self.client
            .post(url)
            .header(
                reqwest::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            )
            .body(body)
            .send()
            .await
            .context("連線綠界物流 API")?
            .error_for_status()
            .context("綠界物流 API HTTP 錯誤")?
            .text()
            .await
            .context("讀取綠界物流回應")
    }
}

/// 測試用：依請求欄位組一個像綠界的成功回應（`1|…`，帶正確 MD5 MAC；用 stage 憑證）
pub fn fake_success_body(fields: &BTreeMap<String, String>) -> String {
    let (merchant_id, key, iv) = crate::config::STAGE_LOGISTICS;
    let get = |name: &str| fields.get(name).cloned().unwrap_or_default();
    let trade_no = get("MerchantTradeNo");
    let sub_type = get("LogisticsSubType");
    let mut params: Vec<(String, String)> = [
        ("MerchantID", merchant_id.to_string()),
        ("MerchantTradeNo", trade_no.clone()),
        ("RtnCode", "300".to_string()),
        ("RtnMsg", "訂單處理中(已收到訂單資料)".to_string()),
        ("AllPayLogisticsID", format!("FAKE{trade_no}")),
        ("LogisticsType", "CVS".to_string()),
        ("LogisticsSubType", sub_type.clone()),
        ("GoodsAmount", get("GoodsAmount")),
        ("UpdateStatusDate", "2026/09/08 12:00:00".to_string()),
        ("ReceiverName", get("ReceiverName")),
        ("ReceiverPhone", String::new()),
        ("ReceiverCellPhone", get("ReceiverCellPhone")),
        ("ReceiverEmail", get("ReceiverEmail")),
        ("ReceiverAddress", String::new()),
        ("CVSPaymentNo", "F0001234".to_string()),
        (
            "CVSValidationNo",
            if sub_type == "UNIMARTC2C" { "1234" } else { "" }.to_string(),
        ),
        ("BookingNote", String::new()),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v))
    .collect();
    let mac = mac::check_mac_value_md5(key, iv, &params);
    params.push(("CheckMacValue".to_string(), mac));
    let encoded = form_urlencoded::Serializer::new(String::new())
        .extend_pairs(params.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .finish();
    format!("1|{encoded}")
}

/// 測試用：記錄請求、回排好的回應；沒排就回 fake_success_body
#[derive(Default)]
pub struct FakeLogisticsGateway {
    calls: Mutex<Vec<(String, BTreeMap<String, String>)>>,
    queued: Mutex<VecDeque<Result<String, String>>>,
}

impl FakeLogisticsGateway {
    pub fn calls(&self) -> Vec<(String, BTreeMap<String, String>)> {
        self.calls.lock().expect("fake logistics calls").clone()
    }

    /// 下一次呼叫回這段純文字（例如 `0|收件人姓名格式錯誤`）
    pub fn respond_with(&self, body: &str) {
        self.queued
            .lock()
            .expect("fake logistics queue")
            .push_back(Ok(body.to_string()));
    }

    /// 下一次呼叫回連線層錯誤
    pub fn fail_next(&self, msg: &str) {
        self.queued
            .lock()
            .expect("fake logistics queue")
            .push_back(Err(msg.to_string()));
    }
}

/// 可替換的閘道：正式打綠界；測試用 Fake（同 InvoiceGateway 的形狀）
pub enum LogisticsGateway {
    Ecpay(EcpayLogisticsClient),
    Fake(FakeLogisticsGateway),
}

impl LogisticsGateway {
    pub fn ecpay() -> anyhow::Result<Self> {
        Ok(Self::Ecpay(EcpayLogisticsClient::new()?))
    }

    pub async fn post_form(
        &self,
        url: &str,
        fields: &BTreeMap<String, String>,
    ) -> anyhow::Result<String> {
        match self {
            Self::Ecpay(client) => client.post_form(url, fields).await,
            Self::Fake(fake) => {
                fake.calls
                    .lock()
                    .expect("fake logistics calls")
                    .push((url.to_string(), fields.clone()));
                let next = fake.queued.lock().expect("fake logistics queue").pop_front();
                match next {
                    Some(Ok(body)) => Ok(body),
                    Some(Err(msg)) => anyhow::bail!("{msg}"),
                    None => Ok(fake_success_body(fields)),
                }
            }
        }
    }
}
```

- [ ] **Step 5: 接上 AppState、main.rs、測試共用模組**

`api/src/state.rs`：

```rust
use std::sync::Arc;

use sqlx::PgPool;

use crate::{
    config::Config,
    ecpay::{invoice::InvoiceGateway, logistics::LogisticsGateway},
    mail::Mailer,
};

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub config: Arc<Config>,
    /// Email 出口（SMTP／只記 log／測試擷取）
    pub mailer: Arc<Mailer>,
    /// 電子發票出口（綠界／測試 Fake）
    pub invoices: Arc<InvoiceGateway>,
    /// 物流建單出口（綠界／測試 Fake）
    pub logistics: Arc<LogisticsGateway>,
}
```

`api/src/main.rs` 建立 state 的地方改成：

```rust
    let mailer = Arc::new(mail::Mailer::from_config(&config)?);
    let invoices = Arc::new(ecpay::invoice::InvoiceGateway::ecpay(&config.ecpay)?);
    let logistics = Arc::new(ecpay::logistics::LogisticsGateway::ecpay()?);
    let state = AppState {
        db,
        config: config.clone(),
        mailer,
        invoices,
        logistics,
    };
```

`api/tests/common/mod.rs`：import 加 `ecpay::logistics::{FakeLogisticsGateway, LogisticsGateway}`，`state()` 的 `AppState { … }` 加 `logistics: Arc::new(LogisticsGateway::Fake(FakeLogisticsGateway::default())),`；檔案最後加：

```rust
/// 測試裡的假物流閘道（看 calls()、排下一次的回應）
pub fn fake_logistics(state: &AppState) -> &FakeLogisticsGateway {
    match &*state.logistics {
        LogisticsGateway::Fake(fake) => fake,
        LogisticsGateway::Ecpay(_) => panic!("測試的 AppState 要用 LogisticsGateway::Fake"),
    }
}
```

- [ ] **Step 6: 跑單元測試、整合測試、fmt、clippy**

Run: `export PATH="$HOME/.cargo/bin:$PATH" && cargo test --manifest-path api/Cargo.toml --lib`（`run_in_background: true`）
Expected: 全綠，含 `ecpay::logistics::tests` 10 個與 `domain::shipments::tests` 2 個。
Run: `DATABASE_URL=postgres://dog_shop:dog_shop@localhost:5435/dog_shop cargo test --manifest-path api/Cargo.toml`（前面加 `export PATH=…`；`run_in_background: true`）
Expected: 全綠（AppState 多欄位後所有整合測試都要能編譯）。
Run: fmt check 與 clippy（同 Task 1 Step 9）。
Expected: 乾淨。

- [ ] **Step 7: Commit**

```bash
git add api/src/ecpay/logistics.rs api/src/ecpay/mod.rs api/src/domain/shipments.rs api/src/domain/mod.rs api/src/state.rs api/src/main.rs api/tests/common/mod.rs
git commit -m "feat(api): 綠界物流純函式（地圖、建單、列印、通知解析、貨態對照）與可替換閘道"
```

---

### Task 3: 結帳選門市：`cvs-map`、`map-reply`、結帳頁按鈕、Playwright 超商路徑

**Files:**
- Create: `api/migrations/0004_cvs_map_requests.sql`
- Modify: `api/src/auth/tokens.rs`（`generate_short_token`）
- Modify: `api/src/domain/cvs_stores.rs`（`MAP_REQUEST_TTL_MINUTES`、`STORE_TTL_MINUTES`、`insert_map_request`、`take_map_request`）
- Create: `api/src/routes/ecpay_callback.rs`（回呼共用工具）
- Modify: `api/src/routes/ecpay_payment.rs`（改用共用工具）
- Modify: `api/src/routes/checkout.rs`（`POST /api/checkout/cvs-map`）
- Create: `api/src/routes/ecpay_logistics.rs`（`map-reply`；Task 4 再加兩條）
- Modify: `api/src/routes/mod.rs`（`pub mod ecpay_callback; pub mod ecpay_logistics;`）、`api/src/app.rs:45`（`.merge(routes::ecpay_logistics::router())` 放在 `ecpay_payment` 之後）
- Modify: `api/src/jobs/scheduled.rs:127-`（purge 加 `cvs_map_requests`）
- Create: `api/tests/ecpay_logistics.rs`
- Modify: `web/src/lib/checkout.ts`、`web/src/lib/checkout.test.ts`（`isMobileDevice`、`storeErrorMessage`）
- Modify: `web/src/routes/checkout/+page.server.ts`（`storeError`）、`web/src/routes/checkout/+page.svelte`（按鈕、換超商清門市、錯誤 toast）
- Modify: `web/e2e/checkout.spec.ts`（第二條測試）

**Interfaces:**
- Consumes: `logistics::map_form`／`parse_map_reply`／`is_sub_type`（Task 2）；`cvs_stores::{CvsStore, insert, get_valid}`；`GET /api/checkout/cvs-store/{token}`（已存在）；前端 `postToEcpay`、`api()`、`CHECKOUT_STORAGE_KEY` 草稿。
- Produces：`POST /api/checkout/cvs-map` body `{ "sub_type": "UNIMARTC2C" | "FAMIC2C" | "HILIFEC2C", "device": 0 | 1 }` → 200 `CheckoutForm { action, fields }`（`fields.ExtraData` = `fields.MerchantTradeNo` = 20 碼 token）；`POST /api/ecpay/logistics/map-reply`（form-urlencoded）→ 303 `Location: {PUBLIC_BASE_URL}/checkout?store=<token>`，失敗 303 `…/checkout?store_error=expired|invalid|server`；`tokens::generate_short_token() -> String`（20 碼 `[A-Za-z0-9]`）；`cvs_stores::insert_map_request(db, token, sub_type, ttl_minutes)`、`cvs_stores::take_map_request(db, token) -> Result<Option<String>, sqlx::Error>`（回登記的 sub_type 並刪除；不存在或過期 None）；`routes::ecpay_callback::{MAX_CALLBACK_FIELDS, parse_form, text, callback_error, server_error}`（`pub`，Task 4 用）；前端 `isMobileDevice(userAgent: string): boolean`、`storeErrorMessage(code: string | null): string | null`。

- [ ] **Step 1: migration、token、cvs_stores**

`api/migrations/0004_cvs_map_requests.sql`：

```sql
-- 按「選擇門市」時先登記一筆；綠界地圖回傳時核對 token 存在且未過期，核對成功就刪掉（單次使用）
-- （規格 §11、與規格不同之處 34）。過期的由每日 purge 清
CREATE TABLE cvs_map_requests (
    token      text PRIMARY KEY,
    sub_type   text NOT NULL CHECK (sub_type IN ('UNIMARTC2C', 'FAMIC2C', 'HILIFEC2C')),
    created_at timestamptz NOT NULL DEFAULT now(),
    expires_at timestamptz NOT NULL
);
```

`api/src/auth/tokens.rs` 加（import 補 `use rand::Rng;`）：

```rust
/// 綠界 ExtraData 只有 20 字：20 碼英數 token（62^20 ≈ 2^119，猜不到）；門市選擇用（規格 §11）
pub const SHORT_TOKEN_LEN: usize = 20;

pub fn generate_short_token() -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::rng();
    (0..SHORT_TOKEN_LEN)
        .map(|_| ALPHABET[rng.random_range(0..ALPHABET.len())] as char)
        .collect()
}
```

同檔的 `mod tests` 加：

```rust
    #[test]
    fn short_token_is_20_alphanumeric_and_random() {
        let a = generate_short_token();
        let b = generate_short_token();
        assert_eq!(a.len(), 20);
        assert!(a.chars().all(|c| c.is_ascii_alphanumeric()));
        assert_ne!(a, b);
    }
```

`api/src/domain/cvs_stores.rs` 加（`insert` 之後）：

```rust
/// 登記與門市選擇都是 1 小時（規格 §3）
pub const MAP_REQUEST_TTL_MINUTES: i64 = 60;
pub const STORE_TTL_MINUTES: i64 = 60;

/// 按「選擇門市」時先登記（與規格不同之處 34）
pub async fn insert_map_request(
    db: &PgPool,
    token: &str,
    sub_type: &str,
    ttl_minutes: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO cvs_map_requests (token, sub_type, expires_at) VALUES ($1, $2, $3)")
        .bind(token)
        .bind(sub_type)
        .bind(Utc::now() + Duration::minutes(ttl_minutes))
        .execute(db)
        .await?;
    Ok(())
}

/// 用掉一筆有效登記（刪掉它，單次使用），回登記時的超商種類；不存在或過期回 None
pub async fn take_map_request(db: &PgPool, token: &str) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar(
        "DELETE FROM cvs_map_requests WHERE token = $1 AND expires_at > now() RETURNING sub_type",
    )
    .bind(token)
    .fetch_optional(db)
    .await
}
```

`api/src/jobs/scheduled.rs` 的 `purge_expired` 在 `cvs_store_selections` 那句之後加：

```rust
    n += sqlx::query("DELETE FROM cvs_map_requests WHERE expires_at <= now()")
        .execute(db)
        .await?
        .rows_affected();
```

並把函式上方的註解改成「每天清理：過期 session、過期或用過的重設 token、過期門市選擇與門市登記、30 天前做完的 job」。

- [ ] **Step 2: 寫整合測試（先失敗）**

`api/tests/ecpay_logistics.rs`：

```rust
mod common;

use axum::{
    Router,
    body::Body,
    http::{HeaderMap, Request, StatusCode, header},
};
use dog_shop_api::{domain::cvs_stores, ecpay::mac, jobs::scheduled};
use serde_json::{Value, json};
use sqlx::PgPool;

/// stage 的物流憑證（Config::for_tests 用同一組）
const KEY: &str = "XBERn1YOvpM9nfZc";
const IV: &str = "h1ONHk4P4yqbl5LK";

fn f(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

/// 模擬綠界（伺服器或買家瀏覽器）：form-urlencoded POST，沒有 Origin、沒有 X-Requested-With
async fn ecpay_post_raw(
    app: &Router,
    path: &str,
    fields: Vec<(String, String)>,
) -> (StatusCode, String, HeaderMap) {
    let body = form_urlencoded::Serializer::new(String::new())
        .extend_pairs(fields.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .finish();
    let request = Request::builder()
        .method("POST")
        .uri(path)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from(body))
        .unwrap();
    let (status, value, headers) = common::send(app, request).await;
    let text = match value {
        Value::String(s) => s,
        Value::Null => String::new(),
        other => other.to_string(),
    };
    (status, text, headers)
}

/// 算好 MD5 CheckMacValue 再送（狀態通知、更新門市通知用）
async fn ecpay_post(
    app: &Router,
    path: &str,
    mut fields: Vec<(String, String)>,
) -> (StatusCode, String) {
    let mac = mac::check_mac_value_md5(KEY, IV, &fields);
    fields.push(("CheckMacValue".to_string(), mac));
    let (status, text, _) = ecpay_post_raw(app, path, fields).await;
    (status, text)
}

/// 買家按「選擇門市」：回綠界表單裡的 token（ExtraData）
async fn map_token(app: &Router, sub_type: &str) -> String {
    let (status, form, _) = common::send(
        app,
        common::req(
            "POST",
            "/api/checkout/cvs-map",
            None,
            Some(json!({ "sub_type": sub_type, "device": 1 })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{form}");
    form["fields"]["ExtraData"].as_str().unwrap().to_string()
}

fn map_reply_fields(token: &str, sub_type: &str) -> Vec<(String, String)> {
    f(&[
        ("MerchantID", "2000933"),
        ("MerchantTradeNo", token),
        ("LogisticsSubType", sub_type),
        ("CVSStoreID", "006598"),
        ("CVSStoreName", "全家測試店"),
        ("CVSAddress", "台北市中正區重慶南路一段 122 號"),
        ("CVSTelephone", "0223456789"),
        ("CVSOutSide", "0"),
        ("ExtraData", token),
    ])
}

fn location(headers: &HeaderMap) -> String {
    headers
        .get(header::LOCATION)
        .expect("location")
        .to_str()
        .unwrap()
        .to_string()
}

#[sqlx::test(migrations = "./migrations")]
async fn cvs_map_returns_form_and_registers_token(pool: PgPool) {
    let app = common::app(pool.clone());
    let (status, form, _) = common::send(
        &app,
        common::req(
            "POST",
            "/api/checkout/cvs-map",
            None,
            Some(json!({ "sub_type": "FAMIC2C", "device": 1 })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{form}");
    assert_eq!(
        form["action"],
        "https://logistics-stage.ecpay.com.tw/Express/map"
    );
    let fields = &form["fields"];
    assert_eq!(fields["MerchantID"], "2000933");
    assert_eq!(fields["LogisticsType"], "CVS");
    assert_eq!(fields["LogisticsSubType"], "FAMIC2C");
    assert_eq!(fields["IsCollection"], "N");
    assert_eq!(fields["Device"], "1");
    assert_eq!(
        fields["ServerReplyURL"],
        "http://localhost:5173/api/ecpay/logistics/map-reply"
    );
    let token = fields["ExtraData"].as_str().unwrap();
    assert_eq!(token.len(), 20);
    assert!(token.chars().all(|c| c.is_ascii_alphanumeric()));
    assert_eq!(fields["MerchantTradeNo"], token);
    assert!(fields.get("CheckMacValue").is_none(), "電子地圖不需簽章");

    let registered: Option<String> =
        sqlx::query_scalar("SELECT sub_type FROM cvs_map_requests WHERE token = $1")
            .bind(token)
            .fetch_optional(&pool)
            .await
            .unwrap();
    assert_eq!(registered.as_deref(), Some("FAMIC2C"));
}

#[sqlx::test(migrations = "./migrations")]
async fn cvs_map_rejects_bad_sub_type(pool: PgPool) {
    let app = common::app(pool);
    let (status, body, _) = common::send(
        &app,
        common::req(
            "POST",
            "/api/checkout/cvs-map",
            None,
            Some(json!({ "sub_type": "TCAT" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "VALIDATION");
    assert!(body["error"]["details"]["fields"]["sub_type"].is_string());
}

#[sqlx::test(migrations = "./migrations")]
async fn map_reply_stores_selection_redirects_and_is_single_use(pool: PgPool) {
    let app = common::app(pool.clone());
    let token = map_token(&app, "FAMIC2C").await;

    let (status, _, headers) = ecpay_post_raw(
        &app,
        "/api/ecpay/logistics/map-reply",
        map_reply_fields(&token, "FAMIC2C"),
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        location(&headers),
        format!("http://localhost:5173/checkout?store={token}")
    );

    let (status, store, _) = common::send(
        &app,
        common::req("GET", &format!("/api/checkout/cvs-store/{token}"), None, None),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{store}");
    assert_eq!(store["sub_type"], "FAMIC2C");
    assert_eq!(store["store_id"], "006598");
    assert_eq!(store["store_name"], "全家測試店");
    assert_eq!(store["store_address"], "台北市中正區重慶南路一段 122 號");
    assert_eq!(store["store_phone"], "0223456789");

    // 登記已用掉：同一個 token 再回傳一次就當過期
    assert!(
        cvs_stores::take_map_request(&pool, &token)
            .await
            .unwrap()
            .is_none()
    );
    let (status, _, headers) = ecpay_post_raw(
        &app,
        "/api/ecpay/logistics/map-reply",
        map_reply_fields(&token, "FAMIC2C"),
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert_eq!(
        location(&headers),
        "http://localhost:5173/checkout?store_error=expired"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn map_reply_without_phone_is_fine_for_unimart(pool: PgPool) {
    let app = common::app(pool.clone());
    let token = map_token(&app, "UNIMARTC2C").await;
    let mut fields = map_reply_fields(&token, "UNIMARTC2C");
    fields.retain(|(k, _)| k != "CVSTelephone");
    let (status, _, headers) =
        ecpay_post_raw(&app, "/api/ecpay/logistics/map-reply", fields).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert!(location(&headers).ends_with(&format!("?store={token}")));
    let store = cvs_stores::get_valid(&pool, &token).await.unwrap().unwrap();
    assert_eq!(store.store_phone, "");
}

#[sqlx::test(migrations = "./migrations")]
async fn map_reply_rejects_unknown_expired_mismatched_or_incomplete(pool: PgPool) {
    let app = common::app(pool.clone());

    // 沒登記過的 token
    let (status, _, headers) = ecpay_post_raw(
        &app,
        "/api/ecpay/logistics/map-reply",
        map_reply_fields("NoSuchTokenAbcdefghij", "FAMIC2C"),
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    assert!(location(&headers).ends_with("/checkout?store_error=expired"));

    // 登記過但過期
    cvs_stores::insert_map_request(&pool, "ExpiredTokenAbcdefgh", "FAMIC2C", -1)
        .await
        .unwrap();
    let (_, _, headers) = ecpay_post_raw(
        &app,
        "/api/ecpay/logistics/map-reply",
        map_reply_fields("ExpiredTokenAbcdefgh", "FAMIC2C"),
    )
    .await;
    assert!(location(&headers).ends_with("/checkout?store_error=expired"));

    // 超商種類與登記的不符
    let token = map_token(&app, "UNIMARTC2C").await;
    let (_, _, headers) = ecpay_post_raw(
        &app,
        "/api/ecpay/logistics/map-reply",
        map_reply_fields(&token, "FAMIC2C"),
    )
    .await;
    assert!(location(&headers).ends_with("/checkout?store_error=invalid"));
    assert!(
        cvs_stores::get_valid(&pool, &token).await.unwrap().is_none(),
        "不符就不寫門市"
    );

    // 欄位不全（沒有 CVSStoreID）
    let token = map_token(&app, "FAMIC2C").await;
    let mut fields = map_reply_fields(&token, "FAMIC2C");
    fields.retain(|(k, _)| k != "CVSStoreID");
    let (_, _, headers) = ecpay_post_raw(&app, "/api/ecpay/logistics/map-reply", fields).await;
    assert!(location(&headers).ends_with("/checkout?store_error=invalid"));
    assert!(
        cvs_stores::take_map_request(&pool, &token)
            .await
            .unwrap()
            .is_some(),
        "欄位不全時登記還在，買家可以再試"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn map_reply_ignores_csrf_headers_but_limits_body(pool: PgPool) {
    let app = common::app(pool);
    // 超過 64 KB 的 body 直接被 DefaultBodyLimit 擋下
    let huge = "x".repeat(70 * 1024);
    let (status, _, _) = ecpay_post_raw(
        &app,
        "/api/ecpay/logistics/map-reply",
        f(&[("ExtraData", &huge)]),
    )
    .await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
}

#[sqlx::test(migrations = "./migrations")]
async fn purge_removes_expired_map_requests(pool: PgPool) {
    cvs_stores::insert_map_request(&pool, "OldTokenAbcdefghijkl", "FAMIC2C", -5)
        .await
        .unwrap();
    cvs_stores::insert_map_request(&pool, "NewTokenAbcdefghijkl", "FAMIC2C", 60)
        .await
        .unwrap();
    scheduled::purge_expired(&pool).await.unwrap();
    let left: Vec<String> = sqlx::query_scalar("SELECT token FROM cvs_map_requests ORDER BY token")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(left, vec!["NewTokenAbcdefghijkl".to_string()]);
}
```

Run: `DATABASE_URL=postgres://dog_shop:dog_shop@localhost:5435/dog_shop cargo test --manifest-path api/Cargo.toml --test ecpay_logistics`（前面加 `export PATH=…`；`run_in_background: true`）
Expected: 編譯失敗（`cvs_stores::take_map_request` 等不存在）；補完 Step 1 後改成 404／方法不存在的失敗。

- [ ] **Step 3: 回呼共用工具與 ecpay_payment.rs 改用**

`api/src/routes/ecpay_callback.rs`：

```rust
//! 綠界回呼共用的小工具：付款（ecpay_payment.rs）與物流（ecpay_logistics.rs）都用。
//! 回純文字：`1|OK`、`0|CheckMacValue Error`（400）、`0|Missing Field`（400）、`0|Server Error`（500，讓綠界重送）
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};

use crate::{ecpay::aio::CallbackError, error::ApiError};

/// 綠界回呼的欄位筆數上限：`ReturnURL` 最多 30 幾個欄位，100 很寬鬆（計畫 3 修正波 #1）
pub const MAX_CALLBACK_FIELDS: usize = 100;

/// 綠界送 application/x-www-form-urlencoded；不用 axum::Form，因為要拿到所有欄位重算簽章
pub fn parse_form(body: &str) -> Vec<(String, String)> {
    form_urlencoded::parse(body.as_bytes())
        .into_owned()
        .collect()
}

pub fn text(status: StatusCode, body: &'static str) -> Response {
    (status, body).into_response()
}

pub fn callback_error(path: &'static str, err: CallbackError) -> Response {
    tracing::warn!(path, error = %err, "綠界回呼簽章或欄位錯誤");
    match err {
        CallbackError::BadMac => text(StatusCode::BAD_REQUEST, "0|CheckMacValue Error"),
        CallbackError::Missing(_) => text(StatusCode::BAD_REQUEST, "0|Missing Field"),
    }
}

pub fn server_error(path: &'static str, err: ApiError) -> Response {
    tracing::error!(path, error = %err, "綠界回呼處理失敗，回 500 讓綠界重送");
    text(StatusCode::INTERNAL_SERVER_ERROR, "0|Server Error")
}
```

`api/src/routes/ecpay_payment.rs`：刪掉自己的 `MAX_CALLBACK_FIELDS`、`parse_form`、`text`、`callback_error`、`server_error`，import 改成：

```rust
use axum::{
    Router,
    extract::{DefaultBodyLimit, State},
    http::StatusCode,
    response::Response,
    routing::post,
};

use crate::{
    domain::payments::{self, InfoOutcome, ReturnOutcome},
    ecpay::aio::{self, CallbackError},
    routes::ecpay_callback::{MAX_CALLBACK_FIELDS, callback_error, parse_form, server_error, text},
    state::AppState,
};
```

其餘 handler 內容不變。`api/src/routes/mod.rs` 加 `pub mod ecpay_callback;` 與 `pub mod ecpay_logistics;`（字母順序）。

- [ ] **Step 4: `POST /api/checkout/cvs-map`**

`api/src/routes/checkout.rs` 整檔改成：

```rust
//! 結帳輔助：門市選擇（規格 §7 第 2 點、§10）
use axum::{Json, Router, extract::State, routing::get};
use serde::Deserialize;

use crate::{
    auth::tokens,
    domain::cvs_stores::{self, CvsStore},
    ecpay::{aio::CheckoutForm, logistics},
    error::{ApiError, ApiResult},
    extract::{AppJson, AppPath},
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/checkout/cvs-map", axum::routing::post(cvs_map))
        .route("/api/checkout/cvs-store/{token}", get(cvs_store))
}

#[derive(Deserialize)]
pub struct CvsMapInput {
    pub sub_type: String,
    /// 1 = 手機（綠界會用手機版地圖）；其他當桌機
    #[serde(default)]
    pub device: i32,
}

/// 登記一個 token，回送往綠界電子地圖的表單（頂層導頁，不用 iframe）
async fn cvs_map(
    State(state): State<AppState>,
    AppJson(input): AppJson<CvsMapInput>,
) -> ApiResult<Json<CheckoutForm>> {
    let sub_type = input.sub_type.trim().to_string();
    if !logistics::is_sub_type(&sub_type) {
        return Err(ApiError::field("sub_type", "請選擇超商"));
    }
    let token = tokens::generate_short_token();
    cvs_stores::insert_map_request(
        &state.db,
        &token,
        &sub_type,
        cvs_stores::MAP_REQUEST_TTL_MINUTES,
    )
    .await?;
    Ok(Json(logistics::map_form(
        &state.config.ecpay,
        &state.config.public_base_url,
        &token,
        &sub_type,
        input.device == 1,
    )))
}

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

- [ ] **Step 5: `map-reply` 路由**

`api/src/routes/ecpay_logistics.rs`：

```rust
//! 綠界物流回呼（規格 §7 第 2 點、§8.3、§11、§14）。map-reply 是買家瀏覽器 POST 過來的（沒有簽章，
//! 靠 token）；status 與 store-update 是綠界伺服器 POST（MD5 簽章）— Task 4。
//! 路徑前綴 `/api/ecpay/` 已在 auth/csrf.rs 的 EXEMPT_PREFIXES 內
use axum::{
    Router,
    extract::{DefaultBodyLimit, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::post,
};

use crate::{
    domain::cvs_stores::{self, CvsStore},
    ecpay::logistics,
    routes::ecpay_callback::{MAX_CALLBACK_FIELDS, parse_form},
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/ecpay/logistics/map-reply", post(map_reply))
        // 回呼一律 < 2 KB；後掛的 layer 在內層，覆蓋 app.rs 給圖片上傳訂的 10 MB（計畫 3 修正波 #1）
        .layer(DefaultBodyLimit::max(64 * 1024))
}

fn redirect(location: String) -> Response {
    (StatusCode::SEE_OTHER, [(header::LOCATION, location)]).into_response()
}

/// 綠界地圖選完，用買家瀏覽器把門市 POST 回來。失敗不回 400 純文字（買家會卡在錯誤頁），
/// 改 303 回結帳頁帶 store_error（與規格不同之處 37）。token 不印進 log
async fn map_reply(State(state): State<AppState>, body: String) -> Response {
    let base = state.config.public_base_url.as_str();
    let fail = |reason: &str| redirect(format!("{base}/checkout?store_error={reason}"));
    let params = parse_form(&body);
    if params.len() > MAX_CALLBACK_FIELDS {
        tracing::warn!("map-reply 欄位太多");
        return fail("invalid");
    }
    let reply = match logistics::parse_map_reply(&params) {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, "map-reply 欄位不全");
            return fail("invalid");
        }
    };
    let registered = match cvs_stores::take_map_request(&state.db, &reply.token).await {
        Ok(Some(sub_type)) => sub_type,
        Ok(None) => {
            tracing::warn!("map-reply 的 token 不存在或已過期");
            return fail("expired");
        }
        Err(e) => {
            tracing::error!(error = %e, "map-reply 查 token 失敗");
            return fail("server");
        }
    };
    if registered != reply.sub_type {
        tracing::warn!(registered, replied = %reply.sub_type, "map-reply 的超商種類與登記不符");
        return fail("invalid");
    }
    let store = CvsStore {
        token: reply.token,
        sub_type: reply.sub_type,
        store_id: reply.store_id,
        store_name: reply.store_name,
        store_address: reply.store_address,
        store_phone: reply.store_phone,
    };
    if let Err(e) = cvs_stores::insert(&state.db, &store, cvs_stores::STORE_TTL_MINUTES).await {
        tracing::error!(error = %e, "map-reply 寫門市失敗");
        return fail("server");
    }
    tracing::info!(sub_type = %store.sub_type, store_id = %store.store_id, "map-reply 門市已存");
    redirect(format!("{base}/checkout?store={}", store.token))
}
```

`api/src/app.rs` 的 `.merge(routes::ecpay_payment::router())` 之後加 `.merge(routes::ecpay_logistics::router())`。

- [ ] **Step 6: 跑 Rust 測試確認通過**

Run: `DATABASE_URL=… cargo test --manifest-path api/Cargo.toml --test ecpay_logistics --test ecpay_payment --test jobs_worker`（`run_in_background: true`）
Expected: 全綠（ecpay_logistics 7 個；ecpay_payment 改用共用工具後行為不變）。
Run: 全部 `cargo test`、fmt、clippy。
Expected: 全綠、乾淨。

- [ ] **Step 7: 前端純函式與測試**

`web/src/lib/checkout.ts` 最後加：

```ts
/** 綠界地圖的 Device：手機 1、桌機 0（規格 §8.3） */
export function isMobileDevice(userAgent: string): boolean {
	return /Android|iPhone|iPad|iPod|Mobile/i.test(userAgent);
}

/** map-reply 失敗時 303 回 /checkout?store_error=… 的文案（與規格不同之處 37） */
export const STORE_ERROR_MESSAGES: Record<string, string> = {
	expired: '門市選擇已逾時（超過 1 小時），請重新選擇門市',
	invalid: '綠界回傳的門市資料不完整，請重新選擇門市',
	server: '暫時無法儲存門市，請稍後再試'
};

export function storeErrorMessage(code: string | null): string | null {
	if (!code) return null;
	return STORE_ERROR_MESSAGES[code] ?? '門市選擇失敗，請重新選擇門市';
}
```

`web/src/lib/checkout.test.ts`：把 `isMobileDevice`、`storeErrorMessage` 併進既有的 `from './checkout'` import，檔案最後加：

```ts
describe('isMobileDevice', () => {
	it('手機 UA 回 true、桌機回 false', () => {
		expect(isMobileDevice('Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15 Mobile/15E148')).toBe(true);
		expect(isMobileDevice('Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 Mobile Safari/537.36')).toBe(true);
		expect(isMobileDevice('Mozilla/5.0 (Macintosh; Intel Mac OS X 14_0) AppleWebKit/605.1.15 Safari/605.1.15')).toBe(false);
	});
});

describe('storeErrorMessage', () => {
	it('已知原因有專屬文案、未知原因有通用文案、沒有原因回 null', () => {
		expect(storeErrorMessage('expired')).toContain('逾時');
		expect(storeErrorMessage('invalid')).toContain('不完整');
		expect(storeErrorMessage('server')).toContain('稍後');
		expect(storeErrorMessage('weird')).toBe('門市選擇失敗，請重新選擇門市');
		expect(storeErrorMessage(null)).toBeNull();
	});
});
```

Run: `pnpm -C web test`
Expected: 原本 27 個 + 2 個全過。

- [ ] **Step 8: 結帳頁接上「選擇門市」**

`web/src/routes/checkout/+page.server.ts` 的 `return` 改成：

```ts
	return { addresses, store, storeError: event.url.searchParams.get('store_error'), title: '結帳' };
```

`web/src/routes/checkout/+page.svelte`：

1. import 那行改成 `import { CHECKOUT_STORAGE_KEY, defaultForm, isMobileDevice, storeErrorMessage, toOrderInput, validateForm, type CheckoutForm } from '$lib/checkout';`
2. `let selectedAddressId = $state('');` 之後加 `let pickingStore = $state(false);`
3. `onMount` 改成：

```ts
	onMount(() => {
		restoreDraft();
		if (store) {
			form.shipping_method = 'cvs';
			form.cvs_sub_type = store.sub_type;
		}
		if (!enabledPayments.includes(form.payment_method)) form.payment_method = enabledPayments[0] ?? 'credit';
		const storeError = storeErrorMessage(data.storeError);
		if (storeError) {
			form.shipping_method = 'cvs';
			toast.show(storeError, 5000);
		}
	});
```

4. `useAddress` 之後加：

```ts
	/** 去綠界電子地圖選門市（規格 §7 第 2 點）：草稿已由上面的 $effect 存進 sessionStorage；頂層導頁離開本頁 */
	async function pickStore() {
		if (pickingStore) return;
		pickingStore = true;
		try {
			const mapForm = await api<EcpayForm>('/api/checkout/cvs-map', {
				method: 'POST',
				body: JSON.stringify({ sub_type: form.cvs_sub_type, device: isMobileDevice(navigator.userAgent) ? 1 : 0 })
			});
			postToEcpay(mapForm);
		} catch (err) {
			toast.show(err instanceof ApiError ? err.message : '無法開啟門市地圖，請再試一次');
			pickingStore = false;
		}
	}

	/** 換了超商就把上次選的門市清掉（門市屬於某一家超商） */
	function onSubTypeChange() {
		if (store && store.sub_type !== form.cvs_sub_type) store = null;
	}
```

5. 超商種類的 radio 加 `onchange={onSubTypeChange}`：

```svelte
					<div class="flex flex-wrap gap-4 text-sm">
						{#each cvsTypes as t (t)}
							<label class="flex items-center gap-2"><input type="radio" bind:group={form.cvs_sub_type} value={t} onchange={onSubTypeChange} /> {CVS_LABELS[t]}</label>
						{/each}
					</div>
```

6. 停用的按鈕換成：

```svelte
					<button type="button" onclick={pickStore} disabled={pickingStore} class="rounded border border-gray-300 px-3 py-2 text-sm disabled:opacity-50">
						{pickingStore ? '前往綠界地圖…' : store ? '重新選擇門市' : '選擇門市'}
					</button>
```

Run: `pnpm -C web check`
Expected: 0 錯誤 0 警告。

- [ ] **Step 9: Playwright 超商路徑**

`web/e2e/checkout.spec.ts` 最後加第二條測試：

```ts
test('超商取貨：選擇門市 → 綠界地圖表單 → 門市回傳 → 結帳頁顯示門市 → 送出訂單', async ({ page, request }) => {
	const product = await seedProduct(request);

	await page.goto(`/products/${product.slug}`);
	await expect(async () => {
		await page.getByRole('button', { name: '加入購物車' }).click();
		await expect(page.getByText('已加入購物車')).toBeVisible({ timeout: 2_000 });
	}).toPass({ timeout: 15_000 });

	await page.goto('/checkout');
	await page.getByLabel(/^Email/).fill('e2e-cvs@test.local');
	await page.getByLabel('收件人').fill('王小明');
	await page.getByLabel('手機', { exact: true }).fill('0912345678');
	await page.getByRole('radio', { name: /超商取貨/ }).check();
	await page.getByRole('radio', { name: '全家' }).check();

	// 攔截送往綠界電子地圖的頂層表單 POST（規格 §8.3：測試環境本來就不顯示地圖）
	await page.route('https://logistics-stage.ecpay.com.tw/**', (route) =>
		route.fulfill({
			status: 200,
			contentType: 'text/html; charset=utf-8',
			body: '<!doctype html><title>ECPay map stub</title><p>ECPay map stub</p>'
		})
	);
	const mapRequest = page.waitForRequest((r) => r.url().includes('/Express/map') && r.method() === 'POST');
	await page.getByRole('button', { name: '選擇門市', exact: true }).click();
	const mapFields = new URLSearchParams((await mapRequest).postData() ?? '');
	expect(mapFields.get('MerchantID')).toBe('2000933');
	expect(mapFields.get('LogisticsType')).toBe('CVS');
	expect(mapFields.get('LogisticsSubType')).toBe('FAMIC2C');
	expect(mapFields.get('IsCollection')).toBe('N');
	expect(mapFields.get('ServerReplyURL')).toMatch(/\/api\/ecpay\/logistics\/map-reply$/);
	expect(mapFields.has('CheckMacValue')).toBe(false);
	const token = mapFields.get('ExtraData') ?? '';
	expect(token).toMatch(/^[A-Za-z0-9]{20}$/);
	expect(mapFields.get('MerchantTradeNo')).toBe(token);
	await expect(page.getByText('ECPay map stub')).toBeVisible();

	// 模擬綠界把門市 POST 回 map-reply（form-urlencoded、沒有我們的 CSRF header）；303 不要自動跟
	const reply = await request.post(`${API}/api/ecpay/logistics/map-reply`, {
		maxRedirects: 0,
		form: {
			MerchantID: '2000933',
			MerchantTradeNo: token,
			LogisticsSubType: 'FAMIC2C',
			CVSStoreID: '006598',
			CVSStoreName: '全家測試店',
			CVSAddress: '台北市中正區重慶南路一段 122 號',
			CVSTelephone: '0223456789',
			CVSOutSide: '0',
			ExtraData: token
		}
	});
	expect(reply.status()).toBe(303);
	const location = reply.headers()['location'] ?? '';
	expect(location).toMatch(new RegExp(`/checkout\\?store=${token}$`));

	// 回到結帳頁：門市顯示出來、sessionStorage 的草稿還原（同一個分頁、同一個 origin）
	await page.goto(location);
	await expect(page.getByText('全家測試店')).toBeVisible();
	await expect(page.getByLabel(/^Email/)).toHaveValue('e2e-cvs@test.local');
	await expect(page.getByRole('radio', { name: '全家' })).toBeChecked();
	await expect(page.getByText('總計')).toBeVisible();

	// 送出訂單 → 攔到送往綠界金流的表單；訂單頁顯示門市
	await page.route('https://payment-stage.ecpay.com.tw/**', (route) =>
		route.fulfill({ status: 200, contentType: 'text/html; charset=utf-8', body: '<!doctype html><title>ECPay stub</title><p>ECPay stub</p>' })
	);
	const aioRequest = page.waitForRequest((r) => r.url().includes('/Cashier/AioCheckOut/V5') && r.method() === 'POST');
	await page.getByRole('button', { name: '送出訂單' }).click();
	const aio = new URLSearchParams((await aioRequest).postData() ?? '');
	expect(aio.get('MerchantTradeNo')).toMatch(/^DS\d{6}[A-Z0-9]{4}01$/);
	await page.goto(aio.get('ClientBackURL') ?? '');
	await expect(page.getByText('全家測試店')).toBeVisible();
	await expect(page.getByText('待付款')).toBeVisible();
});
```

啟動 api 與 web dev（各自 `run_in_background: true`；等 `curl -s http://localhost:8080/api/settings/public` 與 `curl -s http://localhost:5173/` 都有回應），跑：
Run: `pnpm -C web test:e2e`
Expected: 2 passed。跑完 `lsof -ti :8080`、`lsof -ti :5173` 取 pid 後 `kill`。

- [ ] **Step 10: 全部檢查與 commit**

Run: Rust 全部測試、fmt、clippy；`pnpm -C web check`、`pnpm -C web test`、`pnpm -C web build`。
Expected: 全綠、乾淨。

```bash
git add api/migrations/0004_cvs_map_requests.sql api/src/auth/tokens.rs api/src/domain/cvs_stores.rs api/src/routes/ecpay_callback.rs api/src/routes/ecpay_payment.rs api/src/routes/checkout.rs api/src/routes/ecpay_logistics.rs api/src/routes/mod.rs api/src/app.rs api/src/jobs/scheduled.rs api/tests/ecpay_logistics.rs web/src/lib/checkout.ts web/src/lib/checkout.test.ts web/src/routes/checkout/+page.server.ts web/src/routes/checkout/+page.svelte web/e2e/checkout.spec.ts
git commit -m "feat: 結帳選門市：cvs-map 登記 token、綠界地圖回傳 map-reply、結帳頁選擇門市與 Playwright 超商路徑"
```

---

### Task 4: 物流狀態回呼與更新門市通知

**Files:**
- Modify: `api/src/domain/shipments.rs`（`Shipment` 列、`SHIPMENT_COLUMNS`、`get_by_order`、`apply_status`、`apply_store_update`）
- Modify: `api/src/routes/ecpay_logistics.rs`（`status`、`store-update`）
- Modify: `api/tests/ecpay_logistics.rs`（狀態回呼測試）

**Interfaces:**
- Consumes: `logistics::{parse_status, parse_store_update, store_update_message, shipment_status_for, StatusNotification, StoreUpdate}`、`shipments::should_apply`（Task 2）；`ecpay_callback::*`（Task 3）；`orders::{STATUS_SHIPPED, STATUS_COMPLETED}`。
- Produces: `shipments::Shipment`（全部欄位，`raw` 不序列化）、`shipments::SHIPMENT_COLUMNS`、`shipments::get_by_order<'e, E: PgExecutor<'e>>(exec, order_id) -> Result<Option<Shipment>, sqlx::Error>`、`shipments::StatusOutcome::{Unknown, Updated { status: String, order_completed: bool }, Recorded}`、`shipments::apply_status(db, &StatusNotification) -> Result<StatusOutcome, ApiError>`、`shipments::apply_store_update(db, &StoreUpdate) -> Result<bool, ApiError>`（false = 找不到 `AllPayLogisticsID`）；`POST /api/ecpay/logistics/status` 與 `POST /api/ecpay/logistics/store-update`（純文字回應，同付款回呼的狀態碼規則）。`shipments.raw` 的形狀固定為 `{ "create_request": {...}, "create_response": {...}, "create_response_text": "...", "last_notification": {...}, "store_updates": [...] }`（key 各自由對應的步驟寫入、同 key 覆蓋、`store_updates` 追加）。

- [ ] **Step 1: 寫整合測試（先失敗）**

`api/tests/ecpay_logistics.rs` 最後加：

```rust
use uuid::Uuid;

fn cvs_order_body(variant: &str, token: &str) -> Value {
    json!({
        "items": [{ "variant_id": variant, "qty": 2 }],
        "email": "buyer@test.local",
        "recipient_name": "王小明",
        "recipient_phone": "0912345678",
        "shipping_method": "cvs",
        "cvs_store_token": token,
        "invoice": { "type": "personal", "carrier_type": "1" },
        "payment_method": "credit",
        "note": ""
    })
}

/// 建一筆超商訂單（訪客），用 SQL 直接標成已付款＋已出貨、物流單已建立（Task 6 的建單流程在這裡跳過）
async fn shipped_cvs_order(app: &Router, pool: &PgPool, mtn: &str, logistics_id: &str) -> Uuid {
    let (variant, _) = common::active_product(pool, "雞肉狗糧", 300, 5).await;
    let token = common::cvs_store_token(pool).await;
    let (status, created, _) = common::send(
        app,
        common::req(
            "POST",
            "/api/orders",
            None,
            Some(cvs_order_body(&variant.to_string(), &token)),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let id = Uuid::parse_str(created["order_id"].as_str().unwrap()).unwrap();
    sqlx::query(
        "UPDATE orders SET status = 'shipped', paid_at = now(), shipped_at = now() WHERE id = $1",
    )
    .bind(id)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE shipments SET status = 'created', ecpay_merchant_trade_no = $2, ecpay_logistics_id = $3 WHERE order_id = $1",
    )
    .bind(id)
    .bind(mtn)
    .bind(logistics_id)
    .execute(pool)
    .await
    .unwrap();
    id
}

fn status_fields(mtn: &str, logistics_id: &str, code: &str, msg: &str) -> Vec<(String, String)> {
    f(&[
        ("MerchantID", "2000933"),
        ("MerchantTradeNo", mtn),
        ("RtnCode", code),
        ("RtnMsg", msg),
        ("AllPayLogisticsID", logistics_id),
        ("LogisticsType", "CVS"),
        ("LogisticsSubType", "UNIMARTC2C"),
        ("GoodsAmount", "600"),
        ("UpdateStatusDate", "2026/09/10 18:30:00"),
        ("ReceiverName", "王小明"),
        ("ReceiverCellPhone", "0912345678"),
        ("CVSPaymentNo", "F0001234"),
        ("CVSValidationNo", "1234"),
    ])
}

/// (shipments.status, orders.status, last_status_code, last_status_msg, orders.completed_at 有無)
async fn snapshot(pool: &PgPool, id: Uuid) -> (String, String, Option<String>, Option<String>, bool) {
    sqlx::query_as(
        "SELECT s.status, o.status, s.last_status_code, s.last_status_msg, o.completed_at IS NOT NULL
         FROM shipments s JOIN orders o ON o.id = s.order_id WHERE s.order_id = $1",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .unwrap()
}

#[sqlx::test(migrations = "./migrations")]
async fn status_callback_rejects_bad_mac_and_missing_fields(pool: PgPool) {
    let app = common::app(pool.clone());
    let id = shipped_cvs_order(&app, &pool, "DS260908AAAAL01", "10035").await;

    let mut fields = status_fields("DS260908AAAAL01", "10035", "2030", "物流中心驗收成功");
    fields.push(("CheckMacValue".to_string(), "0".repeat(32)));
    let (status, text, _) = ecpay_post_raw(&app, "/api/ecpay/logistics/status", fields).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(text, "0|CheckMacValue Error");

    let mut fields = status_fields("DS260908AAAAL01", "10035", "2030", "x");
    fields.retain(|(k, _)| k != "RtnCode");
    let (status, text) = ecpay_post(&app, "/api/ecpay/logistics/status", fields).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(text, "0|Missing Field");

    let (s, o, code, _, _) = snapshot(&pool, id).await;
    assert_eq!((s.as_str(), o.as_str(), code), ("created", "shipped", None));
}

#[sqlx::test(migrations = "./migrations")]
async fn status_callback_unknown_trade_no_answers_0(pool: PgPool) {
    let app = common::app(pool);
    let (status, text) = ecpay_post(
        &app,
        "/api/ecpay/logistics/status",
        status_fields("DS000000ZZZZL01", "99999", "2030", "x"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(text, "0|Unknown MerchantTradeNo");
}

#[sqlx::test(migrations = "./migrations")]
async fn status_callback_moves_forward_completes_on_pickup_and_never_regresses(pool: PgPool) {
    let app = common::app(pool.clone());
    let id = shipped_cvs_order(&app, &pool, "DS260908BBBBL01", "10036").await;
    let post = |code: &'static str, msg: &'static str| {
        let app = app.clone();
        async move {
            ecpay_post(
                &app,
                "/api/ecpay/logistics/status",
                status_fields("DS260908BBBBL01", "10036", code, msg),
            )
            .await
        }
    };

    assert_eq!(post("2030", "物流中心驗收成功").await, (StatusCode::OK, "1|OK".to_string()));
    let (s, o, code, msg, _) = snapshot(&pool, id).await;
    assert_eq!((s.as_str(), o.as_str()), ("in_transit", "shipped"));
    assert_eq!(code.as_deref(), Some("2030"));
    assert_eq!(msg.as_deref(), Some("物流中心驗收成功"));

    assert_eq!(post("2073", "商品配達買家取貨門市").await.1, "1|OK");
    assert_eq!(snapshot(&pool, id).await.0, "arrived");

    // 晚到的物流中心通知：不倒退，但代碼照記
    assert_eq!(post("2030", "物流中心驗收成功").await.1, "1|OK");
    let (s, _, code, _, _) = snapshot(&pool, id).await;
    assert_eq!((s.as_str(), code.as_deref()), ("arrived", Some("2030")));

    assert_eq!(post("2067", "消費者成功取件").await.1, "1|OK");
    let (s, o, _, _, completed) = snapshot(&pool, id).await;
    assert_eq!((s.as_str(), o.as_str(), completed), ("picked_up", "completed", true));

    // 重複通知：no-op 仍回 1|OK；之後任何代碼都不改終態
    assert_eq!(post("2067", "消費者成功取件").await.1, "1|OK");
    assert_eq!(post("2074", "消費者七天未取").await.1, "1|OK");
    let (s, o, _, _, _) = snapshot(&pool, id).await;
    assert_eq!((s.as_str(), o.as_str()), ("picked_up", "completed"));

    let raw: Value = sqlx::query_scalar("SELECT raw FROM shipments WHERE order_id = $1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(raw["last_notification"]["RtnCode"], "2074");
    assert!(raw["last_notification"]["CheckMacValue"].is_string(), "原始 payload 整包存進 raw");
}

#[sqlx::test(migrations = "./migrations")]
async fn status_callback_returned_keeps_order_shipped_and_can_be_redelivered(pool: PgPool) {
    let app = common::app(pool.clone());
    let id = shipped_cvs_order(&app, &pool, "DS260908CCCCL01", "10037").await;
    let mut fields = status_fields("DS260908CCCCL01", "10037", "3018", "到店尚未取貨，簡訊通知取件");
    fields[6].1 = "FAMIC2C".to_string();
    ecpay_post(&app, "/api/ecpay/logistics/status", fields).await;
    assert_eq!(snapshot(&pool, id).await.0, "arrived");

    ecpay_post(
        &app,
        "/api/ecpay/logistics/status",
        status_fields("DS260908CCCCL01", "10037", "3020", "貨件未取退回物流中心"),
    )
    .await;
    let (s, o, _, _, completed) = snapshot(&pool, id).await;
    assert_eq!((s.as_str(), o.as_str(), completed), ("returned", "shipped", false));

    // 重新配達取件門市 → 回到 arrived
    ecpay_post(
        &app,
        "/api/ecpay/logistics/status",
        status_fields("DS260908CCCCL01", "10037", "2098", "包裹重新配達取件門市"),
    )
    .await;
    assert_eq!(snapshot(&pool, id).await.0, "arrived");
}

#[sqlx::test(migrations = "./migrations")]
async fn status_callback_unknown_code_only_records_and_falls_back_to_logistics_id(pool: PgPool) {
    let app = common::app(pool.clone());
    let id = shipped_cvs_order(&app, &pool, "DS260908DDDDL01", "10038").await;

    // 門市關轉店：不在對照表，只記代碼與訊息
    let (status, text) = ecpay_post(
        &app,
        "/api/ecpay/logistics/status",
        status_fields("DS260908DDDDL01", "10038", "2101", "門市關轉店"),
    )
    .await;
    assert_eq!((status, text.as_str()), (StatusCode::OK, "1|OK"));
    let (s, _, code, msg, _) = snapshot(&pool, id).await;
    assert_eq!((s.as_str(), code.as_deref(), msg.as_deref()), ("created", Some("2101"), Some("門市關轉店")));

    // MerchantTradeNo 對不上（例如綠界自己補的號）但 AllPayLogisticsID 對得上
    let (status, text) = ecpay_post(
        &app,
        "/api/ecpay/logistics/status",
        status_fields("SOMETHING_ELSE", "10038", "2030", "物流中心驗收成功"),
    )
    .await;
    assert_eq!((status, text.as_str()), (StatusCode::OK, "1|OK"));
    assert_eq!(snapshot(&pool, id).await.0, "in_transit");
}

#[sqlx::test(migrations = "./migrations")]
async fn store_update_records_message_without_changing_status(pool: PgPool) {
    let app = common::app(pool.clone());
    let id = shipped_cvs_order(&app, &pool, "DS260908EEEEL01", "10039").await;
    let fields = f(&[
        ("MerchantID", "2000933"),
        ("AllPayLogisticsID", "10039"),
        ("GoodsName", "雞肉狗糧"),
        ("GoodsAmount", "600"),
        ("StoreType", "01"),
        ("Status", "01"),
        ("StoreID", "991182"),
    ]);
    let (status, text) = ecpay_post(&app, "/api/ecpay/logistics/store-update", fields.clone()).await;
    assert_eq!((status, text.as_str()), (StatusCode::OK, "1|OK"));
    let (s, o, _, msg, _) = snapshot(&pool, id).await;
    assert_eq!((s.as_str(), o.as_str()), ("created", "shipped"));
    assert_eq!(msg.as_deref(), Some("取件門市異動：門市關轉店（991182）"));
    let raw: Value = sqlx::query_scalar("SELECT raw FROM shipments WHERE order_id = $1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(raw["store_updates"].as_array().unwrap().len(), 1);

    // 第二次追加、不覆蓋
    ecpay_post(&app, "/api/ecpay/logistics/store-update", fields).await;
    let raw: Value = sqlx::query_scalar("SELECT raw FROM shipments WHERE order_id = $1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(raw["store_updates"].as_array().unwrap().len(), 2);

    let mut bad = f(&[("MerchantID", "2000933"), ("AllPayLogisticsID", "nope"), ("StoreType", "01"), ("Status", "01"), ("StoreID", "1")]);
    let mac = mac::check_mac_value_md5(KEY, IV, &bad);
    bad.push(("CheckMacValue".to_string(), mac));
    let (status, text, _) = ecpay_post_raw(&app, "/api/ecpay/logistics/store-update", bad).await;
    assert_eq!((status, text.as_str()), (StatusCode::OK, "0|Unknown AllPayLogisticsID"));

    let (status, text, _) = ecpay_post_raw(
        &app,
        "/api/ecpay/logistics/store-update",
        f(&[("AllPayLogisticsID", "10039"), ("CheckMacValue", "bad")]),
    )
    .await;
    assert_eq!((status, text.as_str()), (StatusCode::BAD_REQUEST, "0|CheckMacValue Error"));
}
```

Run: `DATABASE_URL=… cargo test --manifest-path api/Cargo.toml --test ecpay_logistics`（`run_in_background: true`）
Expected: 新測試失敗（路由不存在 → 404／405）。

- [ ] **Step 2: `domain/shipments.rs` 加完整列與狀態套用**

在 `api/src/domain/shipments.rs` 的常數與 `should_apply` 之後（tests 之前）加：

```rust
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{Value, json};
use sqlx::{PgExecutor, PgPool};
use uuid::Uuid;

use crate::domain::orders::{STATUS_COMPLETED, STATUS_SHIPPED};
use crate::ecpay::logistics::{self, StatusNotification, StoreUpdate};
use crate::error::ApiError;

/// 後台與回呼用的完整 shipments 列（訂單頁的 orders::ShipmentRow 是子集合）。raw 不給前端
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Shipment {
    pub id: Uuid,
    pub order_id: Uuid,
    pub method: String,
    pub cvs_sub_type: Option<String>,
    pub cvs_store_id: Option<String>,
    pub cvs_store_name: Option<String>,
    pub cvs_store_address: Option<String>,
    pub cvs_store_phone: Option<String>,
    pub home_postal_code: Option<String>,
    pub home_city: Option<String>,
    pub home_district: Option<String>,
    pub home_street: Option<String>,
    pub status: String,
    pub ecpay_logistics_id: Option<String>,
    pub ecpay_merchant_trade_no: Option<String>,
    pub cvs_payment_no: Option<String>,
    pub cvs_validation_no: Option<String>,
    pub carrier: Option<String>,
    pub tracking_no: Option<String>,
    pub last_status_code: Option<String>,
    pub last_status_msg: Option<String>,
    #[serde(skip)]
    pub raw: Option<Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub const SHIPMENT_COLUMNS: &str = "id, order_id, method, cvs_sub_type, cvs_store_id, cvs_store_name, cvs_store_address, cvs_store_phone,
     home_postal_code, home_city, home_district, home_street, status, ecpay_logistics_id, ecpay_merchant_trade_no,
     cvs_payment_no, cvs_validation_no, carrier, tracking_no, last_status_code, last_status_msg, raw, created_at, updated_at";

pub async fn get_by_order<'e, E: PgExecutor<'e>>(
    exec: E,
    order_id: Uuid,
) -> Result<Option<Shipment>, sqlx::Error> {
    let sql = format!("SELECT {SHIPMENT_COLUMNS} FROM shipments WHERE order_id = $1");
    sqlx::query_as::<_, Shipment>(sqlx::AssertSqlSafe(sql))
        .bind(order_id)
        .fetch_optional(exec)
        .await
}

/// 狀態通知的處理結果；除了 Unknown 都回綠界 `1|OK`
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatusOutcome {
    /// MerchantTradeNo 與 AllPayLogisticsID 都找不到 → `0|Unknown MerchantTradeNo`
    Unknown,
    /// shipments.status 改了；picked_up 且訂單是 shipped 時一併 completed
    Updated {
        status: String,
        order_completed: bool,
    },
    /// 代碼不在對照表、或不能倒退／重複：只記代碼、訊息與 raw
    Recorded,
}

/// 套用一則狀態通知（規格 §8.3、與規格不同之處 42）。一個交易：先鎖 orders 列、再鎖 shipments 列
/// （計畫 4 的鎖序 orders → shipments）。一律更新 last_status_code／last_status_msg／raw.last_notification；
/// 有對照且不倒退才改 status；picked_up 且訂單是 shipped → completed。重複通知是 no-op
pub async fn apply_status(db: &PgPool, n: &StatusNotification) -> Result<StatusOutcome, ApiError> {
    let mut tx = db.begin().await?;
    let mut found: Option<(Uuid,)> =
        sqlx::query_as("SELECT order_id FROM shipments WHERE ecpay_merchant_trade_no = $1")
            .bind(&n.merchant_trade_no)
            .fetch_optional(&mut *tx)
            .await?;
    if found.is_none() && !n.logistics_id.is_empty() {
        found = sqlx::query_as("SELECT order_id FROM shipments WHERE ecpay_logistics_id = $1")
            .bind(&n.logistics_id)
            .fetch_optional(&mut *tx)
            .await?;
    }
    let Some((order_id,)) = found else {
        return Ok(StatusOutcome::Unknown);
    };
    let order_status: String =
        sqlx::query_scalar("SELECT status FROM orders WHERE id = $1 FOR UPDATE")
            .bind(order_id)
            .fetch_one(&mut *tx)
            .await?;
    let current: String =
        sqlx::query_scalar("SELECT status FROM shipments WHERE order_id = $1 FOR UPDATE")
            .bind(order_id)
            .fetch_one(&mut *tx)
            .await?;
    let next = logistics::shipment_status_for(n.rtn_code).filter(|next| should_apply(&current, next));
    sqlx::query(
        "UPDATE shipments SET status = COALESCE($2, status), last_status_code = $3, last_status_msg = $4,
                raw = COALESCE(raw, '{}'::jsonb) || $5, updated_at = now()
         WHERE order_id = $1",
    )
    .bind(order_id)
    .bind(next)
    .bind(n.rtn_code.to_string())
    .bind(&n.rtn_msg)
    .bind(json!({ "last_notification": n.raw }))
    .execute(&mut *tx)
    .await?;
    let mut order_completed = false;
    if next == Some(SHIPMENT_PICKED_UP) && order_status == STATUS_SHIPPED {
        sqlx::query("UPDATE orders SET status = $2, completed_at = now() WHERE id = $1")
            .bind(order_id)
            .bind(STATUS_COMPLETED)
            .execute(&mut *tx)
            .await?;
        order_completed = true;
    }
    tx.commit().await?;
    Ok(match next {
        Some(status) => StatusOutcome::Updated {
            status: status.to_string(),
            order_completed,
        },
        None => StatusOutcome::Recorded,
    })
}

/// 更新門市通知（與規格不同之處 35）：只記一句話與 raw.store_updates（追加），不改狀態。
/// 回 false = 找不到 AllPayLogisticsID
pub async fn apply_store_update(db: &PgPool, u: &StoreUpdate) -> Result<bool, ApiError> {
    let n = sqlx::query(
        "UPDATE shipments SET last_status_msg = $2,
                raw = jsonb_set(COALESCE(raw, '{}'::jsonb), '{store_updates}',
                                COALESCE(raw -> 'store_updates', '[]'::jsonb) || $3::jsonb),
                updated_at = now()
         WHERE ecpay_logistics_id = $1",
    )
    .bind(&u.logistics_id)
    .bind(logistics::store_update_message(u))
    .bind(json!([u.raw]))
    .execute(db)
    .await?
    .rows_affected();
    Ok(n > 0)
}
```

- [ ] **Step 3: 兩條回呼路由**

`api/src/routes/ecpay_logistics.rs` 的 router 加兩條、import 補 `domain::shipments::{self, StatusOutcome}`、`ecpay::aio::CallbackError`、`routes::ecpay_callback::{callback_error, server_error, text}`：

```rust
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/ecpay/logistics/map-reply", post(map_reply))
        .route("/api/ecpay/logistics/status", post(status))
        .route("/api/ecpay/logistics/store-update", post(store_update))
        // 回呼一律 < 2 KB；後掛的 layer 在內層，覆蓋 app.rs 給圖片上傳訂的 10 MB（計畫 3 修正波 #1）
        .layer(DefaultBodyLimit::max(64 * 1024))
}
```

檔案最後加：

```rust
/// 物流狀態通知（綠界伺服器 POST；規格 §8.3、§14）
async fn status(State(state): State<AppState>, body: String) -> Response {
    let params = parse_form(&body);
    if params.len() > MAX_CALLBACK_FIELDS {
        return callback_error("logistics-status", CallbackError::BadMac);
    }
    let notification = match logistics::parse_status(&state.config.ecpay, &params) {
        Ok(n) => n,
        Err(e) => return callback_error("logistics-status", e),
    };
    match shipments::apply_status(&state.db, &notification).await {
        Ok(StatusOutcome::Unknown) => {
            tracing::warn!(merchant_trade_no = %notification.merchant_trade_no, logistics_id = %notification.logistics_id, "物流狀態通知找不到單");
            text(StatusCode::OK, "0|Unknown MerchantTradeNo")
        }
        Ok(outcome) => {
            tracing::info!(merchant_trade_no = %notification.merchant_trade_no, rtn_code = notification.rtn_code, ?outcome, "物流狀態通知處理完成");
            text(StatusCode::OK, "1|OK")
        }
        Err(e) => server_error("logistics-status", e),
    }
}

/// 更新門市通知（7-11 C2C；與規格不同之處 35）
async fn store_update(State(state): State<AppState>, body: String) -> Response {
    let params = parse_form(&body);
    if params.len() > MAX_CALLBACK_FIELDS {
        return callback_error("logistics-store-update", CallbackError::BadMac);
    }
    let update = match logistics::parse_store_update(&state.config.ecpay, &params) {
        Ok(u) => u,
        Err(e) => return callback_error("logistics-store-update", e),
    };
    match shipments::apply_store_update(&state.db, &update).await {
        Ok(false) => {
            tracing::warn!(logistics_id = %update.logistics_id, "更新門市通知找不到單");
            text(StatusCode::OK, "0|Unknown AllPayLogisticsID")
        }
        Ok(true) => {
            tracing::info!(logistics_id = %update.logistics_id, status = %update.status, store_type = %update.store_type, "更新門市通知已記錄");
            text(StatusCode::OK, "1|OK")
        }
        Err(e) => server_error("logistics-store-update", e),
    }
}
```

- [ ] **Step 4: 跑測試、fmt、clippy**

Run: `DATABASE_URL=… cargo test --manifest-path api/Cargo.toml --test ecpay_logistics`
Expected: 13 個全綠。
Run: 全部測試、fmt、clippy。
Expected: 全綠、乾淨。

- [ ] **Step 5: Commit**

```bash
git add api/src/domain/shipments.rs api/src/routes/ecpay_logistics.rs api/tests/ecpay_logistics.rs
git commit -m "feat(api): 物流狀態回呼（貨態對照、不倒退、取件完成 → 訂單完成）與 7-11 更新門市通知"
```

---

### Task 5: 後台訂單讀取 API：列表、明細、全部付款嘗試

**Files:**
- Modify: `api/src/domain/payments.rs:17-53`（`Payment` 加 `created_at` 與 `Serialize`、`PAYMENT_COLUMNS`、`list_for_order`）
- Create: `api/src/domain/admin_orders.rs`
- Modify: `api/src/domain/mod.rs`（`pub mod admin_orders;`）
- Create: `api/src/routes/admin_orders.rs`
- Modify: `api/src/routes/mod.rs`（`pub mod admin_orders;`）、`api/src/app.rs`（`.merge(routes::admin_orders::router())` 放在 `admin_settings` 之後）
- Create: `api/tests/admin_orders.rs`

**Interfaces:**
- Consumes: `shipments::{Shipment, get_by_order}`（Task 4）；`orders::OrderItemRow`、`orders::STATUS_*`；`products::{Page, clean, clamp_paging}`；`invoices` 表；`AdminUser`。
- Produces: `payments::Payment` 多 `created_at: DateTime<Utc>` 並 `Serialize`；`payments::list_for_order(db, order_id) -> Result<Vec<Payment>, sqlx::Error>`（新到舊）；`admin_orders::STATUSES`、`admin_orders::Flag::{NeedsRefund, CvsReturned, InvoiceFailed}` 與 `Flag::parse(&str) -> Option<Flag>`；`admin_orders::AdminOrderListItem { id, order_no, status, email, recipient_name, shipping_method, total, item_count, needs_refund, shipment_status: Option<String>, invoice_status: Option<String>, created_at, paid_at }`；`admin_orders::list(db, q: Option<&str>, status: Option<&str>, flag: Option<Flag>, page: i64, per_page: i64) -> Result<Page<AdminOrderListItem>, ApiError>`；`admin_orders::AdminOrderRow`（訂單主檔含 `user_id`、`needs_refund`，無 `guest_token`）、`AdminInvoiceRow { status, invoice_no, invoice_date, random_number, error, updated_at }`、`AdminOrderDetail { #[serde(flatten)] order: AdminOrderRow, items: Vec<OrderItemRow>, shipment: Option<Shipment>, payments: Vec<Payment>, invoice: Option<AdminInvoiceRow> }`、`admin_orders::get_detail(db, id) -> Result<Option<AdminOrderDetail>, ApiError>`；`routes::admin_orders::admin_detail(state: &AppState, id: Uuid) -> ApiResult<Json<AdminOrderDetail>>`（`pub(crate)`，Task 6、7 的動作結束後回整份明細）；HTTP：`GET /api/admin/orders?q=&status=&flag=&page=&per_page=`（預設 20、最多 100）、`GET /api/admin/orders/{id}`。

- [ ] **Step 1: 寫整合測試（先失敗）**

`api/tests/admin_orders.rs`：

```rust
mod common;

use axum::{Router, http::StatusCode};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

fn order_body(variant: &str, method: &str, token: Option<&str>) -> Value {
    json!({
        "items": [{ "variant_id": variant, "qty": 2 }],
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

/// 建一筆 2 × 300 的訪客訂單（超商運費 60 → 660；宅配運費 100 → 700），回 (order_id, order_no, guest_token)
async fn place_order(app: &Router, pool: &PgPool, method: &str) -> (Uuid, String, String) {
    let (variant, _) = common::active_product(pool, "雞肉狗糧", 300, 5).await;
    let token = if method == "cvs" {
        Some(common::cvs_store_token(pool).await)
    } else {
        None
    };
    let (status, created, _) = common::send(
        app,
        common::req(
            "POST",
            "/api/orders",
            None,
            Some(order_body(&variant.to_string(), method, token.as_deref())),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    (
        Uuid::parse_str(created["order_id"].as_str().unwrap()).unwrap(),
        created["order_no"].as_str().unwrap().to_string(),
        created["guest_token"].as_str().unwrap().to_string(),
    )
}

/// 直接用 SQL 標成已付款（付款回呼的邏輯在 tests/ecpay_payment.rs 已測過）
async fn mark_paid(pool: &PgPool, id: Uuid) {
    sqlx::query("UPDATE orders SET status = 'paid', paid_at = now() WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("UPDATE payments SET status = 'paid', payment_date = now() WHERE order_id = $1")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
}

async fn get(app: &Router, cookie: &str, path: &str) -> (StatusCode, Value) {
    let (status, body, _) = common::send(app, common::req("GET", path, Some(cookie), None)).await;
    (status, body)
}

async fn post(app: &Router, cookie: &str, path: &str, body: Value) -> (StatusCode, Value) {
    let (status, value, _) =
        common::send(app, common::req("POST", path, Some(cookie), Some(body))).await;
    (status, value)
}

#[sqlx::test(migrations = "./migrations")]
async fn admin_order_routes_require_admin(pool: PgPool) {
    let app = common::app(pool.clone());
    let (id, _, _) = place_order(&app, &pool, "home").await;

    let (status, _, _) = common::send(&app, common::req("GET", "/api/admin/orders", None, None)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _, _) = common::send(
        &app,
        common::req("GET", &format!("/api/admin/orders/{id}"), None, None),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let customer = common::customer_cookie(&app, &pool).await;
    assert_eq!(get(&app, &customer, "/api/admin/orders").await.0, StatusCode::FORBIDDEN);
    assert_eq!(
        get(&app, &customer, &format!("/api/admin/orders/{id}")).await.0,
        StatusCode::FORBIDDEN
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn list_filters_by_status_flag_query_and_pages(pool: PgPool) {
    let app = common::app(pool.clone());
    let admin = common::admin_cookie(&app, &pool).await;
    let (pending, _, _) = place_order(&app, &pool, "home").await;
    let (refund, refund_no, _) = place_order(&app, &pool, "cvs").await;
    let (returned, _, _) = place_order(&app, &pool, "cvs").await;
    let (inv_failed, _, _) = place_order(&app, &pool, "home").await;
    mark_paid(&pool, refund).await;
    mark_paid(&pool, returned).await;
    mark_paid(&pool, inv_failed).await;
    sqlx::query("UPDATE orders SET needs_refund = true WHERE id = $1")
        .bind(refund)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE orders SET status = 'shipped', shipped_at = now() WHERE id = $1")
        .bind(returned)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE shipments SET status = 'returned' WHERE order_id = $1")
        .bind(returned)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE invoices SET status = 'failed', error = '綠界回錯' WHERE order_id = $1")
        .bind(inv_failed)
        .execute(&pool)
        .await
        .unwrap();

    let (status, page) = get(&app, &admin, "/api/admin/orders").await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert_eq!(page["total"], 4);
    assert_eq!(page["items"].as_array().unwrap().len(), 4);
    let first = &page["items"][0];
    assert_eq!(first["id"], inv_failed.to_string(), "新到舊");
    assert_eq!(first["item_count"], 2);
    assert_eq!(first["shipment_status"], "pending");
    assert_eq!(first["invoice_status"], "failed");
    assert_eq!(first["needs_refund"], false);
    assert_eq!(first["shipping_method"], "home");
    assert_eq!(first["total"], 700);
    assert!(first["paid_at"].is_string());

    let (_, page) = get(&app, &admin, "/api/admin/orders?status=paid").await;
    assert_eq!(page["total"], 2);
    let ids: Vec<&str> = page["items"].as_array().unwrap().iter().map(|i| i["id"].as_str().unwrap()).collect();
    assert!(ids.contains(&refund.to_string().as_str()) && ids.contains(&inv_failed.to_string().as_str()));

    let (_, page) = get(&app, &admin, "/api/admin/orders?status=pending_payment").await;
    assert_eq!(page["items"][0]["id"], pending.to_string());

    let (_, page) = get(&app, &admin, "/api/admin/orders?flag=needs_refund").await;
    assert_eq!(page["total"], 1);
    assert_eq!(page["items"][0]["id"], refund.to_string());
    assert_eq!(page["items"][0]["needs_refund"], true);

    let (_, page) = get(&app, &admin, "/api/admin/orders?flag=cvs_returned").await;
    assert_eq!(page["total"], 1);
    assert_eq!(page["items"][0]["id"], returned.to_string());
    assert_eq!(page["items"][0]["shipment_status"], "returned");

    let (_, page) = get(&app, &admin, "/api/admin/orders?flag=invoice_failed").await;
    assert_eq!(page["total"], 1);
    assert_eq!(page["items"][0]["id"], inv_failed.to_string());

    let (_, page) = get(&app, &admin, &format!("/api/admin/orders?q={refund_no}")).await;
    assert_eq!(page["total"], 1);
    let (_, page) = get(&app, &admin, "/api/admin/orders?q=%E7%8E%8B%E5%B0%8F%E6%98%8E").await; // 王小明
    assert_eq!(page["total"], 4);
    let (_, page) = get(&app, &admin, "/api/admin/orders?q=buyer%40test.local&status=paid").await;
    assert_eq!(page["total"], 2);

    let (_, page) = get(&app, &admin, "/api/admin/orders?per_page=2&page=2").await;
    assert_eq!(page["total"], 4);
    assert_eq!(page["items"].as_array().unwrap().len(), 2);
    assert_eq!(page["page"], 2);

    let (status, body) = get(&app, &admin, "/api/admin/orders?status=bogus").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "VALIDATION");
    let (status, _) = get(&app, &admin, "/api/admin/orders?flag=bogus").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "./migrations")]
async fn detail_has_full_shipment_all_payments_and_no_guest_token(pool: PgPool) {
    let app = common::app(pool.clone());
    let admin = common::admin_cookie(&app, &pool).await;
    let (id, order_no, token) = place_order(&app, &pool, "cvs").await;
    // 買家重新付款一次 → 兩筆付款嘗試
    let (status, _, _) = common::send(
        &app,
        common::req(
            "POST",
            &format!("/api/orders/{id}/repay?t={token}"),
            None,
            Some(json!({ "payment_method": "atm" })),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, d) = get(&app, &admin, &format!("/api/admin/orders/{id}")).await;
    assert_eq!(status, StatusCode::OK, "{d}");
    assert_eq!(d["order_no"], order_no);
    assert_eq!(d["status"], "pending_payment");
    assert_eq!(d["needs_refund"], false);
    assert!(d["user_id"].is_null(), "訪客訂單 user_id 是 null");
    assert!(d.get("guest_token").is_none(), "後台明細不回 guest_token");
    assert_eq!(d["items"][0]["product_name"], "雞肉狗糧");
    assert_eq!(d["items"][0]["quantity"], 2);

    let s = &d["shipment"];
    assert_eq!(s["method"], "cvs");
    assert_eq!(s["status"], "pending");
    assert_eq!(s["cvs_sub_type"], "UNIMARTC2C");
    assert_eq!(s["cvs_store_id"], "131386");
    assert_eq!(s["cvs_store_name"], "測試門市");
    assert!(s["ecpay_logistics_id"].is_null());
    assert!(s["ecpay_merchant_trade_no"].is_null());
    assert!(s["last_status_code"].is_null());
    assert!(s.get("raw").is_none(), "raw 不給前端");
    assert!(s["created_at"].is_string());

    let payments = d["payments"].as_array().unwrap();
    assert_eq!(payments.len(), 2);
    assert_eq!(payments[0]["merchant_trade_no"], format!("{order_no}02"), "新到舊");
    assert_eq!(payments[0]["method"], "atm");
    assert_eq!(payments[1]["merchant_trade_no"], format!("{order_no}01"));
    assert_eq!(payments[1]["status"], "pending");
    assert_eq!(payments[1]["amount"], 660);
    assert!(payments[1]["created_at"].is_string());

    assert_eq!(d["invoice"]["status"], "pending");
    assert!(d["invoice"]["error"].is_null());
    assert!(d["invoice"]["updated_at"].is_string());

    let (status, _) = get(&app, &admin, &format!("/api/admin/orders/{}", Uuid::now_v7())).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
```

Run: `DATABASE_URL=… cargo test --manifest-path api/Cargo.toml --test admin_orders`（`run_in_background: true`）
Expected: 失敗（404）。

- [ ] **Step 2: `payments::list_for_order`**

`api/src/domain/payments.rs`：`use serde::Serialize;`；`Payment` 改成 `#[derive(Debug, Clone, Serialize, sqlx::FromRow)]` 並在 `expire_at` 之後加 `pub created_at: DateTime<Utc>,`；`PAYMENT_COLUMNS` 改成：

```rust
const PAYMENT_COLUMNS: &str =
    "id, order_id, merchant_trade_no, method, status, amount, ecpay_trade_no, payment_type,
     payment_date, atm_bank_code, atm_vaccount, cvs_payment_no, expire_at, created_at";
```

（三個 `query_as::<_, Payment>` 都用 `PAYMENT_COLUMNS`，`From<Payment> for PaymentRow` 不變。）`get` 之後加：

```rust
/// 後台看全部付款嘗試（新到舊；OrderDetail.payment 只有最新一筆）
pub async fn list_for_order(db: &PgPool, order_id: Uuid) -> Result<Vec<Payment>, sqlx::Error> {
    let sql = format!(
        "SELECT {PAYMENT_COLUMNS} FROM payments WHERE order_id = $1 ORDER BY created_at DESC, id DESC"
    );
    sqlx::query_as::<_, Payment>(sqlx::AssertSqlSafe(sql))
        .bind(order_id)
        .fetch_all(db)
        .await
}
```

- [ ] **Step 3: `domain/admin_orders.rs`**

```rust
//! 後台訂單（規格 §6.1、§10）：列表、明細；Task 7 加退款／完成／清除需退款，Task 8 加儀表板
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::orders::{
    OrderItemRow, STATUS_CANCELLED, STATUS_COMPLETED, STATUS_PAID, STATUS_PENDING_PAYMENT,
    STATUS_REFUNDED, STATUS_SHIPPED,
};
use crate::domain::payments::{self, Payment};
use crate::domain::products::Page;
use crate::domain::shipments::{self, Shipment};
use crate::error::ApiError;

pub const STATUSES: &[&str] = &[
    STATUS_PENDING_PAYMENT,
    STATUS_PAID,
    STATUS_SHIPPED,
    STATUS_COMPLETED,
    STATUS_CANCELLED,
    STATUS_REFUNDED,
];

/// 列表的特殊篩選（與規格不同之處 45）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flag {
    /// 遲到付款、金額不符（orders.needs_refund）
    NeedsRefund,
    /// 超商未取退回：訂單 shipped 且 shipments.status = returned
    CvsReturned,
    /// 發票開立失敗
    InvoiceFailed,
}

impl Flag {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "needs_refund" => Some(Self::NeedsRefund),
            "cvs_returned" => Some(Self::CvsReturned),
            "invoice_failed" => Some(Self::InvoiceFailed),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::NeedsRefund => "needs_refund",
            Self::CvsReturned => "cvs_returned",
            Self::InvoiceFailed => "invoice_failed",
        }
    }
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct AdminOrderListItem {
    pub id: Uuid,
    pub order_no: String,
    pub status: String,
    pub email: String,
    pub recipient_name: String,
    pub shipping_method: String,
    pub total: i32,
    pub item_count: i32,
    pub needs_refund: bool,
    pub shipment_status: Option<String>,
    pub invoice_status: Option<String>,
    pub created_at: DateTime<Utc>,
    pub paid_at: Option<DateTime<Utc>>,
    #[serde(skip)]
    pub total_count: i64,
}

/// 後台列表：q 比對訂單編號、Email、收件人（不分大小寫、部分相符）；新到舊
pub async fn list(
    db: &PgPool,
    q: Option<&str>,
    status: Option<&str>,
    flag: Option<Flag>,
    page: i64,
    per_page: i64,
) -> Result<Page<AdminOrderListItem>, ApiError> {
    let rows = sqlx::query_as::<_, AdminOrderListItem>(
        "SELECT o.id, o.order_no, o.status, o.email, o.recipient_name, o.shipping_method, o.total, o.needs_refund,
                o.created_at, o.paid_at,
                (SELECT COALESCE(SUM(oi.quantity), 0) FROM order_items oi WHERE oi.order_id = o.id)::int AS item_count,
                s.status AS shipment_status,
                i.status AS invoice_status,
                COUNT(*) OVER () AS total_count
         FROM orders o
         LEFT JOIN shipments s ON s.order_id = o.id
         LEFT JOIN invoices i ON i.order_id = o.id
         WHERE ($1::text IS NULL OR o.order_no ILIKE '%' || $1 || '%' OR o.email ILIKE '%' || $1 || '%'
                OR o.recipient_name ILIKE '%' || $1 || '%')
           AND ($2::text IS NULL OR o.status = $2)
           AND ($3::text IS NULL
                OR ($3 = 'needs_refund' AND o.needs_refund)
                OR ($3 = 'cvs_returned' AND o.status = 'shipped' AND s.status = 'returned')
                OR ($3 = 'invoice_failed' AND i.status = 'failed'))
         ORDER BY o.created_at DESC, o.id DESC
         LIMIT $4 OFFSET $5",
    )
    .bind(q)
    .bind(status)
    .bind(flag.map(Flag::as_str))
    .bind(per_page)
    .bind((page - 1) * per_page)
    .fetch_all(db)
    .await?;
    let total = rows.first().map(|r| r.total_count).unwrap_or(0);
    Ok(Page {
        items: rows,
        total,
        page,
        per_page,
    })
}

/// 後台看的訂單主檔：比 orders::OrderRow 多 user_id 與 needs_refund；不含 guest_token（規格 §11、與規格不同之處 48）
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct AdminOrderRow {
    pub id: Uuid,
    pub order_no: String,
    pub user_id: Option<Uuid>,
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
    pub needs_refund: bool,
    pub created_at: DateTime<Utc>,
    pub paid_at: Option<DateTime<Utc>>,
    pub shipped_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub cancelled_at: Option<DateTime<Utc>>,
    pub cancel_reason: Option<String>,
}

const ADMIN_ORDER_COLUMNS: &str = "id, order_no, user_id, status, email, recipient_name, recipient_phone, shipping_method,
     subtotal, shipping_fee, total, note, invoice_type, invoice_carrier_type, invoice_carrier_num, invoice_tax_id,
     invoice_title, invoice_address, invoice_love_code, needs_refund, created_at, paid_at, shipped_at, completed_at,
     cancelled_at, cancel_reason";

/// 後台看的發票：多 error 與 updated_at
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct AdminInvoiceRow {
    pub status: String,
    pub invoice_no: Option<String>,
    pub invoice_date: Option<DateTime<Utc>>,
    pub random_number: Option<String>,
    pub error: Option<String>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct AdminOrderDetail {
    #[serde(flatten)]
    pub order: AdminOrderRow,
    pub items: Vec<OrderItemRow>,
    pub shipment: Option<Shipment>,
    /// 全部付款嘗試，新到舊
    pub payments: Vec<Payment>,
    pub invoice: Option<AdminInvoiceRow>,
}

pub async fn get_detail(db: &PgPool, id: Uuid) -> Result<Option<AdminOrderDetail>, ApiError> {
    let sql = format!("SELECT {ADMIN_ORDER_COLUMNS} FROM orders WHERE id = $1");
    let Some(order) = sqlx::query_as::<_, AdminOrderRow>(sqlx::AssertSqlSafe(sql))
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
    let shipment = shipments::get_by_order(db, id).await?;
    let payments = payments::list_for_order(db, id).await?;
    let invoice = sqlx::query_as::<_, AdminInvoiceRow>(
        "SELECT status, invoice_no, invoice_date, random_number, error, updated_at FROM invoices WHERE order_id = $1",
    )
    .bind(id)
    .fetch_optional(db)
    .await?;
    Ok(Some(AdminOrderDetail {
        order,
        items,
        shipment,
        payments,
        invoice,
    }))
}
```

`api/src/domain/mod.rs` 加 `pub mod admin_orders;`（放在 `addresses` 之後）。

- [ ] **Step 4: `routes/admin_orders.rs`**

```rust
//! 後台訂單 API（規格 §10）。權限靠 AdminUser 擷取器（未登入 401、非 admin 403）。
//! 動作（Task 6、7）做完都回整份明細，前端直接覆蓋
use axum::{Json, Router, extract::State, routing::get};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    auth::extract::AdminUser,
    domain::{
        admin_orders::{self, AdminOrderDetail, AdminOrderListItem, Flag},
        products::{self, Page},
    },
    error::{ApiError, ApiResult},
    extract::{AppPath, AppQuery},
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/admin/orders", get(list))
        .route("/api/admin/orders/{id}", get(detail))
}

#[derive(Deserialize)]
pub struct ListQuery {
    pub q: Option<String>,
    pub status: Option<String>,
    pub flag: Option<String>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

async fn list(
    _admin: AdminUser,
    State(state): State<AppState>,
    AppQuery(query): AppQuery<ListQuery>,
) -> ApiResult<Json<Page<AdminOrderListItem>>> {
    let status = products::clean(&query.status);
    if let Some(s) = status.as_deref()
        && !admin_orders::STATUSES.contains(&s)
    {
        return Err(ApiError::field("status", "狀態不正確"));
    }
    let flag = match products::clean(&query.flag) {
        None => None,
        Some(f) => Some(Flag::parse(&f).ok_or_else(|| {
            ApiError::field(
                "flag",
                "篩選只能是 needs_refund、cvs_returned 或 invoice_failed",
            )
        })?),
    };
    let (page, per_page) = products::clamp_paging(query.page, query.per_page, 20, 100);
    let q = products::clean(&query.q);
    let result =
        admin_orders::list(&state.db, q.as_deref(), status.as_deref(), flag, page, per_page)
            .await?;
    Ok(Json(result))
}

async fn detail(
    _admin: AdminUser,
    State(state): State<AppState>,
    AppPath(id): AppPath<Uuid>,
) -> ApiResult<Json<AdminOrderDetail>> {
    admin_detail(&state, id).await
}

/// 動作結束後回整份明細（Task 6、7 共用）
pub(crate) async fn admin_detail(state: &AppState, id: Uuid) -> ApiResult<Json<AdminOrderDetail>> {
    admin_orders::get_detail(&state.db, id)
        .await?
        .map(Json)
        .ok_or(ApiError::NotFound)
}
```

`api/src/routes/mod.rs` 加 `pub mod admin_orders;`；`api/src/app.rs` 在 `.merge(routes::admin_settings::router())` 之後加 `.merge(routes::admin_orders::router())`。

- [ ] **Step 5: 跑測試、fmt、clippy**

Run: `DATABASE_URL=… cargo test --manifest-path api/Cargo.toml --test admin_orders`
Expected: 3 個全綠。
Run: 全部測試、fmt、clippy。
Expected: 全綠、乾淨。

- [ ] **Step 6: Commit**

```bash
git add api/src/domain/payments.rs api/src/domain/admin_orders.rs api/src/domain/mod.rs api/src/routes/admin_orders.rs api/src/routes/mod.rs api/src/app.rs api/tests/admin_orders.rs
git commit -m "feat(api): 後台訂單列表（狀態／旗標／關鍵字）與明細（完整出貨、全部付款嘗試、發票錯誤）"
```

---

### Task 6: 出貨：建立物流單、宅配單號、列印託運單、標記完成

**Files:**
- Modify: `api/src/domain/shipments.rs`（`next_merchant_trade_no`、`claim_create`、`record_create_failure`、`record_create_ok`、`finalize_cvs`、`ship_home`）
- Modify: `api/src/domain/admin_orders.rs`（`complete`）
- Modify: `api/src/routes/admin_orders.rs`（`ship-cvs`、`ship-home`、`print-label`、`complete`）
- Modify: `api/tests/admin_orders.rs`
- Test: `api/src/domain/shipments.rs`（`next_merchant_trade_no` 單元）

**Interfaces:**
- Consumes: `logistics::{CreateRequest, create_fields, create_url, parse_create_response, CreateError, CreateOk, print_form, goods_name, sanitize_name}`（Task 2）；`state.logistics.post_form`；`settings::get_all`（`sender`、`return_store`）；`users::is_tw_mobile`；`jobs::enqueue`；`orders::get_detail`。
- Produces: `shipments::{CREATING, CREATE_FAILED, CREATE_ERROR}`（`last_status_code` 的三個特殊值）；`shipments::next_merchant_trade_no(order_no: &str, previous: Option<&str>) -> Result<String, ApiError>`；`shipments::claim_create(db, order_id, merchant_trade_no, request_fields: &Value) -> Result<bool, sqlx::Error>`；`shipments::record_create_failure(db, order_id, code: &str, msg: &str, response_text: Option<&str>) -> Result<(), sqlx::Error>`；`shipments::record_create_ok(db, order_id, ok: &CreateOk) -> Result<(), sqlx::Error>`；`shipments::finalize_cvs(db, order_id) -> Result<bool, ApiError>`（false = 訂單不是 paid；已 shipped 回 true）；`shipments::ship_home(db, order_id, carrier, tracking_no) -> Result<bool, ApiError>`；`admin_orders::complete(db, id) -> Result<bool, sqlx::Error>`；HTTP：`POST /api/admin/orders/{id}/ship-cvs`（body `{}` 或無）→ 200 明細；`POST …/ship-home` body `{ carrier, tracking_no }` → 200 明細；`POST …/print-label` → 200 `CheckoutForm`；`POST …/complete` → 200 明細。狀態不允許一律 400 `VALIDATION`（`details.fields.status` 或 `fields.shipment`／`fields.sender`）；綠界失敗 502 `ECPAY_ERROR` 固定文案。

- [ ] **Step 1: 寫整合測試（先失敗）**

`api/tests/admin_orders.rs` 加（import 補 `use dog_shop_api::{domain::settings, ecpay::mac, jobs::worker, state::AppState};`）：

```rust
/// 後台設定寄件人（建物流單必填）
async fn set_sender(pool: &PgPool, return_store: Option<(&str, &str)>) {
    let mut all = settings::get_all(pool).await.unwrap();
    all.sender.name = "狗狗商店".to_string();
    all.sender.phone = "0987654321".to_string();
    if let Some((sub_type, store_id)) = return_store {
        all.return_store.sub_type = sub_type.to_string();
        all.return_store.store_id = store_id.to_string();
        all.return_store.store_name = "退貨門市".to_string();
    }
    settings::put_all(pool, &all).await.unwrap();
}

async fn run_all_jobs(state: &AppState) {
    while worker::run_once(state).await.unwrap() > 0 {}
}

/// (orders.status, orders.shipped_at 有無, shipments.status, ecpay_merchant_trade_no, ecpay_logistics_id, last_status_code, last_status_msg)
async fn ship_snapshot(
    pool: &PgPool,
    id: Uuid,
) -> (String, bool, String, Option<String>, Option<String>, Option<String>, Option<String>) {
    sqlx::query_as(
        "SELECT o.status, o.shipped_at IS NOT NULL, s.status, s.ecpay_merchant_trade_no, s.ecpay_logistics_id,
                s.last_status_code, s.last_status_msg
         FROM orders o JOIN shipments s ON s.order_id = o.id WHERE o.id = $1",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .unwrap()
}

#[sqlx::test(migrations = "./migrations")]
async fn ship_cvs_creates_logistics_order_ships_and_mails(pool: PgPool) {
    let (app, state) = common::app_with_state(pool.clone());
    let admin = common::admin_cookie(&app, &pool).await;
    set_sender(&pool, None).await;
    let (id, order_no, _) = place_order(&app, &pool, "cvs").await;
    mark_paid(&pool, id).await;

    let (status, d) = post(&app, &admin, &format!("/api/admin/orders/{id}/ship-cvs"), json!({})).await;
    assert_eq!(status, StatusCode::OK, "{d}");
    assert_eq!(d["status"], "shipped");
    assert!(d["shipped_at"].is_string());
    assert_eq!(d["shipment"]["status"], "created");
    assert_eq!(d["shipment"]["ecpay_merchant_trade_no"], format!("{order_no}L01"));
    assert_eq!(d["shipment"]["ecpay_logistics_id"], format!("FAKE{order_no}L01"));
    assert_eq!(d["shipment"]["cvs_payment_no"], "F0001234");
    assert_eq!(d["shipment"]["cvs_validation_no"], "1234");
    assert_eq!(d["shipment"]["last_status_code"], "300");

    let calls = common::fake_logistics(&state).calls();
    assert_eq!(calls.len(), 1);
    let (url, fields) = &calls[0];
    assert_eq!(url, "https://logistics-stage.ecpay.com.tw/Express/Create");
    assert_eq!(fields["MerchantID"], "2000933");
    assert_eq!(fields["MerchantTradeNo"], format!("{order_no}L01"));
    assert_eq!(fields["LogisticsSubType"], "UNIMARTC2C");
    assert_eq!(fields["GoodsAmount"], "600", "商品小計，不含運費");
    assert_eq!(fields["GoodsName"], "雞肉狗糧");
    assert_eq!(fields["SenderName"], "狗狗商店");
    assert_eq!(fields["SenderCellPhone"], "0987654321");
    assert_eq!(fields["ReceiverName"], "王小明");
    assert_eq!(fields["ReceiverCellPhone"], "0912345678");
    assert_eq!(fields["ReceiverEmail"], "buyer@test.local");
    assert_eq!(fields["ReceiverStoreID"], "131386");
    assert_eq!(fields["ServerReplyURL"], "http://localhost:5173/api/ecpay/logistics/status");
    assert_eq!(fields["LogisticsC2CReplyURL"], "http://localhost:5173/api/ecpay/logistics/store-update");
    assert!(!fields.contains_key("ReturnStoreID"), "沒設退貨門市");
    let params: Vec<(String, String)> = fields.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    assert!(mac::verify_md5("XBERn1YOvpM9nfZc", "h1ONHk4P4yqbl5LK", &params));

    let raw: Value = sqlx::query_scalar("SELECT raw FROM shipments WHERE order_id = $1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(raw["create_request"]["MerchantTradeNo"], format!("{order_no}L01"), "先存請求再送");
    assert_eq!(raw["create_response"]["RtnCode"], "300");

    // 出貨信排在同一個交易、worker 寄出
    let queued: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM jobs WHERE kind = 'send_email' AND dedupe_key = $1 AND status = 'queued'",
    )
    .bind(format!("email:order_shipped:{id}"))
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(queued, 1);
    run_all_jobs(&state).await;
    let mail = common::sent_emails(&state)
        .into_iter()
        .find(|m| m.subject.contains("已出貨"))
        .expect("出貨信");
    assert!(mail.text.contains("測試門市"), "{}", mail.text);

    // 已出貨就不能再建
    let (status, body) = post(&app, &admin, &format!("/api/admin/orders/{id}/ship-cvs"), json!({})).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["error"]["code"], "VALIDATION");
    assert_eq!(common::fake_logistics(&state).calls().len(), 1);
}

#[sqlx::test(migrations = "./migrations")]
async fn ship_cvs_sends_return_store_only_for_matching_sub_type(pool: PgPool) {
    let (app, state) = common::app_with_state(pool.clone());
    let admin = common::admin_cookie(&app, &pool).await;
    set_sender(&pool, Some(("FAMIC2C", "006598"))).await;
    let (id, _, _) = place_order(&app, &pool, "cvs").await; // UNIMARTC2C
    mark_paid(&pool, id).await;
    let (status, _) = post(&app, &admin, &format!("/api/admin/orders/{id}/ship-cvs"), json!({})).await;
    assert_eq!(status, StatusCode::OK);
    assert!(!common::fake_logistics(&state).calls()[0].1.contains_key("ReturnStoreID"), "退貨門市是全家、訂單是 7-11：不帶");

    set_sender(&pool, Some(("UNIMARTC2C", "991182"))).await;
    let (id2, _, _) = place_order(&app, &pool, "cvs").await;
    mark_paid(&pool, id2).await;
    let (status, _) = post(&app, &admin, &format!("/api/admin/orders/{id2}/ship-cvs"), json!({})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(common::fake_logistics(&state).calls()[1].1["ReturnStoreID"], "991182");
}

#[sqlx::test(migrations = "./migrations")]
async fn ship_cvs_rejection_keeps_order_paid_records_reason_and_retries_with_next_no(pool: PgPool) {
    let (app, state) = common::app_with_state(pool.clone());
    let admin = common::admin_cookie(&app, &pool).await;
    set_sender(&pool, None).await;
    let (id, order_no, _) = place_order(&app, &pool, "cvs").await;
    mark_paid(&pool, id).await;

    common::fake_logistics(&state).respond_with("0|收件人姓名格式錯誤");
    let (status, body) = post(&app, &admin, &format!("/api/admin/orders/{id}/ship-cvs"), json!({})).await;
    assert_eq!(status, StatusCode::BAD_GATEWAY, "{body}");
    assert_eq!(body["error"]["code"], "ECPAY_ERROR");
    assert!(!body["error"]["message"].as_str().unwrap().contains("收件人"), "綠界原文不進 message：{body}");
    let (o, shipped, s, mtn, lid, code, msg) = ship_snapshot(&pool, id).await;
    assert_eq!((o.as_str(), shipped, s.as_str()), ("paid", false, "pending"));
    assert_eq!(mtn.as_deref(), Some(format!("{order_no}L01").as_str()));
    assert!(lid.is_none());
    assert_eq!(code.as_deref(), Some("create_failed"));
    assert_eq!(msg.as_deref(), Some("收件人姓名格式錯誤"));
    let queued: i64 = sqlx::query_scalar("SELECT count(*) FROM jobs WHERE dedupe_key = $1")
        .bind(format!("email:order_shipped:{id}"))
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(queued, 0, "失敗不寄出貨信");

    // 連線失敗
    common::fake_logistics(&state).fail_next("connection reset");
    let (status, _) = post(&app, &admin, &format!("/api/admin/orders/{id}/ship-cvs"), json!({})).await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);
    let (_, _, _, mtn, _, code, _) = ship_snapshot(&pool, id).await;
    assert_eq!(mtn.as_deref(), Some(format!("{order_no}L02").as_str()), "每次嘗試換新號");
    assert_eq!(code.as_deref(), Some("create_error"));

    // 回應簽章不符：也當失敗，訊息說明
    let good = dog_shop_api::ecpay::logistics::fake_success_body(&common::fake_logistics(&state).calls()[0].1);
    common::fake_logistics(&state).respond_with(&good.replace("RtnCode=300", "RtnCode=301"));
    let (status, _) = post(&app, &admin, &format!("/api/admin/orders/{id}/ship-cvs"), json!({})).await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);
    let (_, _, _, _, _, code, msg) = ship_snapshot(&pool, id).await;
    assert_eq!(code.as_deref(), Some("create_failed"));
    assert!(msg.as_deref().unwrap().starts_with("回應簽章不符"));

    // 第四次成功：L04
    let (status, d) = post(&app, &admin, &format!("/api/admin/orders/{id}/ship-cvs"), json!({})).await;
    assert_eq!(status, StatusCode::OK, "{d}");
    assert_eq!(d["shipment"]["ecpay_merchant_trade_no"], format!("{order_no}L04"));
    assert_eq!(d["status"], "shipped");
    assert_eq!(common::fake_logistics(&state).calls().len(), 4);
}

#[sqlx::test(migrations = "./migrations")]
async fn ship_cvs_requires_paid_cvs_order_and_sender(pool: PgPool) {
    let (app, state) = common::app_with_state(pool.clone());
    let admin = common::admin_cookie(&app, &pool).await;
    let (pending, _, _) = place_order(&app, &pool, "cvs").await;
    let (status, body) = post(&app, &admin, &format!("/api/admin/orders/{pending}/ship-cvs"), json!({})).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"]["details"]["fields"]["status"].is_string());

    let (paid, _, _) = place_order(&app, &pool, "cvs").await;
    mark_paid(&pool, paid).await;
    let (status, body) = post(&app, &admin, &format!("/api/admin/orders/{paid}/ship-cvs"), json!({})).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(body["error"]["details"]["fields"]["sender"].is_string(), "沒設寄件人");

    let (home, _, _) = place_order(&app, &pool, "home").await;
    mark_paid(&pool, home).await;
    set_sender(&pool, None).await;
    let (status, body) = post(&app, &admin, &format!("/api/admin/orders/{home}/ship-cvs"), json!({})).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"]["details"]["fields"]["shipping_method"].is_string());
    assert!(common::fake_logistics(&state).calls().is_empty());

    let (status, _) = post(&app, &admin, &format!("/api/admin/orders/{}/ship-cvs", Uuid::now_v7()), json!({})).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "./migrations")]
async fn ship_cvs_finishes_interrupted_transition_without_calling_ecpay_again(pool: PgPool) {
    let (app, state) = common::app_with_state(pool.clone());
    let admin = common::admin_cookie(&app, &pool).await;
    set_sender(&pool, None).await;
    let (id, _, _) = place_order(&app, &pool, "cvs").await;
    mark_paid(&pool, id).await;
    let (status, _) = post(&app, &admin, &format!("/api/admin/orders/{id}/ship-cvs"), json!({})).await;
    assert_eq!(status, StatusCode::OK);

    // 模擬「綠界建單成功、單號已存，但收尾交易失敗」：訂單還是 paid、shipments 還是 pending、但有綠界單號
    sqlx::query("UPDATE orders SET status = 'paid', shipped_at = NULL WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE shipments SET status = 'pending' WHERE order_id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();

    let (status, d) = post(&app, &admin, &format!("/api/admin/orders/{id}/ship-cvs"), json!({})).await;
    assert_eq!(status, StatusCode::OK, "{d}");
    assert_eq!(d["status"], "shipped");
    assert_eq!(d["shipment"]["status"], "created");
    assert_eq!(common::fake_logistics(&state).calls().len(), 1, "不再打綠界");
}

#[sqlx::test(migrations = "./migrations")]
async fn ship_home_sets_carrier_and_tracking_and_mails(pool: PgPool) {
    let (app, state) = common::app_with_state(pool.clone());
    let admin = common::admin_cookie(&app, &pool).await;
    let (id, _, _) = place_order(&app, &pool, "home").await;

    // 還沒付款
    let (status, _) = post(&app, &admin, &format!("/api/admin/orders/{id}/ship-home"), json!({ "carrier": "黑貓", "tracking_no": "900123456789" })).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    mark_paid(&pool, id).await;
    let (status, body) = post(&app, &admin, &format!("/api/admin/orders/{id}/ship-home"), json!({ "carrier": " ", "tracking_no": "" })).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"]["details"]["fields"]["carrier"].is_string());
    assert!(body["error"]["details"]["fields"]["tracking_no"].is_string());

    let (status, d) = post(&app, &admin, &format!("/api/admin/orders/{id}/ship-home"), json!({ "carrier": " 黑貓 ", "tracking_no": " 900123456789 " })).await;
    assert_eq!(status, StatusCode::OK, "{d}");
    assert_eq!(d["status"], "shipped");
    assert_eq!(d["shipment"]["status"], "shipped");
    assert_eq!(d["shipment"]["carrier"], "黑貓");
    assert_eq!(d["shipment"]["tracking_no"], "900123456789");
    assert!(d["shipped_at"].is_string());

    run_all_jobs(&state).await;
    let mail = common::sent_emails(&state).into_iter().find(|m| m.subject.contains("已出貨")).expect("出貨信");
    assert!(mail.text.contains("黑貓") && mail.text.contains("900123456789"), "{}", mail.text);

    // 超商訂單不能走宅配出貨
    let (cvs, _, _) = place_order(&app, &pool, "cvs").await;
    mark_paid(&pool, cvs).await;
    let (status, body) = post(&app, &admin, &format!("/api/admin/orders/{cvs}/ship-home"), json!({ "carrier": "黑貓", "tracking_no": "1" })).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"]["details"]["fields"]["shipping_method"].is_string());
}

#[sqlx::test(migrations = "./migrations")]
async fn print_label_returns_signed_form_after_shipping(pool: PgPool) {
    let app = common::app(pool.clone());
    let admin = common::admin_cookie(&app, &pool).await;
    set_sender(&pool, None).await;
    let (id, order_no, _) = place_order(&app, &pool, "cvs").await;
    mark_paid(&pool, id).await;

    let (status, body) = post(&app, &admin, &format!("/api/admin/orders/{id}/print-label"), json!({})).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "還沒建單：{body}");

    post(&app, &admin, &format!("/api/admin/orders/{id}/ship-cvs"), json!({})).await;
    let (status, form) = post(&app, &admin, &format!("/api/admin/orders/{id}/print-label"), json!({})).await;
    assert_eq!(status, StatusCode::OK, "{form}");
    assert_eq!(form["action"], "https://logistics-stage.ecpay.com.tw/Express/PrintUniMartC2COrderInfo");
    assert_eq!(form["fields"]["MerchantID"], "2000933");
    assert_eq!(form["fields"]["AllPayLogisticsID"], format!("FAKE{order_no}L01"));
    assert_eq!(form["fields"]["CVSPaymentNo"], "F0001234");
    assert_eq!(form["fields"]["CVSValidationNo"], "1234");
    let params: Vec<(String, String)> = form["fields"].as_object().unwrap().iter().map(|(k, v)| (k.clone(), v.as_str().unwrap().to_string())).collect();
    assert!(mac::verify_md5("XBERn1YOvpM9nfZc", "h1ONHk4P4yqbl5LK", &params));
}

#[sqlx::test(migrations = "./migrations")]
async fn complete_marks_shipped_order_completed(pool: PgPool) {
    let app = common::app(pool.clone());
    let admin = common::admin_cookie(&app, &pool).await;
    let (id, _, _) = place_order(&app, &pool, "home").await;
    mark_paid(&pool, id).await;
    let (status, _) = post(&app, &admin, &format!("/api/admin/orders/{id}/complete"), json!({})).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "只有已出貨能完成");

    post(&app, &admin, &format!("/api/admin/orders/{id}/ship-home"), json!({ "carrier": "黑貓", "tracking_no": "1" })).await;
    let (status, d) = post(&app, &admin, &format!("/api/admin/orders/{id}/complete"), json!({})).await;
    assert_eq!(status, StatusCode::OK, "{d}");
    assert_eq!(d["status"], "completed");
    assert!(d["completed_at"].is_string());
}

#[sqlx::test(migrations = "./migrations")]
async fn mutating_admin_order_routes_require_admin(pool: PgPool) {
    let app = common::app(pool.clone());
    let (id, _, _) = place_order(&app, &pool, "home").await;
    let (status, _, _) = common::send(
        &app,
        common::req("POST", &format!("/api/admin/orders/{id}/ship-home"), None, Some(json!({ "carrier": "a", "tracking_no": "b" }))),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let customer = common::customer_cookie(&app, &pool).await;
    let (status, _) = post(&app, &customer, &format!("/api/admin/orders/{id}/ship-cvs"), json!({})).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}
```

Run: `DATABASE_URL=… cargo test --manifest-path api/Cargo.toml --test admin_orders`
Expected: 新測試失敗（405／404）。

- [ ] **Step 2: `domain/shipments.rs` 的出貨寫入**

先加單元測試到 `api/src/domain/shipments.rs` 的 `mod tests`：

```rust
    #[test]
    fn merchant_trade_no_increments_per_attempt() {
        assert_eq!(next_merchant_trade_no("DS260908ABLD", None).unwrap(), "DS260908ABLDL01");
        assert_eq!(next_merchant_trade_no("DS260908ABLD", Some("DS260908ABLDL01")).unwrap(), "DS260908ABLDL02");
        assert_eq!(next_merchant_trade_no("DS260908ABLD", Some("DS260908ABLDL09")).unwrap(), "DS260908ABLDL10");
        assert_eq!(next_merchant_trade_no("DS260908ABLD", Some("garbage")).unwrap(), "DS260908ABLDL01");
        assert!(next_merchant_trade_no("DS260908ABLD", Some("DS260908ABLDL99")).is_err());
    }
```

再加實作（`apply_store_update` 之後；import 補 `use serde_json::json;`（已有）、`use sqlx::{Postgres, Transaction};`、`use crate::domain::jobs;`、`use crate::domain::orders::{SHIPPING_HOME, STATUS_PAID};`、`use crate::ecpay::logistics::CreateOk;`）：

```rust
/// last_status_code 的三個特殊值（不是綠界代碼）：建單中、綠界拒絕、連線失敗（與規格不同之處 41）
pub const CREATING: &str = "creating";
pub const CREATE_FAILED: &str = "create_failed";
pub const CREATE_ERROR: &str = "create_error";

/// 物流單流水：order_no + "L" + 兩碼，每次嘗試換新號（規格 §8.3）；上次的號從 ecpay_merchant_trade_no 讀。
/// order_no 本身可能含 L，所以從右邊取最後一段
pub fn next_merchant_trade_no(order_no: &str, previous: Option<&str>) -> Result<String, ApiError> {
    let last = previous
        .and_then(|p| p.strip_prefix(order_no))
        .and_then(|rest| rest.strip_prefix('L'))
        .and_then(|seq| seq.parse::<u32>().ok())
        .unwrap_or(0);
    let seq = last + 1;
    if seq > 99 {
        return Err(ApiError::field("shipment", "物流單重試次數過多，請聯絡綠界客服"));
    }
    Ok(format!("{order_no}L{seq:02}"))
}

/// 建單前的認領：只有 pending、還沒有綠界單號、且不是別人正在建（或上次卡住超過 2 分鐘）才能建；
/// 同時寫入這次的 MerchantTradeNo 與請求欄位（規格 §14 先存再送）。回 false = 別人正在建或已建過
pub async fn claim_create(
    db: &PgPool,
    order_id: Uuid,
    merchant_trade_no: &str,
    request_fields: &Value,
) -> Result<bool, sqlx::Error> {
    let n = sqlx::query(
        "UPDATE shipments SET ecpay_merchant_trade_no = $2, last_status_code = $3, last_status_msg = NULL,
                raw = COALESCE(raw, '{}'::jsonb) || $4, updated_at = now()
         WHERE order_id = $1 AND status = $5 AND ecpay_logistics_id IS NULL
           AND (last_status_code IS DISTINCT FROM $3 OR updated_at < now() - interval '2 minutes')",
    )
    .bind(order_id)
    .bind(merchant_trade_no)
    .bind(CREATING)
    .bind(json!({ "create_request": request_fields }))
    .bind(SHIPMENT_PENDING)
    .execute(db)
    .await?
    .rows_affected();
    Ok(n > 0)
}

/// 綠界拒絕或連不上：記原因（給老闆看），訂單維持 paid、shipments 維持 pending
pub async fn record_create_failure(
    db: &PgPool,
    order_id: Uuid,
    code: &str,
    msg: &str,
    response_text: Option<&str>,
) -> Result<(), sqlx::Error> {
    let msg: String = msg.chars().take(200).collect();
    let patch = match response_text {
        Some(t) => json!({ "create_response_text": t.chars().take(2000).collect::<String>() }),
        None => json!({}),
    };
    sqlx::query(
        "UPDATE shipments SET last_status_code = $2, last_status_msg = $3,
                raw = COALESCE(raw, '{}'::jsonb) || $4, updated_at = now()
         WHERE order_id = $1",
    )
    .bind(order_id)
    .bind(code)
    .bind(msg)
    .bind(patch)
    .execute(db)
    .await?;
    Ok(())
}

/// 綠界成功：先把單號存起來（自己一句 UPDATE，不在收尾的交易裡），收尾失敗時下次不會重複建單
pub async fn record_create_ok(db: &PgPool, order_id: Uuid, ok: &CreateOk) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE shipments SET ecpay_logistics_id = $2, cvs_payment_no = NULLIF($3, ''), cvs_validation_no = NULLIF($4, ''),
                last_status_code = $5, last_status_msg = $6,
                raw = COALESCE(raw, '{}'::jsonb) || $7, updated_at = now()
         WHERE order_id = $1",
    )
    .bind(order_id)
    .bind(&ok.logistics_id)
    .bind(&ok.cvs_payment_no)
    .bind(&ok.cvs_validation_no)
    .bind(ok.rtn_code.to_string())
    .bind(&ok.rtn_msg)
    .bind(json!({ "create_response": ok.raw }))
    .execute(db)
    .await?;
    Ok(())
}

/// orders → shipped、shipped_at、排出貨信（超商與宅配共用；規格 §12、計畫 3 交接 3）。呼叫者已鎖住訂單列
async fn ship_order_in_tx(tx: &mut Transaction<'_, Postgres>, order_id: Uuid) -> Result<(), ApiError> {
    sqlx::query("UPDATE orders SET status = $2, shipped_at = now() WHERE id = $1")
        .bind(order_id)
        .bind(STATUS_SHIPPED)
        .execute(&mut **tx)
        .await?;
    jobs::enqueue(
        tx,
        jobs::KIND_SEND_EMAIL,
        json!({ "template": "order_shipped", "order_id": order_id }),
        Some(&format!("email:order_shipped:{order_id}")),
    )
    .await?;
    Ok(())
}

/// 超商建單的收尾：一個交易內 shipments → created、orders → shipped、排出貨信（鎖序 orders → shipments）。
/// 回 false = 訂單不是 paid（例如同時被標退款）；已經 shipped 回 true（冪等）
pub async fn finalize_cvs(db: &PgPool, order_id: Uuid) -> Result<bool, ApiError> {
    let mut tx = db.begin().await?;
    let status: Option<String> =
        sqlx::query_scalar("SELECT status FROM orders WHERE id = $1 FOR UPDATE")
            .bind(order_id)
            .fetch_optional(&mut *tx)
            .await?;
    match status.as_deref() {
        Some(STATUS_PAID) => {}
        Some(STATUS_SHIPPED) => {
            tx.rollback().await?;
            return Ok(true);
        }
        _ => {
            tx.rollback().await?;
            return Ok(false);
        }
    }
    sqlx::query("UPDATE shipments SET status = $2, updated_at = now() WHERE order_id = $1 AND status = $3")
        .bind(order_id)
        .bind(SHIPMENT_CREATED)
        .bind(SHIPMENT_PENDING)
        .execute(&mut *tx)
        .await?;
    ship_order_in_tx(&mut tx, order_id).await?;
    tx.commit().await?;
    Ok(true)
}

/// 宅配出貨（規格 §4、§6.1）：填貨運公司與單號 → shipments shipped、orders shipped、排出貨信。
/// 回 false = 訂單不是 paid 或不是宅配
pub async fn ship_home(
    db: &PgPool,
    order_id: Uuid,
    carrier: &str,
    tracking_no: &str,
) -> Result<bool, ApiError> {
    let mut tx = db.begin().await?;
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT status, shipping_method FROM orders WHERE id = $1 FOR UPDATE")
            .bind(order_id)
            .fetch_optional(&mut *tx)
            .await?;
    let Some((status, method)) = row else {
        tx.rollback().await?;
        return Ok(false);
    };
    if status != STATUS_PAID || method != SHIPPING_HOME {
        tx.rollback().await?;
        return Ok(false);
    }
    sqlx::query(
        "UPDATE shipments SET status = $2, carrier = $3, tracking_no = $4, updated_at = now() WHERE order_id = $1",
    )
    .bind(order_id)
    .bind(SHIPMENT_SHIPPED)
    .bind(carrier)
    .bind(tracking_no)
    .execute(&mut *tx)
    .await?;
    ship_order_in_tx(&mut tx, order_id).await?;
    tx.commit().await?;
    Ok(true)
}
```

`api/src/domain/admin_orders.rs` 加：

```rust
/// 後台標記完成（規格 §4）：shipped → completed，不看 shipments.status（與規格不同之處 47）
pub async fn complete(db: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
    let n = sqlx::query(
        "UPDATE orders SET status = $2, completed_at = now() WHERE id = $1 AND status = $3",
    )
    .bind(id)
    .bind(STATUS_COMPLETED)
    .bind(STATUS_SHIPPED)
    .execute(db)
    .await?
    .rows_affected();
    Ok(n > 0)
}
```

- [ ] **Step 3: 四條路由**

`api/src/routes/admin_orders.rs`：router 加四條、import 補：

```rust
use axum::routing::post;
use chrono::Utc;
use serde_json::json;

use crate::{
    domain::{
        orders::{self, SHIPPING_CVS, SHIPPING_HOME, STATUS_PAID},
        settings,
        shipments::{self, CREATE_ERROR, CREATE_FAILED, SHIPMENT_PENDING},
        users::is_tw_mobile,
    },
    ecpay::{
        aio::CheckoutForm,
        logistics::{self, CreateError, CreateRequest},
    },
};
```

```rust
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/admin/orders", get(list))
        .route("/api/admin/orders/{id}", get(detail))
        .route("/api/admin/orders/{id}/ship-cvs", post(ship_cvs))
        .route("/api/admin/orders/{id}/ship-home", post(ship_home))
        .route("/api/admin/orders/{id}/print-label", post(print_label))
        .route("/api/admin/orders/{id}/complete", post(complete))
}
```

檔案最後加：

```rust
/// 建立綠界 C2C 物流單（規格 §8.3、與規格不同之處 36、39、40、41）：同步呼叫，錯誤直接回給後台。
/// 認領 → 打綠界 → 存單號 → 收尾（shipments created、orders shipped、排出貨信）
async fn ship_cvs(
    _admin: AdminUser,
    State(state): State<AppState>,
    AppPath(id): AppPath<Uuid>,
) -> ApiResult<Json<AdminOrderDetail>> {
    let detail = orders::get_detail(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    if detail.order.status != STATUS_PAID {
        return Err(ApiError::field("status", "只有已付款的訂單能出貨"));
    }
    if detail.order.shipping_method != SHIPPING_CVS {
        return Err(ApiError::field(
            "shipping_method",
            "這筆訂單是宅配，請填貨運公司與單號",
        ));
    }
    let shipment = shipments::get_by_order(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let sub_type = shipment
        .cvs_sub_type
        .clone()
        .filter(|s| logistics::is_sub_type(s))
        .ok_or_else(|| ApiError::field("shipment", "訂單沒有超商資料"))?;
    let store_id = shipment
        .cvs_store_id
        .clone()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ApiError::field("shipment", "訂單沒有取貨門市"))?;

    // 上次綠界建單成功、單號已存，但收尾沒做完（例如當時資料庫斷線）：不再打綠界，只補收尾
    if shipment.ecpay_logistics_id.is_some() && shipment.status == SHIPMENT_PENDING {
        if !shipments::finalize_cvs(&state.db, id).await? {
            return Err(ApiError::field("status", "只有已付款的訂單能出貨"));
        }
        return admin_detail(&state, id).await;
    }
    if shipment.status != SHIPMENT_PENDING {
        return Err(ApiError::field("status", "物流單已建立"));
    }

    let all = settings::get_all(&state.db).await?;
    let sender_name = logistics::sanitize_name(&all.sender.name);
    if sender_name.is_empty() || !is_tw_mobile(all.sender.phone.trim()) {
        return Err(ApiError::field(
            "sender",
            "請先到「設定」填寫寄件人姓名（中文 5 字內）與手機",
        ));
    }
    let return_store_id = (all.return_store.sub_type == sub_type
        && !all.return_store.store_id.trim().is_empty())
    .then(|| all.return_store.store_id.trim().to_string());
    let merchant_trade_no = shipments::next_merchant_trade_no(
        &detail.order.order_no,
        shipment.ecpay_merchant_trade_no.as_deref(),
    )?;
    let req = CreateRequest {
        merchant_trade_no,
        sub_type,
        goods_amount: detail.order.subtotal,
        goods_name: logistics::goods_name(&detail),
        sender_name,
        sender_phone: all.sender.phone.trim().to_string(),
        receiver_name: logistics::sanitize_name(&detail.order.recipient_name),
        receiver_phone: detail.order.recipient_phone.clone(),
        receiver_email: detail.order.email.clone(),
        receiver_store_id: store_id,
        return_store_id,
    };
    let fields = logistics::create_fields(
        &state.config.ecpay,
        &state.config.public_base_url,
        &req,
        Utc::now(),
    );
    let claimed =
        shipments::claim_create(&state.db, id, &req.merchant_trade_no, &json!(fields)).await?;
    if !claimed {
        return Err(ApiError::field("status", "物流單建立中或已建立，請重新整理"));
    }

    let url = logistics::create_url(&state.config.ecpay);
    let body = match state.logistics.post_form(&url, &fields).await {
        Ok(body) => body,
        Err(e) => {
            tracing::error!(order_id = %id, error = %format!("{e:#}"), "綠界建立物流單連線失敗");
            shipments::record_create_failure(&state.db, id, CREATE_ERROR, "連線綠界失敗", None)
                .await?;
            return Err(ApiError::EcpayError(
                "連線綠界物流失敗，請稍後再試".to_string(),
            ));
        }
    };
    match logistics::parse_create_response(&state.config.ecpay, &body) {
        Ok(ok) => {
            tracing::info!(order_id = %id, logistics_id = %ok.logistics_id, rtn_code = ok.rtn_code, "綠界物流單已建立");
            shipments::record_create_ok(&state.db, id, &ok).await?;
            if !shipments::finalize_cvs(&state.db, id).await? {
                return Err(ApiError::field("status", "訂單狀態已改變，請重新整理"));
            }
            admin_detail(&state, id).await
        }
        Err(err) => {
            // 綠界原文只進 log 與 shipments.last_status_msg，不進回應的 message（計畫 3 審查交接 2）
            tracing::warn!(order_id = %id, error = %err, "綠界建立物流單失敗");
            let msg = match &err {
                CreateError::Rejected(m) => m.clone(),
                CreateError::BadMac => {
                    format!("回應簽章不符：{}", body.chars().take(200).collect::<String>())
                }
                CreateError::Malformed(m) => format!("回應格式不符：{m}"),
            };
            shipments::record_create_failure(&state.db, id, CREATE_FAILED, &msg, Some(&body))
                .await?;
            Err(ApiError::EcpayError(
                "綠界沒有接受這張物流單，原因請看訂單頁的出貨區".to_string(),
            ))
        }
    }
}

#[derive(Deserialize)]
pub struct ShipHomeInput {
    #[serde(default)]
    pub carrier: String,
    #[serde(default)]
    pub tracking_no: String,
}

/// 宅配出貨：填貨運公司與單號（規格 §6.1）
async fn ship_home(
    _admin: AdminUser,
    State(state): State<AppState>,
    AppPath(id): AppPath<Uuid>,
    AppJson(input): AppJson<ShipHomeInput>,
) -> ApiResult<Json<AdminOrderDetail>> {
    let carrier = input.carrier.trim().to_string();
    let tracking_no = input.tracking_no.trim().to_string();
    let mut errors = crate::error::FieldErrors::new();
    if carrier.is_empty() || carrier.chars().count() > 30 {
        errors.add("carrier", "必填，最多 30 字");
    }
    if tracking_no.is_empty() || tracking_no.chars().count() > 50 {
        errors.add("tracking_no", "必填，最多 50 字");
    }
    errors.into_result()?;
    let detail = orders::get_detail(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    if detail.order.shipping_method != SHIPPING_HOME {
        return Err(ApiError::field(
            "shipping_method",
            "這筆訂單是超商取貨，請用「建立物流單」",
        ));
    }
    if !shipments::ship_home(&state.db, id, &carrier, &tracking_no).await? {
        return Err(ApiError::field("status", "只有已付款的訂單能出貨"));
    }
    admin_detail(&state, id).await
}

/// 列印託運單的表單（規格 §8.3）：前端在新分頁 POST 到綠界
async fn print_label(
    _admin: AdminUser,
    State(state): State<AppState>,
    AppPath(id): AppPath<Uuid>,
) -> ApiResult<Json<CheckoutForm>> {
    let shipment = shipments::get_by_order(&state.db, id)
        .await?
        .ok_or(ApiError::NotFound)?;
    if shipment.method != SHIPPING_CVS {
        return Err(ApiError::field("shipping_method", "宅配沒有託運單可列印"));
    }
    let (Some(sub_type), Some(logistics_id), Some(payment_no)) = (
        shipment.cvs_sub_type.as_deref(),
        shipment.ecpay_logistics_id.as_deref(),
        shipment.cvs_payment_no.as_deref(),
    ) else {
        return Err(ApiError::field("shipment", "還沒有綠界物流單，請先建立"));
    };
    let form = logistics::print_form(
        &state.config.ecpay,
        sub_type,
        logistics_id,
        payment_no,
        shipment.cvs_validation_no.as_deref().unwrap_or(""),
    )
    .map_err(ApiError::Internal)?;
    Ok(Json(form))
}

/// 後台標記完成（規格 §4）
async fn complete(
    _admin: AdminUser,
    State(state): State<AppState>,
    AppPath(id): AppPath<Uuid>,
) -> ApiResult<Json<AdminOrderDetail>> {
    if orders::get_detail(&state.db, id).await?.is_none() {
        return Err(ApiError::NotFound);
    }
    if !admin_orders::complete(&state.db, id).await? {
        return Err(ApiError::field("status", "只有已出貨的訂單能標記完成"));
    }
    admin_detail(&state, id).await
}
```

（`AppJson` 要加進 `extract` 的 import。`crate::error::FieldErrors` 已存在，`into_result` 回 `Result<(), ApiError>`。）

- [ ] **Step 4: 跑測試、fmt、clippy**

Run: `DATABASE_URL=… cargo test --manifest-path api/Cargo.toml --test admin_orders --lib domain::shipments`
Expected: 全綠（admin_orders 12 個）。
Run: 全部測試、fmt、clippy。
Expected: 全綠、乾淨。

- [ ] **Step 5: Commit**

```bash
git add api/src/domain/shipments.rs api/src/domain/admin_orders.rs api/src/routes/admin_orders.rs api/tests/admin_orders.rs
git commit -m "feat(api): 後台出貨：綠界建單（認領、先存後送、冪等收尾）、宅配單號、列印託運單、標記完成"
```

---

### Task 7: 後台取消、標記已退款、重開發票、清除需退款

**Files:**
- Modify: `api/src/domain/orders.rs:806-865`（`restore_stock_in_tx`、`cancel_with_payments_in_tx`、`cancel` 改用）
- Modify: `api/src/domain/invoices.rs:86-107`（`record_failure` 守衛、`reset_for_retry_in_tx`）
- Modify: `api/src/domain/admin_orders.rs`（`cancel`、`mark_refunded`、`clear_refund`）
- Modify: `api/src/routes/admin_orders.rs`（四條路由）
- Modify: `api/tests/admin_orders.rs`

**Interfaces:**
- Consumes: `orders::cancel_in_tx`；`payments::{PAYMENT_PENDING, PAYMENT_EXPIRED}`；`invoices::{STATUS_PENDING, STATUS_FAILED, STATUS_ISSUED}`；`jobs::{enqueue, KIND_ISSUE_INVOICE}`；`FakeInvoiceGateway`（測試）。
- Produces: `orders::restore_stock_in_tx(tx, order_id) -> Result<(), ApiError>`；`orders::cancel_with_payments_in_tx(tx, order_id, reason) -> Result<bool, ApiError>`（false 時呼叫者要 rollback）；`invoices::reset_for_retry_in_tx(tx, order_id) -> Result<bool, sqlx::Error>`；`admin_orders::cancel(db, id) -> Result<bool, ApiError>`、`admin_orders::mark_refunded(db, id) -> Result<bool, ApiError>`、`admin_orders::clear_refund(db, id) -> Result<bool, sqlx::Error>`；HTTP：`POST /api/admin/orders/{id}/cancel`、`…/mark-refunded`、`…/retry-invoice`、`…/clear-refund` 都回 200 明細，狀態不允許 400 `VALIDATION`。

- [ ] **Step 1: 寫整合測試（先失敗）**

`api/tests/admin_orders.rs` 加（import 補 `use dog_shop_api::domain::invoices;`）：

```rust
async fn stock_of(pool: &PgPool, id: Uuid) -> i32 {
    sqlx::query_scalar(
        "SELECT pv.stock FROM product_variants pv JOIN order_items oi ON oi.variant_id = pv.id WHERE oi.order_id = $1",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn payment_statuses(pool: &PgPool, id: Uuid) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT status FROM payments WHERE order_id = $1 ORDER BY created_at, id",
    )
    .bind(id)
    .fetch_all(pool)
    .await
    .unwrap()
}

#[sqlx::test(migrations = "./migrations")]
async fn admin_cancel_only_pending_restores_stock_and_expires_payments(pool: PgPool) {
    let app = common::app(pool.clone());
    let admin = common::admin_cookie(&app, &pool).await;
    let (id, _, _) = place_order(&app, &pool, "cvs").await;
    assert_eq!(stock_of(&pool, id).await, 3);

    let (status, d) = post(&app, &admin, &format!("/api/admin/orders/{id}/cancel"), json!({})).await;
    assert_eq!(status, StatusCode::OK, "{d}");
    assert_eq!(d["status"], "cancelled");
    assert_eq!(d["cancel_reason"], "admin");
    assert!(d["cancelled_at"].is_string());
    assert_eq!(stock_of(&pool, id).await, 5, "庫存歸還");
    assert_eq!(payment_statuses(&pool, id).await, vec!["expired".to_string()]);
    assert_eq!(d["payments"][0]["status"], "expired");

    let (status, _) = post(&app, &admin, &format!("/api/admin/orders/{id}/cancel"), json!({})).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "取消過的不能再取消");

    let (paid, _, _) = place_order(&app, &pool, "home").await;
    mark_paid(&pool, paid).await;
    let (status, body) = post(&app, &admin, &format!("/api/admin/orders/{paid}/cancel"), json!({})).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"]["details"]["fields"]["status"].as_str().unwrap().contains("標記已退款"));
    assert_eq!(stock_of(&pool, paid).await, 3, "已付款的取消不動庫存");

    let (status, _) = post(&app, &admin, &format!("/api/admin/orders/{}/cancel", Uuid::now_v7()), json!({})).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "./migrations")]
async fn mark_refunded_paid_order_restores_stock_expires_pending_and_clears_flag(pool: PgPool) {
    let app = common::app(pool.clone());
    let admin = common::admin_cookie(&app, &pool).await;
    let (id, _, token) = place_order(&app, &pool, "cvs").await;
    // 第一筆付款成功、之後買家又按了一次重新付款（遲到付款的情境）
    mark_paid(&pool, id).await;
    sqlx::query("UPDATE orders SET status = 'pending_payment' WHERE id = $1").bind(id).execute(&pool).await.unwrap();
    let (status, _, _) = common::send(&app, common::req("POST", &format!("/api/orders/{id}/repay?t={token}"), None, Some(json!({})))).await;
    assert_eq!(status, StatusCode::OK);
    sqlx::query("UPDATE orders SET status = 'paid', needs_refund = true WHERE id = $1").bind(id).execute(&pool).await.unwrap();
    assert_eq!(payment_statuses(&pool, id).await, vec!["paid".to_string(), "pending".to_string()]);

    let (status, d) = post(&app, &admin, &format!("/api/admin/orders/{id}/mark-refunded"), json!({})).await;
    assert_eq!(status, StatusCode::OK, "{d}");
    assert_eq!(d["status"], "refunded");
    assert_eq!(d["needs_refund"], false);
    assert_eq!(d["cancel_reason"], "refunded");
    assert!(d["cancelled_at"].is_string());
    assert_eq!(stock_of(&pool, id).await, 5, "已付未出貨：庫存歸還");
    assert_eq!(payment_statuses(&pool, id).await, vec!["paid".to_string(), "expired".to_string()], "成功的不動、等待中的作廢");

    let (status, _) = post(&app, &admin, &format!("/api/admin/orders/{id}/mark-refunded"), json!({})).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "退過款的不能再退");
}

#[sqlx::test(migrations = "./migrations")]
async fn mark_refunded_shipped_order_keeps_stock(pool: PgPool) {
    let app = common::app(pool.clone());
    let admin = common::admin_cookie(&app, &pool).await;
    let (id, _, _) = place_order(&app, &pool, "home").await;
    mark_paid(&pool, id).await;
    post(&app, &admin, &format!("/api/admin/orders/{id}/ship-home"), json!({ "carrier": "黑貓", "tracking_no": "1" })).await;

    let (status, d) = post(&app, &admin, &format!("/api/admin/orders/{id}/mark-refunded"), json!({})).await;
    assert_eq!(status, StatusCode::OK, "{d}");
    assert_eq!(d["status"], "refunded");
    assert_eq!(stock_of(&pool, id).await, 3, "已出貨：庫存不加回");

    let (pending, _, _) = place_order(&app, &pool, "home").await;
    let (status, _) = post(&app, &admin, &format!("/api/admin/orders/{pending}/mark-refunded"), json!({})).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "待付款的沒有錢可退");
}

#[sqlx::test(migrations = "./migrations")]
async fn retry_invoice_resets_failed_invoice_and_enqueues_job(pool: PgPool) {
    let (app, state) = common::app_with_state(pool.clone());
    let admin = common::admin_cookie(&app, &pool).await;
    let (id, _, _) = place_order(&app, &pool, "home").await;
    mark_paid(&pool, id).await;

    let (status, body) = post(&app, &admin, &format!("/api/admin/orders/{id}/retry-invoice"), json!({})).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "pending 不能重試：{body}");

    sqlx::query("UPDATE invoices SET status = 'failed', error = '綠界回錯' WHERE order_id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    let (status, d) = post(&app, &admin, &format!("/api/admin/orders/{id}/retry-invoice"), json!({})).await;
    assert_eq!(status, StatusCode::OK, "{d}");
    assert_eq!(d["invoice"]["status"], "pending");
    assert!(d["invoice"]["error"].is_null());
    let key: String = sqlx::query_scalar(
        "SELECT dedupe_key FROM jobs WHERE kind = 'issue_invoice' AND status = 'queued' AND dedupe_key LIKE $1",
    )
    .bind(format!("invoice:{id}:retry:%"))
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(key.starts_with(&format!("invoice:{id}:retry:")));

    run_all_jobs(&state).await;
    assert_eq!(common::fake_invoices(&state).calls().len(), 1);
    let (_, d) = get(&app, &admin, &format!("/api/admin/orders/{id}")).await;
    assert_eq!(d["invoice"]["status"], "issued");
    assert!(d["invoice"]["invoice_no"].is_string());

    let (status, _) = post(&app, &admin, &format!("/api/admin/orders/{id}/retry-invoice"), json!({})).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "已開立不能重試");
}

#[sqlx::test(migrations = "./migrations")]
async fn record_failure_never_downgrades_issued_invoice(pool: PgPool) {
    let app = common::app(pool.clone());
    let (id, _, _) = place_order(&app, &pool, "home").await;
    sqlx::query("UPDATE invoices SET status = 'issued', invoice_no = 'AB12345678' WHERE order_id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    invoices::record_failure(&pool, id, None, "晚到的失敗", true).await.unwrap();
    let (status, error): (String, Option<String>) =
        sqlx::query_as("SELECT status, error FROM invoices WHERE order_id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(status, "issued");
    assert!(error.is_none());
}

#[sqlx::test(migrations = "./migrations")]
async fn clear_refund_clears_flag_once(pool: PgPool) {
    let app = common::app(pool.clone());
    let admin = common::admin_cookie(&app, &pool).await;
    let (id, _, _) = place_order(&app, &pool, "home").await;
    let (status, _) = post(&app, &admin, &format!("/api/admin/orders/{id}/clear-refund"), json!({})).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "本來就沒有需退款");

    sqlx::query("UPDATE orders SET needs_refund = true WHERE id = $1").bind(id).execute(&pool).await.unwrap();
    let (status, d) = post(&app, &admin, &format!("/api/admin/orders/{id}/clear-refund"), json!({})).await;
    assert_eq!(status, StatusCode::OK, "{d}");
    assert_eq!(d["needs_refund"], false);
    assert_eq!(d["status"], "pending_payment", "只清旗標、不動狀態");
}

#[sqlx::test(migrations = "./migrations")]
async fn buyer_cancel_also_expires_pending_payments(pool: PgPool) {
    let app = common::app(pool.clone());
    let (id, _, token) = place_order(&app, &pool, "home").await;
    let (status, _, _) = common::send(
        &app,
        common::req("POST", &format!("/api/orders/{id}/cancel?t={token}"), None, None),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(payment_statuses(&pool, id).await, vec!["expired".to_string()]);
    assert_eq!(stock_of(&pool, id).await, 5);
}
```

（買家取消的回應狀態碼以 `api/src/routes/orders.rs::cancel` 現況為準：它回 `ApiResult<StatusCode>`；若不是 204 就把斷言改成該值。）

Run: `DATABASE_URL=… cargo test --manifest-path api/Cargo.toml --test admin_orders`
Expected: 新測試失敗。

- [ ] **Step 2: orders.rs 與 invoices.rs**

`api/src/domain/orders.rs`：import 加 `use crate::domain::payments;`（與其他 `crate::domain::` import 放一起）。`cancel_in_tx` 改成：

```rust
/// 只有 pending_payment 能取消；成功就把 order_items 的數量加回庫存（規格 §4、§5）。
/// 回 Ok(false) 表示狀態不允許。過期 job、後台取消都經過這裡
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
    restore_stock_in_tx(tx, order_id).await?;
    Ok(true)
}

/// 把 order_items 的數量加回庫存（取消、過期、已付未出貨的退款共用；規格 §5）。呼叫者已持有訂單列的鎖
pub async fn restore_stock_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    order_id: Uuid,
) -> Result<(), ApiError> {
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
    Ok(())
}

/// 取消並把還在等的付款嘗試標 expired（計畫 3 審查交接 3、與規格不同之處 38）。
/// 鎖序 payments → orders → product_variants，和 expire_one／apply_return 一致。
/// 回 false 時呼叫者要 rollback（payments 的更新一併撤銷）
pub async fn cancel_with_payments_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    order_id: Uuid,
    reason: &str,
) -> Result<bool, ApiError> {
    sqlx::query(
        "UPDATE payments SET status = $2, updated_at = now() WHERE order_id = $1 AND status = $3",
    )
    .bind(order_id)
    .bind(payments::PAYMENT_EXPIRED)
    .bind(payments::PAYMENT_PENDING)
    .execute(&mut **tx)
    .await?;
    cancel_in_tx(tx, order_id, reason).await
}
```

`cancel`（買家）裡的 `if !cancel_in_tx(&mut tx, id, reason).await?` 改成 `if !cancel_with_payments_in_tx(&mut tx, id, reason).await?`（回 Err 時 `tx` 被丟掉就是 rollback）。

`api/src/domain/invoices.rs`：`record_failure` 的 SQL 加守衛（與規格不同之處 44）：

```rust
/// 記錄一次失敗；最後一次嘗試時把狀態標 failed（後台顯示、重試在計畫 4）。
/// 已開立的永遠不降級：一次成功一次失敗的並行不能把 issued 蓋成 failed
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
         WHERE order_id = $1 AND status <> $6",
    )
    .bind(order_id)
    .bind(response)
    .bind(error)
    .bind(final_attempt)
    .bind(STATUS_FAILED)
    .bind(STATUS_ISSUED)
    .execute(db)
    .await?;
    Ok(())
}

/// 後台重開（與規格不同之處 44）：只有 failed 能重設成 pending，之後排新的 issue_invoice job。回 false = 不是 failed
pub async fn reset_for_retry_in_tx(
    tx: &mut Transaction<'_, Postgres>,
    order_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let n = sqlx::query(
        "UPDATE invoices SET status = $2, error = NULL, updated_at = now() WHERE order_id = $1 AND status = $3",
    )
    .bind(order_id)
    .bind(STATUS_PENDING)
    .bind(STATUS_FAILED)
    .execute(&mut **tx)
    .await?
    .rows_affected();
    Ok(n > 0)
}
```

- [ ] **Step 3: `domain/admin_orders.rs` 與路由**

`api/src/domain/admin_orders.rs` 加（import 補 `use crate::domain::orders;`）：

```rust
/// 後台取消（規格 §4、與規格不同之處 43）：只允許 pending_payment；歸還庫存、pending 付款標 expired。
/// 回 false = 狀態不允許
pub async fn cancel(db: &PgPool, id: Uuid) -> Result<bool, ApiError> {
    let mut tx = db.begin().await?;
    if !orders::cancel_with_payments_in_tx(&mut tx, id, "admin").await? {
        tx.rollback().await?;
        return Ok(false);
    }
    tx.commit().await?;
    Ok(true)
}

/// 標記已退款（規格 §4、與規格不同之處 43）：paid 或 shipped → refunded；paid 時歸還庫存；pending 付款標
/// expired；needs_refund 清掉。錢由老闆在綠界後台退。鎖序 payments → orders → product_variants
pub async fn mark_refunded(db: &PgPool, id: Uuid) -> Result<bool, ApiError> {
    let mut tx = db.begin().await?;
    sqlx::query(
        "UPDATE payments SET status = $2, updated_at = now() WHERE order_id = $1 AND status = $3",
    )
    .bind(id)
    .bind(payments::PAYMENT_EXPIRED)
    .bind(payments::PAYMENT_PENDING)
    .execute(&mut *tx)
    .await?;
    let status: Option<String> =
        sqlx::query_scalar("SELECT status FROM orders WHERE id = $1 FOR UPDATE")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;
    let Some(status) = status else {
        tx.rollback().await?;
        return Ok(false);
    };
    if status != STATUS_PAID && status != STATUS_SHIPPED {
        tx.rollback().await?;
        return Ok(false);
    }
    if status == STATUS_PAID {
        orders::restore_stock_in_tx(&mut tx, id).await?;
    }
    sqlx::query(
        "UPDATE orders SET status = $2, needs_refund = false, cancelled_at = now(), cancel_reason = 'refunded' WHERE id = $1",
    )
    .bind(id)
    .bind(STATUS_REFUNDED)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(true)
}

/// 遲到付款已在綠界後台退款 → 清掉「需退款」（規格 §4）。回 false = 本來就沒有
pub async fn clear_refund(db: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
    let n = sqlx::query("UPDATE orders SET needs_refund = false WHERE id = $1 AND needs_refund")
        .bind(id)
        .execute(db)
        .await?
        .rows_affected();
    Ok(n > 0)
}
```

`api/src/routes/admin_orders.rs`：router 加：

```rust
        .route("/api/admin/orders/{id}/cancel", post(cancel))
        .route("/api/admin/orders/{id}/mark-refunded", post(mark_refunded))
        .route("/api/admin/orders/{id}/retry-invoice", post(retry_invoice))
        .route("/api/admin/orders/{id}/clear-refund", post(clear_refund))
```

import 補 `domain::{invoices, jobs}`。檔案最後加：

```rust
async fn ensure_exists(state: &AppState, id: Uuid) -> ApiResult<()> {
    if orders::get_detail(&state.db, id).await?.is_none() {
        return Err(ApiError::NotFound);
    }
    Ok(())
}

/// 後台取消：只有待付款（規格 §4）
async fn cancel(
    _admin: AdminUser,
    State(state): State<AppState>,
    AppPath(id): AppPath<Uuid>,
) -> ApiResult<Json<AdminOrderDetail>> {
    ensure_exists(&state, id).await?;
    if !admin_orders::cancel(&state.db, id).await? {
        return Err(ApiError::field(
            "status",
            "只有待付款的訂單能取消；已付款的請先在綠界後台退款，再按「標記已退款」",
        ));
    }
    admin_detail(&state, id).await
}

/// 標記已退款（規格 §4）
async fn mark_refunded(
    _admin: AdminUser,
    State(state): State<AppState>,
    AppPath(id): AppPath<Uuid>,
) -> ApiResult<Json<AdminOrderDetail>> {
    ensure_exists(&state, id).await?;
    if !admin_orders::mark_refunded(&state.db, id).await? {
        return Err(ApiError::field(
            "status",
            "只有已付款或已出貨的訂單能標記退款",
        ));
    }
    admin_detail(&state, id).await
}

/// 重開發票（規格 §8.4、與規格不同之處 44）：failed → pending，排新的 issue_invoice job
async fn retry_invoice(
    _admin: AdminUser,
    State(state): State<AppState>,
    AppPath(id): AppPath<Uuid>,
) -> ApiResult<Json<AdminOrderDetail>> {
    ensure_exists(&state, id).await?;
    let mut tx = state.db.begin().await?;
    if !invoices::reset_for_retry_in_tx(&mut tx, id).await? {
        tx.rollback().await?;
        return Err(ApiError::field("invoice", "只有開立失敗的發票能重試"));
    }
    jobs::enqueue(
        &mut tx,
        jobs::KIND_ISSUE_INVOICE,
        json!({ "order_id": id }),
        Some(&format!("invoice:{id}:retry:{}", Utc::now().timestamp())),
    )
    .await?;
    tx.commit().await?;
    admin_detail(&state, id).await
}

/// 遲到付款已處理（規格 §4）
async fn clear_refund(
    _admin: AdminUser,
    State(state): State<AppState>,
    AppPath(id): AppPath<Uuid>,
) -> ApiResult<Json<AdminOrderDetail>> {
    ensure_exists(&state, id).await?;
    if !admin_orders::clear_refund(&state.db, id).await? {
        return Err(ApiError::field("needs_refund", "這筆訂單沒有待處理的退款"));
    }
    admin_detail(&state, id).await
}
```

- [ ] **Step 4: 跑測試、fmt、clippy**

Run: `DATABASE_URL=… cargo test --manifest-path api/Cargo.toml --test admin_orders --test orders --test jobs_worker --test invoice_job`
Expected: 全綠（admin_orders 19 個；orders、jobs_worker、invoice_job 行為不變）。
Run: 全部測試、fmt、clippy。
Expected: 全綠、乾淨。

- [ ] **Step 5: Commit**

```bash
git add api/src/domain/orders.rs api/src/domain/invoices.rs api/src/domain/admin_orders.rs api/src/routes/admin_orders.rs api/tests/admin_orders.rs
git commit -m "feat(api): 後台取消（付款嘗試一併作廢）、標記已退款（未出貨歸還庫存）、重開發票、清除需退款"
```

---

### Task 8: 儀表板 API

**Files:**
- Modify: `api/src/domain/admin_orders.rs`（`Dashboard`、`dashboard`）
- Modify: `api/src/routes/admin_orders.rs`（`GET /api/admin/dashboard`）
- Modify: `api/tests/admin_orders.rs`

**Interfaces:**
- Consumes: `admin_orders::list`（Task 5）。
- Produces: `admin_orders::DASHBOARD_LIST_LIMIT: i64 = 5`；`admin_orders::Dashboard { today_orders: i64, today_paid_total: i64, pending_shipment: i64, invoice_failed: i64, needs_refund: i64, cvs_returned: i64, pending_shipment_items, needs_refund_items, cvs_returned_items, invoice_failed_items: Vec<AdminOrderListItem> }`；`admin_orders::dashboard(db) -> Result<Dashboard, ApiError>`；HTTP `GET /api/admin/dashboard`。

- [ ] **Step 1: 寫整合測試（先失敗）**

`api/tests/admin_orders.rs` 加：

```rust
#[sqlx::test(migrations = "./migrations")]
async fn dashboard_counts_today_and_lists_attention_items(pool: PgPool) {
    let app = common::app(pool.clone());
    let admin = common::admin_cookie(&app, &pool).await;

    let (status, d) = get(&app, &admin, "/api/admin/dashboard").await;
    assert_eq!(status, StatusCode::OK, "{d}");
    assert_eq!(d["today_orders"], 0);
    assert_eq!(d["pending_shipment"], 0);
    assert!(d["pending_shipment_items"].as_array().unwrap().is_empty());

    let (_pending, _, _) = place_order(&app, &pool, "home").await; // 今日、待付款
    let (paid, _, _) = place_order(&app, &pool, "cvs").await; // 今日、已付款 660
    mark_paid(&pool, paid).await;
    let (old, _, _) = place_order(&app, &pool, "home").await; // 前天、已付款、需退款、發票失敗
    mark_paid(&pool, old).await;
    sqlx::query("UPDATE orders SET created_at = now() - interval '2 days', needs_refund = true WHERE id = $1")
        .bind(old)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE invoices SET status = 'failed', error = 'x' WHERE order_id = $1")
        .bind(old)
        .execute(&pool)
        .await
        .unwrap();
    let (returned, _, _) = place_order(&app, &pool, "cvs").await; // 今日、已出貨、超商退回
    mark_paid(&pool, returned).await;
    sqlx::query("UPDATE orders SET status = 'shipped', shipped_at = now() WHERE id = $1")
        .bind(returned)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE shipments SET status = 'returned' WHERE order_id = $1")
        .bind(returned)
        .execute(&pool)
        .await
        .unwrap();

    let (status, d) = get(&app, &admin, "/api/admin/dashboard").await;
    assert_eq!(status, StatusCode::OK, "{d}");
    assert_eq!(d["today_orders"], 3, "前天那筆不算");
    assert_eq!(d["today_paid_total"], 660 + 660, "今日已付款（含已出貨）的總額");
    assert_eq!(d["pending_shipment"], 2, "paid 的兩筆（含前天）");
    assert_eq!(d["invoice_failed"], 1);
    assert_eq!(d["needs_refund"], 1);
    assert_eq!(d["cvs_returned"], 1);
    let ids = |key: &str| -> Vec<String> {
        d[key].as_array().unwrap().iter().map(|i| i["id"].as_str().unwrap().to_string()).collect()
    };
    assert_eq!(ids("pending_shipment_items"), vec![paid.to_string(), old.to_string()], "新到舊：old 的 created_at 被改成前天");
    assert_eq!(ids("needs_refund_items"), vec![old.to_string()]);
    assert_eq!(ids("cvs_returned_items"), vec![returned.to_string()]);
    assert_eq!(ids("invoice_failed_items"), vec![old.to_string()]);
    assert_eq!(d["cvs_returned_items"][0]["shipment_status"], "returned");

    let (status, _, _) = common::send(&app, common::req("GET", "/api/admin/dashboard", None, None)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
```

Run: `DATABASE_URL=… cargo test --manifest-path api/Cargo.toml --test admin_orders dashboard`
Expected: 404 失敗。

- [ ] **Step 2: 實作**

`api/src/domain/admin_orders.rs` 加：

```rust
/// 儀表板每個清單最多幾筆
pub const DASHBOARD_LIST_LIMIT: i64 = 5;

/// 後台首頁（規格 §6.1）：今日訂單、待出貨、發票開立失敗、需退款、超商退回。「今日」用台北日期（與規格不同之處 45）
#[derive(Debug, Serialize)]
pub struct Dashboard {
    pub today_orders: i64,
    /// 今日成立且已付款（paid／shipped／completed）的總額
    pub today_paid_total: i64,
    pub pending_shipment: i64,
    pub invoice_failed: i64,
    pub needs_refund: i64,
    pub cvs_returned: i64,
    pub pending_shipment_items: Vec<AdminOrderListItem>,
    pub needs_refund_items: Vec<AdminOrderListItem>,
    pub cvs_returned_items: Vec<AdminOrderListItem>,
    pub invoice_failed_items: Vec<AdminOrderListItem>,
}

pub async fn dashboard(db: &PgPool) -> Result<Dashboard, ApiError> {
    let (today_orders, today_paid_total): (i64, i64) = sqlx::query_as(
        "SELECT COUNT(*),
                COALESCE(SUM(total) FILTER (WHERE status IN ('paid', 'shipped', 'completed')), 0)::bigint
         FROM orders
         WHERE created_at >= (date_trunc('day', now() AT TIME ZONE 'Asia/Taipei') AT TIME ZONE 'Asia/Taipei')",
    )
    .fetch_one(db)
    .await?;
    let (pending_shipment, invoice_failed, needs_refund, cvs_returned): (i64, i64, i64, i64) =
        sqlx::query_as(
            "SELECT (SELECT COUNT(*) FROM orders WHERE status = 'paid'),
                    (SELECT COUNT(*) FROM invoices WHERE status = 'failed'),
                    (SELECT COUNT(*) FROM orders WHERE needs_refund),
                    (SELECT COUNT(*) FROM orders o JOIN shipments s ON s.order_id = o.id
                      WHERE o.status = 'shipped' AND s.status = 'returned')",
        )
        .fetch_one(db)
        .await?;
    let limit = DASHBOARD_LIST_LIMIT;
    Ok(Dashboard {
        today_orders,
        today_paid_total,
        pending_shipment,
        invoice_failed,
        needs_refund,
        cvs_returned,
        pending_shipment_items: list(db, None, Some(STATUS_PAID), None, 1, limit).await?.items,
        needs_refund_items: list(db, None, None, Some(Flag::NeedsRefund), 1, limit).await?.items,
        cvs_returned_items: list(db, None, None, Some(Flag::CvsReturned), 1, limit).await?.items,
        invoice_failed_items: list(db, None, None, Some(Flag::InvoiceFailed), 1, limit).await?.items,
    })
}
```

`api/src/routes/admin_orders.rs`：router 加 `.route("/api/admin/dashboard", get(dashboard))`（放在最前面），import 補 `admin_orders::Dashboard`，加：

```rust
async fn dashboard(
    _admin: AdminUser,
    State(state): State<AppState>,
) -> ApiResult<Json<Dashboard>> {
    Ok(Json(admin_orders::dashboard(&state.db).await?))
}
```

- [ ] **Step 3: 跑測試、fmt、clippy、commit**

Run: `DATABASE_URL=… cargo test --manifest-path api/Cargo.toml --test admin_orders`；全部測試、fmt、clippy。
Expected: 全綠、乾淨。

```bash
git add api/src/domain/admin_orders.rs api/src/routes/admin_orders.rs api/tests/admin_orders.rs
git commit -m "feat(api): 儀表板 API（今日訂單、待出貨、發票失敗、需退款、超商退回）"
```

---

### Task 9: 前端後台：訂單列表、訂單明細（所有動作）、儀表板

**Files:**
- Modify: `web/src/lib/types.ts`（後台型別）、`web/src/lib/labels.ts`（出貨／付款／發票狀態、旗標文案）、`web/src/lib/ecpay.ts`（`target`）
- Create: `web/src/lib/adminOrders.ts`、`web/src/lib/adminOrders.test.ts`
- Modify: `web/src/routes/admin/+layout.svelte:7-12`（「訂單」連結）
- Create: `web/src/routes/admin/+page.server.ts`；Modify: `web/src/routes/admin/+page.svelte`（儀表板）
- Create: `web/src/routes/admin/orders/+page.server.ts`、`+page.svelte`、`web/src/routes/admin/orders/[id]/+page.server.ts`、`+page.svelte`

**Interfaces:**
- Consumes: Task 5～8 的 JSON 形狀（`AdminOrderListItem`、`AdminOrderDetail`、`Dashboard`、`CheckoutForm`）；`serverApi`、`api`、`toast`、`invalidateAll`、`Pagination`、`formatDate`、`twd`、`ORDER_STATUS_LABELS`、`CVS_LABELS`、`PAYMENT_LABELS`、`INVOICE_LABELS`。
- Produces: `postToEcpay(form, target?: '_blank')`；`availableActions(o: AdminOrderDetail): AdminAction[]`、`attentionNotes(o: AdminOrderDetail): string[]`；頁面 `/admin`、`/admin/orders`（查詢參數 `q`、`status`、`flag`、`page`）、`/admin/orders/[id]`。

- [ ] **Step 1: 型別、文案、`postToEcpay` 的 target**

`web/src/lib/types.ts` 最後加：

```ts
// ───── 後台訂單（計畫 4）─────
export type ShipmentStatus = 'pending' | 'created' | 'in_transit' | 'arrived' | 'picked_up' | 'returned' | 'shipped';
export type InvoiceStatus = 'pending' | 'issued' | 'failed';
export type PaymentStatus = 'pending' | 'paid' | 'failed' | 'expired';
export type AdminOrderFlag = 'needs_refund' | 'cvs_returned' | 'invoice_failed';
export type AdminOrderListItem = {
	id: string;
	order_no: string;
	status: OrderStatus;
	email: string;
	recipient_name: string;
	shipping_method: ShippingMethod;
	total: number;
	item_count: number;
	needs_refund: boolean;
	shipment_status: ShipmentStatus | null;
	invoice_status: InvoiceStatus | null;
	created_at: string;
	paid_at: string | null;
};
export type AdminShipment = {
	id: string;
	order_id: string;
	method: ShippingMethod;
	cvs_sub_type: CvsSubType | null;
	cvs_store_id: string | null;
	cvs_store_name: string | null;
	cvs_store_address: string | null;
	cvs_store_phone: string | null;
	home_postal_code: string | null;
	home_city: string | null;
	home_district: string | null;
	home_street: string | null;
	status: ShipmentStatus;
	ecpay_logistics_id: string | null;
	ecpay_merchant_trade_no: string | null;
	cvs_payment_no: string | null;
	cvs_validation_no: string | null;
	carrier: string | null;
	tracking_no: string | null;
	/** 綠界貨態代碼，或 creating／create_failed／create_error */
	last_status_code: string | null;
	last_status_msg: string | null;
	created_at: string;
	updated_at: string;
};
export type AdminPayment = {
	id: string;
	order_id: string;
	merchant_trade_no: string;
	method: PaymentMethod | 'cod';
	status: PaymentStatus;
	amount: number;
	ecpay_trade_no: string | null;
	payment_type: string | null;
	payment_date: string | null;
	atm_bank_code: string | null;
	atm_vaccount: string | null;
	cvs_payment_no: string | null;
	expire_at: string | null;
	created_at: string;
};
export type AdminInvoice = OrderInvoice & { error: string | null; updated_at: string };
export type AdminOrderDetail = Omit<OrderDetail, 'shipment' | 'payment' | 'invoice'> & {
	user_id: string | null;
	needs_refund: boolean;
	shipment: AdminShipment | null;
	/** 全部付款嘗試，新到舊 */
	payments: AdminPayment[];
	invoice: AdminInvoice | null;
};
export type Dashboard = {
	today_orders: number;
	today_paid_total: number;
	pending_shipment: number;
	invoice_failed: number;
	needs_refund: number;
	cvs_returned: number;
	pending_shipment_items: AdminOrderListItem[];
	needs_refund_items: AdminOrderListItem[];
	cvs_returned_items: AdminOrderListItem[];
	invoice_failed_items: AdminOrderListItem[];
};
```

`web/src/lib/labels.ts` 最後加（import 補 `AdminOrderFlag, InvoiceStatus, PaymentStatus, ShipmentStatus`）：

```ts
export const SHIPMENT_STATUS_LABELS: Record<ShipmentStatus, string> = {
	pending: '未出貨',
	created: '物流單已建立',
	in_transit: '運送中',
	arrived: '已到門市',
	picked_up: '已取件',
	returned: '未取退回',
	shipped: '已寄出'
};
export const PAYMENT_STATUS_LABELS: Record<PaymentStatus, string> = {
	pending: '等待付款',
	paid: '已付款',
	failed: '失敗',
	expired: '已作廢'
};
export const INVOICE_STATUS_LABELS: Record<InvoiceStatus, string> = {
	pending: '開立中',
	issued: '已開立',
	failed: '開立失敗'
};
export const ADMIN_FLAG_LABELS: Record<AdminOrderFlag, string> = {
	needs_refund: '需退款',
	cvs_returned: '超商退回',
	invoice_failed: '發票失敗'
};
```

`web/src/lib/ecpay.ts` 改成：

```ts
import type { EcpayForm } from '$lib/types';

/** 用隱藏表單把欄位 POST 到綠界（規格 §7 第 7 點）。target='_blank' 開新分頁（列印託運單；綠界禁止 iframe） */
export function postToEcpay(form: EcpayForm, target?: '_blank'): void {
	const el = document.createElement('form');
	el.method = 'POST';
	el.action = form.action;
	if (target) el.target = target;
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
	if (target) el.remove();
}
```

- [ ] **Step 2: `adminOrders.ts` 與測試（先寫測試）**

`web/src/lib/adminOrders.test.ts`：

```ts
import { describe, expect, it } from 'vitest';
import { attentionNotes, availableActions } from './adminOrders';
import type { AdminOrderDetail, AdminShipment } from './types';

function shipment(over: Partial<AdminShipment> = {}): AdminShipment {
	return {
		id: 's1',
		order_id: 'o1',
		method: 'cvs',
		cvs_sub_type: 'UNIMARTC2C',
		cvs_store_id: '131386',
		cvs_store_name: '測試門市',
		cvs_store_address: '台北市',
		cvs_store_phone: null,
		home_postal_code: null,
		home_city: null,
		home_district: null,
		home_street: null,
		status: 'pending',
		ecpay_logistics_id: null,
		ecpay_merchant_trade_no: null,
		cvs_payment_no: null,
		cvs_validation_no: null,
		carrier: null,
		tracking_no: null,
		last_status_code: null,
		last_status_msg: null,
		created_at: '2026-09-08T00:00:00Z',
		updated_at: '2026-09-08T00:00:00Z',
		...over
	};
}

function order(over: Partial<AdminOrderDetail> = {}): AdminOrderDetail {
	return {
		id: 'o1',
		order_no: 'DS260908ABCD',
		status: 'paid',
		email: 'a@b.tw',
		recipient_name: '王小明',
		recipient_phone: '0912345678',
		shipping_method: 'cvs',
		subtotal: 600,
		shipping_fee: 60,
		total: 660,
		note: '',
		invoice_type: 'personal',
		invoice_carrier_type: '1',
		invoice_carrier_num: null,
		invoice_tax_id: null,
		invoice_title: null,
		invoice_address: null,
		invoice_love_code: null,
		created_at: '2026-09-08T00:00:00Z',
		paid_at: '2026-09-08T00:00:00Z',
		shipped_at: null,
		completed_at: null,
		cancelled_at: null,
		cancel_reason: null,
		items: [],
		user_id: null,
		needs_refund: false,
		shipment: shipment(),
		payments: [],
		invoice: { status: 'pending', invoice_no: null, invoice_date: null, random_number: null, error: null, updated_at: '2026-09-08T00:00:00Z' },
		...over
	};
}

describe('availableActions', () => {
	it('已付款超商：建物流單、標記退款；宅配：填單號', () => {
		expect(availableActions(order())).toEqual(['ship_cvs', 'mark_refunded']);
		expect(availableActions(order({ shipping_method: 'home', shipment: shipment({ method: 'home' }) }))).toEqual(['ship_home', 'mark_refunded']);
	});
	it('已出貨超商：列印、完成、標記退款', () => {
		const o = order({ status: 'shipped', shipment: shipment({ status: 'created', ecpay_logistics_id: '10035', cvs_payment_no: 'F1' }) });
		expect(availableActions(o)).toEqual(['print_label', 'complete', 'mark_refunded']);
	});
	it('待付款只能取消；完成／取消／退款沒有動作', () => {
		expect(availableActions(order({ status: 'pending_payment' }))).toEqual(['cancel']);
		expect(availableActions(order({ status: 'completed' }))).toEqual([]);
		expect(availableActions(order({ status: 'cancelled' }))).toEqual([]);
		expect(availableActions(order({ status: 'refunded' }))).toEqual([]);
	});
	it('發票失敗可重開、需退款可清除', () => {
		const o = order({ status: 'completed', needs_refund: true, invoice: { status: 'failed', invoice_no: null, invoice_date: null, random_number: null, error: 'x', updated_at: '' } });
		expect(availableActions(o)).toEqual(['retry_invoice', 'clear_refund']);
	});
});

describe('attentionNotes', () => {
	it('正常訂單沒有提醒', () => {
		expect(attentionNotes(order())).toEqual([]);
	});
	it('需退款、超商退回、建單失敗、發票失敗、退款後發票已開立', () => {
		const o = order({
			status: 'shipped',
			needs_refund: true,
			shipment: shipment({ status: 'returned', last_status_code: 'create_failed', last_status_msg: '收件人姓名格式錯誤' }),
			invoice: { status: 'failed', invoice_no: null, invoice_date: null, random_number: null, error: '綠界回錯', updated_at: '' }
		});
		const notes = attentionNotes(o);
		expect(notes.some((n) => n.includes('已處理退款'))).toBe(true);
		expect(notes.some((n) => n.includes('超商未取件'))).toBe(true);
		expect(notes.some((n) => n.includes('收件人姓名格式錯誤'))).toBe(true);
		expect(notes.some((n) => n.includes('綠界回錯'))).toBe(true);
		const refunded = order({ status: 'refunded', invoice: { status: 'issued', invoice_no: 'AB1', invoice_date: null, random_number: '1', error: null, updated_at: '' } });
		expect(attentionNotes(refunded).some((n) => n.includes('作廢發票'))).toBe(true);
	});
});
```

`web/src/lib/adminOrders.ts`：

```ts
import type { AdminOrderDetail } from '$lib/types';

export type AdminAction =
	| 'ship_cvs'
	| 'ship_home'
	| 'print_label'
	| 'complete'
	| 'cancel'
	| 'mark_refunded'
	| 'retry_invoice'
	| 'clear_refund';

/** 依訂單狀態決定哪些按鈕可用（規格 §4、§6.1；與後端各路由的狀態檢查一致） */
export function availableActions(o: AdminOrderDetail): AdminAction[] {
	const actions: AdminAction[] = [];
	if (o.status === 'paid' && o.shipping_method === 'cvs') actions.push('ship_cvs');
	if (o.status === 'paid' && o.shipping_method === 'home') actions.push('ship_home');
	if (o.shipping_method === 'cvs' && o.shipment?.ecpay_logistics_id && o.shipment.cvs_payment_no) actions.push('print_label');
	if (o.status === 'shipped') actions.push('complete');
	if (o.status === 'pending_payment') actions.push('cancel');
	if (o.status === 'paid' || o.status === 'shipped') actions.push('mark_refunded');
	if (o.invoice?.status === 'failed') actions.push('retry_invoice');
	if (o.needs_refund) actions.push('clear_refund');
	return actions;
}

/** 需要老闆注意的提醒（列表標紅、明細頁上方；規格 §4） */
export function attentionNotes(o: AdminOrderDetail): string[] {
	const notes: string[] = [];
	if (o.needs_refund) notes.push('有一筆遲到或金額不符的付款：請到綠界後台退款後按「已處理退款」');
	if (o.status === 'shipped' && o.shipment?.status === 'returned') notes.push('超商未取件已退回：請到綠界後台退款後按「標記已退款」');
	const code = o.shipment?.last_status_code;
	if (code === 'create_failed' || code === 'create_error') notes.push(`上次建立物流單失敗：${o.shipment?.last_status_msg ?? '請稍後再試'}`);
	if (o.invoice?.status === 'failed') notes.push(`發票開立失敗：${o.invoice.error ?? ''}`);
	if (o.status === 'refunded' && o.invoice?.status === 'issued') notes.push('已退款但發票已開立：請至綠界後台作廢發票');
	return notes;
}
```

Run: `pnpm -C web test`
Expected: 全過（原本 29 個 + 6 個）。

- [ ] **Step 3: 側欄與儀表板**

`web/src/routes/admin/+layout.svelte` 的 `links` 改成：

```ts
	const links = [
		{ href: '/admin', label: '儀表板' },
		{ href: '/admin/orders', label: '訂單' },
		{ href: '/admin/products', label: '商品' },
		{ href: '/admin/categories', label: '分類' },
		{ href: '/admin/settings', label: '設定' }
	];
```

`web/src/routes/admin/+page.server.ts`：

```ts
import { serverApi } from '$lib/server/api';
import type { Dashboard } from '$lib/types';
import type { PageServerLoad } from './$types';

export const load: PageServerLoad = async (event) => {
	const dashboard = await serverApi<Dashboard>(event, '/api/admin/dashboard');
	return { dashboard };
};
```

`web/src/routes/admin/+page.svelte` 整檔改成：

```svelte
<script lang="ts">
	import { formatDate, twd } from '$lib/format';
	import { ORDER_STATUS_LABELS } from '$lib/labels';
	import type { AdminOrderListItem } from '$lib/types';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
	const d = $derived(data.dashboard);

	const tiles = $derived([
		{ label: '今日訂單', value: String(d.today_orders), note: `已付款 ${twd(d.today_paid_total)}`, href: '/admin/orders' },
		{ label: '待出貨', value: String(d.pending_shipment), note: '已付款、還沒出貨', href: '/admin/orders?status=paid' },
		{ label: '發票開立失敗', value: String(d.invoice_failed), note: '到訂單頁重開', href: '/admin/orders?flag=invoice_failed' },
		{ label: '需退款', value: String(d.needs_refund), note: '遲到或金額不符的付款', href: '/admin/orders?flag=needs_refund' },
		{ label: '超商退回', value: String(d.cvs_returned), note: '買家未取件', href: '/admin/orders?flag=cvs_returned' }
	]);
</script>

<svelte:head><title>儀表板</title></svelte:head>

<h1 class="text-2xl font-bold">儀表板</h1>

<div class="mt-4 grid grid-cols-2 gap-3 md:grid-cols-5">
	{#each tiles as t (t.label)}
		<a href={t.href} class="rounded border border-gray-200 bg-white p-4 hover:border-gray-400">
			<div class="text-sm text-gray-500">{t.label}</div>
			<div class="text-2xl font-bold">{t.value}</div>
			<div class="text-xs text-gray-500">{t.note}</div>
		</a>
	{/each}
</div>

{#snippet list(title: string, items: AdminOrderListItem[], href: string)}
	<section class="rounded border border-gray-200 bg-white">
		<div class="flex items-center justify-between border-b border-gray-200 px-4 py-2">
			<h2 class="font-medium">{title}</h2>
			<a {href} class="text-sm underline">全部</a>
		</div>
		{#if items.length === 0}
			<p class="p-4 text-sm text-gray-500">沒有</p>
		{:else}
			<ul class="divide-y divide-gray-100 text-sm">
				{#each items as o (o.id)}
					<li class="flex items-center justify-between gap-2 px-4 py-2">
						<a href={`/admin/orders/${o.id}`} class="font-medium hover:underline">{o.order_no}</a>
						<span class="min-w-0 flex-1 truncate text-gray-600">{o.recipient_name}｜{o.item_count} 件｜{twd(o.total)}</span>
						<span class="text-gray-500">{ORDER_STATUS_LABELS[o.status]}</span>
						<span class="text-xs text-gray-500">{formatDate(o.created_at)}</span>
					</li>
				{/each}
			</ul>
		{/if}
	</section>
{/snippet}

<div class="mt-6 grid gap-4 lg:grid-cols-2">
	{@render list('待出貨', d.pending_shipment_items, '/admin/orders?status=paid')}
	{@render list('需退款', d.needs_refund_items, '/admin/orders?flag=needs_refund')}
	{@render list('超商退回', d.cvs_returned_items, '/admin/orders?flag=cvs_returned')}
	{@render list('發票開立失敗', d.invoice_failed_items, '/admin/orders?flag=invoice_failed')}
</div>
```

- [ ] **Step 4: 訂單列表頁**

`web/src/routes/admin/orders/+page.server.ts`：

```ts
import { serverApi } from '$lib/server/api';
import type { AdminOrderListItem, Page } from '$lib/types';
import type { PageServerLoad } from './$types';

export const load: PageServerLoad = async (event) => {
	const sp = event.url.searchParams;
	const qs = new URLSearchParams();
	for (const key of ['q', 'status', 'flag', 'page']) {
		const value = sp.get(key);
		if (value) qs.set(key, value);
	}
	const result = await serverApi<Page<AdminOrderListItem>>(event, `/api/admin/orders?${qs}`);
	return { result, q: sp.get('q') ?? '', status: sp.get('status') ?? '', flag: sp.get('flag') ?? '' };
};
```

`web/src/routes/admin/orders/+page.svelte`：

```svelte
<script lang="ts">
	import Pagination from '$lib/components/Pagination.svelte';
	import { formatDate, twd } from '$lib/format';
	import { ADMIN_FLAG_LABELS, INVOICE_STATUS_LABELS, ORDER_STATUS_LABELS, SHIPMENT_STATUS_LABELS } from '$lib/labels';
	import type { AdminOrderFlag, AdminOrderListItem, OrderStatus } from '$lib/types';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	const statuses = Object.keys(ORDER_STATUS_LABELS) as OrderStatus[];
	const flags = Object.keys(ADMIN_FLAG_LABELS) as AdminOrderFlag[];

	/** 要老闆注意的列標紅（規格 §4） */
	function attention(o: AdminOrderListItem): string[] {
		const tags: string[] = [];
		if (o.needs_refund) tags.push('需退款');
		if (o.status === 'shipped' && o.shipment_status === 'returned') tags.push('超商退回');
		if (o.invoice_status === 'failed') tags.push('發票失敗');
		return tags;
	}
</script>

<svelte:head><title>訂單管理</title></svelte:head>

<h1 class="text-2xl font-bold">訂單</h1>

<form method="GET" class="mt-4 flex flex-wrap gap-2">
	<input name="q" value={data.q} placeholder="訂單編號、Email、收件人" class="rounded border border-gray-300 px-3 py-2 text-sm" />
	<select name="status" value={data.status} class="rounded border border-gray-300 px-3 py-2 text-sm">
		<option value="">全部狀態</option>
		{#each statuses as s (s)}
			<option value={s}>{ORDER_STATUS_LABELS[s]}</option>
		{/each}
	</select>
	<select name="flag" value={data.flag} class="rounded border border-gray-300 px-3 py-2 text-sm">
		<option value="">全部</option>
		{#each flags as f (f)}
			<option value={f}>{ADMIN_FLAG_LABELS[f]}</option>
		{/each}
	</select>
	<button type="submit" class="rounded border border-gray-300 px-4 py-2 text-sm">篩選</button>
</form>

<div class="mt-4 overflow-x-auto">
	<table class="w-full bg-white text-sm">
		<thead>
			<tr class="border-b border-gray-200 text-left">
				<th class="p-2">訂單</th>
				<th class="p-2">時間</th>
				<th class="p-2">買家</th>
				<th class="p-2">取貨</th>
				<th class="p-2">金額</th>
				<th class="p-2">狀態</th>
				<th class="p-2">提醒</th>
			</tr>
		</thead>
		<tbody>
			{#each data.result.items as o (o.id)}
				{@const tags = attention(o)}
				<tr class="border-b border-gray-100 {tags.length > 0 ? 'bg-red-50' : ''}">
					<td class="p-2"><a href={`/admin/orders/${o.id}`} class="font-medium hover:underline">{o.order_no}</a></td>
					<td class="p-2 text-gray-500">{formatDate(o.created_at)}</td>
					<td class="p-2">
						<div>{o.recipient_name}</div>
						<div class="text-xs text-gray-500">{o.email}</div>
					</td>
					<td class="p-2">
						<div>{o.shipping_method === 'cvs' ? '超商取貨' : '宅配'}</div>
						{#if o.shipment_status}<div class="text-xs text-gray-500">{SHIPMENT_STATUS_LABELS[o.shipment_status]}</div>{/if}
					</td>
					<td class="p-2">{twd(o.total)}<div class="text-xs text-gray-500">{o.item_count} 件</div></td>
					<td class="p-2">
						<div>{ORDER_STATUS_LABELS[o.status]}</div>
						{#if o.invoice_status}<div class="text-xs text-gray-500">發票{INVOICE_STATUS_LABELS[o.invoice_status]}</div>{/if}
					</td>
					<td class="p-2 text-red-700">{tags.join('、')}</td>
				</tr>
			{:else}
				<tr><td colspan="7" class="p-6 text-center text-gray-500">沒有訂單</td></tr>
			{/each}
		</tbody>
	</table>
</div>

<Pagination page={data.result.page} perPage={data.result.per_page} total={data.result.total} />
```

（svelte-check 的 unused import 警告算錯：只 import 有用到的東西。）

- [ ] **Step 5: 訂單明細頁**

`web/src/routes/admin/orders/[id]/+page.server.ts`：

```ts
import { error } from '@sveltejs/kit';
import { ApiError, serverApi } from '$lib/server/api';
import type { AdminOrderDetail } from '$lib/types';
import type { PageServerLoad } from './$types';

export const load: PageServerLoad = async (event) => {
	try {
		const order = await serverApi<AdminOrderDetail>(event, `/api/admin/orders/${encodeURIComponent(event.params.id)}`);
		return { order };
	} catch (e) {
		if (e instanceof ApiError && (e.status === 404 || e.status === 400)) error(404, '找不到這筆訂單');
		throw e;
	}
};
```

`web/src/routes/admin/orders/[id]/+page.svelte`：

```svelte
<script lang="ts">
	import { invalidateAll } from '$app/navigation';
	import { type AdminAction, attentionNotes, availableActions } from '$lib/adminOrders';
	import { api, ApiError } from '$lib/api';
	import { postToEcpay } from '$lib/ecpay';
	import { formatDate, twd } from '$lib/format';
	import { CVS_LABELS, INVOICE_LABELS, INVOICE_STATUS_LABELS, ORDER_STATUS_LABELS, PAYMENT_LABELS, PAYMENT_STATUS_LABELS, SHIPMENT_STATUS_LABELS } from '$lib/labels';
	import { toast } from '$lib/toast.svelte';
	import type { EcpayForm } from '$lib/types';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
	const o = $derived(data.order);
	const actions = $derived(availableActions(o));
	const notes = $derived(attentionNotes(o));

	let busy = $state(false);
	let confirming = $state<AdminAction | null>(null);
	let carrier = $state('');
	let trackingNo = $state('');
	let errors = $state<Record<string, string>>({});

	function paymentLabel(method: string): string {
		return method === 'cod' ? '取貨付款' : PAYMENT_LABELS[method as keyof typeof PAYMENT_LABELS];
	}

	/** 打一個後台動作；成功後重新載入明細 */
	async function act(path: string, body: unknown = {}, done = '完成') {
		if (busy) return;
		busy = true;
		errors = {};
		try {
			await api(`/api/admin/orders/${o.id}/${path}`, { method: 'POST', body: JSON.stringify(body) });
			confirming = null;
			await invalidateAll();
			toast.show(done);
		} catch (err) {
			if (err instanceof ApiError) {
				errors = err.fields();
				const first = Object.values(errors)[0];
				toast.show(first ?? err.message, 5000);
			} else {
				toast.show('操作失敗，請再試一次');
			}
			confirming = null;
			await invalidateAll();
		} finally {
			busy = false;
		}
	}

	/** 列印託運單：拿簽好的表單，在新分頁 POST 到綠界（規格 §8.3） */
	async function printLabel() {
		if (busy) return;
		busy = true;
		try {
			const form = await api<EcpayForm>(`/api/admin/orders/${o.id}/print-label`, { method: 'POST', body: '{}' });
			postToEcpay(form, '_blank');
		} catch (err) {
			toast.show(err instanceof ApiError ? err.message : '無法取得託運單');
		} finally {
			busy = false;
		}
	}

	/** 兩段確認的動作：第一次按顯示確認鈕，第二次才真的送出 */
	const CONFIRM = {
		complete: { path: 'complete', label: '標記完成', confirm: '確定標記完成', done: '已標記完成', danger: false },
		cancel: { path: 'cancel', label: '取消訂單', confirm: '確定取消這筆訂單', done: '訂單已取消', danger: true },
		mark_refunded: { path: 'mark-refunded', label: '標記已退款', confirm: '確定已在綠界後台退款', done: '已標記退款', danger: true },
		clear_refund: { path: 'clear-refund', label: '已處理退款', confirm: '確定已處理退款', done: '已清除需退款', danger: true }
	} as const;
	type ConfirmAction = keyof typeof CONFIRM;
	const confirmActions = Object.keys(CONFIRM) as ConfirmAction[];

	const btn = 'rounded px-3 py-2 text-sm disabled:opacity-50';
	const primary = `${btn} bg-gray-900 text-white`;
	const secondary = `${btn} border border-gray-300`;
	const danger = `${btn} bg-red-600 text-white`;
</script>

<svelte:head><title>訂單 {o.order_no}</title></svelte:head>

<div class="flex flex-wrap items-baseline justify-between gap-2">
	<div>
		<a href="/admin/orders" class="text-sm text-gray-500 underline">← 訂單列表</a>
		<h1 class="text-2xl font-bold">訂單 {o.order_no}</h1>
		<p class="text-sm text-gray-500">成立 {formatDate(o.created_at)}{#if o.paid_at}｜付款 {formatDate(o.paid_at)}{/if}{#if o.shipped_at}｜出貨 {formatDate(o.shipped_at)}{/if}{#if o.completed_at}｜完成 {formatDate(o.completed_at)}{/if}{#if o.cancelled_at}｜結束 {formatDate(o.cancelled_at)}{/if}</p>
	</div>
	<span class="rounded bg-gray-900 px-3 py-1 text-sm text-white">{ORDER_STATUS_LABELS[o.status]}</span>
</div>

{#if notes.length > 0}
	<div class="mt-4 rounded border border-red-300 bg-red-50 p-4 text-sm text-red-800">
		<ul class="list-disc pl-5">
			{#each notes as n, i (i)}<li>{n}</li>{/each}
		</ul>
	</div>
{/if}

{#if actions.length > 0}
	<section class="mt-4 rounded border border-gray-200 bg-white p-4">
		<h2 class="font-medium">動作</h2>
		<div class="mt-3 flex flex-wrap items-start gap-3">
			{#if actions.includes('ship_cvs')}
				<button type="button" class={primary} disabled={busy} onclick={() => act('ship-cvs', {}, '物流單已建立，訂單已出貨')}>
					建立物流單（{o.shipment?.cvs_sub_type ? CVS_LABELS[o.shipment.cvs_sub_type] : ''} {o.shipment?.cvs_store_name ?? ''}）
				</button>
			{/if}
			{#if actions.includes('ship_home')}
				<form class="flex flex-wrap items-end gap-2" onsubmit={(e) => { e.preventDefault(); void act('ship-home', { carrier, tracking_no: trackingNo }, '已出貨'); }}>
					<label class="block text-sm text-gray-700">
						貨運公司
						<input type="text" bind:value={carrier} placeholder="例如 黑貓" class="mt-1 block rounded border border-gray-300 px-3 py-2" />
						{#if errors.carrier}<span class="block text-red-600">{errors.carrier}</span>{/if}
					</label>
					<label class="block text-sm text-gray-700">
						單號
						<input type="text" bind:value={trackingNo} class="mt-1 block rounded border border-gray-300 px-3 py-2" />
						{#if errors.tracking_no}<span class="block text-red-600">{errors.tracking_no}</span>{/if}
					</label>
					<button type="submit" class={primary} disabled={busy}>宅配出貨</button>
				</form>
			{/if}
			{#if actions.includes('print_label')}
				<button type="button" class={secondary} disabled={busy} onclick={printLabel}>列印託運單</button>
			{/if}
			{#if actions.includes('retry_invoice')}
				<button type="button" class={secondary} disabled={busy} onclick={() => act('retry-invoice', {}, '已重新排入開立')}>重開發票</button>
			{/if}
			{#each confirmActions as a (a)}
				{#if actions.includes(a)}
					{@const c = CONFIRM[a]}
					{#if confirming === a}
						<span class="flex items-center gap-2">
							<button type="button" class={c.danger ? danger : primary} disabled={busy} onclick={() => act(c.path, {}, c.done)}>
								{c.confirm}
							</button>
							<button type="button" class={secondary} onclick={() => (confirming = null)}>返回</button>
						</span>
					{:else}
						<button type="button" class={secondary} disabled={busy} onclick={() => (confirming = a)}>{c.label}</button>
					{/if}
				{/if}
			{/each}
		</div>
		{#if actions.includes('mark_refunded')}
			<p class="mt-2 text-xs text-gray-500">退款要先在綠界廠商後台操作；這裡只記錄狀態{o.status === 'paid' ? '並把庫存加回去' : ''}。</p>
		{/if}
	</section>
{/if}

<section class="mt-4 rounded border border-gray-200 bg-white">
	<table class="w-full text-sm">
		<tbody>
			{#each o.items as item, i (i)}
				<tr class="border-b border-gray-100">
					<td class="p-3">
						{#if item.image_path}<img src={item.image_path} alt="" class="h-12 w-12 rounded object-cover" />{/if}
					</td>
					<td class="p-3">{item.product_name}<div class="text-xs text-gray-500">{item.variant_label}</div></td>
					<td class="p-3 text-right">{twd(item.unit_price)} × {item.quantity}</td>
					<td class="p-3 text-right">{twd(item.line_total)}</td>
				</tr>
			{/each}
		</tbody>
	</table>
	<div class="space-y-1 border-t border-gray-200 p-3 text-sm">
		<div class="flex justify-between"><span>商品小計</span><span>{twd(o.subtotal)}</span></div>
		<div class="flex justify-between"><span>運費</span><span>{o.shipping_fee === 0 ? '免運' : twd(o.shipping_fee)}</span></div>
		<div class="flex justify-between font-bold"><span>總計</span><span>{twd(o.total)}</span></div>
	</div>
</section>

<div class="mt-4 grid gap-4 md:grid-cols-2">
	<section class="rounded border border-gray-200 bg-white p-4 text-sm">
		<h2 class="font-medium">取貨</h2>
		<p class="mt-2">{o.recipient_name}　{o.recipient_phone}　<span class="text-gray-500">{o.email}</span></p>
		{#if o.shipment}
			{@const s = o.shipment}
			{#if s.method === 'cvs'}
				<p class="mt-1">超商取貨：{s.cvs_sub_type ? CVS_LABELS[s.cvs_sub_type] : ''} {s.cvs_store_name}（{s.cvs_store_id}）</p>
				<p class="text-gray-600">{s.cvs_store_address}</p>
			{:else}
				<p class="mt-1">宅配：{s.home_postal_code} {s.home_city}{s.home_district}{s.home_street}</p>
				{#if s.tracking_no}<p class="text-gray-600">{s.carrier} {s.tracking_no}</p>{/if}
			{/if}
			<p class="mt-2">出貨狀態：<span class="font-medium">{SHIPMENT_STATUS_LABELS[s.status]}</span></p>
			{#if s.ecpay_logistics_id}
				<p class="text-gray-600">綠界物流單 {s.ecpay_logistics_id}｜廠商單號 {s.ecpay_merchant_trade_no}{#if s.cvs_payment_no}｜寄貨編號 {s.cvs_payment_no}{/if}{#if s.cvs_validation_no}｜驗證碼 {s.cvs_validation_no}{/if}</p>
			{/if}
			{#if s.last_status_code}
				<p class="text-gray-600">最近通知：{s.last_status_code} {s.last_status_msg ?? ''}（{formatDate(s.updated_at)}）</p>
			{/if}
		{/if}
		{#if o.note}<p class="mt-2 text-gray-600">買家備註：{o.note}</p>{/if}
	</section>

	<section class="rounded border border-gray-200 bg-white p-4 text-sm">
		<h2 class="font-medium">發票</h2>
		<p class="mt-2">{INVOICE_LABELS[o.invoice_type]}</p>
		{#if o.invoice_type === 'company'}
			<p class="text-gray-600">統編 {o.invoice_tax_id}｜{o.invoice_title}｜{o.invoice_address}</p>
		{:else if o.invoice_type === 'donation'}
			<p class="text-gray-600">愛心碼 {o.invoice_love_code}</p>
		{:else if o.invoice_carrier_num}
			<p class="text-gray-600">載具 {o.invoice_carrier_num}</p>
		{/if}
		{#if o.invoice}
			<p class="mt-2">狀態：<span class="font-medium">{INVOICE_STATUS_LABELS[o.invoice.status]}</span></p>
			{#if o.invoice.invoice_no}
				<p class="text-gray-600">發票號碼 {o.invoice.invoice_no}　隨機碼 {o.invoice.random_number}{#if o.invoice.invoice_date}　{formatDate(o.invoice.invoice_date)}{/if}</p>
			{/if}
			{#if o.invoice.error}<p class="text-red-700">{o.invoice.error}</p>{/if}
		{/if}
	</section>
</div>

<section class="mt-4 rounded border border-gray-200 bg-white p-4 text-sm">
	<h2 class="font-medium">付款嘗試</h2>
	<div class="mt-2 overflow-x-auto">
		<table class="w-full">
			<thead>
				<tr class="border-b border-gray-200 text-left text-gray-500">
					<th class="p-2">綠界交易編號</th>
					<th class="p-2">方式</th>
					<th class="p-2">狀態</th>
					<th class="p-2">金額</th>
					<th class="p-2">綠界單號</th>
					<th class="p-2">繳費資訊</th>
					<th class="p-2">時間</th>
				</tr>
			</thead>
			<tbody>
				{#each o.payments as p (p.id)}
					<tr class="border-b border-gray-100">
						<td class="p-2 font-mono">{p.merchant_trade_no}</td>
						<td class="p-2">{paymentLabel(p.method)}</td>
						<td class="p-2">{PAYMENT_STATUS_LABELS[p.status]}</td>
						<td class="p-2">{twd(p.amount)}</td>
						<td class="p-2 font-mono">{p.ecpay_trade_no ?? '—'}</td>
						<td class="p-2 text-gray-600">
							{#if p.atm_vaccount}銀行 {p.atm_bank_code} 帳號 {p.atm_vaccount}{:else if p.cvs_payment_no}代碼 {p.cvs_payment_no}{:else}—{/if}
							{#if p.expire_at}<div class="text-xs">期限 {formatDate(p.expire_at)}</div>{/if}
						</td>
						<td class="p-2 text-gray-500">{formatDate(p.payment_date ?? p.created_at)}</td>
					</tr>
				{:else}
					<tr><td colspan="7" class="p-4 text-center text-gray-500">沒有付款紀錄</td></tr>
				{/each}
			</tbody>
		</table>
	</div>
</section>
```

- [ ] **Step 6: 檢查、手動走一次、commit**

Run: `pnpm -C web check`（0 錯誤 0 警告）、`pnpm -C web test`、`pnpm -C web build`。
Expected: 全過。

手動：啟動 api 與 web dev，登入 `admin@example.com` / `admin12345`，開 `/admin`（五個方塊與四個清單）、`/admin/orders`（篩選、標紅）、隨便一筆 `/admin/orders/<id>`（動作區依狀態出現；「取消訂單」兩段確認）。用 stage 走建單見 Task 10。跑完 `lsof -ti :8080`／`lsof -ti :5173` 取 pid `kill`。

```bash
git add web/src/lib/types.ts web/src/lib/labels.ts web/src/lib/ecpay.ts web/src/lib/adminOrders.ts web/src/lib/adminOrders.test.ts web/src/routes/admin/+layout.svelte web/src/routes/admin/+page.server.ts web/src/routes/admin/+page.svelte web/src/routes/admin/orders
git commit -m "feat(web): 後台儀表板、訂單列表（篩選、標紅）、訂單明細（建單、列印、宅配、完成、取消、退款、重開發票、清除需退款）"
```

---

### Task 10: 文件、驗收與交接

**Files:**
- Modify: `docs/dev/ecpay-stage.md`（「超商取貨（物流）」節、「舊訂單沒有 invoices 列」節、前置補一句）

**Interfaces:** 無程式碼。

- [ ] **Step 1: 更新 stage 走查文件**

`docs/dev/ecpay-stage.md` 的「前置」第 3 點結尾加一句：「`ECPAY_LOGISTICS_*` 留空會用物流 C2C 公開測試特店 2000933。」

在「重新付款」節之後、「忘記密碼」之前加：

````markdown
## 超商取貨（物流）

物流 C2C 測試特店 `2000933`；廠商後台 `https://vendor-stage.ecpay.com.tw`（`LogisticsC2CTest` / `test1234`）。測試環境的電子地圖是固定門市、不會跳地圖；**測試環境不會發物流狀態通知**（見第 5 點）。

1. 結帳選「超商取貨」→ 選 7-ELEVEN → 按「選擇門市」→ 綠界直接把固定門市 POST 回 `/api/ecpay/logistics/map-reply` → 回到結帳頁看到門市（網址 `?store=<20 碼 token>`）。換一家超商會清掉門市要重選；超過 1 小時回來會看到「門市選擇已逾時」。
2. 用信用卡付款（同「信用卡」節）→ 訂單「已付款」。
3. 後台 `/admin/settings` 填寄件人姓名（中文 5 字內）與手機（09 開頭 10 碼）；退貨門市可留空。
4. `/admin/orders/<id>` 按「建立物流單」：
   - 成功：狀態變「已出貨」，取貨區顯示「物流單已建立」、綠界物流單號、寄貨編號、驗證碼（7-11 才有）；api log `綠界物流單已建立`；Email log `【…】訂單 … 已出貨`；DB：`SELECT status, ecpay_merchant_trade_no, ecpay_logistics_id, cvs_payment_no, last_status_code, raw->'create_request'->>'GoodsAmount' FROM shipments`。廠商後台 → 物流管理 → 物流建單及查詢 看到同一張（`MerchantTradeNo` = 訂單編號 + `L01`）。
   - 失敗：頁面 toast「綠界沒有接受這張物流單…」，出貨區「上次建立物流單失敗：<綠界原文>」，訂單維持已付款；再按一次會用 `L02`。
   - 待確認（實作時無法自動驗證）：(a) 真實回應能通過 MD5 CheckMacValue 驗證（失敗時出貨區會顯示「回應簽章不符：…」；先到廠商後台確認是否已建單，再回報）；(b) `GoodsAmount` 用商品小計、`ReceiverEmail`、`LogisticsC2CReplyURL` 三個欄位被三家超商接受；(c) 全家、萊爾富回的 `CVSPaymentNo` 形狀（測試門市 全家 `006598`、萊爾富 `2001`）。
5. 「列印託運單」→ 新分頁出現綠界的託運單頁（7-11 `PrintUniMartC2COrderInfo`）。
6. 狀態通知：stage 不會發，用下面的 python 算簽章、curl 打本機（`RtnCode` 換 `2030`／`2073`／`2067` 各打一次，看訂單頁出貨狀態變「運送中」→「已到門市」→「已取件」且訂單變「已完成」；`2074` 看「未取退回」與列表標紅）：

   ```python
   import hashlib, subprocess
   from urllib.parse import quote_plus
   key, iv = "XBERn1YOvpM9nfZc", "h1ONHk4P4yqbl5LK"
   p = {"MerchantID": "2000933", "MerchantTradeNo": "DS260908ABCDL01", "RtnCode": "2030", "RtnMsg": "物流中心驗收成功",
        "AllPayLogisticsID": "10035", "LogisticsType": "CVS", "LogisticsSubType": "UNIMARTC2C", "GoodsAmount": "600",
        "UpdateStatusDate": "2026/09/10 18:30:00", "ReceiverName": "王小明", "ReceiverCellPhone": "0912345678"}
   raw = f"HashKey={key}&" + "&".join(f"{k}={v}" for k, v in sorted(p.items(), key=lambda kv: kv[0].lower())) + f"&HashIV={iv}"
   enc = quote_plus(raw, safe="").lower()
   for a, b in [("%2d", "-"), ("%5f", "_"), ("%2e", "."), ("%21", "!"), ("%2a", "*"), ("%28", "("), ("%29", ")")]:
       enc = enc.replace(a, b)
   p["CheckMacValue"] = hashlib.md5(enc.encode()).hexdigest().upper()
   subprocess.run(["curl", "-s", "-d", "&".join(f"{k}={quote_plus(v)}" for k, v in p.items()), "http://localhost:8080/api/ecpay/logistics/status"])
   ```

   `MerchantTradeNo` 改成你那筆的 `ecpay_merchant_trade_no`。回 `1|OK`；簽章錯回 400 `0|CheckMacValue Error`；找不到單回 `0|Unknown MerchantTradeNo`。
7. 宅配：另下一筆宅配訂單付款後，`/admin/orders/<id>` 填貨運公司與單號 → 「已出貨」、Email log 有貨運公司與單號。
8. 取消／退款／發票：待付款的訂單按「取消訂單」→ 已取消、庫存回來、付款嘗試「已作廢」；已付款的按「標記已退款」→ 已退款、庫存回來（已出貨的不回）；`UPDATE invoices SET status='failed', error='test' WHERE order_id=…` 後訂單頁出現紅字與「重開發票」，按下去幾秒後變已開立；`UPDATE orders SET needs_refund=true …` 後儀表板「需退款」有它，訂單頁「已處理退款」清掉。

## 舊訂單沒有 invoices 列

`0003_invoices.sql` 之前建立的開發用訂單沒有 `invoices` 列，重新付款成功後 `issue_invoice` 會直接失敗（不會誤打綠界）。要補的話：

```sql
INSERT INTO invoices (id, order_id, relate_number)
SELECT gen_random_uuid(), o.id, o.order_no FROM orders o
WHERE NOT EXISTS (SELECT 1 FROM invoices i WHERE i.order_id = o.id);
```

（只在開發資料庫用；正式環境從 0003 之後才會有訂單。）
````

- [ ] **Step 2: 從乾淨狀態跑總驗收（下面「全部做完後」清單第 1～4 點）並 commit**

```bash
git add docs/dev/ecpay-stage.md
git commit -m "docs(dev): 綠界物流 stage 走查（建單、列印、狀態通知模擬、宅配、取消退款）與舊訂單發票 backfill"
```

---

## 全部做完後，從乾淨狀態走一次（每一行都要成立）

1. `docker compose -f deploy/docker-compose.dev.yml up -d db` → healthy；`cargo run --manifest-path api/Cargo.toml` 啟動時跑完 `0004_cvs_map_requests.sql`，log 沒有 rustls panic；`.env` 的 `ECPAY_LOGISTICS_*` 空白也能起來（stage 退回 2000933）。
2. `DATABASE_URL=… cargo test --manifest-path api/Cargo.toml` 全綠（單元 + 23 個整合測試檔），含：MD5 CheckMacValue 對照綠界文件範例、建單欄位與回應解析（成功／拒絕／簽章不符）、貨態代碼對照與不倒退、map-reply 的 token 單次使用與過期、狀態回呼取件完成 → 訂單完成、退回標記、建單認領與冪等收尾、宅配出貨信、後台取消／退款的庫存與付款嘗試、重開發票、儀表板計數、admin 權限（401／403）。`cargo fmt --all --check`、`cargo clippy --all-targets -- -D warnings` 乾淨。
3. `pnpm -C web test`（35 個）、`pnpm -C web check`（0 錯誤 0 警告）、`pnpm -C web build` 全過。
4. `pnpm -C web test:e2e` → 2 passed：宅配主流程（計畫 3）；超商取貨：攔到送往 `logistics-stage.ecpay.com.tw/Express/map` 的表單（`MerchantID=2000933`、`ExtraData` 20 碼、無 `CheckMacValue`）、模擬門市回傳 303、結帳頁還原草稿並顯示門市、送出訂單、訂單頁顯示門市。
5. 後台：`/admin` 五個方塊與四個清單；`/admin/orders` 依狀態／旗標／關鍵字篩選、需退款與超商退回標紅；`/admin/orders/<id>` 動作依狀態出現，取消／退款／完成兩段確認。
6. 依 `docs/dev/ecpay-stage.md`「超商取貨（物流）」節用 cloudflared 走綠界 stage：地圖固定門市回來、建單成功（含三個待確認項）、列印新分頁、模擬狀態通知讓訂單完成、宅配出貨、取消／退款／重開發票。
7. `git log --oneline` 看到本計畫 10 個 commit 都在 `worktree-mvp-design`；`git status` 乾淨；沒有 push。

## 交給計畫 5 的事項（寫計畫 5 時必看）

1. **部署環境變數**：`ECPAY_LOGISTICS_MERCHANT_ID|HASH_KEY|HASH_IV` 在 `ECPAY_ENV=prod` 時必填（`config.rs` 已強制）；`PUBLIC_BASE_URL` 必須是 https 的公開網址（綠界的 `ServerReplyURL`／`LogisticsC2CReplyURL` 不接受 localhost 或非 80/443 埠）；`SMTP_HOST`／`SMTP_FROM` 必填、`MAIL_LOG_BODY` 不要設（計畫 3 交接 8）。Docker 映像要 `ca-certificates`（計畫 3 交接 9）。
2. **worker 多副本**：`worker::mark_done`／`mark_failed_attempt` 沒有 `AND status = 'running'` 守衛（計畫 3 審查交接 5）；建單的認領是單句 UPDATE、多副本安全，但 2 分鐘的「卡住重認領」窗口在多副本下可能重複建單（極少見，且需要前一次真的卡住超過 2 分鐘）。
3. **貨態代碼會不定期更新**（綠界文件註明）：對照表在 `ecpay/logistics.rs::shipment_status_for`；不在表上的代碼只記錄不改狀態，後台看得到代碼與訊息。上線後若某家超商的到店／取件碼變了，補進對照表即可。
4. **`POST /api/checkout/cvs-map` 沒有速率限制**：每次點擊登記一列（1 小時後由每日 purge 清）；正式環境若被刷，掛 `tower_governor`（同 `POST /api/orders` 的做法）。
5. **建單回應的 MD5 驗證是嚴格的**（與規格不同之處 40）：stage 走查第 4 點 (a) 若發現真實回應驗不過（例如綠界對中文 `RtnMsg` 的編碼與我們不同），改 `ecpay/logistics.rs::parse_create_response` 一處即可；`raw.create_response_text` 存有整段回應可比對。
6. **綠界帳戶餘額**（錯誤碼 10500049）：正式環境建單會從綠界帳戶扣運費，餘額不足會建單失敗；部署文件提醒老闆在綠界後台預存。
7. **發票 GetIssue**（計畫 3 交接 11）與**計畫 1／2 的小項**（計畫 3 交接 12）仍未做；上線前挑。
8. **Playwright 不在 CI**（`ci.yml` 只跑 vitest 與 build）；兩條 e2e 要 api 與 web dev 都在跑才能跑。
9. **未走的人工驗收**：計畫 2 items 5／7／8／9、計畫 3 的 stage 走查（含 `CarrierType=1` 不帶 `CustomerID`、`RtnCode` 型別兩個待確認）、本計畫 stage 走查第 4 點的三個待確認項。

<!-- PLAN4-END -->
