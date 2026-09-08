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
   - 待確認：`CarrierType=1` 不帶 `CustomerID` 是否能成功開立（與規格不同之處 29 的未確認項）。
   - 待確認：綠界回應的 `RtnCode` 是數字還是字串（`api/src/ecpay/invoice.rs:218` 只用 `as_i64` 讀，若是字串會一路重試到 `failed`）。
6. 綠界發票 stage 後台 `https://einvoice-stage.ecpay.com.tw`（Stagetest1234 / test1234）→ 發票查詢，能看到同一張。

## 公司發票

1. 結帳時發票選「公司（統一編號）」→ 統一編號 `04595257`、發票抬頭（例如「測試公司」）、發票地址（例如「台北市信義區市府路 1 號」）→ 付款方式「信用卡」→ 走完「信用卡」節步驟 1–3。
2. DB：`SELECT status, invoice_no FROM invoices` 應為 `issued`。
3. 綠界發票 stage 後台查同一張，`Print` 欄位應為 `1`。

## 捐贈發票

1. 結帳時發票選「捐贈」→ 愛心碼 `168` → 付款方式「信用卡」→ 走完「信用卡」節步驟 1–3。
2. DB：`SELECT status, invoice_no FROM invoices` 應為 `issued`。

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
