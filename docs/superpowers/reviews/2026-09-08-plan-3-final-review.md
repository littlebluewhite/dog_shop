# 計畫 3 最終審查：綠界金流、背景工作、Email、電子發票

日期：2026-09-08
範圍：`f8bd3b7..80b9b73`（分支 `worktree-mvp-design`，15 個 commit、58 個檔案、+6592/−50）；修正波後至 `c54d23d`（共 19 個 commit、59 個檔案、+6925/−50，見附錄 B）；codex 第二意見審查修正波後至 `ecd2871`（共 28 個 commit、61 個檔案、+7750/−53，見附錄 C）
審查者：Senior Code Reviewer（整支分支、只讀）
規格權威：`docs/superpowers/specs/2026-09-06-dog-shop-mvp-design.md`；計畫：`docs/superpowers/plans/2026-09-08-dog-shop-plan-3-payments-jobs-email.md`；逐任務裁決：見附錄 A

---

### 總結

**Ready to merge? With fixes。** 沒有 Critical。這一波最需要看對的東西都看對了：CheckMacValue 完全照規格 §8.1（我用 `shasum` 獨立重算綠界文件向量，逐字元相同）、AES 向量用 `openssl` 獨立重算也相同；回呼一律「先驗簽章再看任何欄位」；`payments → orders → product_variants` 的鎖定順序在 `apply_return`、`expire_one`、`cancel_in_tx` 三條路徑上一致，我把兩個方向的交錯逐一推過，不會死鎖也不會重複歸還庫存；遲到付款與金額不符都只標 `payment=paid` + `needs_refund`，不動訂單狀態、不排發票、不動庫存，而且有真的打 Postgres + 走完整 HTTP middleware 的測試蓋住。167 個 Rust 測試、27 個 vitest、1 條 Playwright 主流程在本次審查全部重跑通過，`fmt`／`clippy -D warnings`／`svelte-check` 乾淨。

要先修的是 4 件 Important，沒有一件會弄丟錢，但都在上線前該擋：

1. 綠界回呼端點繼承了 router 層為圖片上傳訂的 10 MB body 上限、沒有限流，而且**在簽章驗證之前**就要把整個 body 攤成參數、clone、排序、逐 byte 編碼。實測 12 個併發 10.4 MB 請求就讓 api 的 RSS 從 76 MB 漲到 528 MB（約 body 的 3.6 倍／in-flight），而規格 §17 的目標機器是 2 vCPU／4 GB。修法 6 行。
2. 規格 §4 明列的「回呼到達時訂單已因**另一筆付款嘗試**變成 `paid`」這條分支沒有測試（只測了「已取消」與「金額不符」）。
3. `/api/ecpay/payment/info` 沒有壞簽章的測試，`/return` 有。
4. `docs/dev/ecpay-stage.md` 的手動走查只走個人發票；公司發票（`Print=1` + `CustomerAddr`）與捐贈發票（`Donation=1` + `LoveCode`）在整支分支上**零實機覆蓋**，而這兩條的綠界欄位驗證只有在 stage 才看得到。

四項都是 10～30 行的修正，一波派工可以收尾。

---

### 審查方式

五個 pass，按控制者指示切：

1. **設定與綠界基元**：`config.rs`、`ecpay/{mod,mac,time,aes}.rs`、`.env.example`。重點在憑證讀取的 stage/prod 分岔、Debug 遮蔽、CheckMacValue 與 AES 的演算法是否逐字照 §8.1／§8.4。
2. **領域與路由**：`migrations/0003_invoices.sql`、`domain/{orders,payments,invoices,jobs}.rs`、`routes/{orders,ecpay_payment}.rs`、`error.rs`、`app.rs`、`state.rs`、`main.rs`。重點在交易邊界、鎖定順序、冪等、`needs_refund` 語意、CSRF 豁免範圍、viewer 規則。
3. **背景工作與 Email／發票**：`jobs/{mod,worker,scheduled,handlers}.rs`、`mail/{mod,templates}.rs`、`templates/mail/*`、`ecpay/invoice.rs`。重點在認領／退避／panic 隔離、排程 SQL、秘密是否會流進 log 與 `jobs.last_error`。
4. **前端**：`lib/{ecpay,orderPage,types}.ts`、`checkout/+page.svelte`、`orders/[id]/+page.svelte`、`e2e/checkout.spec.ts`。
5. **測試與文件 + 擱置事項 triage**：21 個整合測試檔（重讀 `ecpay_payment`、`jobs_worker`、`mail_jobs`、`invoice_job`、`repay` 全文，其餘掃過新增段落）、`docs/dev/ecpay-stage.md`、`final-review-inputs.md` 逐行裁決。

跑過的驗證（全部在 HEAD `80b9b73`，工作樹乾淨）：

| 指令 | 結果 |
|---|---|
| `cargo fmt --all --check` + `cargo clippy --all-targets -- -D warnings` + `DATABASE_URL=…5435 cargo test`（一條 `&&` 鏈） | **exit 0；167 passed**（lib 單元 + 21 個整合測試檔 + main.rs 單元 + doc-tests），fmt／clippy 零輸出 |
| `pnpm -C web check` | **0 ERRORS 0 WARNINGS**（300 files） |
| `pnpm -C web test` | **27 passed**（6 個檔） |
| `pnpm -C web build` | 成功（只有既有的 `chunks/env.js` 空 chunk 提示） |
| `printf … \| shasum -a 256`（`mac.rs:112` 的 raw string） | `6c51c9e6…5b840` = `mac.rs:120` 的期望值，**與綠界文件相同** |
| `printf … \| openssl enc -aes-128-cbc -K 656a…7367 -iv 7139…596b -nosalt -base64 -A` | `uvI4yr…UwQ==` = `aes.rs:74` 的期望值，**與 committed 期望值相同** |
| `git rev-list --count f8bd3b7..80b9b73` | 15（12 個任務 + 3 個任務內修正 commit） |
| `git branch -r` | 空（沒有任何遠端分支，沒有 push） |
| `pnpm -C web test:e2e`（起了 API :8080 與 Vite :5173） | **1 passed（3.2s）** |

另外做了一整輪活體探測（自己算 CheckMacValue、對真的在跑的 api 打十種回呼），見下方「活體探測」節。

---

### 優點

- **CheckMacValue 是對的，而且用得對。** `ecpay/mac.rs:30-49` 完全照 §8.1：忽略大小寫排序 → `HashKey=…&k=v&…&HashIV=…` → .NET 風格 URL encode（`A-Za-z0-9-_.!*()` 原樣、空白 `+`、其餘 `%xx` 小寫）→ 全字串轉小寫 → SHA256 → 大寫。`raw_string` 被單獨抽出來對照文件的中間值（`mac.rs:109-114`），這是我看過最能證明「演算法沒寫歪」的測法。我另外用 `shasum -a 256` 對同一組文件參數獨立算過，結果一致。文件向量裡 `TradeDesc=促銷方案` 是**未編碼的中文**，而雜湊仍相符 —— 這正好反證了「送綠界前要先 URL encode TradeDesc」的疑慮不成立。
- **驗證順序不可繞過。** `parse_notification`（`ecpay/aio.rs:166-198`）第一行就是 `mac::verify`，沒過就 `Err(BadMac)`，之後才讀任何欄位；`verify`（`mac.rs:52-65`）用 `eq_ignore_ascii_case` 找 key、過濾掉**所有**同名的 `CheckMacValue`（不只第一個），所以攻擊者塞第二個 `CheckMacValue` 也偷渡不進去；比對值本身也不分大小寫。
- **鎖定順序全域一致，而且有人真的想過交錯。** `apply_return`（`payments.rs:69-103`）`payments FOR UPDATE` → `orders FOR UPDATE`；`expire_one`（`scheduled.rs:63-80`）先 `UPDATE payments … WHERE status='pending'` 再 `cancel_in_tx`（`orders` → `product_variants`）；`cancel_in_tx`（`orders.rs:821-838`）改成逐列 `ORDER BY variant_id` 歸還。我把「付款成功 vs 過期取消」的兩個方向都推過：先付款者贏時，`expire_one` 的 payments UPDATE 因 `status='pending'` 不再成立而影響 0 列、`cancel_in_tx` 因訂單已 `paid` 回 false → 整筆 rollback；先過期者贏時，`apply_return` 讀到 `expired` 的 payment、訂單 `cancelled` → 走遲到付款。兩邊都不會重複歸還庫存，也沒有環。
- **遲到付款照規格 §4 一字不差。** `payments.rs:117-133`：`on_time = 訂單仍 pending_payment && TradeAmt == payments.amount`，不成立就只標 payment `paid`、`orders.needs_refund = true`、記 `warn`，**不排 `issue_invoice`、不排通知信、不動庫存、不改訂單狀態**。`late_payment_after_cancel_sets_needs_refund`（`tests/ecpay_payment.rs:317`）連「庫存維持在取消時歸還後的 5」都斷言了。
- **回呼的 HTTP 狀態矩陣完整而且分得清。** `routes/ecpay_payment.rs:36-47`：壞簽章 400 `0|CheckMacValue Error`、缺欄位 400 `0|Missing Field`、找不到交易號 200 `0|Unknown MerchantTradeNo`（綠界重送也不會變好）、DB 錯 500 `0|Server Error`（讓綠界重送）、其餘 200 `1|OK`。這正是 §14 要的三分法。
- **jobs worker 的機制寫得很紮實。** 認領用單一 `WITH picked AS (… FOR UPDATE SKIP LOCKED LIMIT 10) UPDATE … RETURNING`（`worker.rs:38-54`），認領即 `attempts+1`，多行程也不會搶到同一筆；退避 `2^attempts` 分鐘、上限 `2^10`；`is_last_attempt()` 讓 handler 能做「最後一次」的收尾（發票標 `failed`）；`requeue_stale` 撿回當機殘留；一輪滿 10 筆就不等 2 秒直接再跑。handler 用 `tokio::spawn` 隔離，panic 會被 `JoinError::is_panic()` 轉成一次失敗嘗試而不是讓整個 worker task 悄悄死掉 —— `panicking_handler_is_treated_as_failure_and_does_not_kill_the_batch`（`tests/jobs_worker.rs:178`）是真的讓 handler panic 再斷言同批後面的 job 仍被處理。
- **`expire_one` 抽成單筆函式 + 逐筆容錯。** `expire_unpaid_orders`（`scheduled.rs:32-56`）先撈 id（`LIMIT 500`），每筆自己一個交易、失敗只記 log 繼續 —— 排在最前面那筆一直失敗不會餓死後面所有訂單。`expire_one_rolls_back_payments_update_when_order_not_cancellable`（`tests/jobs_worker.rs:340`）證明了 rollback 分支不會留下 `expired` 的殘跡。
- **`Option<CheckoutForm>` 的取捨是對的。** `routes/orders.rs:104-126`：訂單一旦 commit（庫存已扣、outbox 已寫），組表單失敗只記 `tracing::error` 並回 201 + `ecpay: null`，前端改成導去訂單頁用重新付款接手。用「一定要讓客戶端拿到 order_id」換掉「客戶端重試造成重複扣庫存」是正確的方向，註解也把理由寫在程式碼裡。
- **秘密處理乾淨。** `Config`／`EcpayCredentials`／`SmtpConfig` 都手寫 `Debug` 遮蔽（`config.rs:22-33,49-57,98-108`），`debug_redacts_secrets` 對六個秘密逐一斷言不出現；`PaymentRow` 的 `id`／`merchant_trade_no` 是 `#[serde(skip)]`；`OrderRow` 的 `guest_token`／`needs_refund` 沿用計畫 2 的 skip；`Mailer::Log` 預設只印 `to`／`subject`，內文要另外開 `mail_body=debug`；`jobs.last_error` 只會收到 lettre／reqwest／askama 的錯誤字串，我逐條追過來源，不含 HashKey、SMTP 密碼或重設 token（token 只出現在渲染後的信件內文裡）。
- **測試是真的在測行為。** 21 個整合測試檔全部用 `#[sqlx::test]` 開獨立臨時 DB 跑三支 migration，回呼測試是自己算 CheckMacValue 後用 `Router::oneshot` 打真的 HTTP 路由（`tests/ecpay_payment.rs:64-95`），worker 測試跑真的 `run_once`，發票測試用可替換的 `InvoiceGateway::Fake` 而不是 mock 掉整個 handler。`full_flow_from_return_callback`（`tests/invoice_job.rs:311`）從綠界回呼一路串到發票開立與兩封信，這條端到端最有價值。
- **前端只做該做的。** `postToEcpay`（`lib/ecpay.ts`）是最小的隱藏表單頂層 POST；`orderPage.ts` 把輪詢／繳費資訊／期限判斷抽成純函式並用 vitest 蓋住；e2e 用 `page.route` 攔住 `payment-stage.ecpay.com.tw` 再用 `postData()` 檢查 16 個欄位與 64 碼 `CheckMacValue`，比「看得到某個字」的斷言有意義得多。
- **`.env.example` 與 `docs/dev/ecpay-stage.md` 寫給人看。** cloudflared 的理由、`mail_body=debug` 的警告、模擬付款怎麼按、收工怎麼關 port 都寫了；stage 憑證是規格 §8.5 明列的公開測試資料，放進 git 沒問題。

---

### 問題

#### Critical（必修）

無。

#### Important（應修）

1. **綠界回呼端點沿用了給圖片上傳用的 10 MB body 上限，而且沒有限流：匿名者可以把它當記憶體放大器。**
   `api/src/app.rs:16,66` 把 `DefaultBodyLimit::max(10 MB + 64 KB)` 掛在整個 router 最外層（為了 `POST /api/admin/uploads`），`routes/ecpay_payment.rs:19-23` 的兩條回呼路徑因此也吃 10 MB。這兩條在 `auth/csrf.rs:11` 的 `EXEMPT_PREFIXES` 內（不用 `X-Requested-With`／`Origin`），也沒有掛 `GovernorLayer`（計畫 3 只替 `POST /api/orders` 加了限流）。`payment_return` 先 `parse_form`（`ecpay_payment.rs:26-30`）把整個 body 展開成 `Vec<(String,String)>`，再進 `parse_notification` → `mac::verify`（`mac.rs:59-63` 又 `clone()` 一份）→ `mac::raw_string`（`sort_by_key` 每次比較配置一個新 String、`dotnet_url_encode` 對每個非英數 byte 做一次 `format!` 配置）。**這些全部發生在簽章驗證之前**，因為要重算簽章就得先把所有欄位攤開。
   **我實測過**（debug build，見「活體探測」P11）：單發一個 10.4 MB 的對抗性 body（52 萬個相異 key、值是多位元組中文）→ 400，耗時 **266 ms**；12 個併發同樣的請求 → 全部 400、總共 0.66 s，api 的 RSS 從 **76 MB 衝到 528 MB**（每個 in-flight 請求約佔 body 的 3.6 倍記憶體），期間 `/api/health` 仍在 0.9 ms 內回應。所以這**不是**演算法爆炸，是線性但係數很大的放大器 —— 但規格 §17 的目標機器是 2 vCPU／4 GB，**約 100 個同時在途的請求就能把 api 打到 OOM**，而 api 行程裡還跑著 worker 與排程（過期歸還庫存），一起陪葬。10 MB 這個數字是為了 multipart 圖片上傳訂的，綠界回呼永遠不會超過 2 KB。
   **為什麼重要**：這是營收路徑上唯二對匿名者開放的寫入端點之一，而且是刻意繞過 CSRF 的那一條；計畫 2 的審查已經替 `POST /api/orders` 補了減速帶，這兩條卻沒有任何上限。計畫 4 還會在同一個前綴下再加兩條物流回呼（`map-reply` 連簽章都沒有），現在把界線劃好最便宜。
   **修法**（約 6 行）：在 `routes::ecpay_payment::router()` 的兩條路由之後掛 `.layer(DefaultBodyLimit::max(64 * 1024))`（後掛的 layer 在內層，會覆蓋 `app.rs:66` 的 10 MB），並在 `parse_form` 之後、`parse_notification` 之前加一個參數筆數上限（綠界 `ReturnURL` 最多 30 幾個欄位，取 100 很寬鬆）。計畫 4 的 `/api/ecpay/logistics/*` 要一起掛同一個 layer。

2. **規格 §4 明列的「遲到付款：訂單已因另一筆付款嘗試變成 `paid`」沒有測試。**
   `api/tests/ecpay_payment.rs:317-365` 只有 `late_payment_after_cancel_sets_needs_refund`（訂單 `cancelled`）與 `amount_mismatch_is_treated_as_late`（訂單仍 `pending_payment`）。`payments.rs:117` 的 `on_time` 判斷式把三件事綁在一起（訂單狀態 + 金額），而「另一筆先付掉」是規格原文明列、實務上最常見的遲到情境（買家在兩個分頁各付一次）。現在這條分支只靠讀碼保證。
   **為什麼重要**：這條分支決定「第二筆錢會不會被誤認成正常付款而覆寫 `paid_at`、重排一次發票 job」。程式讀起來是對的，但沒有東西擋住未來有人把 `on_time` 拆開時弄壞它。
   **修法**：`tests/ecpay_payment.rs` 新增 `late_payment_after_another_attempt_paid`（約 25 行）。

3. **`/api/ecpay/payment/info` 沒有壞簽章的測試。**
   `bad_mac_is_400_and_changes_nothing`（`tests/ecpay_payment.rs:211`）只打 `/return`。兩條路徑各自呼叫 `aio::parse_notification`，共用同一段驗證邏輯，但 `payment_info`（`routes/ecpay_payment.rs:70-87`）是獨立的 handler；今天正確，明天有人在 `/info` 加一段「先解析再驗」就沒有東西會紅。
   **修法**：在既有的 `bad_mac_is_400_and_changes_nothing` 裡多打一次 `/api/ecpay/payment/info`（約 8 行），斷言 400 `0|CheckMacValue Error` 且 `payments` 的 `atm_vaccount` 仍是 NULL。

4. **手動走查文件只走個人發票；公司與捐贈發票零實機覆蓋。**
   `docs/dev/ecpay-stage.md` 的「信用卡」節走的是預設的個人／綠界載具（`CarrierType=1`）。但 `ecpay/invoice.rs:151-184` 對三種發票型別送出完全不同的欄位組合：公司戶 `Print=1` + `CustomerIdentifier` + `CustomerName` + `CustomerAddr` 且**不帶載具**，捐贈 `Donation=1` + `LoveCode`。這兩組只有 Rust 單元測試（斷言我們自己組出的欄位），`EcpayInvoiceClient::issue` 這條真的打綠界的路徑在整支分支上**沒有任何自動覆蓋**（ledger L132 已標 ⚠️），綠界對 `Print=1` 缺地址、`LoveCode` 不存在之類的驗證只有在 stage 才看得到。與規格不同之處 29 還留了一個未確認項（`CarrierType=1` 時 `CustomerID` 是否可空），同樣只能靠實機。
   **為什麼重要**：這是計畫 5 上線前的最後一道人工閘門，文件沒寫就不會有人走；一旦漏掉，公司戶客人的發票會在正式環境第一次開立時才失敗。
   **修法**：在 `docs/dev/ecpay-stage.md` 的「信用卡」節後面加兩小節（各 4～5 行）：一筆公司發票（統編 `04595257`、抬頭、發票地址）與一筆捐贈發票（愛心碼 `168`），各自走到 `invoices.status = issued` 並到 einvoice-stage 後台看到同一張；順便寫上「確認 `CarrierType=1` 不帶 `CustomerID` 也能開立」與「確認回應的 `RtnCode` 是數字而不是字串」（對應 `invoice.rs:218` 只用 `as_i64` 讀）。

#### Minor（可選）

1. `payments::apply_info`（`api/src/domain/payments.rs:171-222`）不看訂單狀態：買家在綠界取號後、繳費資訊回呼到達前把訂單取消掉，繳費資訊仍會寫進 `payments` 並寄出「繳費資訊」信，等於叫客人去付一筆已取消的訂單（真付了會走遲到付款 → `needs_refund`，系統處理得了，但體驗差）。窗口只有幾十秒，成本是加一個訂單狀態檢查。
2. `orders::cancel`（買家取消）不把該訂單 `pending` 的 payments 標 `expired`（只有 `expire_one` 會標），已取消訂單會永遠留著 `pending` 的付款列。純資料衛生，但計畫 4 的後台取消／標記退款要一起處理（見交接 3）。
3. `create_repayment` 的回傳值被丟掉（`routes/orders.rs:224-229`），表單改用 `get_detail` 重讀最新一列。兩個分頁同時按「前往付款」時，其中一頁可能拿到另一頁的 `MerchantTradeNo`，被綠界以重複交易號擋下。這是**可見性**競賽不是排序問題，改 `ORDER BY id DESC` 沒用；唯一乾淨的修法是讓 `checkout_form_for` 接受呼叫者給的 payment。
4. 重新付款會把 `OrderDetail.payment` 換成新的 pending 列，訂單頁上原本顯示的 ATM 虛擬帳號因此消失（即使舊帳號還在期限內）。這是「最新一筆」語意的必然結果，計畫 4 的後台訂單頁需要 `payments::list_for_order` 時會一起浮現。
5. `record_request`／`mark_issued`／`record_failure`（`api/src/domain/invoices.rs:77-136`）都是 `UPDATE … WHERE order_id = $1` 而不檢查影響列數。計畫 2 留下的舊訂單沒有 `invoices` 列（0003 建表時是空的；我在 dev DB 實查是 9 筆訂單對 3 筆 invoices），不過 `issue_invoice` 在 `handlers.rs:308-310` 就會先 `bail!`，**不會誤打綠界**；只是那些舊訂單若被重新付款成功，發票 job 會一路重試到 `failed`。要乾淨的話補一句 backfill SQL 進 `docs/dev/`。
6. `EcpayInvoiceClient::issue` 用 `error_for_status()`（`invoice.rs:275`）丟掉 4xx／5xx 的 body；`TransCode != 1` 時 `invoices.response` 存 `None`（`invoice.rs:284-291`）。都只影響可觀測性。
7. `time::parse_taipei` 失敗時 `invoice_date` 靜靜變成 NULL（`handlers.rs:327`），少一行 `warn`。
8. `worker::mark_done`／`mark_failed_attempt`（`worker.rs:61-95`）沒有 `AND status = 'running'` 守衛；單一 worker 行程 + 10 分鐘 stale 門檻下不可達，計畫 5 若要跑多個 api 副本要先補。
9. 訂單頁的 `needsPolling`（`web/src/lib/orderPage.ts:14-16`）不看 `payment.status`：付款嘗試已 `failed` 時仍顯示「等候綠界付款結果中…」兩分鐘。信用卡被拒多半不會有 `ReturnURL` 通知，所以 `failed` 罕見。
10. `.env.example` 與 `config.rs:69` 都寫死了發票 stage 的 HashKey／HashIV。規格 §8.5 明說是公開測試資料，可以進 git；但 `credentials()` 的 stage fallback 已經涵蓋，`.env.example` 其實可以留空以避免「秘密長得像秘密」的習慣被稀釋。
11. `ApiError::EcpayError(String)` 的 `#[error("{0}")]`（`error.rs:35-36`）會把字串原封不動當成給使用者看的 `message`。本計畫沒有任何地方建構它，但計畫 4 的物流建單會，屆時不能把綠界原始回應直接丟進去。
12. `tests/common/mod.rs:46-51` 的 `sent_emails` 對非 `Capture` mailer 回空 `Vec`，`fake_invoices` 則 `panic!`；兩者不一致，前者會讓誤用的測試「安靜地通過」。

---

### 擱置事項的裁決（`final-review-inputs.md` 逐行）

依 `final-review-inputs.md` 的行號（= SDD ledger 行號）：

- **L37**（Task 1 ⚠️ Config 字面建構）→ **已結案**：本次 `cargo test` 編過 lib + 21 個整合測試檔，仍然沒有其他地方字面建構 `Config`。
- **L38**（`from_env` 沒有負向測試）→ **Fix in 計畫 5**：四個分支（prod 缺憑證、未知 `ECPAY_ENV`、有 `SMTP_HOST` 無 `SMTP_FROM`、`SMTP_PORT` 非數字）都是啟動時才走到，第一次真的用是計畫 5 部署；列進計畫 5 的上線清單，不擋本計畫。
- **L39**（`debug_redacts_secrets` 的失敗訊息會印出 Debug 文字）→ **Accept**：印出來的是 stage/假值，而且只在測試已經失敗時。
- **L44**（`expire_unpaid_orders` 每筆一個交易）→ **已結案**：`scheduled.rs:46-54` 確實逐筆 `expire_one`，`cancel_in_tx` 的鎖序在單筆內排序。
- **L45**（可見性述詞在兩處重複）→ **Accept**：`get_for_viewer` 已改成 `EXISTS` + `get_detail`，只剩 `cancel` 一處相同字串；計畫 4 的後台訂單頁若再複製第三次再抽 helper。
- **L51**（Task 3 修測試的裁決）→ **同意**：我用 `shasum -a 256` 獨立算了同一組文件參數，`mac.rs:112` 的中間值與 `:120` 的 CheckMacValue 都對得上；把弱斷言換成「先算出正確值再用錯 key 驗」是必要的。計畫 4 的 MD5 版本測試不要抄舊形狀。
- **L52**（`dotnet_url_encode`／`raw_string` 沒有空值測試）→ **Accept**：空字串沒有分支（迴圈跑 0 次）。
- **L53**（只差大小寫的 key）→ **Reject**：綠界不會送這種欄位，`sort_by_key` 對相同 key 是穩定排序，行為仍是確定的；為不存在的輸入加測試是雜訊。
- **L61**（`checkout_form_for` 在 commit 之後失敗）→ **已由 L63 的裁決取代**，見下。
- **L63**（`ecpay` 改成 `Option<CheckoutForm>` 的裁決）→ **同意**，而且我認為這是本計畫最重要的一個判斷：訂單 commit 之後任何失敗都不能讓客戶端「以為沒成功而重送」，否則庫存會被扣兩次。`routes/orders.rs:100-126` 的註解把理由留在程式碼裡，很好。
- **L64**（`PUBLIC_BASE_URL` 尾斜線、guest_token 網址安全）→ **已結案**（`config.rs:142-146`、`auth/tokens.rs`）。
- **L65**（`CheckoutForm` derive Debug、`ClientBackURL` 內含 guest_token）→ **Accept**：我 grep 過，`routes/orders.rs`／`ecpay_payment.rs` 都沒有用 `{:?}` 印表單，`tracing::error!` 只帶 `order_id` 與 `error`。計畫 4 若把 `CheckoutForm` 傳到別的模組，建議改成手寫 `Debug` 遮蔽 `fields`。
- **L66**（`tests/orders.rs` 用 `starts_with` 斷言 ItemName）→ **Accept**：`ecpay/aio.rs:314` 的單元測試已經逐字斷言同一個值。
- **L67**（`settings::get_all` 一次下單呼叫兩次）→ **Accept**：一次查詢 5 個 key，下單路徑本來就有十幾次查詢，不值得為此改簽名。
- **L71**（`ecpay = null` 分支沒有故障注入測試）→ **Accept**：依 L63 的裁決成本／價值判斷；e2e 與整合測試都涵蓋 happy path，null 分支只有兩行。
- **L75／L77**（Task 5 的 `data.user` 判別）→ **同意，已結案**：與後端 `routes/orders.rs:110` 的 `user.is_none()` 同一套規則，我對照過。
- **L78**（結帳頁說明文字過時）→ **已結案**：`checkout/+page.svelte:313` 現在是「送出後會建立訂單並前往綠界付款頁；付款完成會回到訂單頁。」
- **L79**（fallback 分支沒有重設 `submitting`）→ **Accept**：下一行就 `goto` 離開本頁。
- **L84**（Task 6 ⚠️ dedupe key、500 路徑）→ **已結案**：`email:payment_instructions:{payment_id}` 是對的（重新付款必須能再寄一次繳費資訊）。
- **L85**（`apply_info` 在 payment 非 pending 時不存 raw）→ **Reject（現行行為比建議更好）**：若覆寫，一筆遲到的 info 回呼會蓋掉付款成功那次的 `raw`，反而破壞對帳。
- **L86**（`apply_info` 無條件綁 `expire_at` 等欄位）→ **Defer 計畫 4**：一行 `COALESCE` 的硬化，和物流回呼的欄位處理一起做。
- **L87**（SimulatePaid 短路在查列之前）→ **Accept**：規格 §8.2 只要求「只記 log 不改狀態」，綠界對模擬付款不看回應內容。
- **L88**（三個覆蓋缺口）→ **部分升級**：「另一筆已付」與「`/info` 壞簽章」升級成修正波第 2、3 項；「已 paid 之後收到 `RtnCode≠1`」→ **Accept**，因為 `apply_return` 在 `payment.status == paid` 時一律走 `Duplicate` 早退（`payments.rs:79-81`），與已有的重複通知測試是同一條路徑。
- **L89**（fmt/clippy 證據是轉述）→ **已結案**：本次重跑，見驗證表。
- **L90**（金額不符的訂單仍會過期取消）→ **同意**：錢已標 `needs_refund`，貨不能用錯的金額出。
- **L91**（`expire_one` 內先 UPDATE payments 再 `cancel_in_tx`，false 就 rollback）→ **同意**，而且我把兩個方向的交錯逐一推過（見「優點」第 3 點），這個裁決是對的。
- **L97**（重新付款的競賽）→ **Defer 計畫 4**：真正的成因是可見性而非排序，唯一乾淨的修法是用 `create_repayment` 的回傳值組表單；影響是同一位買家在兩個分頁同時按時，其中一頁被綠界擋下、可重試。列進計畫 4 整理 payment 取得方式時一起做。
- **L98**（未知 `payment_method` 的訊息）→ **Accept**：訊息稍鈍但不會誤導，前端只送三個合法值。
- **L99**（repay 先 commit 再組表單、可能留孤兒列）→ **Accept**：與 L63 同一個取捨；孤兒列會被過期掃描標 `expired`。
- **L105**（panic 隔離 + `expire_one` 抽出的裁決）→ **同意**：兩件都落地，而且各有一個真的會 panic／真的會 rollback 的測試。
- **L106**（Task 8 證據待重驗）→ **已結案**：本次重跑。
- **L107**（`mark_done`／`mark_failed_attempt` 缺 `AND status='running'`）→ **Defer 計畫 5**：單一 worker + 10 分鐘 stale 門檻下不可達；計畫 5 決定是否跑多副本時再補（見交接 5）。
- **L108**（標記失敗會中斷同批其餘 job）→ **Accept**：`requeue_stale` 會撿回來。
- **L109**（`MissedTickBehavior::Burst`）→ **Accept**：10 分鐘／1 小時／1 天的間隔不會落後到需要補跑。
- **L110**（handler 錯誤字串會進 `jobs.last_error`）→ **已結案 + 本次複核**：`send_email` 的錯誤來自 lettre（`SMTP 寄送失敗` + 伺服器回應碼）、`issue_invoice` 來自 reqwest（含 URL、不含金鑰）與綠界的 `RtnMsg`、`password_reset` 失敗時錯誤來自 mailer（重設 token 只出現在已渲染的信件內文，不在錯誤裡）。沒有秘密外洩路徑。
- **L114**（spawn 出去的 handler 是 detached）→ **Accept**：沒有呼叫端用 timeout/select 取消 `run_once_with`。
- **L115**（rollback 測試沒有先斷言 pending）→ **Accept**：測試在呼叫後斷言仍是 `pending`，已足以證明 UPDATE 被撤銷。
- **L120**（Task 9 ⚠️ `send_email` 私有）→ **已結案**。
- **L121**（`payment_instructions` 不核對 `payment.order_id`）→ **Accept**：payload 由 `apply_info` 自己產生，不是外部輸入。計畫 4 若讓後台手動排這種 job，要補這個檢查。
- **L122**（補償 DELETE 用 `?` 會蓋掉原錯誤）→ **Accept**：兩個錯誤都是 DB 錯誤時，處置方式相同。
- **L123**（`Mailer::Smtp` 沒有測試）→ **Fix in 計畫 5**：第一次真的用 SMTP 是部署，`from_config` 的單元測試（假 `SmtpConfig` → 檢查 465/587 分岔與 `SMTP_FROM` 解析）很便宜，列進計畫 5。
- **L124**（畸形 payload 未測）→ **Accept**：`bad_jobs_fail_and_retry`（`tests/mail_jobs.rs:363`）已蓋未知 kind、未知模板、訂單不存在；缺 `order_id` 走的是同一個 `payload_uuid`。
- **L125**（`sent_emails` 對非 Capture 回空 Vec）→ **Accept**（列為 Minor 12，一行改成 `panic!` 可與其他小修一起做）。
- **L130**（AES 已知向量的重算裁決）→ **同意**：我用 `openssl enc -aes-128-cbc -K … -iv … -nosalt -base64 -A` 重算，與 `aes.rs:74` 的期望值逐字元相同。
- **L132**（`EcpayInvoiceClient::issue` 無自動覆蓋，必須人工走）→ **升級**：併入修正波第 4 項（文件補公司／捐贈發票與兩個待確認項）。程式面接受現狀。
- **L133**（envelope／response decode 難測）→ **Accept**：抽出 `envelope()` 只能測我們自己組的鍵名，真正的風險在綠界的驗證規則，只有 stage 走查能覆蓋。
- **L134**（`record_failure` 缺 `AND status <> issued`）→ **Defer 計畫 4**：只有「同一個 job 被 stale requeue 後兩次並行」才會出現，計畫 4 做後台重開發票時一起補。
- **L135**（`invoice_issued` 排程失敗後永遠不再寄）→ **Defer 計畫 4**：影響只有少一封通知信（綠界自己也會寄一封），修法是把 enqueue 也放進「已開立」的略過分支。
- **L136**（`TransCode != 1` 存 `None`、`error_for_status` 丟 body）→ **Fix in 計畫 5**（可觀測性，見 Minor 6）。
- **L137**（`RtnCode` 只用 `as_i64` 讀）→ **Accept**：綠界 B2C 發票文件的 `RtnCode` 是整數；已在修正波第 4 項的文件裡要求 stage 走查時確認一次（若是字串，現行行為是「開不出來並標 failed」，是安全的方向）。
- **L138**（`parse_taipei` 失敗靜靜 NULL）→ **Accept**（Minor 7，一行 `warn` 可順手加）。
- **L139**（`issue_invoice` 私有，重開發票要排 job）→ **已在交接清單第 4 項**。
- **L141**（把 Task 5 的文案修正併進 Task 11）→ **同意，已結案**。
- **L145**（brief Step 6 的手動走查沒有人走過）→ **人工驗收項**：列在「驗收清單對照」第 5、6 項。
- **L146**（`hasPaymentInfo`／`needsPolling` 不看 `payment.status`）→ **Defer 計畫 4**（Minor 9）：信用卡被拒多半沒有 `ReturnURL` 通知，`failed` 狀態罕見；計畫 4 動訂單頁時一起做。
- **L147**（同路由重用時輪詢不會重啟）→ **Accept**：今天不可達，計畫 4 若加「上一筆／下一筆訂單」導覽再補 `{#key o.id}`。

---

### 修正波清單（Fix wave）

一次派工、一個或多個 commit，都在本機分支、不 push。每項都要維持 `cargo test`／`cargo fmt --check`／`clippy -D warnings`／`pnpm -C web check` 全綠。

1. **`api/src/routes/ecpay_payment.rs:19-30`（＋ `api/src/app.rs:16` 的常數不動）— 綠界回呼加 body 上限與參數筆數上限。**
   在 `router()` 的兩條路由之後加 `.layer(axum::extract::DefaultBodyLimit::max(64 * 1024))`（後掛的 layer 在內層，會覆蓋 `app.rs:66` 的 10 MB；`use axum::extract::DefaultBodyLimit;`）。在 `parse_form` 之後、`aio::parse_notification` 之前加一個守衛：`if params.len() > MAX_CALLBACK_FIELDS { return callback_error(path, CallbackError::BadMac); }`，`const MAX_CALLBACK_FIELDS: usize = 100;`（綠界 `ReturnURL` 最多 30 幾個欄位）。兩條 handler 都要加。
   測試：`api/tests/ecpay_payment.rs` 新增 `oversized_callback_is_rejected_cheaply`：用 `ecpay_post_raw` 送一個 200 KB 的 body（例如 `"a=1&".repeat(50_000)`）到 `/api/ecpay/payment/return` → 狀態是 413（`DefaultBodyLimit` 的回應）而不是 200，且該筆 `payments` 的 `status` 仍是 `pending`；再送 150 個合法欄位（大小沒超過但筆數超過）→ 400 `0|CheckMacValue Error`。兩條路徑（`/return`、`/info`）都要測。
   驗證：`cargo test --test ecpay_payment`；另外重跑我做過的活體探測（12 併發 10.4 MB）確認 RSS 不再爬升。

2. **`api/tests/ecpay_payment.rs`（接在 `:365` 之後）— 補「另一筆付款嘗試已讓訂單變 paid」的遲到付款測試。**
   新增 `late_payment_after_another_attempt_paid`：`place_order` 拿到 `mtn`（`…01`）→ 呼叫 `POST /api/orders/{id}/repay?t=…`（body `{"payment_method":"credit"}`）產生 `…02` → 先對 `…02` 送成功回呼（訂單變 `paid`）→ 再對 `…01` 送 `RtnCode=1`、`TradeAmt` 正確的回呼。斷言：HTTP 200 `1|OK`；`payments` 的 `…01` 是 `paid`；`orders.status` 仍是 `paid`、`paid_at` 沒有被第二次覆寫；`orders.needs_refund = true`；`issue_invoice` 與 `email:payment_received` 的 job 各只有一筆（不會因為第二次回呼多排）。
   驗證：`cargo test --test ecpay_payment late_payment_after_another_attempt_paid`。

3. **`api/tests/ecpay_payment.rs:211-246`（`bad_mac_is_400_and_changes_nothing`）— 壞簽章也要打 `/info`。**
   在同一個測試裡（或新增一個孿生測試）對 `/api/ecpay/payment/info` 送一組帶 ATM 欄位、但 `CheckMacValue` 被竄改的表單：斷言 400、body 是 `0|CheckMacValue Error`、`payments` 的 `atm_vaccount`／`atm_bank_code`／`expire_at` 都仍是 NULL、`status` 仍是 `pending`、沒有新的 `send_email` job。
   驗證：`cargo test --test ecpay_payment bad_mac`。

4. **`docs/dev/ecpay-stage.md`（「信用卡」節之後）— 補公司發票與捐贈發票的實機走查，並列出兩個待確認項。**
   新增「公司發票」小節：結帳選公司 → 統編 `04595257`、抬頭、發票地址 → 信用卡付款 → 斷言 `invoices.status = issued`，到 einvoice-stage 後台看到 `Print=1` 的那張。新增「捐贈發票」小節：愛心碼 `168` → 付款 → `issued`。在「信用卡」節第 5 步後加兩行待確認：(a) `CarrierType=1` 不帶 `CustomerID` 能否開立（與規格不同之處 29 的未確認項）；(b) 綠界回應的 `RtnCode` 是數字還是字串（`api/src/ecpay/invoice.rs:218` 只用 `as_i64` 讀，若是字串會一路重試到 `failed`）。
   驗證：文件變更，無自動測試；`git diff --stat` 確認只動這一個檔。

（可選、若同一波有餘裕：Minor 1（`apply_info` 加訂單狀態檢查 + 一個測試）、Minor 7（`parse_taipei` 失敗補 `warn`）、Minor 12（`sent_emails` 改 `panic!`）各 1～3 行；不要求。）

---

### 交給計畫 4 的事項（補充計畫 7087–7101 行沒寫到的）

1. **物流回呼要一起吃 body 上限。** 修正波第 1 項會在 `routes::ecpay_payment::router()` 上掛 `DefaultBodyLimit::max(64 * 1024)`；`routes/ecpay_logistics.rs` 的 `map-reply`／`status` 兩條也在 `/api/ecpay/` 的 CSRF 豁免內，必須掛同一個 layer（`map-reply` 甚至沒有簽章，只靠 `ExtraData` 的 token，更要限大小）。
2. **`ApiError::EcpayError(String)` 會把字串當成給使用者看的 `message`**（`api/src/error.rs:35-36`、`:69`、`:84`）。計畫 4 的物流建單是第一個會建構它的地方，**不要**把綠界的原始回應整段丟進去；用一句固定文案，細節記 log。
3. **後台取消／標記退款要把 `pending` 的 payments 標 `expired`。** 現在只有 `jobs/scheduled.rs::expire_one` 會標；買家取消（`orders::cancel`）不會，計畫 4 的 `admin/orders/{id}/cancel`、`mark-refunded` 也別忘了。建議把「UPDATE payments pending→expired 再 cancel_in_tx」抽成 `orders::cancel_with_payments_in_tx`，並保持 `payments → orders → product_variants` 的鎖序（`expire_one` 的形狀就是範本）。順帶：已取消訂單若之後收到 `PaymentInfoURL` 回呼，現在仍會寄「繳費資訊」信（Minor 1），修 `apply_info` 時一起處理。
4. **`OrderDetail.payment` 是最新一筆，後台需要全部。** 寫 `payments::list_for_order(db, order_id)`（`PAYMENT_COLUMNS` 已抽出，`domain/payments.rs:34`）。同一次也把 `checkout_form_for`（`routes/orders.rs:75-98`）改成可以接受呼叫者給定的 payment，順手解掉重新付款的競賽（ledger L97）。
5. **worker 多副本的前提。** `worker::mark_done`／`mark_failed_attempt` 沒有 `AND status = 'running'` 守衛，`requeue_stale` 的 10 分鐘門檻下單一行程安全；計畫 5 若讓 api 跑多個副本（或把 worker 拆成獨立行程），要先補這個守衛，否則「stale 撿回 + 舊行程晚到」會互相蓋掉狀態。
6. **重開發票的 `record_failure` 守衛。** `invoices::record_failure`（`domain/invoices.rs:115-136`）沒有 `AND status <> 'issued'`；做 `retry-invoice` 時補上，避免一次成功一次失敗的並行把已開立的發票標成 `failed`。同一段還要處理 `invoice_issued` 通知信的排程（ledger L135）：把 `jobs::enqueue` 也放進 `handlers.rs:311-314` 的「已開立就略過」分支。
7. **`RUST_LOG` 與 `mail_body`。** `main.rs:16-18` 的預設是 `info,tower_http=info`，`mail_body=debug` 必須明確開啟才會印信件內文（含重設連結與訪客訂單網址）。計畫 5 的部署文件要把「正式環境不要開 `mail_body`」寫進 `.env` 註解與檢查清單。
8. **開發用 DB 的舊訂單沒有 `invoices` 列。** `0003` 建表時是空的；那 4 筆計畫 2 的訂單若被重新付款成功，`issue_invoice` 會在 `handlers.rs:308-310` 直接 `bail!` 並重試到 `failed`（不會誤打綠界）。要乾淨的話在 `docs/dev/` 補一句 backfill SQL；正式環境沒有這個問題（0003 之後才會有訂單）。
9. **stage 憑證是公開的。** `docs/dev/ecpay-stage.md` 教人用 cloudflared 把本機曝到網際網路，而 stage 的 HashKey／HashIV 是規格 §8.5 的公開資料 —— 任何知道那個 trycloudflare 網址的人都能偽造一筆「已付款」回呼。開發機、臨時網址、可拋棄的資料庫下可以接受，但計畫 5 的部署文件要明說：**正式站的憑證不得用 stage 值，`ECPAY_ENV=prod` 時 `config.rs:129` 會強制三個變數都要有**。

---

### 驗收清單對照（計畫 7074–7085 行，items 1–8）

| # | 項目 | 狀態 |
|---|---|---|
| 1 | db healthy；`cargo run` 跑完 `0003_invoices.sql`，log 有「SMTP 未設定」warn 與「jobs worker 與排程工作已啟動」，沒有 rustls panic | **自動驗證**：`docker ps` 顯示 `dog_shop-db-1 Up 2 days (healthy)`；`cargo run` 的啟動 log 依序是 migration notice → `SMTP 未設定（SMTP_HOST 空白）：Email 只會記 log，不會真的寄出`（WARN）→ `jobs worker 與排程工作已啟動`（INFO）→ `api listening on http://0.0.0.0:8080`，**沒有 rustls panic**；dev DB 的 `_sqlx_migrations` 三支 v1/v2/v3 皆 `success = t`，`invoices` 的欄位、UNIQUE、CHECK、FK 與 `0003_invoices.sql` 一致 |
| 2 | `cargo test` 全綠（單元 + 21 個整合測試檔）＋ `fmt`／`clippy` 乾淨 | **自動驗證**：167 passed、21 個整合測試檔、fmt/clippy 零輸出（本次重跑，exit 0）。清單點名的八項都在：CheckMacValue 文件向量（`mac.rs:117`）、AES openssl 向量（`aes.rs:66`）、重複通知冪等（`ecpay_payment.rs:148`）、遲到付款 `needs_refund`（`:317`）、過期歸還庫存（`jobs_worker.rs:261`）、worker 退避與 failed（`:68`）、密碼重設節流（`mail_jobs.rs:297`）、發票開立與失敗標記（`invoice_job.rs:115,192`） |
| 3 | `pnpm -C web test`（27）、`check` 0/0、`build` | **自動驗證**：27 passed（6 檔）、0 ERRORS 0 WARNINGS（300 files）、build 成功 |
| 4 | `test:e2e` 1 passed，攔到送往 `payment-stage.ecpay.com.tw` 的表單、`MerchantID=3002607`、`ChoosePayment=Credit`、`CheckMacValue` 64 碼、經 `ClientBackURL` 回訂單頁、取消成功 | **自動驗證**：本次在 HEAD 起 API + Vite 後重跑，**1 passed（3.2s）**；測試本身斷言了 `MerchantID`、`ChoosePayment`、`PaymentType`、`EncryptType`、`MerchantTradeNo` 結尾 `01`、`CheckMacValue` 64 碼十六進位、`ReturnURL`／`PaymentInfoURL`／`ClientBackURL` 形狀，並真的走 `ClientBackURL` 回訂單頁 |
| 5 | 依 `docs/dev/ecpay-stage.md` 走綠界 stage：信用卡 → 已付款 + `invoices.status=issued` + 兩封信；ATM → 顯示帳號與期限；模擬付款只記 log | **需手動**（沒有人走過，ledger L132／L145 已標）。回呼與發票的**邏輯**有整合測試蓋住，但真的打綠界的 `EcpayInvoiceClient::issue`（envelope、TLS、綠界欄位驗證）沒有任何自動覆蓋；修正波第 4 項要求走查時多走公司與捐贈發票 |
| 6 | 訂單頁 3 秒輪詢、2 分鐘後顯示「請重新整理」；「前往付款」的 `MerchantTradeNo` 結尾 `02` | **部分自動**：`orderPage.test.ts` 蓋了 `needsPolling`／`hasPaymentInfo`／`paymentExpired` 的判斷，`tests/repay.rs:59` 斷言重新付款的 `merchant_trade_no` 結尾是 `02`、`ecpay.fields` 完整；**輪詢的計時行為與 2 分鐘逾時文案要人工看一次**（brief Step 6，沒有人走過） |
| 7 | `/forgot-password` log 有重設信 subject；`mail_body=debug` 看得到連結且能用；10 分鐘內第二次不寄 | **部分自動**：`password_reset_creates_token_throttles_and_skips_unknown_user`（`tests/mail_jobs.rs:297`）驗了節流、token 產生、未知使用者略過；`mail_body=debug` 的實際輸出與「連結能用」要人工走一次 |
| 8 | 12 個 commit 都在 `worktree-mvp-design`；`git status` 乾淨；沒有 push | **自動驗證**：`git rev-list --count f8bd3b7..80b9b73` = **15**（12 個任務 commit + 3 個任務內修正：`6980386` 測試修正、`f587b45` ecpay null、`a6dcdc5` worker panic 隔離），全部在 `worktree-mvp-design`；`git status` 乾淨；`git branch -r` 空。清單寫「12 個 commit」，實際 15 個，差額是控制者裁決產生的修正 commit，屬正常 |

未達成：第 5 項（綠界 stage 手動全流程）與第 6、7 項的人工部分尚未有人走過，是**上線前的人工閘門**，不是本計畫的程式缺陷。

---

### 活體探測

起了 API（:8080，debug build、`.env` 的 `ECPAY_ENV=stage`／`SMTP_HOST` 空）與 Vite（:5173），用 Node 自己寫了一份獨立的 CheckMacValue 實作（`/tmp/dogshop-probe/mac.js`，先用綠界文件範例自我檢查通過 `6C51C9E6…5B840`）來簽出各種回呼，直接對真的在跑的 api 打。**沒有呼叫任何綠界的線上 API**（探測全程 `issue_invoice` job 是 0 筆，見 P8）。

**先驗簽章工具本身**：對 `POST /api/orders` 回來的 17 個欄位用我自己的實作重算 CheckMacValue → `241D0083…CA15`，與伺服器產生的**逐字元相同**。表單欄位齊全（規格 §8.2 的 16 個 + CheckMacValue），`ExpireDate=3`、`StoreExpireDate=4320`、`NeedExtraPaidInfo=N`、`EncryptType=1`、`CustomField1=order_id`、`ItemName=雞肉狗糧(預設) x 1`、`ClientBackURL` 帶 `?t=<64 碼>`。

回呼矩陣（訂單 `DS260908A658`，總額 600，訪客）：

| # | 送什麼 | 回應 | DB 側效果 |
|---|---|---|---|
| P1 | `/return` 竄改 `CheckMacValue` 最後一碼 | **400 `0\|CheckMacValue Error`** | 無 |
| P2 | `/return` 完全不帶 `CheckMacValue` | **400 `0\|CheckMacValue Error`** | 無 |
| P3 | `/return` 簽章正確但沒有 `MerchantTradeNo` | **400 `0\|Missing Field`** | 無 |
| P4 | `/return` 簽章正確、`MerchantTradeNo` 不存在 | **200 `0\|Unknown MerchantTradeNo`** | 無 |
| P5 | `/return` `RtnCode=1` + `SimulatePaid=1` | **200 `1\|OK`**，log `outcome=Simulated` | 無（訂單仍 `pending_payment`、payment 仍 `pending`、`raw` 仍 NULL） |
| P6 | `/info` 竄改 `CheckMacValue` | **400 `0\|CheckMacValue Error`** | 無 |
| P7 | `/info` `RtnCode=2` + `BankCode=812`／`vAccount`／`ExpireDate=2026/09/11` | **200 `1\|OK`**，log `outcome=Stored` | `atm_bank_code=812`、`atm_vaccount=9998887776665554`、`expire_at=2026-09-11 15:59:59+00`（＝台北 23:59:59，**ATM 只給日期要當天末的規則正確**）、`ecpay_trade_no`、`raw` 存好；排了 `email:payment_instructions:{payment_id}` 的 job，worker 2 秒內做完，log 只印 `to` 與 `subject`（`【dog_shop】訂單 DS260908A658 繳費資訊`），**沒有內文** |
| P8 | 先用 `POST /api/orders/{id}/cancel?t=` 取消（204、庫存 9→10、`cancel_reason=buyer`），再送 `/return` `RtnCode=1` `TradeAmt=600` | **200 `1\|OK`**，log `遲到或金額不符的付款…（規格 §4）` + `outcome=Late` | 訂單維持 `cancelled`、`paid_at` 仍 NULL、**`needs_refund=t`**、payment `paid`、`payment_date=2026-09-08 02:45:00+00`（＝送的台北時間 `10:45:00`，時區換算正確）、**庫存維持 10（沒有二次歸還）**、**`issue_invoice` job 0 筆**、沒有 `payment_received` 信、`invoices` 仍 `pending` |
| P9 | 同一份 body 再送一次 | **200 `1\|OK`**，log `outcome=Duplicate` | 完全沒有變化 |
| P10 | 另一筆訂單送 `/return` `RtnCode=10100248` | **200 `1\|OK`** | payment `failed`、`raw.RtnCode=10100248`、訂單維持 `pending_payment`、`needs_refund=f`、**庫存不歸還**（規格 §4：訂單不動）、沒有排任何 job |
| P11 | `/return` 10.4 MB 對抗性 body（52 萬個相異 key、值多位元組） | 400（花 266 ms）；12 併發 → 全 400、0.66 s，api RSS 76 MB → **528 MB**，`/api/health` 仍 0.9 ms 回應 | 無（見 Important 1） |
| P12 | `/return` 20 MB body | **413**（`DefaultBodyLimit` 有生效，只是上限訂在 10 MB） | 無 |

其他探測：

- **重新付款**：`POST /api/orders/{id}/repay?t=…` body `{"payment_method":"atm"}` → 200，新表單的 `MerchantTradeNo` 是 `DS260908AJWR`**`02`**、`ChoosePayment=ATM`、`ClientBackURL` 仍帶 `?t=`。驗收清單第 6 項的「結尾 `02`」live 驗過。
- **viewer 規則**：`GET /api/orders/{id}` 不帶 `?t=` → **404**；帶正確 token → 200。回應裡 `payment` 只有 `method/status/amount/atm_*/cvs_payment_no/expire_at`（**沒有** `id`、`merchant_trade_no`），頂層**沒有** `guest_token`、`user_id`、`needs_refund`。
- **訂單頁 SSR**：已取消的那筆 → `<title>訂單 DS260908A658</title>`、顯示「已取消」、**不顯示**繳費資訊也不顯示「重新付款」；待付款的那筆 → 顯示「待付款」「等候綠界付款結果」「重新付款」「前往付款」「ATM 轉帳」。
- **秘密不進 log**：整份 api log `grep` 不到任何一個 guest_token（0 筆）、也 `grep` 不到四個 HashKey／HashIV（0 筆）。
- **job payload 清理**：做完的 `send_email` job 的 `payload->>'template'` 是 NULL（已被 `mark_done` 清成 `{}`），與規格不同之處 27 一致。
- **計畫 2 資料相容**：worker 啟動後把 dev DB 裡計畫 2 遺留的 10 筆 `send_email` outbox 全部處理成 `done`（走 `Mailer::Log`），沒有一筆失敗；9 筆舊訂單無損。

**副作用揭露**：e2e 在 dev DB 留下一個 `E2E 狗糧 <timestamp>` 商品與一筆已取消訂單（與計畫 2 相同）；我另外建了兩筆探測訂單 `DS260908A658`（已取消、`needs_refund=t`、payment `paid`，正好是「需退款」的樣本，計畫 4 的儀表板可以拿來看）與 `DS260908AJWR`（`pending_payment`，佔著 1 件庫存，3 天後會被過期掃描歸還）。工作樹、index、HEAD 未動（`git status` 乾淨，HEAD 仍 `80b9b73`）。探測腳本在 `/tmp/dogshop-probe/`，不在 repo 內。伺服器已 `kill`，`lsof -ti :8080`／`:5173` 皆空。

---

### 評估

**Ready to merge? With fixes。**

理由：這一波是整個專案第一次碰錢，而該對的地方都對了。CheckMacValue 我用兩套獨立實作（`shasum` 手算 + Node 重寫）對過綠界文件向量與伺服器實際產生的表單，逐字元相同；AES 用 `openssl` 對過。回呼的十種情境我一條一條打過真的在跑的 api：壞簽章 400、缺欄位 400、未知交易號 200 `0|Unknown`、模擬付款不動狀態、失敗碼只標 payment、遲到付款只標 `needs_refund` 而且庫存不會二次歸還、重複通知完全 no-op —— 全部與規格 §4／§8.2／§14 一致，而且和整合測試的斷言互相印證。鎖定順序 `payments → orders → product_variants` 在三條寫入路徑上一致，我把「付款成功 vs 過期取消」兩個方向的交錯推過，沒有環也沒有雙重歸還。秘密處理是本專案目前最嚴謹的一次：四個 HashKey／HashIV 與 guest_token 在整份執行期 log 裡都是 0 筆。

要修的四件都不會弄丟錢，但都該在合併前處理：一個是把給圖片上傳用的 10 MB body 上限從匿名的綠界回呼上拿掉（實測 12 個併發請求就能讓 RSS 從 76 MB 漲到 528 MB，而規格 §17 的目標機器只有 4 GB），兩個是規格明列分支的測試缺口（「另一筆嘗試已付」與 `/info` 壞簽章），一個是把公司／捐贈發票補進 stage 手動走查文件 —— 那條路徑（真的打綠界的 `EcpayInvoiceClient::issue`）在整支分支上零自動覆蓋，只能靠人工走，而文件沒寫就不會有人走。四項合計 60 行以內，一波派工可以收尾。

真正的上線閘門不在程式碼裡：驗收清單第 5 項（綠界 stage 全流程）到現在沒有人走過。整合測試把**我們這一側**的邏輯蓋得很紮實，但綠界對發票欄位的驗證規則、`CarrierType=1` 是否要 `CustomerID`、回應 `RtnCode` 的型別，只有實機能回答。這件事屬於人工驗收，不擋本計畫合併，但計畫 5 上線前必須完成。

---

## 附錄 A：控制者裁決（SDD ledger 全部 `Ruling` 行，共 8 條，依時間順序；開工前的預檢衝突掃描為乾淨、不需裁定）

- Task 3: Ruling: fix the test as the reviewer suggests (push a valid CheckMacValue into a params copy, then verify with "wrongkey") — the plan text mandated the weak assertion, but spec §8.1 requires the doc-example tests to prove the signature logic, and a mislabeled assertion proves nothing — cost if wrong: none, test-only change. Plan doc left as-is; Plan 4 MD5 verify tests must not copy this shape (carry-over note).
- Task 4: Ruling: make CreateOrderResponse.ecpay Option<CheckoutForm>; on checkout_form_for error log tracing::error with order_id and return 201 with ecpay = null instead of failing; Task 5 web treats ecpay null by goto to the order page (guests with ?t=), where Task 7/11 repay covers payment — why: the client must always learn the order id once the order is committed (idempotency of the order path beats a hard failure); the spec does not address this failure mode, so this is the smallest change that removes the duplicate-order risk — cost if wrong: the null branch is rarely exercised and hard to integration-test (needs DB fault injection); a bug there would surface only during a DB hiccup, where the user still lands on an error page with a recoverable URL.
- Task 5: implementer DONE_WITH_CONCERNS at 07fd10a (check 0/0, vitest 24/24, build ok, e2e 1 passed). Concern accepted as a Ruling amendment: fallback goto adds ?t= only when data.user is null (guest_token is always generated server-side, so token presence cannot signal guest vs member; matches backend ClientBackURL logic at routes/orders.rs:109) — cost if wrong: none beyond the fallback branch.
- Task 6: Ruling (for Task 8): amount-mismatch orders (needs_refund=true, still pending_payment) DO expire per the plan SQL — the money is flagged for refund and the order must not ship at a wrong amount — cost if wrong: an admin who wanted to accept the payment must refund and ask the customer to reorder.
- Task 6: Ruling (for Task 8): in expire_unpaid_orders, inside the per-order transaction run the payments UPDATE (pending→expired) BEFORE cancel_in_tx, and roll back instead of commit when cancel_in_tx returns false — keeps the global lock order payments → orders → product_variants consistent with apply_return (payments → orders) and avoids deadlock/500-retry noise — cost if wrong: none functionally; a reorder within one transaction.
- Task 8: Ruling: fix both now — (1) isolate each handler run (tokio::spawn + JoinError::is_panic → failed attempt, or futures-util catch_unwind) so the loop survives and the panic counts as a failed attempt; (2) extract expire_one(db, id) -> Result<bool>, match per order, log + continue, fix the comment, and add a test that expire_one on a paid order returns false and leaves the payment pending (covers the rollback branch of the Task 6 ruling) — why: spec §9 requires a long-running worker that enforces max_attempts, spec §5 requires expiry for every overdue order — cost if wrong: a few lines of worker mechanics; no interface change for Tasks 9/10.
- Task 10: Ruling: the brief AES known-vector constant was wrong (plan defect: computed for a 50-byte plaintext; the test plaintext is 51 bytes). Controller recomputed with openssl (-K 656a…7367 -iv 7139…596b) and got uvI4yrErM37XNQkXGAgRgJAgHn2t72jahaMZzYhWL1EKCKsTJAo/hk3KsD1/DLNXvLojRdhbR7dzkFTVZwgUwQ== — identical to the committed expectation; implementation untouched. Accepted — cost if wrong: none (openssl is the oracle). main.rs &config.ecpay compile fix also accepted.
- Task 11: BASE 8a2b6e9; Ruling: fold the Task 5 deferred copy fix into Task 11 — checkout/+page.svelte help text must say submit goes to ECPay for payment (one line) — cost if wrong: none.

---

## 附錄 B：修正波結果與範圍複審

修正波：一次派工（sonnet），基底 `80b9b73`，4 個 commit，全在本機分支、沒有 push：

| Commit | 內容 | 對應項目 |
|---|---|---|
| `9eed271` | 綠界回呼加 64 KB body 上限（`DefaultBodyLimit::max(64 * 1024)` 掛在兩條回呼路由，覆蓋 `app.rs` 的 10 MB）與 `MAX_CALLBACK_FIELDS = 100` 守衛；`apply_info` 加訂單狀態檢查（非待付款就不寫繳費資訊、不寄信，仍回 `1\|OK`）；新測試 `oversized_callback_is_rejected_cheaply`、`late_payment_after_another_attempt_paid`、`/info` 壞簽章測試、`apply_info` 狀態測試 | 1、2、3、Minor 1 |
| `bf20436` | `InvoiceDate` 解析失敗時記一行 `warn` | Minor 7 |
| `5d1728a` | 測試輔助 `sent_emails` 誤用非 Capture mailer 時改為 `panic!` | Minor 12 |
| `c54d23d` | `docs/dev/ecpay-stage.md` 補公司發票、捐贈發票走查與兩個待確認項（`CarrierType=1` 不帶 `CustomerID`；`RtnCode` 型別） | 4 |

修正波自報閘門：`cargo fmt --check` 乾淨、`clippy -D warnings` 乾淨、Rust 測試 170／170（基準 167 ＋ 新增 3）；web 沒有動。

與清單的差異（控制者裁決，皆已記入 ledger）：
- 項目 1 的「200 KB body」用既有的 `ecpay_post_raw` 送 50,000 組 `a=1` 欄位，而不是原始字串——位元組等價，斷言沒有放寬。
- 項目 1 要求重跑的「12 併發 10.4 MB 活體探測」沒有重跑：新的整合測試對 200 KB body 斷言 413，已證明 64 KB layer 在該路由生效；探測只是再觀察同一個機制一次。若判斷錯誤，代價是回呼路由在真實洪流下才會顯現的記憶體成長，下一次 stage 走查可再探。

範圍複審（sonnet，`review-80b9b73..c54d23d.diff`，6 個檔案 +289／−2）：7 項（1–4、Minor 1、7、12）全部 ADDRESSED；指名檢查全部確認——`apply_info` 在訂單非待付款時仍提交交易並回 `1|OK`（綠界不會無限重送）、鎖順序維持 payments → orders、`DefaultBodyLimit` 的覆蓋優先序對照 axum-core 原始碼驗證、欄位數守衛在算 MAC 之前、`paid_at` 與 job 數量的斷言是精確相等而非 `>=`、`/info` 壞簽章測試竄改的是一組合法 ATM 表單的 `CheckMacValue`、diff 沒有記錄任何 HashKey／HashIV／token。修正波自己引入的問題：無。範圍外觀察：無。

**修正波後的裁決：Ready to merge（本機分支）。** 人工驗收項目不變：綠界 stage 全流程走查（`docs/dev/ecpay-stage.md`）與訂單頁的 SQL 模擬走查仍未有人走過，計畫 5 上線前必須完成。

控制者在 `c54d23d` 重跑全套閘門：`cargo fmt --check` 乾淨、`cargo clippy --all-targets -D warnings` 乾淨、`cargo test` 170／170（67 個單元測試 ＋ 21 個整合測試檔 103 個）；`pnpm -C web check` 0 錯誤 0 警告、vitest 27／27、`pnpm -C web build` 成功。修正波沒有動 web，Playwright 主流程維持 Task 12 在 `80b9b73` 的 1 passed。

---

## 附錄 C：codex 第二意見審查與修正（`/codex-review-fix`）

依使用者指示，計畫收尾時用 `codex exec -s read-only`（codex CLI 0.153.4、model `gpt-6-astra`、reasoning `xhigh`）對 `f8bd3b7..9ca51a3` 做一次只讀的第二意見審查。codex 回報 **6 條可行動的問題（2×P1、4×P2、0×P3）**；控制者逐條打開引用的 file:line 對照程式碼與規格後，**6 條全部判定為真**，一次派工（opus）全部修正，每條附一個「修正前會失敗」的回歸測試。

| # | 優先 | codex 的發現 | 控制者判定 | 修正 |
|---|---|---|---|---|
| 1 | P1 | SMTP 伺服器接了連線但不回應時，`Mailer::send` 與 worker 對 handler 的 `await` 都沒有期限，一筆卡住會堵死所有 email／發票 job；`requeue_stale` 救不了卡住的 future | 真。lettre builder 沒設 timeout，`transport.send` 無期限；`run_once_with` 對 spawned handler 無期限；發票的 reqwest 已有 20 秒 | `2090fff` |
| 2 | P1 | `expire_unpaid_orders` 的 SELECT 先算到期，`expire_one` 進交易後只靠 `cancel_in_tx` 查狀態；中間若 `PaymentInfoURL` 寫入未來的 `expire_at`，仍會取消訂單、歸還庫存，買家拿有效帳號去繳就變退款 | 真。`expire_one` 只有 payments UPDATE 與 `cancel_in_tx`，沒有在鎖內重算到期 | `ed44af6` |
| 3 | P2 | `repay` 丟掉 `create_repayment` 回傳的 payment，再重新載入「最新一筆」組表單；並發的兩個 repay 會拿到同一筆（含對方選的付款方式） | 真。`routes/orders.rs:224-230` 確實如此 | `8d6ba63` |
| 4 | P2 | `payment_instructions` 信只用 `payment_id` 載入就寄，排入與寄出之間訂單可能已取消或已由另一筆付清，等於叫客人去付已取消的訂單 | 真。`handlers.rs:165-183` 沒有狀態檢查 | `3b892cf` |
| 5 | P2 | `invoices::mark_issued` 先提交，`invoice_issued` 通知信另開交易排程；兩者之間出錯 → 發票 issued、重試在「已開立」分支直接 return，通知信永遠不寄。違反規格 §9「寫入業務資料與排 job 在同一個交易」 | 真。這條就是 ledger L135／最終審查「擱置事項」裡被判 Defer 到計畫 4 的項目；規格明文要求同一交易，codex 又獨立抓到，改為現在修，**撤銷**那條 Defer | `9ed2522` |
| 6 | P2 | `Mailer::Log` 用 `tracing::debug!(target: "mail_body")` 印整封內文，`RUST_LOG=debug` 就會印出密碼重設連結與訪客訂單 token，不是計畫決定 22 所說的明確 opt-in | 真。規格 §11：重設 token、guest_token 不得進 log | `4e7d017` |

修正內容（一次派工 opus，`e4b2afa..4e7d017`，6 個 commit、9 個新測試，全部在本機分支、沒有 push）：

1. `api/src/mail/mod.rs`：新增 `SMTP_TIMEOUT = 30s`，`Mailer::Smtp` 帶 `send_timeout`，lettre builder 加 `.timeout(...)`，`send` 用 `tokio::time::timeout` 包住整個 `transport.send`。`api/src/jobs/worker.rs`：新增 `JOB_TIMEOUT = 120s` 與 `run_once_with_timeout`，逾時就 `handle.abort()`、視為一次失敗嘗試（`max_attempts` 照樣生效）；`run_once_with` 簽名不變。測試：`smtp_send_times_out_instead_of_hanging`（假 SMTP 接了連線不回應）、`stuck_handler_is_aborted_and_counted_as_failed_attempt`。
2. `api/src/jobs/scheduled.rs`：抽出 `deadline(created_at, max_expire)`（`max(expire_at)+2h`，否則 `created_at+3d`），`expire_one` 在 payments UPDATE 之後、`cancel_in_tx` 之前，於同一交易內 `FOR UPDATE OF o` 重算到期；沒到期就 rollback 回 `Ok(false)`。鎖順序仍是 payments → orders。測試：`expire_one_keeps_order_whose_deadline_was_extended`。
3. `api/src/routes/orders.rs`：新增 `checkout_form_for_payment(state, &detail, &payment, guest_token)`，`repay` 用 `create_repayment` 剛回傳的那一筆組表單；`checkout_form_for` 保留給下單用。`domain/payments.rs` 加 `From<Payment> for PaymentRow`。測試：`repay_form_uses_the_attempt_it_just_created_not_the_latest_row`（修正前拿到 `…99` 而非 `…03`）、`concurrent_repays_get_distinct_attempts`（守衛用；`#[sqlx::test]` 是單執行緒 runtime，`tokio::join!` 不會像正式環境那樣交錯，修正前也會過——已記錄）。
4. `api/src/jobs/handlers.rs`：`payment_instructions` 分支載入 payment 後，`payment.status != pending` 或 `order.status != pending_payment` 就記一行 info（只有 id 與狀態）並回 `Ok(())`（job 標 done、不寄）。測試：`payment_instructions_not_sent_after_order_cancelled`。
5. `api/src/domain/invoices.rs`：`mark_issued` 改成 `mark_issued_in_tx`（唯一呼叫者）；`handlers.rs` 的發票成功分支改為同一交易內 `mark_issued_in_tx` ＋ `jobs::enqueue(invoice_issued)` 再 commit；`record_request` 維持先用 pool 寫入（規格 §14 送出前先存請求）。測試：`mark_issued_and_invoice_mail_roll_back_together`。
6. `api/src/config.rs`：新增 `mail_log_body: bool`（環境變數 `MAIL_LOG_BODY`，只有 `1`／`true` 為開，`for_tests` 為 false）；`mail/mod.rs`：`Mailer::Log { log_body }`，內文只在旗標開著時輸出（`body_log_line` 純函式），等級由 debug 改為 info（旗標才是安全邊界，控制者裁決接受）。`docs/dev/` 的 `mail_body=debug` 說明改為 `MAIL_LOG_BODY=1`。測試：`mail_log_body_defaults_off_and_needs_explicit_opt_in`、`log_mailer_without_opt_in_does_not_format_body`。

每條的回歸測試都先寫、先跑一次看它在修正前失敗（證據在修正報告裡）；第 1 條的兩個測試用 10 秒的 timeout 包住測試體，修正前是明確失敗而不是掛死。

兩個新旋鈕，交給計畫 5 的部署清單：`MAIL_LOG_BODY` 正式環境不要設（未設＝關）；`JOB_TIMEOUT` 是每筆 job 的硬上限 120 秒（目前最慢的 handler：發票 reqwest 20 秒、SMTP 30 秒），未來有更慢的 job kind 要一起調。

擱置／駁回：無（6 條全修）。

codex 也註明 `git diff --check` 通過、在只讀沙箱裡沒有跑測試、沒有改任何檔案。

同一階段控制者自己補的一條：Playwright 主流程在 `9ca51a3` 連續兩次在「取消訂單」那步逾時。trace 顯示 vite client 連上 2 ms 後就點了按鈕，路由模組還在載入——點擊落在 hydration 之前（冷的 vite dev 每次都重現），瀏覽器沒有任何 console／page error，之後的輪詢請求證明頁面最後有 hydrate。判定為測試自己的競態、不是程式缺陷；比照同檔案商品頁既有的做法，用 `expect().toPass()` 重試到確認按鈕出現（`e4b2afa`，test-only）。修正後 1 passed。

控制者在 `4e7d017` 重跑全套閘門：`cargo fmt --check` 乾淨、`cargo clippy --all-targets -D warnings` 乾淨、`cargo test` 179／179（170 ＋ 新增 9）；`pnpm -C web check` 0 錯誤 0 警告、vitest 27／27、`pnpm -C web build` 成功；Playwright 主流程 1 passed（api ＋ web dev 起來跑，跑完關掉）。

範圍複審（opus，`review-e4b2afa..4e7d017.diff`）：6 條全部 ADDRESSED，9 個新測試都在樹上且斷言符合要求；指名檢查 (a)–(h) 全部確認——逾時的 job 會被 `abort()`、只走一次 `mark_failed_attempt`、`last_error` 沒有 payload；`JOB_TIMEOUT`（120 秒）小於 `requeue_stale` 的 10 分鐘，不會把還在跑的 job 重排；到期重查在 payments 鎖之後、`orders` 列鎖之下，鎖順序 payments → orders 不變、沒有新的鎖類別、與 `apply_info`／`apply_return`／`cancel_in_tx`／`create_repayment` 都不成環；diff 沒有記錄任何秘密。Critical：無。

複審另外提出：
- **Important（測試面，不擋）**：既有測試 `expire_one_rolls_back_payments_update_when_order_not_cancellable` 的訂單是新建的（`expire_at` NULL），新的到期重查算出 `created_at+3d` 未到期就先 rollback，原本要守的「`cancel_in_tx` 回 false 也要 rollback」分支不再被跑到。控制者直接修：該測試的 `created_at` 調成 4 天前（`ecd2871`）。
- Minor（擱置）：SMTP 送信被逾時取消後，lettre 連線池可能把一條協定已錯位的連線放回去（最多多失敗一次、之後自癒）；worker 的 `abort()` 沒有 await，handler 恰好在逾時同一瞬間完成時仍會被記成失敗並重試（極窄的窗口，發票的情況落在既有決定 29）；計畫文件「交給計畫 5」第 8 點原本寫 `mail_body=debug`，控制者改為 `MAIL_LOG_BODY`。
- 第 6 條的「修正前失敗證據」只有編譯錯誤（新旗標修正前不存在）——控制者裁決接受：旗標本身就是修正，行為面的修正前失敗沒有可測的形狀。
- 更正一處控制者的口誤：`RepayResponse.ecpay` 一直是 `CheckoutForm`（`Option` 的是 `CreateOrderResponse.ecpay`），repay 的錯誤傳播與修正前相同。
