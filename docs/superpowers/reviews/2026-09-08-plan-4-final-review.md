# 計畫 4 最終審查：綠界物流、後台訂單管理、儀表板

日期：2026-09-08
範圍：`53cef86..3ab13c7`（分支 `worktree-mvp-design`，13 個 commit、50 個檔案、+5644/−98；程式碼基準是計畫 3 結束的 `ccb4c88`）。審查後的修正：修正波 `003c6e9`（附錄 B）、codex 第二意見修正 `082103c`（附錄 C）；計畫 4 最終 HEAD = `082103c`，共 15 個 commit（`53cef86..082103c`）。
審查者：Senior Code Reviewer（整支分支、只讀）
規格權威：`docs/superpowers/specs/2026-09-06-dog-shop-mvp-design.md`；計畫：`docs/superpowers/plans/2026-09-08-dog-shop-plan-4-logistics-admin-orders-dashboard.md`；逐任務裁決：見附錄 A

---

### 總結

**Ready to merge? With fixes。** 沒有 Critical。這一波最該看對的三件事都看對了：

1. **MD5 CheckMacValue 是對的。** 我用自己寫的 .NET-風格編碼器（`A-Za-z0-9-_.!*()` 原樣、空白 `+`、其餘小寫 `%xx`、全字串轉小寫）獨立重算綠界文件向量，得到 `692FD6E2CDB539CCDB7206C76DC239AD`，與 `mac.rs:206` 的期望值逐字元相同；`check_mac_value_md5` 與 SHA256 版共用同一個 `raw_string`，`verify_with` 把兩種雜湊抽成參數，所以「排序 → 串接 → 編碼」這段只有一份實作、只有一份測試風險。我另外用同一支實作對真的在跑的 api 要來的 `print-label` 表單重算 MAC，**與伺服器產生的完全相同**。
2. **物流單不會重複建。** `ship_cvs`（`routes/admin_orders.rs:115`）的順序是：狀態守衛 → 組欄位 → `claim_create` 先存請求再送（規格 §14）→ 打綠界 → `record_create_ok` 單獨一句存單號 → `finalize_cvs` 收尾。認領 SQL（`shipments.rs:211-219`）同時擋 `status <> 'pending'`、`ecpay_logistics_id IS NOT NULL`、`last_status_code = 'creating'`；而且「有綠界單號但 shipments 還是 pending」在**認領之前**就分岔成「只補收尾、不打綠界」（`admin_orders.rs:147`）。我在真的在跑的 api 上把這兩條路各走了一次（下方 P23／P24），**全程零對外請求**。
3. **鎖序全域一致。** 計畫 3 的 `payments → orders → product_variants` 沒有被破壞，計畫 4 新增的 `orders → shipments` 沒有任何一條路反過來：`apply_status`（`shipments.rs:105`）先用**不加鎖**的 SELECT 找 `order_id`，再 `orders FOR UPDATE`、再 `shipments FOR UPDATE`；`finalize_cvs`／`ship_home` 都是 `orders FOR UPDATE` 之後才動 shipments；`claim_create`／`record_create_*`／`apply_store_update` 是單句 autocommit UPDATE，不持有第二把鎖。`mark_refunded`（`admin_orders.rs:300`）照計畫走 payments → orders → product_variants，而且「還是 paid 才歸還庫存」的判斷在 `FOR UPDATE` 之後才做。我把 ship-cvs／mark-refunded／狀態回呼／過期掃描的交錯逐一推過，沒有環。

Task 10 在同一棵樹跑過的閘門我全部核對過，並自己重跑了 `cargo fmt --all --check`（exit 0）、`pnpm -C web test`（**36 passed**）、`pnpm -C web check`（**312 files, 0 ERRORS 0 WARNINGS**）。Rust 231 passed／0 failed（27 個 `test result` 行）、clippy 零輸出、`pnpm -C web build` 成功、Playwright **2 passed**。

要修的是 2 件 Important 加幾件很便宜的補洞：

1. **建單逾時後的重試會把第一張單的所有證據抹掉。** `post_form` 的 20 秒逾時觸發時（綠界已建單但回應沒回來），錯誤 log **沒有帶 `MerchantTradeNo`**，而下一次認領的 `raw = raw || {"create_request": …}` 會**覆蓋**掉上一次的請求欄位、`ecpay_merchant_trade_no` 也被改成 `L02` —— 於是 `L01` 在資料庫與 log 裡都不存在。而回應給老闆的文案正是「請稍後再試」。這比擱置事項 26 那條（DB 斷線）更糟，因為那條至少還有 `綠界物流單已建立` 的 info log 留著 `AllPayLogisticsID`。
2. **`apply_status` 會無條件覆寫 `last_status_code`**，包含建單認領用的 `'creating'` 標記。狀態通知若在建單 request 往返途中抵達、而且帶的是對照表上沒有的代碼，認領鎖就被解開了 —— 這時第二次點擊會通過認領，用 `L02` 再建一張真的物流單。窗口比第 1 條窄（要三件事同時發生），但修法只是 SQL 裡一個 `CASE`。
3. 幾個規格明列或很便宜的測試缺口：認領守衛（`claim_create` 回 false）沒有直接測試、`apply_status` 的「訂單不是 shipped 就不 completed」守衛沒有測試、`cvs-map` 沒有 CSRF 負向測試、admin 401/403 矩陣少了 5 條路由。
4. `map-reply` 的 413／400 兩條例外路徑會把 axum 的裸錯誤（實測 `Failed to buffer the request body: length limit exceeded`）丟給**買家的瀏覽器**，而同一支 handler 的其他失敗都已經 303 回結帳頁。

四類合計 70 行以內，一波派工可以收尾。

---

### 審查方式

五個 pass，按控制者指示切（沒有再細分）：

1. **憑證與綠界基元**：`config.rs`、`ecpay/{mod,mac,logistics}.rs`、`.env.example`。重點在 stage/prod 分岔、Debug 遮蔽、MD5 演算法是否逐字照 §8.1、`/Express/Create` 欄位集合與回應解析、`mac::raw_string` 有沒有進 log、姓名與寬度清理、`MerchantTradeNo` 長度。
2. **領域與後台路由**：`migrations/0004_cvs_map_requests.sql`、`domain/{shipments,orders,admin_orders,invoices,payments,jobs,cvs_stores}.rs`、`routes/admin_orders.rs`、`jobs/scheduled.rs`、`app.rs`、`state.rs`、`main.rs`。重點在鎖序交錯、認領／冪等收尾、「先存請求再送」、狀態不倒退、`needs_refund` 語意、`retry_invoice` 守衛與去重鍵、明細不回 `guest_token`、`jobs.last_error` 乾淨。
3. **公開回呼與結帳**：`routes/{checkout,ecpay_logistics,ecpay_callback,rate_limit,ecpay_payment,orders}.rs`、`domain/cvs_stores.rs`、`auth/{csrf,tokens}.rs`。重點在 CSRF 豁免範圍、`DefaultBodyLimit` 64 KB + 100 欄位、壞簽章 400／重複 `1|OK`／暫時性 500、map-reply token 單次使用與 303 契約、cvs-map 限流、log 不吃使用者可控的無界字串。
4. **前端**：`lib/{adminOrders,types,labels,ecpay,checkout}.ts`（含測試）、`routes/checkout/**`、`routes/admin/**`、`e2e/checkout.spec.ts`。重點在 Svelte 5 runes、變更請求走 `api()`、TS 型別對 Rust struct、`availableActions` 對後端守衛、標紅規則對 §4、錯誤 toast 是固定文案、列印新分頁、`?store=` 的草稿還原。
5. **測試、文件與擱置事項 triage**：24 個整合測試檔（`admin_orders.rs`、`ecpay_logistics.rs` 全文；其餘掃過計畫 4 的新增段落）、`docs/dev/ecpay-stage.md`、`final-review-inputs.md` 43 條逐行裁決。

跑過的驗證（全部在 HEAD `3ab13c7`、工作樹乾淨）：

| 指令／動作 | 結果 |
|---|---|
| `python3 -c` 自寫 .NET-風格編碼器重算綠界物流文件向量 | `692FD6E2CDB539CCDB7206C76DC239AD`，**與 `mac.rs:206` 的期望值相同** |
| `cargo fmt --all --check`（本次重跑） | **exit 0，零輸出** |
| `pnpm -C web test`（本次重跑） | **36 passed**（7 個檔；計畫寫 35，多的是 `storeErrorMessage` 那組） |
| `pnpm -C web check`（本次重跑） | **312 files、0 ERRORS、0 WARNINGS** |
| Task 10 `task10-full-suite.log` | **231 passed／0 failed**，27 個 `test result` 行（lib 85 + 24 個整合測試檔 + main.rs + doc-tests） |
| Task 10 `task10-clippy.log` | `cargo clippy --all-targets -- -D warnings` 零警告 |
| Task 10 `task10-web-build.log` | build 成功（只有既有的 `chunks/env.js` 空 chunk 提示） |
| Task 10 `task10-e2e.log` | **2 passed（9.3 s）**：宅配主流程 + 超商取貨路徑 |
| Task 10 `task10-api.log` | `cargo run` 跑完 `0004_cvs_map_requests.sql`、`ECPAY_LOGISTICS_*` 空白也起得來 |
| `cargo run`（本次自己起，:8080） | migration notice → SMTP 未設定 WARN → jobs worker 啟動 → listening，**沒有 rustls panic** |
| `git rev-list --count 53cef86..3ab13c7` | **13**（10 個任務 commit + 3 個任務內修正） |
| `git branch -r` | **空**（沒有任何遠端分支，沒有 push） |
| `git status --short` | **空**（工作樹乾淨；HEAD 仍 `3ab13c7`） |
| 活體探測 P1–P28（見下方「活體探測」節） | 28 條，全部符合預期 |

---

### 計畫 3 交接事項對照（`2026-09-08-plan-3-final-review.md` 的「交給計畫 4 的事項」1–9）

| # | 交接內容 | 有沒有做到 |
|---|---|---|
| 1 | 物流回呼要一起吃 body 上限（64 KB + 100 欄位） | **做到**。`routes/ecpay_logistics.rs:27` 一次掛在 router 上，三條路由都吃到；`MAX_CALLBACK_FIELDS` 抽進 `routes/ecpay_callback.rs` 與付款回呼共用。我對三條各送 70 KB 與 150 欄位實測（P26、P6b） |
| 2 | `ApiError::EcpayError(String)` 不要放綠界原文 | **做到**。兩個構造點（`admin_orders.rs:207`、`:236`）都是固定文案，原文只進 log 與 `shipments.last_status_msg`；`ship_cvs_rejection_…` 直接斷言「綠界原文不進 message」 |
| 3 | 後台取消／標記退款要把 pending payments 標 expired，抽 `cancel_with_payments_in_tx`，保持鎖序 | **做到，而且比要求多**。`orders.rs:857` 抽出了建議的函式，**買家取消也改走它**（與規格不同之處 38）；`mark_refunded` 另寫一份（交易邊界不同）但鎖序相同。附帶的「已取消訂單仍寄繳費資訊信」在計畫 3 自己的修正波就已修掉（`payments.rs:216-226`），本計畫沒有回歸 |
| 4 | `payments::list_for_order`；`checkout_form_for` 改成可接受呼叫者給定的 payment | **做到**。前半段是本計畫的 `payments.rs:66`；後半段（`checkout_form_for_payment`）在 `ccb4c88` 就已存在，是計畫 3 自己的修正波做的，本計畫不需要再動 |
| 5 | worker 多副本的 `AND status = 'running'` 守衛 | **正確地留給計畫 5**。計畫的交接第 2 條有寫；本計畫沒有引入新的多副本假設 |
| 6 | `invoices::record_failure` 加 `AND status <> 'issued'` | **做到**（`invoices.rs:98`），並有 `record_failure_never_downgrades_issued_invoice` 測試。附帶的「已開立就略過的分支也要排通知信」依與規格不同之處 44 已因計畫 3 的 codex 修正 `9ed2522` 而不需要 |
| 7 | `RUST_LOG` 與 `mail_body` 寫進部署檢查清單 | **正確地留給計畫 5**。計畫的交接第 1 條有寫 `MAIL_LOG_BODY` 不要設 |
| 8 | 開發用 DB 的舊訂單沒有 `invoices` 列 → 補一句 backfill SQL | **做到**。`docs/dev/ecpay-stage.md` 新增「舊訂單沒有 invoices 列」一節，含可直接跑的 SQL 與「只在開發資料庫用」的但書 |
| 9 | stage 憑證是公開的，部署文件要明說正式站不得沿用 | **部分**。`config.rs` 的 prod 強制檢查在（三個 `ECPAY_LOGISTICS_*` 也納入了），`.env.example` 的註解也寫了「正式憑證只放伺服器的 `.env`」；但「任何知道公開網址的人都能偽造回呼」這句警語沒有進任何文件，而本計畫又多了一條可以把訂單推成 `completed` 的無憑證入口。見下方「交給計畫 5 的事項」第 2 條 |

---

### 優點

- **MD5 這件事是用「可以被別人重算」的方式寫的。** `mac.rs` 把 SHA256／MD5 的差異縮到只有雜湊函式一行（`verify_with` 收 `compute: fn(...)`），`raw_string` 仍然只有一份、仍然被單獨測試對照文件中間值。`md5_check_mac_value_matches_logistics_doc`（`mac.rs:202`）用的是綠界「檢查碼機制」頁的完整 11 個參數（含未編碼中文 `收件者姓名`），而不是自己捏的向量 —— 這是唯一能證明「編碼規則沒寫歪」的測法。`verify_md5_accepts_real_mac_and_rejects_tampered_or_wrong_key` 還斷言了「SHA256 版不能拿 MD5 的簽章過關」。
- **「先存請求再送」是真的先存。** `claim_create`（`shipments.rs:208`）在同一句 UPDATE 裡把 `ecpay_merchant_trade_no` 與 `raw.create_request`（完整送出欄位，含 CheckMacValue）寫進去，而且**這句 UPDATE 就是認領本身** —— 規格 §14 的「先存後送」與計畫 41 的「單句認領」被合成一個原子動作，不會出現「認領成功但請求沒存到」或「存了請求但沒認領到」。回應那一側同樣：成功走 `record_create_ok` 單獨一句（`raw.create_response` + 綠界單號），失敗走 `record_create_failure`（`raw.create_response_text` 截 2000 字）。
- **重複建單的三道閘門互相補位。** (a) `admin_orders.rs:147` 的「有單號＋shipments 還 pending → 只補收尾」在認領前分岔；(b) 認領 SQL 同時要求 `status = 'pending'` **且** `ecpay_logistics_id IS NULL`；(c) `finalize_cvs`（`shipments.rs:304`）對「訂單已經 shipped」直接回 `true`（冪等），出貨信用 `email:order_shipped:{order_id}` 去重鍵 + `ON CONFLICT DO NOTHING`。整合測試 `ship_cvs_finishes_interrupted_transition_without_calling_ecpay_again` 直接斷言 `fake_logistics.calls().len() == 1`，我又在真的在跑的 api 上重現了一次（P24）。
- **失敗重試的號碼推進是對的，而且被測到底。** `ship_cvs_rejection_keeps_order_paid_records_reason_and_retries_with_next_no` 連續走「綠界拒單 → 連線失敗 → 回應簽章不符 → 成功」四次，斷言 `L01/L02/L03/L04`、每次的 `last_status_code`（`create_failed`／`create_error`／`create_failed`）、訂單全程維持 `paid`、失敗不排出貨信，**而且斷言綠界原文沒有出現在回應的 `message` 裡** —— 計畫 3 交接第 2 條被當成可執行的斷言而不是註解。
- **貨態對照與不倒退規則寫得比規格還細，而且有排序語意。** `status_rank`／`should_apply`（`shipments.rs:14-30`）把 `arrived` 與 `returned` 放同一階（退回後可重新配達）、`picked_up` 與宅配 `shipped` 是終態，三個單元測試把正向、逆向、終態、退回—重配四種都蓋了。`shipment_status_for` 的 `2078..=2093` 用 range pattern 一次收掉 7-11 的 16 個退回細分碼。不在表上的代碼只記 `last_status_code`／`raw.last_notification`，不改狀態 —— 我用 `2101` 實測確認（P14）。
- **狀態回呼的鎖序是刻意設計的，不是碰巧。** `apply_status` 的第一次 shipments 查詢**故意不加 `FOR UPDATE`**（只取 `order_id`），因為加了就會變成 shipments → orders，與 `finalize_cvs` 相反。找不到 `MerchantTradeNo` 時再用 `AllPayLogisticsID` 找一次也在同一個交易裡（規格 §8.3 的「都沒有回 `0|Unknown`」）。
- **三條物流回呼的安全前導完全一致，而且是抽出來共用的。** `routes/ecpay_callback.rs` 把 `parse_form`／`MAX_CALLBACK_FIELDS`／`callback_error`／`server_error` 從 `ecpay_payment.rs` 提出來，`ecpay_logistics.rs` 的 router 一次掛上 `DefaultBodyLimit::max(64 * 1024)`（計畫 3 交接第 1 條）。我對三條路徑各送 70 KB 與 150 個欄位實測：**全部 413 / 400（map-reply 是 303 `store_error=invalid`）**，沒有一條漏掉。
- **map-reply 的 303 契約完整。** token 單次使用（`take_map_request` 是 `DELETE … RETURNING`，原子）、有效期 1 小時、超商種類要與登記時相符、失敗一律 303 帶 `store_error`，前端 `storeErrorMessage` 有三段專屬文案 + 一段通用。實測：壞 token → `store_error=expired`；同一個 token 用第二次 → `expired`；種類不符 → `invalid`；成功 → `?store=<token>` 且 `GET /api/checkout/cvs-store/{token}` 讀得到。
- **`cvs-map` 的限流是共用抽出來的，不是複製貼上。** `routes/rate_limit.rs::anonymous_write` 讓 `POST /api/orders` 與 `POST /api/checkout/cvs-map` 用同一份設定但**各自獨立配額**，註解把「只在組路由時呼叫」寫清楚（每次呼叫會起一支清理執行緒）。實測突發 10 次後第 11 次 429。
- **後台明細的資料邊界乾淨。** `AdminOrderRow` 是另寫的欄位清單、沒有 `guest_token`；`Shipment.raw` 是 `#[serde(skip)]`；`Payment`（新加 `Serialize`）只出現在 `AdminOrderDetail.payments`，買家路徑仍走 `PaymentRow`（`id`／`merchant_trade_no` 被 skip）。實測 admin 明細的 30 個頂層 key 裡沒有 `guest_token`，`shipment` 裡沒有 `raw`。
- **`retry_invoice` 的訂單狀態守衛有真正的理由，而且寫在程式碼裡。** `handlers.rs:328-334` 的 `issue_invoice` 對非 paid/shipped/completed 的訂單是**靜默成功**（`Ok(())`），所以把發票 reset 成 `pending` 之後就再也沒有路能變回 `failed` —— `routes/admin_orders.rs:367-369` 的註解把這個死鎖講清楚，測試 `retry_invoice_rejects_non_payable_order` 蓋住，前端 `availableActions` 也對齊（`adminOrders.test.ts` 有一條專門測「已退款 + 發票失敗 → 不給重開」）。
- **儀表板的「今日」時區換算是對的。** `date_trunc('day', now() AT TIME ZONE 'Asia/Taipei') AT TIME ZONE 'Asia/Taipei'` 兩次轉換一進一出，得到台北當日零點的 `timestamptz`。五個方塊、四個清單與規格 §6.1 一一對應，每個方塊都連到對應的列表篩選網址。
- **e2e 的超商路徑測到了該測的。** 攔截 `logistics-stage.ecpay.com.tw/**` 後檢查 `MerchantID=2000933`、`LogisticsSubType`、`IsCollection=N`、`ServerReplyURL` 形狀、**沒有 `CheckMacValue`**、`ExtraData` 是 20 碼英數且等於 `MerchantTradeNo`；再用 `request.post` 模擬綠界回傳（`maxRedirects: 0`）驗 303 與 Location；回到結帳頁驗草稿還原（Email、超商選擇）與門市顯示；最後送出訂單驗 AIO 表單與訂單頁門市。這條把「跨頁面的 sessionStorage + token 往返」整段串起來了。
- **文件寫給真的要走一次的人。** `docs/dev/ecpay-stage.md` 的「超商取貨（物流）」節不只列步驟，還列了三個「實作時無法自動驗證」的待確認項（回應 MAC、三家超商是否接受 `LogisticsC2CReplyURL`／`ReceiverEmail`、全家萊爾富的 `CVSPaymentNo` 形狀）、附了一段可直接跑的 python 模擬狀態通知（因為 stage 不發通知）、並提醒 Safari 可能擋列印新分頁。

---

### 問題

#### Critical（必修）

無。

#### Important（應修）

**I1. `api/src/routes/admin_orders.rs:204` + `api/src/domain/shipments.rs:215` — 建單逾時後的重試會建出第二張真的物流單，而第一張在資料庫與 log 裡都不留痕跡。**

`EcpayLogisticsClient` 的 `HTTP_TIMEOUT_SECS = 20`（`logistics.rs:34`）。綠界收到並建立了 `L01`、但回應在 20 秒內沒回來時：

```rust
Err(e) => {
    tracing::error!(order_id = %id, error = %format!("{e:#}"), "綠界建立物流單連線失敗");
    shipments::record_create_failure(&state.db, id, CREATE_ERROR, "連線綠界失敗", None).await?;
    return Err(ApiError::EcpayError("連線綠界物流失敗，請稍後再試".to_string()));
}
```

三個問題疊在一起：

- **log 沒有 `MerchantTradeNo`。** 只有 `order_id` 與 reqwest 的錯誤字串。相較之下成功路徑的 `tracing::info!` 有 `logistics_id`。
- **`record_create_failure(…, None)` 不寫任何回應痕跡**（`patch = json!({})`），`last_status_msg` 只有固定的「連線綠界失敗」。
- **下一次認領會把上一次的證據蓋掉。** `claim_create` 的 `raw = COALESCE(raw,'{}') || $4`（`shipments.rs:216`）在頂層合併，同名 key 直接取代 —— `raw.create_request` 變成 `L02` 的欄位；同一句 UPDATE 也把 `ecpay_merchant_trade_no` 改成 `L02`、`last_status_msg` 設回 NULL。

結果：`L01` 這張真的存在於綠界的託運單，在我們這邊**完全沒有紀錄**，老闆只能靠 `order_no` 前綴去綠界廠商後台一張張比對。而且回應文案「請稍後再試」正是在鼓勵這個動作。

這條不需要未對照代碼、不需要雙擊、也不需要資料庫故障 —— 只要綠界回應慢一次。計畫的與規格不同之處 41 明文指定「連線失敗：`last_status_code = 'create_error'`……下次重試流水號 +1」，所以「允許換號重試」是設計決定；但「把上一次的請求與單號抹掉」不是設計決定，那是設計沒顧到的副作用。

**我把它評為 Important 而非 Critical**，理由是：損失上限是一次運費、可以在綠界廠商後台用 `order_no` 前綴人工對出來（雖然很痛）、而且需要綠界回應超過 20 秒。但它落在控制者所定「ECPay-order duplication」這一類，所以放在修正波第一位。

**修法（都是加東西，不改控制流）：**
1. 兩個失敗分支的 log 都補 `merchant_trade_no = %req.merchant_trade_no`（`admin_orders.rs:204`、`:222`）。
2. `create_error` 的回應文案改成「連線綠界失敗，**請先到綠界廠商後台確認這筆是否已建單**，再決定要不要重試」——不要叫老闆盲目重按。
3. `claim_create` 保留歷次請求：
   ```sql
   raw = COALESCE(raw, '{}'::jsonb)
         || jsonb_build_object('create_request', $4::jsonb,
                               'create_requests',
                               COALESCE(raw -> 'create_requests', '[]'::jsonb) || jsonb_build_array($4::jsonb))
   ```
   （`$4` 目前是 `{"create_request": <fields>}`，改成直接傳 `<fields>`。）

**測試：** 在 `ship_cvs_rejection_keeps_order_paid_records_reason_and_retries_with_next_no` 的第四次成功之後加斷言：`raw->'create_requests'` 是長度 4 的陣列，四個元素的 `MerchantTradeNo` 依序是 `L01`～`L04`。

**I2. `api/src/domain/shipments.rs:134` — 狀態回呼會覆寫建單認領用的 `'creating'` 標記，窄窗下可重複建一張真的物流單。**

`apply_status` 的 UPDATE 無條件寫 `last_status_code = $3`（綠界代碼），而 `claim_create`（`shipments.rs:215-219`）正是用 `last_status_code = 'creating'` 當認領鎖：

```
WHERE order_id = $1 AND status = $5 AND ecpay_logistics_id IS NULL
  AND (last_status_code IS DISTINCT FROM 'creating' OR updated_at < now() - interval '2 minutes')
```

情境：後台按下「建立物流單」→ 認領成功（`creating`）→ request 在往返途中 → 綠界推一則狀態通知進來 → `apply_status` 把 `last_status_code` 改成該代碼。若那個代碼**不在對照表上**（`shipment_status_for` 回 `None`），`shipments.status` 仍是 `pending`、`ecpay_logistics_id` 仍是 NULL，於是認領的三個條件全部再度成立 —— 這一秒內的第二次點擊會通過認領，用 `L02` 對 `/Express/Create` 再建一張。結果是重複運費 + 兩張託運單，而第一張的 `AllPayLogisticsID` 只會落在 `record_create_ok` 覆寫前的那一刻。

（若代碼**在**對照表上，`status` 會變成 `created`，認領被 `status = 'pending'` 擋住，沒有問題。前端 `busy` 旗標也擋得住同一分頁的連點。所以真正需要的是「代碼未對照 + 通知剛好在往返途中 + 另一個分頁／另一位管理員同時點」三件事同時發生。）

**修法（1 行 SQL）：** `apply_status` 的 UPDATE 改成

```sql
last_status_code = CASE WHEN last_status_code = 'creating' THEN last_status_code ELSE $3 END,
```

代碼本身仍完整保存在 `raw.last_notification`，`last_status_msg` 照舊更新；`record_create_ok`／`record_create_failure` 之後一定會把 `creating` 換掉，所以標記不會卡住（真的卡住還有 2 分鐘的重認領窗口）。

**測試：** `api/tests/ecpay_logistics.rs` 新增一條：把 shipment 設成 `status='pending'`、`last_status_code='creating'`、`updated_at=now()`，**而且要設 `ecpay_merchant_trade_no`**（`apply_status` 是用這個欄位找單的，只設 `last_status_code` 找不到），送一則未對照代碼（例如 `2101`）的通知 → 回 `1|OK`、`last_status_code` 仍是 `'creating'`、`raw.last_notification.RtnCode = '2101'`；接著呼叫 `POST /api/admin/orders/{id}/ship-cvs` → 400 且 `fake_logistics.calls()` 沒有增加。

**附帶取捨（接受，不需另外處理）：** 這個 UPDATE 仍會寫 `updated_at = now()`，所以每來一則通知就把「2 分鐘後可重認領」的時鐘往後推。這在正確方向上：卡住的建單本來就不該因為綠界持續發通知而被搶著重建；真的需要重建時，`create_failed`／`create_error` 兩條路徑都會把 `creating` 換掉。

**這一項動到認領／鎖邏輯。**

#### Minor（可選）

1. **`api/src/routes/ecpay_logistics.rs:36` — `map_reply` 的 body extractor 失敗會把裸錯誤丟給買家。** handler 內部的每一條失敗都 303 回 `/checkout?store_error=…`，但 `body: String` 這個 extractor 本身的拒絕（>64 KB、非 UTF-8）發生在 handler 之前。實測送 70 KB → `413 Failed to buffer the request body: length limit exceeded` 純文字。綠界不會送這麼大的 body，但這條的呼叫者是**買家的瀏覽器**，跟其他失敗路徑的體驗不一致。修法 4 行：把參數改成 `body: Result<String, axum::extract::rejection::StringRejection>`，`Err` 就 `fail("invalid")`。（擱置事項 13）

2. **`api/src/ecpay/logistics.rs:250` — `CreateError::Malformed` 的 `other` 分支沒有上界。** 回應若是「前面一大段、中間才有 `|`」的東西（例如反向代理的錯誤頁），`flag` 會是整段前綴，`format!("開頭是 {other}")` 原封不動進 `tracing::warn!`。DB 那側安全（`record_create_failure` 會截 200 字、`ApiError::EcpayError` 是固定文案），所以只是 log 膨脹。修法：`truncate_chars(other, 200)`。（擱置事項 1）

3. **`docs/dev/ecpay-stage.md` 第 6 點的 python 片段與 .NET 編碼在 `~` 上不一致。** 片段用 `quote_plus(raw, safe="")`，而 Python 的「永不編碼」集合含 `~`；.NET 的 `HttpUtility.UrlEncode` 會把 `~` 編成 `%7e`。我實測：一般欄位兩者結果相同，但值裡只要出現 `~` 就會算出不同的 MAC，走查的人會看到莫名其妙的 400。修法：替換表加一行 `enc = enc.replace("~", "%7e")`（要在 `.lower()` 之後）。

4. **`api/src/domain/shipments.rs:105`／`:166` — `last_status_code` 與 `last_status_msg` 會配成不相稱的一對。** 我實測（P12）：訂單已 `completed`／shipment 已 `picked_up` 之後再收一則晚到的 `2030`，狀態正確不倒退，但 `last_status_code` 變成 `2030`、訊息變成「物流中心驗收成功」；(P15) 更新門市通知只改 `last_status_msg` 不改 `last_status_code`，後台明細那行變成「最近通知：2101 取件門市異動：門市關轉店（991182）」。`attentionNotes` 不受影響（它只看 `create_failed`／`create_error`），純外觀。（擱置事項 21）

5. **`api/src/ecpay/logistics.rs:468` — `goods_name` 先組字串再截寬度，長品名會吃掉「等 N 件」。** 單元測試自己就示範了：40 個中文字的品名截成 25 字，如果是多品項，「等 3 件」根本進不去。託運單上少一個「還有其他商品」的提示，不影響配送。（擱置事項 2）

6. **`api/src/domain/admin_orders.rs:76` — `q` 的 ILIKE 沒有 escape `%`／`_`。** 全部參數化（`$1`～`$5`），**沒有注入風險**；只是老闆在後台搜尋框輸入 `_` 會比對到任意單一字元。（擱置事項 24）

7. **`api/src/domain/admin_orders.rs:113` — `page` 超出範圍時 `total` 回 0。** `COUNT(*) OVER ()` 取自第一列，沒有列就沒有計數。實測 `?page=99` → `total=0`。沿用 `products.rs` 的既有慣例，前端 `Pagination` 因此在最後一頁之後不顯示總數。（擱置事項 25）

8. **`api/src/ecpay/logistics.rs:27` — `SENDER_NAME_WIDTH_MAX` 也拿來限制收件人。** 數字（寬度 10）對兩者都正確（綠界：收件人 4～10 字元、中文 2～5），只是常數名稱誤導。（擱置事項 5）

9. **`api/src/ecpay/logistics.rs:265` — `RtnCode` 非數字時 `unwrap_or(0)`。** 這是**已通過 MD5 驗簽**的成功回應，欄位型別由綠界保證；`parse_status` 那側則正確地把非數字當 `Missing("RtnCode")`。兩邊寬嚴不同但都說得通。（擱置事項 3）

10. **`api/src/routes/admin_orders.rs` 已 414 行、`ship_cvs` 單一 handler 126 行。** 計畫指定的形狀（動作做完回整份明細），可讀性尚可，但下一個計畫再加動作就該拆。`web/src/routes/admin/orders/[id]/+page.svelte` 254 行同理。（擱置事項 35、43）

---

### 擱置事項的裁決（`final-review-inputs.md` 逐行）

> 每一條的原文都抄在這裡（工作區檔案會被刪除），後面接裁決與理由。

**Task 2（`api/src/ecpay/logistics.rs` 純函式與閘道，commit e8e0cd4）**

1. 「`logistics.rs:250` `Malformed` 的 `other` 沒截 200 字就存進 `last_status_msg`／log。」→ **修**。實際上 DB 那側安全（`record_create_failure` 自己截 200），只有 `tracing::warn!` 無界；1 行 `truncate_chars`。見 Minor 2。
2. 「`goods_name` 先組字串再截斷，會吃掉「等 N 件」尾巴。」→ **不修**。要正確保留尾巴得先算尾巴寬度再回頭截前綴，是新邏輯不是一行；託運單少一個提示不影響配送。見 Minor 5。
3. 「`RtnCode` 非數字時 `unwrap_or(0)`，靜默當成 0。」→ **不修**。這是驗簽通過後的綠界成功回應，欄位型別由綠界保證；`parse_status` 那條真正會被外部餵的路徑已經正確地回 `Missing`。
4. 「`sub_type` 不合法時回 `Missing` 而非 `Invalid`。」→ **不修**。`CallbackError` 只有兩個變體且被付款回呼共用，加變體要改兩支 handler 的對應；map-reply 兩者都映到 `store_error=invalid`，對外行為相同。
5. 「`SENDER_NAME_WIDTH_MAX` 也拿來限制收件人，命名誤導。」→ **不修**。純命名，數值對兩者都正確。見 Minor 8。
6. 「缺 `GoodsAmount` 下界測試、Fake 閘道四條路徑測試、`store_update_message` 未知分支測試。」→ **不修**。上界已測（25000 → 20000），下界是同一句 `clamp`；Fake 閘道的三條路徑（`respond_with`／`fail_next`／預設）已被 `admin_orders.rs` 的建單測試實際走過。
7. 「（流程）整合測試腳本的 exit code 來自 `tail`，下次加 `pipefail`。」→ **不修**（流程備忘，非程式碼）。本次驗證我直接看 `test result` 行與 exit code，沒有依賴 `tail`。
8. 「`status_rank` 對未知字串回 0，`should_apply(未知, x)` 恆 true。」→ **不修**。`current` 只可能來自 `shipments.status`，該欄位有 CHECK 約束，七個值全在 `status_rank` 的表上。
9. 「⚠️ `LogisticsC2CReplyURL` 對 FAMIC2C／HILIFEC2C 也送出，三家超商是否都接受。」→ **交計畫 5**。只有實機能答；`docs/dev/ecpay-stage.md` 第 4 點 (b) 已列。
10. 「⚠️ 真實 `/Express/Create` 回應是否 percent-encoded、能否通過嚴格 MD5 驗證。」→ **交計畫 5**。同上，計畫的交接第 5 條已寫且指出改一處即可（`parse_create_response`），`raw.create_response_text` 留有原文可比對。

**Task 3（結帳選門市，commits 417b589 + ef5f5ed）**

11. 「`onSubTypeChange` 無單元測試。」→ **不修**。兩行的元件內函式，抽出來測的成本大於價值；e2e 已走過「選超商 → 選門市 → 回來仍是同一家」。
12. 「`map-reply` 的 token `take` 之後 insert 失敗會讓 token 變 `expired`（買家要重選）。」→ **不修**。`cvs_store_selections` 的欄位全是 `text`（`0002:145-154`），不會因為門市名稱過長而失敗，只有資料庫真的出問題才會走到；那時 303 `store_error=server` 的中文提示與「重新選擇門市」按鈕已經在位。包成交易要把 `take_map_request`／`insert` 兩個函式的簽章都改成收 executor，收益不成比例。
13. 「`map-reply` 的 413／400 兩條非 303 例外路徑。」→ **修**。實測確認買家會看到 axum 的裸錯誤；`Result<String, StringRejection>` 4 行。見 Minor 1。
14. 「cvs-map 缺 CSRF 負向測試（沒帶 `X-Requested-With` 應被擋）。」→ **修**。我實測是 403（行為正確），但沒有測試釘住 —— cvs-map 是匿名可呼叫的寫入端點，這條該有回歸測試。
15. 「限流配額數字重複寫在 `routes/rate_limit.rs` 兩處與 `routes/orders.rs:27` 註解。」→ **不修**。註解重複，程式碼只有一份（`anonymous_write`）。
16. 「⚠️ 真實瀏覽器跨站 POST→303→GET 經 Vite proxy／反向代理未被 e2e 覆蓋。」→ **交計畫 5**。`docs/dev/ecpay-stage.md` 第 1 點已加走查句；正式環境是 Caddy，只有部署後能驗。

**Task 4（物流狀態回呼與更新門市通知，commit 45fb483）**

17. 「`order_status == shipped` 守衛無測試。」→ **修**。規格 §4 明列的分支（取貨完成只在訂單已出貨時才轉 completed），補法簡單：`arrived` 之後把訂單改回 `paid` 再送 `2067`，斷言 shipment 進 `picked_up` 但訂單仍 `paid`、`completed_at` 仍 NULL。
18. 「`shipments.raw` 保留其他 key 無測試。」→ **不修**。我在活體探測直接驗到了：同一列同時有 `create_request`／`last_notification`／`store_updates` 三個 key（P15 印出 `raw keys: store_updates,last_notification`，加上建單路徑的 `create_request`），`||` 與 `jsonb_set` 的合併語意如預期。
19. 「`use` 宣告落在檔案中段（`tests/ecpay_logistics.rs:346`、`domain/shipments.rs:32`）。」→ **不修**。風格；`fmt`／`clippy` 都不抱怨，移動會讓 diff 變髒。
20. 「測試用 `fields[6]` 魔術索引。」→ **不修**。風格，且 `status_fields` 是同檔案內的固定順序建構子。
21. 「store-update 只改 `last_status_msg` 不動 `last_status_code`；「最近通知」那行可能顯示舊 code + 門市更新訊息。」→ **不修**。已在活體探測重現（P15），確認純外觀且 `attentionNotes` 不受影響。要修得引入「通知種類」的概念，超出一行。見 Minor 4。
22. 「四個回呼 handler 的四行前導可抽進 `ecpay_callback`。」→ **不修**。已經抽了一半（`parse_form`／`MAX_CALLBACK_FIELDS`／`callback_error`／`server_error` 都在 `routes/ecpay_callback.rs`），剩下的三行在四支 handler 裡形狀不同（map-reply 回 303、另兩支回純文字）；再抽會讓控制流變隱晦。我逐條核對過四支都掛了 body 上限與欄位上限，並實測驗證。
23. 「`shipments.ecpay_merchant_trade_no`／`ecpay_logistics_id` 無唯一約束。」→ **不修**。`MerchantTradeNo` 是 `order_no + L + 兩碼`，`order_no` 本身 UNIQUE，同一訂單只有一列 shipment（`order_id` UNIQUE），碰撞不可能；`AllPayLogisticsID` 由綠界保證唯一。加約束是防禦深度，但會讓計畫 5 的匯入／備援還原多一個失敗模式。
24. 「`admin_orders::list` 的 `q` 用 ILIKE 沒 escape `%`／`_`。」→ **不修**。全部參數化，無注入；後台自用搜尋。見 Minor 6。
25. 「`page` 超出範圍時 `total` 回 0。」→ **不修**。沿用 `products.rs` 的既有慣例，改一處就得改兩處並重測分頁元件。見 Minor 7。

**Task 6（出貨，commit b8a08ac）**

26. 「⚠️ `record_create_ok` 之後若 DB 斷線，2 分鐘後重認領會在綠界建第二張單，第一張的 `AllPayLogisticsID` 只在 log。」→ **交計畫 5**，但**它的兄弟情境要修**。計畫的交接第 2 條已列這一條；審查時我發現**同一族還有一條更容易發生的**（建單逾時，不需要 DB 故障），而且證據更少 —— 見 Important I1，已進修正波第 1 項。至於本條原文的情境：`AllPayLogisticsID` 在 `record_create_ok` **之前**就已經用 `tracing::info!(order_id, logistics_id, rtn_code, "綠界物流單已建立")` 記下來（`admin_orders.rs:212`），真的發生時可以從 log 救回來 —— 部署文件應把這句 log 列為「必須保留」。
27. 「`claim_create` 回 false 的 SQL 守衛無直接測試。」→ **修**。我在真的在跑的 api 上把這條路走過（P23：`last_status_code='creating'` + `updated_at=now()` → 400 且沒有寫入 `raw.create_request`），確認行為正確，但這是**擋重複建單的那道鎖**，必須有回歸測試釘住。**動到認領邏輯。**
28. 「`creating` 鎖標記寄生在 `last_status_code`，`apply_status` 無條件覆寫 → 通知帶未對照代碼時秒級雙擊可重認領建第二張。」→ **修**。升格為本文件的 Important I2；1 行 `CASE WHEN`。**動到認領邏輯。**
29. 「`BadMac` 當拒單換 `L02`，綠界可能已建 `L01` 成孤兒。」→ **不修**。計畫指定（與規格不同之處 40：合作特店必須檢查回應簽章）；`last_status_msg` 存「回應簽章不符：<前 200 字>」、`raw.create_response_text` 存整段，老闆可到綠界後台核對。改成「簽章不符也當成功」會比孤兒更危險。走查第 4 點 (a) 就是為了確認這條不會在正常情況觸發。
30. 「`next_merchant_trade_no` doc comment 與實作不符。」→ **不修**。註解寫「從右邊取最後一段」，實作是 `strip_prefix(order_no)` 再 `strip_prefix('L')` —— 對 `order_no` 本身含 `L` 的情況兩者結果相同（因為 prefix 是完整比對），敘述不精確但沒有行為差異。
31. 「`MerchantTradeNo` 無 ≤20 字守衛（`order_no` 現 12 字 + `L01` = 15）。」→ **修**。1 行 + 1 個 unit assert；擋的是未來 `order_no` 格式改動時的靜默截斷（綠界 String(20)）。
32. 「`require_admin` 測試：ship-cvs 401、ship-home 403、print-label、complete 仍未逐條覆蓋。」→ **修**。與第 40 條合併成一個迴圈，10 行以內。規格 §15 明列 admin 權限測試。
33. 「`ship_home` 未檢查 shipments UPDATE 的 rows_affected。」→ **不修**。`create_order` 對每筆訂單一定插一列 shipment（`order_id` UNIQUE NOT NULL），且 `ship_home` 已在同一交易先 `orders FOR UPDATE` 確認 `shipping_method = 'home'`；rows_affected 只可能是 1。要寫測試得先人工刪掉 shipment 列，測的是不可能的狀態。
34. 「`complete`／`ship_cvs` 重複抓明細 2–3 次。」→ **不修**。計畫指定（交易外檢查給欄位級錯誤、交易內 `FOR UPDATE` 防競賽），這是刻意的兩段式；每次 `get_detail` 是 4 個小查詢，後台流量下不是問題。
35. 「`ship_cvs` handler 126 行、`routes/admin_orders.rs` 已超過 400 行。」→ **不修**。計畫指定的形狀；拆分屬重構，應該在有新需求時一起做。見 Minor 10。

**Task 7（後台取消／退款／重開發票／清除，commits 34c15fd + 1ec9e8e）**

36. 「「作廢待付款嘗試」那句 SQL 三份逐字複本。」→ **不修**。三處的交易邊界不同（`expire_one` 要在兩步之間重查到期條件、`cancel_with_payments_in_tx` 要能整筆 rollback、`mark_refunded` 之後還要判斷是否歸還庫存），抽共用函式會把差異藏起來。計畫已明示這是刻意的。
37. 「`complete` 仍用內嵌 NotFound 而非新的 `ensure_exists`。」→ **不修**。純風格，3 行。
38. 「`apply_info` 的訂單狀態守衛因 `ecpay_payment.rs:780` 期望值改成 `expired` 而失去唯一測試覆蓋。」→ **修**。`payments.rs:216-226` 是一段有真實效果的守衛（避免叫客人去付一筆已取消／已由另一筆付掉的訂單）而目前零覆蓋。補法乾淨：訂單用第二筆嘗試付成功變 `paid` → 對第一筆（仍 `pending`）送 `PaymentInfoURL` 回呼 → 斷言不寫繳費資訊、不排 `payment_instructions` 信。約 20 行。
39. 「`retry_invoice` 先 `get_detail` 再另開 tx，check-then-act 之間無列鎖。」→ **不修**。最壞情況是「發票被 reset 成 pending 但訂單同時變 refunded」，結果是發票停在 `pending`（job 靜默略過），沒有錢的損失也沒有錯誤資料；需要老闆在同一瞬間按下兩個不同動作。與 `ship_cvs` 的慣例一致。

**Task 8（儀表板，commit f908718）**

40. 「`GET /api/admin/dashboard` 只測 401 未測 403。」→ **修**。併入第 32 條的同一個迴圈。

**Task 9（前端後台頁，commits c6466a4 + b188989）**

41. 「`printLabel` 在 `await api()` 之後才 `postToEcpay(form, '_blank')`，Safari 可能當彈出視窗擋掉。」→ **不修**（並**交計畫 5** 驗證）。可靠的修法是先 `window.open('', '_blank')` 佔位再把表單 submit 進去，會改變 `postToEcpay` 的契約且無法用現有測試覆蓋；`docs/dev/ecpay-stage.md` 第 5 點已寫「被擋就允許彈出視窗或改用 Chrome」。請在 stage 走查時用老闆實際會用的瀏覽器確認一次。
42. 「列表頁 `attention()` 與 `lib/adminOrders.ts::attentionNotes()` 平行實作。」→ **不修**。輸入型別不同（`AdminOrderListItem` 只有 `shipment_status`／`invoice_status` 兩個扁平欄位，明細有完整 `shipment`／`invoice`），硬要共用得先造一個中介型別。兩邊的三條紅字規則我逐條比對過，一致。
43. 「`admin/orders/[id]/+page.svelte` 254 行未拆子元件。」→ **不修**。見第 35 條。

---

### 修正波清單（Fix wave）

一次派工、一個或多個 commit，都在本機分支、不 push。每項都要維持 `cargo test`／`cargo fmt --all --check`／`cargo clippy --all-targets -- -D warnings`／`pnpm -C web check`／`pnpm -C web test` 全綠。依價值排序。

1. **【動到認領／鎖邏輯】`api/src/routes/admin_orders.rs:204`／`:222` + `api/src/domain/shipments.rs:215`（Important I1）— 建單失敗要留下可對帳的證據，文案不要叫老闆盲目重試。**
   (a) 兩個失敗分支的 log 各補 `merchant_trade_no = %req.merchant_trade_no`。
   (b) 連線失敗的回應文案改成「連線綠界失敗，請先到綠界廠商後台確認這筆是否已建單，再決定要不要重試」。
   (c) `claim_create` 的 `raw` 改成同時累積歷次請求：`raw = COALESCE(raw,'{}'::jsonb) || jsonb_build_object('create_request', $4::jsonb, 'create_requests', COALESCE(raw -> 'create_requests', '[]'::jsonb) || jsonb_build_array($4::jsonb))`（`$4` 改成直接綁 `fields`，不再包一層 `{"create_request": …}`）。
   測試：`ship_cvs_rejection_keeps_order_paid_records_reason_and_retries_with_next_no` 的第四次成功之後加斷言 —— `raw->'create_requests'` 長度 4、四個元素的 `MerchantTradeNo` 依序 `L01`～`L04`；`raw->'create_request'->>'MerchantTradeNo'` 仍是最後一次。
   驗證：`cargo test --test admin_orders ship_cvs`。

2. **【動到認領／鎖邏輯】`api/src/domain/shipments.rs:134`（Important I2、擱置 28）— `apply_status` 不覆寫 `'creating'`。**
   UPDATE 的 `last_status_code = $3` 改成 `last_status_code = CASE WHEN last_status_code = 'creating' THEN last_status_code ELSE $3 END`。其餘欄位（`last_status_msg`、`raw.last_notification`、`status`）不動。
   測試：`api/tests/ecpay_logistics.rs` 新增 `status_notification_does_not_release_the_create_claim`：shipment 設 `status='pending'`／`last_status_code='creating'`／`updated_at=now()`／**`ecpay_merchant_trade_no` 要設**（`apply_status` 靠它找單），送 `RtnCode=2101` → `1|OK`、`last_status_code` 仍 `'creating'`、`raw.last_notification.RtnCode='2101'`；再 `POST /api/admin/orders/{id}/ship-cvs` → 400 且 `fake_logistics(&state).calls()` 沒有增加。
   驗證：`cargo test --test ecpay_logistics`。

3. **【動到認領／鎖邏輯】`api/tests/admin_orders.rs`（擱置 27）— 認領守衛的直接測試。**
   新增 `ship_cvs_rejects_while_another_create_is_in_flight`：`place_order(cvs)` + `mark_paid` + `set_sender`，用 SQL 把 shipment 設成 `last_status_code='creating'`、`updated_at=now()` → `ship-cvs` 回 400（`details.fields.status` 是字串）且 `fake_logistics.calls().is_empty()`、`raw` 沒有 `create_request`；接著把 `updated_at` 退 3 分鐘 → 再按可以建單成功、`ecpay_merchant_trade_no` 是 `{order_no}L01`。
   驗證：`cargo test --test admin_orders ship_cvs_rejects_while_another_create`。

4. **`api/tests/ecpay_logistics.rs`（擱置 17）— 「訂單不是 shipped 就不 completed」的守衛測試。**
   在 `status_callback_moves_forward_completes_on_pickup_and_never_regresses` 之後新增一條：先送 `2073` 讓 shipment 進 `arrived`，用 SQL 把 `orders.status` 改回 `'paid'`、`completed_at=NULL`，再送 `2067` → `1|OK`、`shipments.status='picked_up'`、`orders.status` 仍 `'paid'`、`completed_at` 仍 NULL。
   驗證：`cargo test --test ecpay_logistics`。

5. **`api/tests/admin_orders.rs:815`（擱置 32 + 40）— 補完 admin 401/403 矩陣。**
   把 `mutating_admin_order_routes_require_admin` 的迴圈擴成八條動作路由（`ship-cvs`、`ship-home`、`print-label`、`complete`、`cancel`、`mark-refunded`、`retry-invoice`、`clear-refund`）各驗 401 + 403；`admin_order_routes_require_admin` 加 `GET /api/admin/dashboard` 的 401 + 403。注意 `ship-home` 的 body 要帶 `carrier`／`tracking_no`（權限檢查在 extractor，先於欄位驗證，但保持 body 合法比較不會誤讀失敗原因）。
   驗證：`cargo test --test admin_orders require_admin`。

6. **`api/tests/ecpay_logistics.rs`（擱置 14）— cvs-map 的 CSRF 負向測試。**
   新增 `cvs_map_requires_the_csrf_marker`：用 `common::req` 之外的方式送一個沒有 `X-Requested-With` 的 `POST /api/checkout/cvs-map`（`tests/csrf.rs` 有現成寫法）→ 403 `FORBIDDEN`，且 `cvs_map_requests` 沒有新列。
   驗證：`cargo test --test ecpay_logistics cvs_map_requires`。

7. **`api/src/routes/ecpay_logistics.rs:36`（Minor 1、擱置 13）— map-reply 的 extractor 拒絕也走 303。**
   handler 簽章改成 `body: Result<String, axum::extract::rejection::StringRejection>`，開頭 `let Ok(body) = body else { tracing::warn!("map-reply body 過大或不是 UTF-8"); return fail("invalid"); };`。
   測試：把 `map_reply_ignores_csrf_headers_but_limits_body` 的斷言從 `PAYLOAD_TOO_LARGE` 改成 303 + `Location` 結尾 `store_error=invalid`。
   驗證：`cargo test --test ecpay_logistics map_reply`。

8. **`api/src/ecpay/logistics.rs:250`（Minor 2、擱置 1）— `Malformed` 的 `other` 截 200 字。**
   `format!("開頭是 {}", truncate_chars(other, 200))`。
   測試：在 `parse_create_response_success_rejected_bad_mac_malformed` 加一句：`parse_create_response(&cfg.ecpay, &("x".repeat(5000) + "|y"))` 的 `Malformed` 訊息長度 ≤ 210。
   驗證：`cargo test --lib ecpay::logistics`。

9. **`docs/dev/ecpay-stage.md` 第 6 點（Minor 3）— python 片段補 `~` 的編碼。**
   替換表後加一行 `enc = enc.replace("~", "%7e")`（`.lower()` 之後、`md5()` 之前），並在該段加一句「這段只是把 `ecpay/mac.rs` 的規則用 python 重寫，權威在 `mac.rs`」。
   驗證：文件變更；可用 `python3` 對綠界文件向量 `692FD6E2CDB539CCDB7206C76DC239AD` 自我檢查一次。

10. **`api/src/domain/shipments.rs:190`（擱置 31）— `next_merchant_trade_no` 加 ≤ 20 字守衛。**
   `format!` 之後檢查 `result.chars().count() > 20` 就回 `ApiError::field("shipment", "訂單編號過長，無法建立物流單")`。
   測試：`merchant_trade_no_increments_per_attempt` 加一句：`next_merchant_trade_no(&"A".repeat(20), None).is_err()`。
   驗證：`cargo test --lib domain::shipments`。

11. **`api/tests/ecpay_payment.rs`（擱置 38）— 補回 `apply_info` 訂單狀態守衛的覆蓋。**
    新增 `payment_info_after_another_attempt_paid_is_ignored`：`place_order` 拿 `…01` → `POST /api/orders/{id}/repay` 產生 `…02` → 對 `…02` 送成功回呼（訂單 `paid`）→ 對 `…01` 送 `PaymentInfoURL`（`RtnCode=2` + ATM 欄位）→ 斷言 200 `1|OK`、`…01` 的 `atm_vaccount`／`expire_at` 仍 NULL、沒有新的 `email:payment_instructions` job。
    驗證：`cargo test --test ecpay_payment payment_info_after_another_attempt`。

（第 1、2、3 項動到認領／鎖邏輯，請一起做並在同一次 `cargo test --test admin_orders --test ecpay_logistics` 下驗證。第 3–6、11 是純測試，第 7–10 各是 1～4 行。）

---

### 交給計畫 5 的事項（補充計畫 5871–5883 行沒寫到的）

1. **要有一份「孤兒物流單對帳程序」，而且 log 是它的唯一輸入。** `routes/admin_orders.rs:212` 的 `tracing::info!(order_id, logistics_id, rtn_code, "綠界物流單已建立")` 在 `record_create_ok` **之前**就寫下綠界單號，是擱置事項 26 那條情境（DB 斷線）的唯一救援線索；修正波第 1 項會把 `merchant_trade_no` 加進兩個失敗分支的 log、把歷次請求存進 `raw.create_requests`，讓 I1 那條（逾時重試）也可救。部署文件要寫明三件事：(a) 這兩條 log 必須保留（`RUST_LOG` 不得把 `dog_shop_api::routes` 降到 warn 以下）且可搜尋（規格 §11 的 JSON log + request id）；(b) 對帳步驟 —— 到綠界廠商後台「物流建單及查詢」用 `order_no` 前綴搜尋，同一筆訂單出現多張 `…L01`／`…L02` 就是孤兒；(c) 孤兒要在綠界後台自行處理（本專案沒有取消物流單的 API）。

2. **可選的更強防護（不放本次修正波）：`apply_status` 回填 `ecpay_logistics_id`。** 用 `MerchantTradeNo` 找到單時順手 `ecpay_logistics_id = COALESCE(ecpay_logistics_id, NULLIF($n, ''))`，並把 `ship_cvs` 的「只補收尾」條件從 `ecpay_logistics_id.is_some() && status == pending` 放寬成 `ecpay_logistics_id.is_some()`。這樣一來，只要綠界的第一則狀態通知比老闆的重試先到，逾時那張單就會被自動認回來、重試變成「只補收尾」。限制很明確：通知比重試慢就沒用（通知是批次、非即時），所以它是加分不是解法，而且會動到認領路徑，建議在計畫 5 有 stage 實測資料之後再評估。
3. **`ECPAY_LOGISTICS_*` 的 stage 憑證已經進了 `.env.example`（2000933 / `XBERn1YOvpM9nfZc` / `h1ONHk4P4yqbl5LK`）。** 這與 AIO、發票一致、是規格 §8.5 允許的公開測試資料，但意味著**只要有人拿到 `PUBLIC_BASE_URL` 就能偽造一則物流狀態通知**（把訂單推成 `completed`）。計畫 3 交接第 9 條只提到付款回呼；物流回呼現在也在同一風險面上。部署檢查清單要有一行「`ECPAY_ENV=prod` 且三組憑證都不是 `.env.example` 的值」——`config.rs:143` 的 prod 強制檢查只保證「有設」，不保證「不是測試值」。
4. **`POST /api/checkout/cvs-map` 已經掛上限流，計畫的交接第 4 條可以劃掉。** `routes/rate_limit.rs::anonymous_write`，每 IP 突發 10 次、之後每 12 秒補 1（我實測第 11 次 429）。但要注意：`SmartIpKeyExtractor` 取的是 `X-Forwarded-For`／`X-Real-IP`，**Caddy 反向代理後面必須確定這兩個 header 是 Caddy 寫的、不是客戶端可偽造的**，否則限流形同虛設（`POST /api/orders` 也一樣）。
5. **`cvs_map_requests` 只有每日 purge 會清。** `scheduled.rs:142` 的 `purge_expired` 一天跑一次，所以最壞情況下這張表會累積「一天份的點擊次數」列。限流之後上界大約是 每IP 5次/分 × IP 數，正常流量下微不足道，但若計畫 5 要加監控，這張表的列數是一個便宜的「有人在刷」指標。
6. **計畫 3 交接第 4 條的後半段已經在計畫 3 自己的修正波做掉了。** `routes/orders.rs:66` 的 `checkout_form_for_payment(state, detail, payment, guest_token)` 已存在，重新付款的競賽（ledger L97）不需要計畫 5 再處理。前半段（`payments::list_for_order`）由本計畫完成。
7. **`auto_complete_shipped` 現在真的會動了。** `shipped_at` 從本計畫開始有值（`ship_order_in_tx`），所以 14 天自動完成的排程第一次會有對象。`scheduled.rs:111-122` 已排除 `shipments.status = 'returned'`，但**不排除 `arrived`**：超商到店 14 天沒人取、綠界又還沒發退回碼的訂單會被自動標 `completed`。實務上綠界 7 天就會發退回碼（C2C 訂單有效日 6～7 天），所以順序上安全，但上線後值得看一次真實時序。
8. **儀表板的 `pending_shipment` 把宅配與超商混在一起數。** 規格 §6.1 只說「待出貨」，實作是 `orders.status = 'paid'`，正確；但老闆若同時有兩種出貨方式，清單上要點進去才知道該按哪個按鈕。若計畫 5 要調整儀表板，這是最便宜的一個改進（列表已經顯示「超商取貨／宅配」）。
9. **開發資料庫留下了本次探測的訂單狀態**（見下方「活體探測」的副作用揭露）。計畫 5 若要做匯入或部署演練，請注意 `DS260908UHSU`／`DS2609083TN4` 的 `ecpay_logistics_id` 是 `PROBE…` 假值，不能拿去對綠界後台。

> **控制者註（審查後）：** 第 2 條已在 codex 第二意見修正 `082103c` 實作（`apply_status` 回填單號、`ship_cvs` 有單號即補收尾），見附錄 C 第 1 項，計畫 5 不必再做；第 1 條仍然有效，而且 `082103c` 新增一條必須保留的 log —— `record_create_ok` 寫不到（嘗試已被重新認領）時的 `tracing::error!`，裡面有孤兒的 `logistics_id`。其餘各條不受影響。

---

### 驗收清單對照（計畫 5861–5869 行，items 1–7）

| # | 項目 | 狀態 |
|---|---|---|
| 1 | db healthy；`cargo run` 跑完 `0004_cvs_map_requests.sql`，log 沒有 rustls panic；`ECPAY_LOGISTICS_*` 空白也能起來 | **自動驗證**：`docker ps` 顯示 `dog_shop-db-1 Up 2 days (healthy)`；我自己起的 `cargo run` log 依序是 migration notice → `SMTP 未設定…`（WARN）→ `jobs worker 與排程工作已啟動` → `api listening on http://0.0.0.0:8080`，**沒有 rustls panic**；根目錄 `.env` 的三個 `ECPAY_LOGISTICS_*` 是空的，`cvs-map` 仍回 `MerchantID=2000933`（stage 退回生效） |
| 2 | `cargo test` 全綠（單元 + 整合），含清單點名的 12 項；`fmt`／`clippy` 乾淨 | **自動驗證**：231 passed／0 failed、27 個 `test result` 行、**24** 個整合測試檔（清單寫 23，實際 24，差額是 `create_admin.rs`）；`fmt --check` 本次重跑 exit 0、clippy 零警告。點名的 12 項我逐條找到對應測試：MD5 文件向量（`mac.rs:202`）、建單欄位與三種回應（`logistics.rs` 的 `create_fields_match_spec_and_mac_verifies`／`parse_create_response_…`）、貨態對照與不倒退（`status_code_table`／`forward_moves_…`／`returned_and_arrived_…`）、map-reply 單次使用與過期（`map_reply_stores_selection_redirects_and_is_single_use`／`…rejects_unknown_expired_mismatched_or_incomplete`）、取件完成 → 訂單完成（`status_callback_moves_forward_completes_on_pickup_…`）、退回標記（`status_callback_returned_keeps_order_shipped_…`）、認領與冪等收尾（`ship_cvs_finishes_interrupted_transition_without_calling_ecpay_again`）、宅配出貨信（`ship_home_sets_carrier_and_tracking_and_mails`）、取消／退款的庫存與付款嘗試（`admin_cancel_only_pending_…`／`mark_refunded_paid_order_…`／`mark_refunded_shipped_order_keeps_stock`）、重開發票（`retry_invoice_resets_failed_invoice_and_enqueues_job`）、儀表板計數（`dashboard_counts_today_and_lists_attention_items`）、admin 權限（`admin_order_routes_require_admin`／`mutating_admin_order_routes_require_admin`，**覆蓋不完整，見修正波第 4 項**） |
| 3 | `pnpm -C web test`（35 個）、`check`（0/0）、`build` | **自動驗證**（本次重跑）：**36 passed**（7 檔；比清單多 1，是 `storeErrorMessage` 那組）、**312 files 0 ERRORS 0 WARNINGS**、build 成功 |
| 4 | `test:e2e` → 2 passed，超商路徑攔到 `/Express/map` 的表單（`MerchantID=2000933`、`ExtraData` 20 碼、無 `CheckMacValue`）、模擬門市回傳 303、草稿還原、送出訂單、訂單頁顯示門市 | **自動驗證**：Task 10 的 `task10-e2e.log` **2 passed（9.3 s）**；測試本身斷言了清單點名的每一項，我逐行讀過 `web/e2e/checkout.spec.ts:99-174` |
| 5 | 後台：`/admin` 五方塊四清單；`/admin/orders` 篩選與標紅；`/admin/orders/<id>` 動作依狀態出現、兩段確認 | **大部分自動驗證**（本次 SSR 探測，見 P29–P35）：`/admin` 五個方塊（今日訂單／待出貨／發票開立失敗／需退款／超商退回）與四個清單都在，需退款清單有 `DS260908A658`；`/admin/orders` 的 `?q=`／`?status=`／`?flag=` 三種篩選都正確，需退款那列有 `bg-red-50`；`/admin/orders/<id>` 三種狀態各驗一次：已完成的沒有「標記完成」有「列印託運單」、已付款宅配的有「宅配出貨」「標記已退款」沒有「取消訂單」「建立物流單」、已出貨超商的有「列印託運單」「標記完成」。**「兩段確認」的點擊行為要人工看一次**（SSR 只能驗第一段按鈕存在） |
| 6 | 依 `docs/dev/ecpay-stage.md` 用 cloudflared 走綠界 stage：地圖固定門市、建單成功（含三個待確認項）、列印新分頁、模擬狀態通知、宅配出貨、取消／退款／重開發票 | **需手動，且是本計畫唯一的上線閘門**。真的打 `/Express/Create` 與 `/Express/Print*` 的那一段在整支分支上**零自動覆蓋**（`LogisticsGateway::Fake` 取代了網路層），三個待確認項（回應能否通過嚴格 MD5、三家超商是否接受 `LogisticsC2CReplyURL`／`ReceiverEmail`、全家萊爾富的 `CVSPaymentNo` 形狀）只有實機能答。我在本次審查中**刻意沒有**觸發任何一次真的建單（見「活體探測」的邊界說明） |
| 7 | `git log --oneline` 看到 10 個 commit 都在 `worktree-mvp-design`；`git status` 乾淨；沒有 push | **自動驗證**：`git rev-list --count 53cef86..3ab13c7` = **13**（10 個任務 commit + 3 個任務內修正：`ef5f5ed` cvs-map 限流、`1ec9e8e` retry-invoice 守衛與權限測試、`b188989` 前端重開發票按鈕），全部在 `worktree-mvp-design`；`git status --short` 空；`git branch -r` 空 |

未達成：第 6 項（綠界 stage 手動走查）。第 5 項的「兩段確認點擊」與第 6 項是**上線前的人工閘門**，不是本計畫的程式缺陷。

---

### 活體探測

起了 API（:8080，debug build、根目錄 `.env` 的 `ECPAY_ENV=stage`、`ECPAY_LOGISTICS_*` 空白、`SMTP_HOST` 空）與 Vite（:5173）。**硬邊界：全程沒有任何一個請求送到 `*.ecpay.com.tw`。** 探測結束後我 `grep` 整份 api log，`logistics-stage`／`einvoice-stage`／`payment-stage` 各 0 筆、`綠界物流單已建立`／`綠界建立物流單失敗` 各 0 筆、`issue_invoice` job 0 筆。做法：從不對「已付款的超商訂單」按 `ship-cvs`（只走兩條在打綠界**之前**就返回的分支）、不模擬付款回呼（改用 SQL 標記狀態）、不把 `print-label`／`cvs-map` 的表單往下送（只檢查欄位並自己重算 MAC）、不碰 `retry-invoice` 的成功路徑（只驗它的守衛）。

自己用 python 寫了一份獨立的 .NET-風格 URL encoder + MD5 實作，先用綠界文件向量自我檢查通過（`692FD6E2CDB539CCDB7206C76DC239AD`）再拿來簽所有回呼。

| # | 送什麼 | 觀察 |
|---|---|---|
| P0 | 自寫實作算綠界文件向量 | `692FD6E2CDB539CCDB7206C76DC239AD`，**與 `mac.rs:206` 逐字元相同** |
| P1 | `POST /api/checkout/cvs-map`（`UNIMARTC2C`、`device=1`） | 200；`action=https://logistics-stage.ecpay.com.tw/Express/map`；8 個欄位 `MerchantID=2000933`／`MerchantTradeNo`＝`ExtraData`＝20 碼英數／`LogisticsType=CVS`／`LogisticsSubType`／`IsCollection=N`／`ServerReplyURL=…/api/ecpay/logistics/map-reply`／`Device=1`；**沒有 `CheckMacValue`** |
| P2 | 同上但不帶 `X-Requested-With` | **403** `FORBIDDEN 缺少 X-Requested-With header`（cvs-map 不在 `/api/ecpay/` 豁免內，正確） |
| P3 | 連打 12 次 cvs-map | `[200×9, 429, 429, 429]`（P1 已用掉 1 個，突發 10 正確） |
| P4 | `map-reply` 帶不存在的 token | **303 → `/checkout?store_error=expired`** |
| P5 | `map-reply` 帶 P1 的有效 token，連送兩次 | 1st **303 → `/checkout?store=VLTkKu4VnS9qmVqhG7ep`**；2nd **303 → `store_error=expired`**（單次使用）；`GET /api/checkout/cvs-store/{token}` 回門市 `991182`／`測試門市`／地址，`store_phone` 空（7-11 不回電話） |
| P6b | `map-reply` 送 150 個欄位 | **303 → `store_error=invalid`**（100 欄位上限生效） |
| P26 | 三條物流回呼各送 70 KB body | 三條都 **413**（`DefaultBodyLimit::max(64*1024)` 生效） |
| P27 | 登記 `HILIFEC2C` 的 token，用 `FAMIC2C` 回傳 | **303 → `store_error=invalid`**；token 列已被消耗（0），`cvs_store_selections` 沒有寫入 |
| P7 | 未登入打 `/api/admin/dashboard`、`/api/admin/orders` | 兩條都 **401 `UNAUTHORIZED`** |
| P9 | 狀態回呼竄改 `RtnCode`（簽章不符） | **400 `0\|CheckMacValue Error`** |
| P10 | 狀態回呼 `MerchantTradeNo` 與 `AllPayLogisticsID` 都不存在 | **200 `0\|Unknown MerchantTradeNo`**（綠界重送也不會變好） |
| P11 | 對一筆 SQL 標成 `shipped`＋shipment `created` 的超商訂單依序送 `2030`／`2073`／`2067` | 三次都 `1\|OK`；`in_transit` → `arrived` → **`picked_up` 且訂單 `completed`、`completed_at` 寫入** |
| P12 | 重送 `2067`；再送晚到的 `2030` | 兩次都 `1\|OK`；狀態維持 `picked_up`／`completed`（終態不再改、不倒退）。**但 `last_status_code` 被改成 `2030`**（見 Minor 4） |
| P13 | 另一筆送 `2074`，再送 `2098` | `returned`（訂單維持 `shipped`）→ `arrived`（退回後重新配達），與規格 §4／不同之處 42 一致 |
| P14 | 用**錯的** `MerchantTradeNo` + **對的** `AllPayLogisticsID` 送未對照代碼 `2101` | `1\|OK`；用 `AllPayLogisticsID` 找到了單，狀態不變（`arrived`），只記 `last_status_code=2101` |
| P15 | 更新門市通知（`StoreType=01`／`Status=01`／`StoreID=991182`） | `1\|OK`；`last_status_msg` = `取件門市異動：門市關轉店（991182）`、`raw.store_updates` 是含整包 payload 的陣列、`raw` 同時保有 `last_notification`（key 不互相覆蓋）。竄改簽章 → **400 `0\|CheckMacValue Error`** |
| P16 | `ship-cvs` 打在「已付款的宅配訂單」與「已出貨的超商訂單」 | 兩條都 **400**，欄位分別是 `shipping_method`／`status`，**在打綠界之前就返回** |
| P17 | `ship-home` 打在 SQL 標 `paid` 的宅配訂單 | 200；訂單 `shipped`、shipment `shipped`、`carrier=黑貓宅急便`／`tracking_no=PROBE-1234567890`；排了 `email:order_shipped:{order_id}` job（Mailer 是 Log，沒有對外連線）。空的 `carrier`／`tracking_no` → 400 兩個欄位錯誤 |
| P18 | `print-label`（**不往下送綠界**） | 200；`action=…/Express/PrintFAMIC2COrderInfo`、欄位 `MerchantID`／`AllPayLogisticsID`／`CVSPaymentNo`／`CheckMacValue`（非 7-11 不帶 `CVSValidationNo`）；**我用自己的實作重算 MAC，與伺服器的完全相同** |
| P19 | `GET /api/admin/orders/{id}` | 30 個頂層 key，**沒有 `guest_token`**；`shipment` 裡**沒有 `raw`**；`payments` 是陣列 |
| P20 | 列表篩選 | `?status=shipped` 2 筆、`?flag=needs_refund` 1 筆（`DS260908A658`）、`?q=UHSU` 1 筆、`?flag=cvs_returned` 0 筆；`?status=bogus`／`?flag=bogus` → **400**；`?page=99` → 200 但 `total=0`（Minor 7） |
| P21 | `GET /api/admin/dashboard` | `today_orders=14`、`today_paid_total=1520`、`pending_shipment=1`、`invoice_failed=0`、`needs_refund=1`、`cvs_returned=0`；四個清單的內容與計數一致 |
| P22 | 各動作的守衛 | `retry-invoice`（發票是 pending）→ 400 `invoice`；`cancel`（已付款）→ 400 `status` 且文案指向「標記已退款」；`clear-refund`（沒有旗標）→ 400 `needs_refund`；`complete`（已付款未出貨）→ 400 `status` |
| **P23** | **認領守衛**：SQL 把 shipment 設成 `pending`／`last_status_code='creating'`／`updated_at=now()`（同一次腳本、相差 32 ms），然後按 `ship-cvs` | **400「物流單建立中或已建立，請重新整理」**；`ecpay_merchant_trade_no` 仍 NULL、`raw` 仍**沒有** `create_request` → 認領那句 UPDATE 影響 0 列，**沒有任何對外請求** |
| **P24** | **冪等收尾**：SQL 把 shipment 設成 `pending` 但 `ecpay_logistics_id='PROBE…'`，按 `ship-cvs` | **200**；訂單 → `shipped`、shipment → `created`、`shipped_at` 寫入、排了 `email:order_shipped` job；**沒有任何對外請求**。再按一次 → 400「只有已付款的訂單能出貨」 |
| P25 | `docs/dev/ecpay-stage.md` 的 python 片段 vs 我的 .NET-精確實作 | 一般欄位**相同**；值含 `~` 時**不同**（Minor 3） |
| P28 | 整份 api log 的秘密掃描 | 四組 HashKey／HashIV 各 **0** 筆、字串 `HashKey=`（`mac::raw_string` 的形狀）**0** 筆、全部 `guest_token` **0** 筆、全部門市 token **0** 筆、`jobs.last_error` 非 NULL **0** 筆 |
| P29–P35 | SSR：帶 admin session 取 `/admin`、`/admin/orders`（含三種篩選）、三筆不同狀態的 `/admin/orders/{id}`、不存在的 id、`/checkout?store_error=expired` | 全部 200（不存在的 id **404**）；`<title>` 分別是「儀表板」「訂單管理」「訂單 DS…」；五方塊四清單齊全；標紅 `bg-red-50` 出現在需退款那列；動作按鈕依狀態出現／消失（詳見驗收清單第 5 項）；admin 明細 HTML 裡搜不到 `guest_token` |

**副作用揭露**（全部在開發資料庫 `localhost:5435`，都是 e2e 早先留下的探測訂單）：

- `DS260908UHSU`：`pending_payment` → **`completed`**（SQL 標 paid+shipped 後由 `2067` 回呼完成）；shipment `picked_up`、`ecpay_merchant_trade_no=DS260908UHSUL01`、`ecpay_logistics_id=PROBEDS260908UHSU`（**假值，不能拿去對綠界後台**）、`last_status_code=2030`（P12 晚到通知留下的）。
- `DS2609083TN4`：`pending_payment` → **`shipped`**；shipment `created`、`ecpay_logistics_id=PROBEDS2609083TN4`、`last_status_code=300`、`last_status_msg` 被我在 P23 的 SQL 清成 NULL；`raw` 有 `last_notification` 與 `store_updates`。
- `DS2609087Z2Y`：`pending_payment` → **`shipped`**（宅配），`carrier=黑貓宅急便`、`tracking_no=PROBE-1234567890`。
- `DS260908LQQ9`：`pending_payment` → **`paid`**（沒有再往下動）。
- **庫存沒有任何變動**（沒有取消或退款任何一筆已付款訂單；P22 的 `cancel` 被守衛擋下）。`jobs` 表多了 2 筆 `send_email`（都是 `order_shipped`，Mailer 是 Log，已 `done`）。
- `cvs_map_requests` 剩 10 列（我的限流測試留下的、1 小時後由每日 purge 清）；`cvs_store_selections` 2 列。
- `settings.sender` 在 P23／P24 期間被暫時設成「狗狗商店／0987654321」，腳本結束時已**還原成空字串**（已驗證）。
- 工作樹、index、HEAD **未動**（`git status --short` 空、HEAD 仍 `3ab13c7`、`git branch -r` 空）。探測腳本與 log 在 `/Users/wilson08/.claude/jobs/82e5d2c3/tmp/`，不在 repo 內。
- 兩台伺服器已 `kill`：`lsof -ti :8080`、`lsof -ti :5173` 皆為空。

---

### 評估

**Ready to merge? With fixes。**

理由：這一波第一次讓後台可以「花錢」（每按一次建物流單就是一次真的運費），而擋重複的三道閘門我不只讀了，還在真的在跑的 api 上把兩條關鍵路徑各走了一次 —— 認領被佔用時 400 且完全沒寫 `create_request`（P23），已有綠界單號時只補收尾而不再打綠界（P24），兩次都零對外請求。MD5 我用自己寫的 .NET-風格編碼器對過綠界文件向量，也對過伺服器實際產生的列印表單，逐字元相同。鎖序 `payments → orders → shipments / product_variants` 在建單收尾、宅配出貨、狀態回呼、後台取消、標記退款五條寫入路徑上一致，`apply_status` 刻意不對第一次 shipments 查詢加鎖這一點，說明作者真的推過交錯而不是照抄。回呼的狀態矩陣（壞簽章 400、未知單號 200 `0|Unknown`、重複 `1|OK` 不重做、退回不倒退、取件才 completed）我逐條打過真的 api，與整合測試互相印證。秘密處理維持計畫 3 的水準：四組 HashKey／HashIV、所有 `guest_token`、所有門市 token 在整份執行期 log 裡都是 0 筆。

要修的兩件 Important 都不會弄丟已經發生的錢，但都會多花一次運費，而且都在「重複建單」這一族：一是**建單逾時後的重試會把第一張單的證據抹掉**（log 沒有 `MerchantTradeNo`、下一次認領覆蓋 `raw.create_request` 與 `ecpay_merchant_trade_no`），而回應文案正是「請稍後再試」—— 這條只要綠界回應慢一次就會發生，不需要資料庫故障；二是**狀態回呼把認領用的 `'creating'` 標記蓋掉**，讓「未對照的代碼剛好在往返途中抵達 + 同時第二次點擊」重新打開認領。前者的修法是加 log、換一句文案、把歷次請求存進 `raw.create_requests`（都是加東西，不改控制流），後者是 SQL 裡一個 `CASE`。其餘九項是很便宜的補洞：擋重複的那道鎖本身沒有回歸測試、規格 §4 明列的「取貨完成只在已出貨時才轉完成」沒有測試、admin 401/403 矩陣少了 5 條路由、cvs-map 的 CSRF 只有我手動驗過、map-reply 的 413 會把裸錯誤丟給買家。十一項合計 70 行以內。

真正的上線閘門仍然不在程式碼裡：驗收清單第 6 項（綠界 stage 走查）沒有人走過，而**真的打 `/Express/Create` 與 `/Express/Print*` 的那一段在整支分支上零自動覆蓋** —— 綠界對建單欄位的驗證規則、回應是否 percent-encoded、三家超商是否都接受 `LogisticsC2CReplyURL`，只有實機能回答。這件事屬於人工驗收，不擋本計畫合併，但計畫 5 上線前必須完成，而且要在老闆實際會用的瀏覽器上確認「列印託運單」的新分頁不被擋。

---

## 附錄 A：控制者裁決（SDD ledger 全部 `Ruling` 行，共 18 條，依時間順序；前 3 條是開工前預檢衝突掃描的裁決，最後 4 條是 codex 第二意見階段的裁決）

- 預檢掃描 — Ruling: 測試檔各自帶 `place_order`／`order_body`／`ecpay_post_raw` 等 helper（與 tests/orders.rs、tests/ecpay_payment.rs 重複）— 這是本 repo 既有慣例（每個整合測試檔自給自足，`common/mod.rs` 只放跨檔共用的）；不視為「邏輯區塊逐字重複」— 代價：若審查者堅持，最終審查可要求抽進 `common`，只是搬檔。
- 預檢掃描 — Ruling: `AdminOrderRow` 與 `orders::OrderRow` 欄位幾乎相同（與規格不同之處 48）— 後台需要 `needs_refund`、`user_id` 序列化且不能有 `guest_token`，用獨立投影比在 `OrderRow` 上加條件序列化清楚 — 代價：兩個 struct 要一起維護（新增訂單欄位時多改一處）。
- 預檢掃描 — Ruling: 兩個「先檢查再動作」的路由（`ship_cvs`、`ship_home`）在交易外用 `orders::get_detail` 讀狀態，交易內再用 `FOR UPDATE` 重查 — 讀兩次是刻意的（交易外的檢查給明確的欄位錯誤，交易內的檢查防競賽）— 代價：多一次查詢。
- Task 3 — Ruling: cvs-map 掛與 /api/orders 相同的每 IP governor（突發 10、每 12 秒補 1），把 orders.rs 的 governor 建構抽成共用 helper 給兩條路由用、orders.rs 註解改寫並加 11 次→429 的整合測試 — 規格 §11 只要求 /api/auth 限流，但 repo 已表態匿名寫入要有減速帶，且新端點每次呼叫寫一筆 DB — 代價：買家一分鐘內連按超過 10 次「選擇門市」會被擋（同 /api/orders 既有行為）。Minor 2（補回文件註解）一併修；其餘 Minor parked。
- Task 7 — Ruling: 接受 ecpay_payment.rs 那一行期望值改動（pending→expired） — brief 的 cancel_with_payments_in_tx 與 commit 標題「付款嘗試一併作廢」明文要求取消時作廢待付款嘗試，舊斷言編碼的是 Task 7 之前的行為；reviewer 要確認該檔只改期望值且遲到付款 → needs_refund 路徑仍成立 — 若錯，代價是遲到付款的對帳行為變了而沒人發現
- Task 7 — Ruling: retry-invoice 加訂單狀態守衛（只允許 paid／shipped／completed，否則 400 VALIDATION fields.status；發票仍留 failed）— 發票 job 對非可開票訂單靜默略過，簡報的 retry-invoice 沒守衛會讓退款後重開回 200 但發票永遠卡 pending 且無法再試，違反規格「狀態不允許回 400」；簡報沒預見退款後重開 — 若錯，代價是某些合法狀態被擋（可再放寬）
- Task 7 — Ruling: Minor 4／6／7 併入 fix round 1（純測試、同檔、直接加固被修的程式碼）；Minor 2／3／5 park 到最終修正波 — 若錯，代價是 fix diff 稍大
- Task 9（派工前）— Ruling: Step 6「手動走一次」改為 curl 打 SSR 頁的煙霧測試（登入拿 cookie → GET /admin、/admin/orders、/admin/orders/<id> 都 200 且含預期文字），真人瀏覽器走查留在 Task 10 驗收清單給 Wilson — 自主執行沒有真人可開瀏覽器 — 若錯：互動／視覺 bug 要等 Wilson 走查才會發現
- Task 9（Task 4 Minor 5 承接）— Ruling: attentionNotes 依簡報用 last_status_code 判斷建單失敗即可，因 record_create_ok（shipments.rs:265-266）會覆寫 code，store-update 訊息只會出現在建單成功之後；「最近通知」那行可能顯示舊 code + 門市更新訊息，屬外觀問題，parked 給最終審查 — 若錯：老闆看到過期的狀態碼，不影響操作
- Task 9（Important 1）— Ruling: 簡報原文落後於 Task 7 fix（後端守衛是 fix round 才加的），以後端為準：availableActions 的 retry_invoice 加上 status ∈ {paid, shipped, completed}，補 refunded+failed → 無 retry_invoice 的 vitest 案例；attentionNotes 的「發票開立失敗」提示維持顯示 — 前端按鈕不該指向必敗的請求 — 若錯：只是少顯示一顆按鈕，老闆仍可從提示得知發票失敗
- Task 10 — Ruling: 接受實作者把簡報的「見第 5 點」改為「見第 6 點」— 簡報自相矛盾（第 5 點是列印、第 6 點才是狀態通知），改成正確參照 — 若錯：一行文字
- 最終審查修正波 — Ruling: 修正波 = 審查者的 11 項逐字（I1、I2/#28、#27、#17、#32+#40、#14、#13、#1、doc ~ 編碼、#31、#38），一次派工、opus（第 1–3 項動到認領／鎖邏輯）— 全部 1–20 行、每項附回歸測試，不需要拆 — 若錯：多一輪範圍複審
- 最終審查修正波 — Ruling: 修正波不加項（Minor 4–10 維持審查者的不修裁決）— 守住「一次派工」— 若錯：外觀問題帶進計畫 5
- 最終審查修正波 — Ruling: 修正波不重跑 web check/test/build 與 Playwright e2e — 本波不動 web/、不動結帳正常路徑（map-reply 只改超大 body 的例外分支）— 若錯：計畫 5 第一次跑 e2e 時才發現
- codex 第二意見修正 — Ruling: codex 6 條全部判真、一次派工（opus，F3/F4 動到認領／收尾邏輯）修並附回歸測試；F3 推翻最終審查「交計畫 5 第 2 條（可選）」的裁決 — 通知不只是幫不上忙，而是主動把 paid 訂單封死在 created/無單號，得靠 SQL 才能救 — 若錯：認領路徑多兩條分支要維護
- codex 第二意見修正 — Ruling: F5 的修法是「不符合保守 email 字元集就不送 ReceiverEmail」而非拒單 — 欄位選填、綠界確切驗證規則未證實 — 若錯：收件人少收綠界通知信
- codex 第二意見修正 — Ruling: 接受 codex 修正實作者揭露的行為改變 — 認領中（creating）若先收到帶 AllPayLogisticsID 的通知，第二次按「建立物流單」改為直接補收尾（200、不打綠界）而非 400；I2 測試改用不帶單號的通知、四個原斷言逐字保留 — 綠界已確認單存在，補收尾是正確結果且仍只出一次貨；殘留只有已出貨列上可能留著 create_error／creating 標記（外觀） — 若錯：老闆看到一列狀態標記不一致
- codex 第二意見修正 — Ruling: codex 修正的範圍複審用 opus 而非原定 sonnet — diff 動到 apply_status／record_create_*／ship_cvs 的並發與認領歸屬邏輯（金流關鍵），並要求複審者逐一推演四種交錯 — 若錯：多花一次 opus 審查的成本

---

## 附錄 B：修正波結果與範圍複審

修正波：一次派工（opus，因第 1–3 項動到認領／鎖邏輯），基底 `3ab13c7`，1 個 commit `003c6e9`（8 個檔案、+398/−38），全在本機分支、沒有 push。

| # | 對應 | 內容 | 位置／測試 |
|---|---|---|---|
| 1 | Important I1 | 兩個建單失敗分支的 log 補 `merchant_trade_no`；連線失敗的回應文案改成「請先到綠界廠商後台確認這筆是否已建單」；`claim_create` 的 `raw` 同時寫 `create_request`（最後一次）與 `create_requests`（累積歷次） | `routes/admin_orders.rs`、`domain/shipments.rs::claim_create`；`ship_cvs_rejection_keeps_order_paid_records_reason_and_retries_with_next_no` 加斷言（陣列長度 4、L01～L04） |
| 2 | Important I2／擱置 28 | `apply_status` 的 `last_status_code` 改 `CASE WHEN last_status_code = 'creating' THEN last_status_code ELSE $3 END`，通知不再解除認領 | `domain/shipments.rs::apply_status`；`status_notification_does_not_release_the_create_claim` |
| 3 | 擱置 27 | 認領守衛的直接測試（`creating` 擋、退 3 分鐘後可重認領） | `ship_cvs_rejects_while_another_create_is_in_flight` |
| 4 | 擱置 17 | 訂單不是 `shipped` 時 2067 不轉 `completed` | `pickup_does_not_complete_an_order_that_is_not_shipped` |
| 5 | 擱置 32＋40 | 八條後台動作路由與 `GET /api/admin/dashboard` 的 401／403 矩陣 | `mutating_admin_order_routes_require_admin`、`admin_order_routes_require_admin` |
| 6 | 擱置 14 | cvs-map 缺 `X-Requested-With` → 403、不寫列 | `cvs_map_requires_the_csrf_marker` |
| 7 | Minor 1／擱置 13 | map-reply 的 body extractor 拒絕（>64 KB、非 UTF-8）也 303 `store_error=invalid` | `routes/ecpay_logistics.rs::map_reply`（`Result<String, StringRejection>`）；`map_reply_ignores_csrf_headers_but_limits_body` |
| 8 | Minor 2／擱置 1 | `CreateError::Malformed` 的 `other` 截 200 字 | `ecpay/logistics.rs`；`parse_create_response_success_rejected_bad_mac_malformed` |
| 9 | Minor 3 | python 片段補 `~ → %7e`，並註明權威在 `ecpay/mac.rs` | `docs/dev/ecpay-stage.md` 第 6 點 |
| 10 | 擱置 31 | `next_merchant_trade_no` 加 ≤ 20 字守衛 | `merchant_trade_no_increments_per_attempt` |
| 11 | 擱置 38 | `apply_info` 訂單狀態守衛的覆蓋 | `payment_info_after_another_attempt_paid_is_ignored` |

閘門（實作者在 `003c6e9`）：`cargo fmt --all --check` exit 0；`cargo clippy --all-targets -- -D warnings` 零輸出；完整 `cargo test` 27 個 `test result` 行、**236 passed／0 failed**（比 `3ab13c7` 多 5 條測試）；python 對綠界文件向量自檢得 `692FD6E2CDB539CCDB7206C76DC239AD`，相符。控制者裁定本波不重跑 web check／test／build 與 Playwright（不動 `web/`、不動結帳正常路徑）。

範圍複審（sonnet，`review-3ab13c7..003c6e9.diff`）：**11 項全部 ADDRESSED，修正未引入新問題，無範圍外觀察。** 複審者獨立重跑 `--test ecpay_logistics`（17）、`--test admin_orders`（23）、`--test ecpay_payment payment_info_after_another_attempt`（1）、`--lib`（85）全綠；並實際以 70 KB body 打 map-reply，確認落入 `Err` 分支回 303 而非裸 413；核對 `record_create_ok`／`record_create_failure` 寫入的 `raw` key 與 `create_requests` 無衝突、全庫沒有其他讀者依賴舊的 `$4` 包裝。實作者揭露的三處偏離（第 11 項多一句 `payment_row(...) == "pending"` 斷言，釘住的是訂單狀態守衛而非付款狀態守衛；第 1／2／10 項各一行引用審查編號的說明註解；第 4 項自訂測試名）複審判為可接受。

---

## 附錄 C：codex 第二意見審查與修正（`/codex-review-fix`）

依使用者指示，計畫收尾時用 `codex exec -s read-only`（codex CLI 0.153.4、model `gpt-6-astra`、reasoning `xhigh`、sandbox read-only）對 `ccb4c88..003c6e9`（計畫 4 全部 commit，含修正波）做一次只讀的第二意見審查；codex 自己跑了 18 個 MAC／物流／shipment 單元測試與 `git diff --check`，沒有改任何檔案。codex 回報 **6 條可行動的問題（2×P1、4×P2、0×P3）**，並確認沒有新的秘密進 log、鎖序沒有反轉。控制者逐條打開引用的 file:line 對照程式碼與規格後，**6 條全部判定為真**，一次派工（opus，C1–C4 動到認領／收尾／通知邏輯）修正並附回歸測試，commit `082103c`：

| # | 優先 | codex 的發現 | 控制者判定 | 修正（`082103c`） |
|---|---|---|---|---|
| 1 | P1 | 狀態通知會把 paid 訂單封死在補救路徑之外：綠界建單後回應遺失或收尾失敗，訂單留 `paid`；接著任何通知把 shipment 從 `pending` 變 `created`，重按建單時無單號者被「物流單已建立」擋住、有單號者被「只補收尾需 pending」擋住（`admin_orders.rs:147`） | 真。最終審查把「`apply_status` 回填單號」列為交計畫 5 的可選項（第 2 條），但 codex 指出的是通知**主動封死**補救、不是「幫不上忙」；控制者推翻該裁決改為現在修 | `apply_status` 回填 `ecpay_logistics_id`／`cvs_payment_no`／`cvs_validation_no`（只在 NULL 時）；`ship_cvs` 的只補收尾條件改為「有單號即補收尾」（不看 shipment 狀態）；測試 `status_callback_backfills_logistics_id_and_retry_only_finalizes`、`retry_after_finalize_failure_finalizes_even_if_status_advanced` |
| 2 | P1 | `record_create_ok`／`record_create_failure` 只比對 `order_id`：2 分鐘後被重認領成 L02 時，原 L01 handler 的遲到寫入會清掉 L02 的 `creating` 標記或蓋掉單號（`shipments.rs:258`／`:278`） | 真但機率極低（HTTP 逾時 20 秒，得在拿到回應後停滯 100 秒以上才寫入）；歸屬守衛便宜，值得加 | 兩個寫入的 WHERE 加 `AND ecpay_merchant_trade_no = $n` 並回傳是否寫到；寫不到時記 log（孤兒單號）、不收尾、回固定文案錯誤；測試 `stale_attempt_cannot_overwrite_a_reclaimed_shipment` |
| 3 | P2 | `arrived` 與 `returned` 同 rank、`UpdateStatusDate` 解析了沒用：退回後重播較早的到店通知會把 shipment 變回 `arrived`，儀表板警示消失、還可能被 14 天自動完成（`shipments.rs:131`） | 真（規格 §14 重複回呼 no-op、§4 狀態機） | `apply_status` 比較 `UpdateStatusDate`，比已處理的舊就忽略（新變體 `StatusOutcome::Stale`，仍回 `1|OK`）；測試 `stale_arrival_replay_does_not_undo_a_return` |
| 4 | P2 | 更新門市通知重播會再追加一筆 `raw.store_updates`，並把 `last_status_msg` 退回舊訊息（`shipments.rs:171`） | 真（規格 §14） | UPDATE 加 `NOT (store_updates @> $3)` 守衛、先 SELECT EXISTS 決定回 `1|OK` 或 `0|Unknown`；測試 `store_update_replay_is_a_no_op` |
| 5 | P2 | `post_form` 先 `error_for_status()` 再 `.text()`，4xx／5xx 的回應 body 被丟掉，`raw` 沒有可對帳的回應（`logistics.rs:515`） | 真（規格 §14 要求存回應） | 先讀 status 與 body，非 2xx 以含狀態碼與截 500 字 body 的錯誤回報；連線失敗分支把錯誤文字存進 `raw.create_response_text`；單元測試 `post_form_keeps_the_body_of_an_http_error` 用 127.0.0.1 的假伺服器回 500 驗證 |
| 6 | P2 | 結帳放行 `o'brien@example.com` 這類 RFC 合法但綠界文件不收的 email，`ReceiverEmail` 一律送出 → 已付款訂單每次建單都被拒（`logistics.rs:195`） | 真（綠界確切驗證規則未證實，但欄位選填，過濾是安全方向） | 新增 `is_ecpay_safe_email`（ASCII、≤50、單一 `@`、保守字元集），不符就不送 `ReceiverEmail`；單元測試 `receiver_email_only_sent_when_ecpay_safe` |

修正後閘門（`082103c`）：`cargo fmt --all --check` exit 0；`cargo clippy --all-targets -- -D warnings` 零輸出；完整 `cargo test` 27 個 `test result` 行、**243 passed／0 failed**（比 `003c6e9` 多 7 條測試：C1×2、C2～C6 各 1）。實作者揭露並經控制者裁定接受的行為改變：認領中（`creating`）若先收到帶 `AllPayLogisticsID` 的通知，第二次按「建立物流單」改為直接補收尾（200、不打綠界）而非 400；審查 I2 的測試因此改用不帶單號的通知，四個原斷言逐字保留。

範圍複審（opus，因動到並發與認領歸屬邏輯；`review-003c6e9..082103c.diff`）：**6 條全部 ADDRESSED，修正未引入 Critical／Important 問題。** 複審者逐一推演四種交錯：(a) 認領中收到帶單號的通知＋第二次點擊 → 通知回填單號、第二次點擊直接補收尾，綠界只被打 1 次，第一手晚到的 `record_create_ok` 寫入同一單號、`finalize_cvs` 走冪等分支；(b) 逾時無單號＋通知＋重試 → 回填後補收尾，1 次；(c) 2 分鐘後重認領＋第一手晚到寫入 → 晚到寫入回 false 只記 log、不收尾，L02 的列不被污染（2 次是既有的重認領設計，本次反而縮小了窗口）；(d) 收尾失敗＋通知＋重試 → 0 次新呼叫，狀態不倒退。§11 核對：新 log 只有 `order_id`／`merchant_trade_no`／`logistics_id`／`rtn_code`；存進 `raw.create_response_text` 的錯誤文字只含 reqwest 錯誤鏈與截過的回應正文；鎖序未變，`Stale` 以 rollback 釋放兩把鎖。Minor（皆為簡報明定的取捨）：新鮮度守衛只比對上一則通知、時間相等照舊處理；復原路徑仰賴通知帶 `AllPayLogisticsID`；(a) 情境第一手若最後失敗，`create_error`／`create_failed` 會蓋在已出貨的列上（外觀）。範圍外觀察：`is_ecpay_safe_email` 對點的位置比 RFC 寬（只求不送不安全字元）；`apply_status` 的 `Unknown` 早退靠 drop rollback（既有行為）。複審者以完整套件 log 獨立確認 27 行 `test result` 全 ok、243 passed，九個相關測試逐一 ok。

**對本文件其他章節的影響：** 「交給計畫 5 的事項」第 2 條（`apply_status` 回填單號、放寬只補收尾）已在 `082103c` 實作，計畫 5 不必再做；第 1 條（孤兒物流單對帳程序）仍然有效，而且 `082103c` 多加了一條要保留的 log —— `record_create_ok` 寫不到（嘗試已被重新認領）時的 `tracing::error!`，裡面有孤兒的 `logistics_id`。
