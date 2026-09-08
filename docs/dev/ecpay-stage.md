# 綠界 stage 手動測試

自動測試不會真的打綠界；付款回呼、繳費資訊、發票開立要用綠界的測試環境走一次。

## 前置

1. `docker compose -f deploy/docker-compose.dev.yml up -d db`
2. 讓綠界打得到你的機器：`cloudflared tunnel --url http://localhost:5173`（沒有就 `brew install cloudflared`），記下它印出的 `https://xxxx.trycloudflare.com`。
3. 根目錄 `.env`：`PUBLIC_BASE_URL=https://xxxx.trycloudflare.com`、`ECPAY_ENV=stage`（AIO／發票憑證留空會用公開測試憑證）。要看信件內容再設 `MAIL_LOG_BODY=1`（只在本機開發；內文含重設連結與訪客訂單網址）。`ECPAY_LOGISTICS_*` 留空會用物流 C2C 公開測試特店 2000933。
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

## 超商取貨（物流）

物流 C2C 測試特店 `2000933`；廠商後台 `https://vendor-stage.ecpay.com.tw`（`LogisticsC2CTest` / `test1234`）。測試環境的電子地圖是固定門市、不會跳地圖；**測試環境不會發物流狀態通知**（見第 6 點）。

1. 結帳選「超商取貨」→ 選 7-ELEVEN → 按「選擇門市」→ 綠界直接把固定門市 POST 回 `/api/ecpay/logistics/map-reply` → 回到結帳頁看到門市（網址 `?store=<20 碼 token>`）。換一家超商會清掉門市要重選；超過 1 小時回來會看到「門市選擇已逾時」。同時確認真實瀏覽器的跨站 POST → 303 → GET 經 Vite proxy（正式環境是反向代理）正常回到結帳頁；e2e 只模擬了這一段。
2. 用信用卡付款（同「信用卡」節）→ 訂單「已付款」。
3. 後台 `/admin/settings` 填寄件人姓名（中文 5 字內）與手機（09 開頭 10 碼）；退貨門市可留空。
4. `/admin/orders/<id>` 按「建立物流單」：
   - 成功：狀態變「已出貨」，取貨區顯示「物流單已建立」、綠界物流單號、寄貨編號、驗證碼（7-11 才有）；api log `綠界物流單已建立`；Email log `【…】訂單 … 已出貨`；DB：`SELECT status, ecpay_merchant_trade_no, ecpay_logistics_id, cvs_payment_no, last_status_code, raw->'create_request'->>'GoodsAmount' FROM shipments`。廠商後台 → 物流管理 → 物流建單及查詢 看到同一張（`MerchantTradeNo` = 訂單編號 + `L01`）。
   - 失敗：頁面 toast「綠界沒有接受這張物流單…」，出貨區「上次建立物流單失敗：<綠界原文>」，訂單維持已付款；再按一次會用 `L02`。
   - 待確認（實作時無法自動驗證）：(a) 真實回應能通過 MD5 CheckMacValue 驗證（失敗時出貨區會顯示「回應簽章不符：…」；先到廠商後台確認是否已建單，再回報）；(b) `GoodsAmount` 用商品小計、`ReceiverEmail`、`LogisticsC2CReplyURL` 三個欄位被三家超商接受；(c) 全家、萊爾富回的 `CVSPaymentNo` 形狀（測試門市 全家 `006598`、萊爾富 `2001`）。
5. 「列印託運單」→ 新分頁出現綠界的託運單頁（7-11 `PrintUniMartC2COrderInfo`）。新分頁是在 API 回應之後才開，Safari 可能當成彈出視窗擋掉：被擋就在 Safari 允許本站彈出視窗，或改用 Chrome。
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
   enc = enc.replace("~", "%7e")  # python 的「永不編碼」集合含 ~，.NET 的 UrlEncode 會編成 %7e
   p["CheckMacValue"] = hashlib.md5(enc.encode()).hexdigest().upper()
   subprocess.run(["curl", "-s", "-d", "&".join(f"{k}={quote_plus(v)}" for k, v in p.items()), "http://localhost:8080/api/ecpay/logistics/status"])
   ```

   這段只是把 `ecpay/mac.rs` 的規則用 python 重寫，權威在 `mac.rs`。
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

## 忘記密碼

`/forgot-password` 送出 → api log 有 `Email（未設定 SMTP，只記 log）subject=【…】重設密碼`；設 `MAIL_LOG_BODY=1` 才看得到連結，貼到瀏覽器可以重設。10 分鐘內再送一次不會再寄（log：`已寄過重設信，略過`）。

## 收工

`lsof -ti :8080`、`lsof -ti :5173` 取 pid 後 `kill`；把 `.env` 的 `PUBLIC_BASE_URL` 改回 `http://localhost:5173`。
