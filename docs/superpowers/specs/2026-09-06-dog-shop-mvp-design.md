# dog_shop MVP 設計文件

日期：2026-09-06
狀態：待使用者審閱

## 1. 目標

把蝦皮賣場 `kuo_hsueh_vhi`（狗狗用品，實際商品清單待使用者提供）搬成自己的獨立電商，省下蝦皮抽成。買家體驗要接近蝦皮：手機優先、看商品、選規格、加購物車、選超商取貨或宅配、線上付款、收 Email 通知。老闆要有後台管商品、庫存、訂單、出貨、發票。

### 1.1 已確認的決定

| 項目 | 決定 |
|---|---|
| 前端 | SvelteKit 2 + Svelte 5（runes），SSR，adapter-node |
| 後端 | Rust 1.98 + axum 0.8 + sqlx 0.9 + tokio |
| 資料庫 | PostgreSQL 17 |
| 金流 | 綠界 ECPay 全方位金流（信用卡、ATM、超商代碼） |
| 物流 | 超商取貨（綠界 C2C：7-11、全家、萊爾富）+ 宅配（老闆自行寄送、後台填單號） |
| 電子發票 | 綠界 B2C 電子發票，付款成功後自動開立 |
| 商品資料 | 後台匯入 Excel（蝦皮繞路匯出檔或本專案範本）+ 後台手動建立 |
| 會員 | Email + 密碼；允許訪客結帳 |
| 部署 | 單台 VPS，Docker Compose，Caddy 自動 HTTPS，圖片存本機 volume |
| 語言 / 幣別 | 繁體中文 / 新台幣整數 |

### 1.2 這次不做（第二階段）

折價券、商品評價、LINE 登入、聊聊、收藏清單、線上退款 API、發票作廢與折讓 API、超商取貨付款（貨到付款）、綠界黑貓宅配 API、多層分類、多管理員權限分級。

資料表為貨到付款預留 `payments.method` 的枚舉值，之後加不用改表結構。

## 2. 系統架構

```
瀏覽器
  │ https
  ▼
Caddy (:443, 自動 TLS)
  ├─ /api/*      ──► api  (Rust axum, :8080)
  ├─ /uploads/*  ──► api  (靜態檔，volume)
  └─ /*          ──► web  (SvelteKit node, :3000)

web 在 SSR 時透過內網 http://api:8080 呼叫 api，並轉送瀏覽器的 Cookie。
瀏覽器端 JS 用同源相對路徑 /api/... 直接打 api。
綠界的伺服器回呼（付款結果、物流狀態）打 https://<domain>/api/ecpay/...
```

三個容器 + 一個資料庫：`caddy`、`web`、`api`、`db`。

### 2.1 開發環境

- `docker compose -f deploy/docker-compose.dev.yml up db`：只跑 Postgres。
- `cd api && cargo run`：api 在 :8080，啟動時自動跑 migration。
- `cd web && pnpm dev`：Vite 在 :5173，`server.proxy` 把 `/api` 與 `/uploads` 轉到 :8080。
- `cloudflared tunnel --url http://localhost:5173`：拿一個公開網址給綠界回呼用。把它設成 `PUBLIC_BASE_URL`。
- 綠界全部用測試環境（stage）與公開測試帳號，見 §8.5。

### 2.2 Repo 佈局

```
dog_shop/
  api/                    Rust crate
    migrations/           sqlx migrations（純 SQL）
    src/
      main.rs             啟動、路由組裝、背景 worker
      config.rs           環境變數
      error.rs            ApiError → JSON
      auth/               session、密碼、middleware
      routes/             HTTP handlers（依資源分檔）
      domain/             orders、products、inventory、settings（純邏輯）
      ecpay/              mac.rs、aes.rs、aio.rs、logistics.rs、invoice.rs
      jobs/               outbox worker 與各 job
      mail/               lettre + askama 模板
      storage/            圖片處理與檔案存放
      import/             Excel 解析與對應
  web/                    SvelteKit
    src/
      hooks.server.ts     讀 session、保護 /admin 與 /account
      lib/api.ts          fetch 包裝（server 端走內網、瀏覽器走 /api）
      lib/cart.svelte.ts  購物車 $state + localStorage
      lib/components/     UI 元件
      routes/             頁面（見 §6）
  deploy/
    docker-compose.yml    正式
    docker-compose.dev.yml 開發（只有 db）
    Caddyfile
    backup.sh             每日 pg_dump
  docs/superpowers/specs/
  .env.example
```

## 3. 資料模型

金額一律 `integer`（新台幣，無小數）。主鍵一律 UUID v7（可排序）。時間一律 `timestamptz`。

| 表 | 重點欄位 | 說明 |
|---|---|---|
| `users` | id, email (citext unique), password_hash, name, phone, role ∈ {customer, admin}, created_at, updated_at | 老闆也是 user，role=admin |
| `sessions` | id, user_id, expires_at, created_at | cookie `sid` 存 session id，30 天 |
| `password_resets` | token_hash pk, user_id, expires_at, used_at | 1 小時有效 |
| `addresses` | id, user_id, recipient_name, phone, postal_code, city, district, street, is_default | 會員常用地址 |
| `categories` | id, slug unique, name, sort_order | 單層分類 |
| `products` | id, slug unique, name, description, category_id, status ∈ {draft, active, archived}, option1_name, option2_name, external_ref unique nullable, sort_order, created_at, updated_at | 最多兩層規格名稱（同蝦皮）。刪除 = archived，不真刪。`slug` 預設 8 碼隨機小寫字母數字，後台可改成英文 slug。`description` 為純文字、保留換行 |
| `product_variants` | id, product_id, option1_value, option2_value, sku, price, compare_at_price nullable, stock, is_active, image_id nullable, sort_order | 每個規格一列。沒規格的商品也有一列「預設」規格 |
| `product_images` | id, product_id, path, thumb_path, alt, sort_order | 最多 9 張 |
| `orders` | id, order_no unique, user_id nullable, guest_token, status ∈ {pending_payment, paid, shipped, completed, cancelled, refunded}, email, recipient_name, recipient_phone, shipping_method ∈ {cvs, home}, subtotal, shipping_fee, total, note, invoice_type ∈ {personal, company, donation}, invoice_carrier_type, invoice_carrier_num, invoice_tax_id, invoice_title, invoice_love_code, created_at, paid_at, shipped_at, completed_at, cancelled_at, cancel_reason | 訂單主檔。`order_no` 格式 `DS` + yyMMdd + 4 碼隨機大寫字母數字 |
| `order_items` | id, order_id, variant_id, product_name, variant_label, unit_price, quantity, line_total, image_path | 下單當時的快照 |
| `payments` | id, order_id, merchant_trade_no unique, method ∈ {credit, atm, cvs_code}, status ∈ {pending, paid, failed, expired}, amount, ecpay_trade_no, payment_type, payment_date, atm_bank_code, atm_vaccount, cvs_payment_no, expire_at, raw jsonb, created_at, updated_at | 一次付款嘗試一列。`merchant_trade_no` = order_no + 兩碼流水（綠界要求唯一，重付要換號） |
| `shipments` | id, order_id unique, method, cvs_sub_type ∈ {UNIMARTC2C, FAMIC2C, HILIFEC2C}, cvs_store_id, cvs_store_name, cvs_store_address, cvs_store_phone, home_postal_code, home_city, home_district, home_street, status ∈ {pending, created, in_transit, arrived, picked_up, returned, shipped}, ecpay_logistics_id, ecpay_merchant_trade_no, cvs_payment_no, cvs_validation_no, carrier, tracking_no, last_status_code, last_status_msg, raw jsonb, created_at, updated_at | 一筆訂單一筆出貨 |
| `invoices` | id, order_id unique, status ∈ {pending, issued, failed}, relate_number, invoice_no, invoice_date, random_number, request jsonb, response jsonb, error, created_at, updated_at | 作廢在第二階段 |
| `jobs` | id bigserial, kind, payload jsonb, dedupe_key unique, status ∈ {queued, running, done, failed}, attempts, max_attempts, run_at, last_error, created_at, updated_at | 背景工作（outbox） |
| `settings` | key pk, value jsonb, updated_at | 商店名稱、聯絡資訊、運費、免運門檻、寄件人、退貨門市、啟用的付款方式 |
| `cvs_store_selections` | token pk, sub_type, store_id, store_name, store_address, store_phone, created_at, expires_at | 綠界地圖選完門市的暫存，1 小時過期 |

索引：`orders(user_id, created_at desc)`、`orders(status)`、`products(category_id, status)`、`product_variants(product_id)`、`jobs(status, run_at)`、`sessions(expires_at)`。

## 4. 訂單狀態機

```
pending_payment ──付款成功回呼──► paid ──後台出貨──► shipped ──取貨完成 / 後台標記 / 14 天自動──► completed
pending_payment ──3 天未付 / 買家取消 / 後台取消──► cancelled（庫存歸還）
paid ──後台取消（先在綠界後台手動退款）──► refunded（後台顯示「請至綠界作廢發票」提醒）
```

- ATM、超商代碼付款：拿到繳費資訊後訂單仍是 `pending_payment`，`payments` 上有虛擬帳號或繳費代碼與期限，訂單頁顯示給買家。
- 出貨的細部狀態在 `shipments.status`：`pending → created → in_transit → arrived → picked_up`（超商），或 `pending → shipped`（宅配）。超商狀態由綠界物流狀態回呼更新，代碼對應表寫在 `ecpay/logistics.rs`。
- 超商未取件被退回：`shipments.status = returned`，訂單維持 `shipped`，後台列表標紅提醒老闆處理（在綠界後台退款後按「標記已退款」）。
- 訪客訂單不會在事後自動連結到同 Email 註冊的會員帳號。

## 5. 庫存規則

- 下單時在同一個交易內對每個品項執行 `UPDATE product_variants SET stock = stock - $qty WHERE id = $id AND stock >= $qty`。任何一列影響筆數為 0 就整筆 rollback，回 `OUT_OF_STOCK` 並列出不足的品項。這樣多人同時搶購也不會超賣。
- 取消或過期：把 `order_items` 的數量加回去。
- 未付款過期：worker 每 10 分鐘掃 `pending_payment` 且 `created_at` 超過 3 天 2 小時的訂單，標 `cancelled(reason=expired)` 並歸還庫存。綠界端 ATM `ExpireDate=3` 天、超商代碼 `StoreExpireDate=4320` 分鐘（3 天），所以不會出現「綠界收到錢但我們已取消」的情形。

## 6. 前端（SvelteKit）

### 6.1 頁面

買家：

| 路徑 | 內容 |
|---|---|
| `/` | 分類入口、最新 8 件上架商品 |
| `/products` | 商品列表。查詢參數 `q`（名稱關鍵字）、`category`（slug）、`sort ∈ {newest, price_asc, price_desc}`（預設 newest）、`page`、`per_page`（預設 24，最大 60）。多規格價格不同時顯示「最低～最高」 |
| `/products/[slug]` | 商品頁：多圖、規格選擇、價格、庫存、加入購物車。含 OG meta 與 JSON-LD Product |
| `/cart` | 購物車，重新向伺服器驗證價格與庫存 |
| `/checkout` | 結帳表單（見 §7） |
| `/orders/[id]` | 訂單結果與明細。訪客要帶 `?t=<guest_token>`。付款中每 3 秒輪詢，最多 2 分鐘，之後顯示「請重新整理」 |
| `/login`、`/register`、`/forgot-password`、`/reset/[token]` | 會員 |
| `/account`、`/account/orders`、`/account/addresses` | 會員中心 |
| `/sitemap.xml`、`/robots.txt` | SEO |

老闆（`/admin/*`，需 role=admin）：

| 路徑 | 內容 |
|---|---|
| `/admin` | 今日訂單、待出貨、待處理發票失敗 |
| `/admin/products`、`/admin/products/new`、`/admin/products/[id]` | 商品 CRUD、拖曳排序圖片、規格表格編輯 |
| `/admin/categories` | 分類 CRUD |
| `/admin/import` | 上傳 Excel → 預覽 → 確認匯入 |
| `/admin/orders`、`/admin/orders/[id]` | 訂單列表（依狀態篩選）、建物流單、列印託運單、填宅配單號、取消、標記已退款、重開發票 |
| `/admin/settings` | 商店資訊、運費、免運門檻、寄件人、退貨門市、付款方式開關 |

### 6.2 關鍵機制

- `hooks.server.ts`：每個請求讀 `sid` cookie，向 api 的 `GET /api/auth/me` 取得使用者放進 `event.locals.user`。`/admin` 非 admin 轉 `/login`；`/account` 未登入轉 `/login`。
- `lib/api.ts`：一個 `api<T>(path, init)`。在伺服器端用 `API_INTERNAL_URL` 並轉送 `cookie` header；在瀏覽器端用 `/api`。所有變更請求加 `X-Requested-With: fetch` header。
- 購物車 `lib/cart.svelte.ts`：Svelte 5 `$state` 類別，內容 `{ variant_id, qty }` 加顯示用快取（名稱、圖、價格）。存 localStorage。伺服器不存購物車；結帳時伺服器重算一切。
- 樣式：Tailwind CSS 4（`@tailwindcss/vite`），自寫少量元件，不引入元件庫。
- 台灣縣市/鄉鎮/郵遞區號用一份靜態 JSON 做下拉選單。

## 7. 結帳流程

1. 買家在 `/checkout` 填：Email、收件人、手機。未登入可訪客結帳；登入者可選常用地址。
2. 選出貨方式：
   - **超商取貨**：選 7-11 / 全家 / 萊爾富，按「選擇門市」。前端先把表單狀態存到 sessionStorage，再呼叫 `POST /api/checkout/cvs-map` 拿綠界電子地圖表單欄位，用隱藏表單 POST 到綠界（頂層導頁，不用 iframe）。買家在綠界地圖選完，綠界用買家的瀏覽器 POST 門市資料到我們的 `POST /api/ecpay/logistics/map-reply`。我們存進 `cvs_store_selections`，回 303 轉到 `/checkout?store=<token>`。頁面還原表單、用 token 取回門市顯示。
   - **宅配**：填郵遞區號、縣市、鄉鎮、地址。
3. 運費：從 `settings` 讀「超商運費」「宅配運費」「免運門檻」（以商品小計 `subtotal` 比較），前端顯示，伺服器重算。超商取貨商品金額上限 20,000 元（綠界 C2C 限制），超過要求改宅配。
4. 發票：個人（預設存入綠界會員載具並寄 Email；可填手機條碼或自然人憑證）、公司（統編 + 抬頭）、捐贈（愛心碼）。
5. 付款方式：信用卡、ATM 轉帳、超商代碼。
6. 送出 `POST /api/orders`，內容含品項 `{variant_id, qty}`、收件、出貨、發票、付款方式、門市 token。伺服器在一個交易內：驗證商品有效、重算金額與運費、扣庫存（§5）、建立 `orders`、`order_items`、`shipments`、`payments(pending)`、`invoices(pending)`，排一個 `send_email:order_created` job。回傳 `{ order_id, order_no, guest_token, ecpay: { action, fields } }`，`fields` 含算好的 `CheckMacValue`。
7. 前端用隱藏表單把 `fields` POST 到綠界 `AioCheckOut/V5`。
8. 綠界處理付款後：
   - 信用卡付款成功：綠界伺服器 POST `ReturnURL` = `/api/ecpay/payment/return`。我們驗 `CheckMacValue`，`RtnCode == 1` 且沒有 `SimulatePaid=1` 就把 payment 標 `paid`、訂單標 `paid`（重複通知為 no-op），排 `issue_invoice` 與 `send_email:payment_received` job，回應純文字 `1|OK`。
   - ATM / 超商代碼：綠界先 POST `PaymentInfoURL` = `/api/ecpay/payment/info` 給繳費資訊（銀行代碼、虛擬帳號或繳費代碼、期限）。存進 `payments`，排 `send_email:payment_instructions`，回 `1|OK`。買家繳費後再走上面的 `ReturnURL`。
   - 買家瀏覽器經 `ClientBackURL` 回到 `/orders/{id}?t=...`，頁面輪詢直到訂單不再是 `pending_payment` 或已有繳費資訊。
9. 買家在訂單頁可以「重新付款」：`POST /api/orders/{id}/repay` 產生新的 `payments` 列與新 `merchant_trade_no`。

## 8. 綠界整合

三套 API、三組憑證，分別放 `ECPAY_AIO_*`、`ECPAY_LOGISTICS_*`、`ECPAY_INVOICE_*` 環境變數。`ECPAY_ENV=stage|prod` 決定網址。

時間：DB 一律存 UTC；送給綠界的所有時間欄位（`MerchantTradeDate`、`Timestamp`）用台北時間（UTC+8）；前端顯示台北時間。

### 8.1 共用：CheckMacValue（`ecpay/mac.rs`）

參數依 key 排序（不分大小寫）→ 串成 `HashKey=...&k1=v1&...&HashIV=...` → 依綠界規定做 URL encode（.NET 風格，再套用綠界的字元替換表）→ 轉小寫 → 雜湊 → 轉大寫。全方位金流用 SHA256（`EncryptType=1`）；物流 API 用 MD5。實作時以綠界文件上的範例做單元測試。

### 8.2 全方位金流（`ecpay/aio.rs`）

- 建立：POST 表單到 `/Cashier/AioCheckOut/V5`。欄位：`MerchantID`、`MerchantTradeNo`、`MerchantTradeDate`、`PaymentType=aio`、`TotalAmount`、`TradeDesc`、`ItemName`（品項用 `#` 連接，超長截斷）、`ReturnURL`、`ChoosePayment ∈ {Credit, ATM, CVS}`、`ClientBackURL`、`PaymentInfoURL`、`ExpireDate=3`、`StoreExpireDate=4320`、`NeedExtraPaidInfo=N`、`EncryptType=1`、`CustomField1=order_id`、`CheckMacValue`。
- 回呼驗證：重算 `CheckMacValue` 比對；不符回 HTTP 400 `0|CheckMacValue Error` 並記 log。找不到 `MerchantTradeNo` 回 `0|Unknown MerchantTradeNo`。
- 測試環境有「模擬付款」按鈕，通知會帶 `SimulatePaid=1`，只記 log 不改狀態。

### 8.3 物流（`ecpay/logistics.rs`）

- 電子地圖：POST 表單到 `/Express/map`，欄位 `MerchantID`、`MerchantTradeNo`、`LogisticsType=CVS`、`LogisticsSubType`、`IsCollection=N`、`ServerReplyURL`、`ExtraData=<token>`、`Device`（手機 1）。不需 `CheckMacValue`。回傳到 `ServerReplyURL` 的欄位：`CVSStoreID`、`CVSStoreName`、`CVSAddress`、`CVSTelephone`、`CVSOutSide`、`ExtraData`。測試環境不會顯示真地圖，會直接回固定門市。
- 建立物流單（後台按「建立物流單」時同步呼叫，不用 job，錯誤直接顯示）：POST `/Express/Create`。欄位含 `MerchantTradeNo`（新號：`order_no` + `L` + 流水）、`MerchantTradeDate`、`LogisticsType=CVS`、`LogisticsSubType`、`GoodsAmount`（1～20000）、`GoodsName`、`SenderName`、`SenderCellPhone`、`ReceiverName`、`ReceiverCellPhone`、`ReceiverEmail`、`ReceiverStoreID`、`ReturnStoreID`（設定裡的退貨門市）、`ServerReplyURL=/api/ecpay/logistics/status`、`IsCollection=N`、`CheckMacValue`。成功回 `1|MerchantID=...&AllPayLogisticsID=...&CVSPaymentNo=...&CVSValidationNo=...`，失敗回 `0|訊息`。存進 `shipments`，訂單轉 `shipped`，排 `send_email:order_shipped`。
- 列印託運單：後台按「列印」，api 回一組帶 `CheckMacValue` 的表單欄位，前端在新分頁 POST 到對應的 `Express/Print{UniMart,FAMI,HILIFE}C2COrderInfo`。
- 狀態回呼：綠界 POST 到 `/api/ecpay/logistics/status`，驗 `CheckMacValue`，依 `RtnCode` 對應到 `shipments.status`，`picked_up` 時訂單轉 `completed`。回 `1|OK`。
- 收件人姓名、手機格式限制（姓名 2～5 個中文字、手機 09 開頭 10 碼）在結帳表單就先驗證。

### 8.4 電子發票（`ecpay/invoice.rs`，`ecpay/aes.rs`）

- 端點 `/B2CInvoice/Issue`，JSON。外層 `{ MerchantID, RqHeader: { Timestamp }, Data }`。`Data` = 內層 JSON → URL encode → AES-128-CBC（HashKey 為 key、HashIV 為 iv、PKCS7）→ Base64。回應反向解開。
- 內層欄位：`RelateNumber=order_no`、`CustomerEmail`、`CustomerPhone`、`Print`、`Donation`、`LoveCode`、`CarrierType`（`1` 綠界載具 / `2` 自然人憑證 / `3` 手機條碼）、`CarrierNum`、`CustomerIdentifier`（統編，公司戶）、`CustomerName`、`CustomerAddr`、`TaxType=1`、`SalesAmount=total`、`InvType=07`、`vat=1`、`Items[]`（每個 `order_item` 一列，運費大於 0 時多一列「運費」）。公司戶依綠界規定不帶載具、`Print=1`。
- 由 `issue_invoice` job 執行：成功存 `InvoiceNo`、`InvoiceDate`、`RandomNumber`，排 `send_email:invoice_issued`（綠界也會寄）。失敗指數退避重試最多 5 次，仍失敗標 `failed`，後台顯示並提供「重試」。`dedupe_key = invoice:{order_id}` 保證不重複開立。

### 8.5 測試環境憑證（公開資料，只能用於 stage）

| API | MerchantID | 備註 |
|---|---|---|
| 全方位金流 | 3002607 | HashKey `pwFHCqoQZGmho4w6`、HashIV `EkRm7iFT261dpevs`；測試卡號 `4311-9511-1111-1111`；後台 `vendor-stage.ecpay.com.tw`（stagetest3 / test1234） |
| 物流 C2C | 實作時到 developers.ecpay.com.tw 物流「測試介接資訊」頁確認 | 放 `.env.example` |
| 電子發票 B2C | 2000132 | HashKey / HashIV 實作時到「測試介接資訊」頁確認 |

正式憑證由使用者從綠界廠商後台取得，只放在伺服器的 `.env`，不進 git。

## 9. 背景工作（`jobs/`）

- `jobs` 表當 outbox。寫入業務資料與排 job 在同一個交易，不會漏。
- 一個 tokio task 每 2 秒 `SELECT ... WHERE status='queued' AND run_at <= now() ORDER BY id FOR UPDATE SKIP LOCKED LIMIT 10`，逐筆執行。
- 失敗：`attempts + 1`，`run_at = now() + 2^attempts 分鐘`，超過 `max_attempts` 標 `failed`。
- job 種類：`send_email`、`issue_invoice`。
- 排程型工作（不走 jobs 表，直接在 worker 內定時）：`expire_unpaid_orders`（每 10 分鐘）、`auto_complete_shipped`（每小時，出貨超過 14 天轉 completed）、`purge_expired_sessions`（每天）。

## 10. 後端 API

回應一律 JSON。錯誤格式：

```json
{ "error": { "code": "OUT_OF_STOCK", "message": "部分商品庫存不足", "details": { "items": [ { "variant_id": "...", "available": 2 } ] } } }
```

錯誤碼：`VALIDATION`、`UNAUTHORIZED`、`FORBIDDEN`、`NOT_FOUND`、`OUT_OF_STOCK`、`CVS_AMOUNT_LIMIT`、`CVS_STORE_REQUIRED`、`ORDER_NOT_PAYABLE`、`ECPAY_ERROR`、`RATE_LIMITED`、`INTERNAL`。

公開：

- `GET /api/products`（查詢參數 `q`、`category`、`sort`、`page`、`per_page`，只回 active）
- `GET /api/products/{slug}`
- `GET /api/categories`
- `GET /api/settings/public`
- `POST /api/cart/validate`
- `POST /api/checkout/cvs-map`
- `GET /api/checkout/cvs-store/{token}`
- `POST /api/orders`
- `GET /api/orders/{id}`（會員看自己的；訪客帶 `?t=`）
- `POST /api/orders/{id}/repay`
- `POST /api/ecpay/payment/return`、`POST /api/ecpay/payment/info`、`POST /api/ecpay/logistics/map-reply`、`POST /api/ecpay/logistics/status`（綠界用，回純文字）

會員：

- `POST /api/auth/register`、`POST /api/auth/login`、`POST /api/auth/logout`、`GET /api/auth/me`、`POST /api/auth/forgot`、`POST /api/auth/reset`
- `GET /api/me/orders`、`GET|PUT /api/me/profile`、`GET|POST /api/me/addresses`、`PUT|DELETE /api/me/addresses/{id}`

後台（`/api/admin/*`，需 admin）：

- `GET|POST /api/admin/products`、`GET|PUT|DELETE /api/admin/products/{id}`（DELETE = archived；規格隨商品 PUT 整組更新，已有訂單的規格只能停用不能刪）
- `POST /api/admin/uploads`（multipart，≤ 10 MB，jpeg/png/webp/gif）
- `GET|POST /api/admin/categories`、`PUT|DELETE /api/admin/categories/{id}`
- `POST /api/admin/import/preview`、`POST /api/admin/import/commit`
- `GET /api/admin/orders`、`GET /api/admin/orders/{id}`
- `POST /api/admin/orders/{id}/ship-cvs`、`POST /api/admin/orders/{id}/print-label`、`POST /api/admin/orders/{id}/ship-home`、`POST /api/admin/orders/{id}/cancel`、`POST /api/admin/orders/{id}/mark-refunded`、`POST /api/admin/orders/{id}/complete`、`POST /api/admin/orders/{id}/retry-invoice`
- `GET|PUT /api/admin/settings`
- `GET /api/admin/dashboard`

## 11. 認證與安全

- 密碼 argon2id。最少 8 碼。
- Session 存 DB，cookie `sid`：`HttpOnly`、`Secure`（開發環境可關）、`SameSite=Lax`、`Path=/`、30 天。
- CSRF：`SameSite=Lax` 擋跨站表單 POST；所有變更請求另外要求 `X-Requested-With: fetch` header 並比對 `Origin`。綠界回呼路徑例外：付款與物流狀態回呼靠 `CheckMacValue` 簽章驗證；門市回傳（`map-reply`）沒有簽章，靠 `ExtraData` 裡的隨機 token 必須存在且未過期，且它只影響該次結帳的門市選擇。
- 速率限制：`/api/auth/*` 每 IP 每分鐘 10 次（`tower_governor`）。
- 忘記密碼：token 32 bytes 隨機，DB 只存 SHA-256，1 小時有效，用過作廢；重設後清掉該使用者全部 session。
- 訪客訂單：`guest_token` 32 bytes 隨機，只出現在訂單頁網址與 Email。
- 第一個 admin：`cargo run -- create-admin <email>` 互動輸入密碼。
- 上傳：伺服器用 `image` crate 重新解碼再轉 WebP（主圖最長邊 1600、縮圖 400），不保留原檔，杜絕夾帶惡意內容。存 `uploads/yyyy/mm/{uuid}.webp`，回應 `Cache-Control: public, max-age=31536000, immutable`。
- Log：`tracing` JSON 格式，每個請求帶 request id。綠界原始 payload 存進各表的 `raw` 欄位方便對帳。
- 不在 log 或錯誤訊息裡輸出 HashKey、HashIV、密碼。

## 12. Email（`mail/`）

`lettre` 走 SMTP（STARTTLS 或 TLS，任何供應商都行）。`askama` 模板，純文字 + 簡單 HTML 各一份。

| 模板 | 觸發 |
|---|---|
| `order_created` | 下單成功 |
| `payment_instructions` | 拿到 ATM 帳號或超商代碼 |
| `payment_received` | 付款成功 |
| `order_shipped` | 超商：門市名稱地址，提醒到店會收簡訊；宅配：貨運公司與單號 |
| `invoice_issued` | 發票號碼、隨機碼 |
| `password_reset` | 重設連結 |

寄件失敗由 job 重試。

## 13. 商品匯入（`import/`）

- 只收 `.xlsx`，用 `calamine` 解析。
- 本專案範本欄位（第一列標題）：`商品編號`、`商品名稱`、`商品描述`、`分類`、`規格名稱1`、`規格選項1`、`規格名稱2`、`規格選項2`、`價格`、`庫存`、`SKU`、`圖片網址`（逗號分隔，最多 9 個）。同一個 `商品編號` 的多列合併成同一商品的多個規格。
- 蝦皮來源檔（批次更新匯出或第三方外掛匯出）用一張「標題別名表」對到上面的欄位，例如 `規格名稱 1`、`商品圖片 1`～`商品圖片 9`、`商品ID`。實際別名在拿到使用者的檔案後補齊；對不上的欄位在預覽頁明白列出。
- 流程：`preview` 回解析結果（商品數、規格數、每列錯誤、缺欄位），老闆確認後 `commit`。
- 圖片網址由伺服器下載（10 秒 timeout、≤ 10 MB、只收圖片 MIME），失敗只記在該列的警告，不擋整批。
- 重複匯入以 `products.external_ref = 商品編號` 更新既有商品。
- 匯入的商品預設 `draft`，老闆在後台檢查後再上架。

## 14. 錯誤處理原則

- 後端所有錯誤走 `ApiError`，統一 JSON 格式與 HTTP 狀態。驗證錯誤帶欄位層級 `details`。內部錯誤只回 `INTERNAL` 與 request id，細節在 log。
- 前端 `+error.svelte` 顯示友善訊息；表單錯誤顯示在欄位旁；購物車操作用 toast。
- 綠界回呼要嚴格區分：簽章錯誤回 400；已處理過的重複通知回 `1|OK` 不重做；暫時性失敗（DB 連不上）回 500 讓綠界重送。
- 任何寫入綠界的呼叫都先把請求存 DB 再送，回應也存，方便對帳與重試。

## 15. 測試策略

- Rust 單元：`mac.rs`（SHA256 與 MD5 兩種、URL encode 特例，對照綠界文件範例）、`aes.rs`（加解密往返與已知向量）、金額與運費計算、狀態轉移合法性。
- Rust 整合（`sqlx::test`，對開發用 Postgres）：下單扣庫存、並發下單不超賣、付款回呼重複通知冪等、過期 job 歸還庫存、後台建物流單狀態轉移、admin 權限。
- 前端：vitest 測購物車邏輯；Playwright 一條主流程「瀏覽 → 加車 → 結帳表單 → 攔截送往綠界的表單並檢查欄位」。
- 手動：綠界 stage 走完整流程，用廠商後台「模擬付款」驗 `ReturnURL`，測 ATM 與超商代碼的 `PaymentInfoURL`，物流建單與列印，發票開立。
- CI：GitHub Actions 跑 `cargo fmt --check`、`cargo clippy`、`cargo test`（帶 Postgres service）、`pnpm check`、`pnpm test`、`pnpm build`。

## 16. 部署

- `deploy/docker-compose.yml`：`caddy`（80/443）、`web`（adapter-node，Node 24）、`api`（多階段建置：rust 1.98 → debian-slim）、`db`（postgres:17，volume `pgdata`）。`api` 另掛 volume `uploads`。
- `Caddyfile`：`<domain>` 反向代理如 §2。
- Migration 在 api 啟動時執行（`sqlx::migrate!`）。
- `deploy/backup.sh`：每日 `pg_dump` 到 `backups` volume，保留 14 天。
- 環境變數（`.env.example` 列全）：`DATABASE_URL`、`PUBLIC_BASE_URL`、`API_INTERNAL_URL`、`COOKIE_SECURE`、`ECPAY_ENV`、`ECPAY_AIO_MERCHANT_ID|HASH_KEY|HASH_IV`、`ECPAY_LOGISTICS_MERCHANT_ID|HASH_KEY|HASH_IV`、`ECPAY_INVOICE_MERCHANT_ID|HASH_KEY|HASH_IV`、`SMTP_HOST|PORT|USER|PASS|FROM`、`UPLOAD_DIR`、`RUST_LOG`。

## 17. 使用者要準備的東西

上線前：

1. 網域名稱與一台 VPS（2 vCPU / 4 GB 足夠）。
2. 綠界正式特店帳號，開通全方位金流、物流（C2C 超商）、電子發票（需統編與字軌），取得三組 HashKey / HashIV。
3. SMTP 帳號（Gmail 應用程式密碼、Resend、Mailgun 任一）。
4. 寄件人姓名、手機、退貨門市（7-11 或全家門市代號）。
5. 商店名稱、Logo、聯絡方式、退換貨說明文字。

開發期間：

6. 商品資料檔（蝦皮繞路匯出或依範本填寫），越早給越好，匯入功能要對著真檔調。

## 18. 建置順序（給實作計畫參考）

1. 骨架：api（axum、config、migration、health）、web（SvelteKit、Tailwind、api 包裝）、dev compose、CI、`.gitignore`、`.env.example`。
2. 商品目錄：schema、後台商品與圖片 CRUD、公開商品頁與列表。
3. 會員：註冊登入、session、hooks、會員中心、忘記密碼。
4. 購物車與結帳（不含綠界）：下單、扣庫存、訂單頁、設定與運費。
5. 綠界金流：`mac.rs`、AIO 建單、`ReturnURL`、`PaymentInfoURL`、重付。
6. jobs worker 與 Email。
7. 電子發票。
8. 超商物流：地圖、建單、列印、狀態回呼。
9. 後台訂單管理、儀表板、宅配單號。
10. Excel 匯入。
11. 部署：Dockerfile、Caddy、compose、備份；stage 全流程驗收。
