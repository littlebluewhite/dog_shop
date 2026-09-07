# 計畫 2 最終審查：會員、購物車結帳、運費設定

日期：2026-09-07
範圍：`4c4c339..21098f7`（分支 `worktree-mvp-design`，28 個 commit、77 個檔案、+6163/−37）
審查者：Senior Code Reviewer（整支分支、只讀）
規格權威：`docs/superpowers/specs/2026-09-06-dog-shop-mvp-design.md`；計畫：`docs/superpowers/plans/2026-09-06-dog-shop-plan-2-members-checkout.md`

---

### 總結

**Ready to merge? With fixes。** 沒有 Critical：下單交易、庫存扣減、取消歸還、訪客 token 隔離、CSRF、admin 閘門、密碼／重設 token 處理都正確而且有真的打 Postgres 的測試蓋住（101 個 Rust 測試、24 個 vitest、1 條 Playwright 主流程，在本次審查全部重跑通過）。要先修兩件 Important：`POST /api/cart/validate` 未驗證的數量在 `merge_items` 相加會整數溢位（本次審查實際打出 panic），以及 `POST /api/orders` 沒有任何濫用防護（未登入就能無限制扣庫存，而這支分支上還沒有過期 job 與後台取消，沒有恢復手段）。兩者都是 10 行以內的修正，加上控制者已排定的 Task 14／16 小修與 `/checkout` 標題修正，一波修正就能收尾。

---

### 審查方式

四個 pass，按控制者指示：

1. **`api/`**：migration `0002`、`domain/{orders, cart, settings, users, addresses, cvs_stores, jobs, password_resets}`、`auth/tokens`、`routes/{auth, me, orders, cart, checkout, admin_settings, settings}`、`error.rs`、`extract.rs`、`csrf.rs`、全部 16 個整合測試檔（含 `orders_domain.rs` 的並發測試）。
2. **`web/`**：`lib/{api, cart.svelte, checkout, validation, tw-address, labels, types}`、`AddressFields`、`InvoiceFields`、routes（register／forgot／reset／account 三頁／cart／checkout／orders/[id]／admin/settings）、`+layout.server.ts`、`hooks.server.ts`、e2e 與 playwright 設定。
3. **橫切面**：規格 §4／§5／§7／§10／§11／§14 對照、安全、資料完整性、錯誤契約、Svelte 5 SSR/CSR 守衛、文案。
4. **擱置事項 triage**：`parked-findings.md` 逐條裁決（見下）。

跑過的驗證（全部在 HEAD `21098f7`，樹乾淨）：

| 指令 | 結果 |
|---|---|
| `DATABASE_URL=…5435 cargo test` | **101 passed**（單元 40 + 整合 61，16 個整合測試檔；含 `concurrent_orders_do_not_oversell`） |
| `cargo fmt --all --check`、`cargo clippy --all-targets -- -D warnings` | 乾淨 |
| `pnpm -C web test` | 24 passed（5 個檔） |
| `pnpm -C web check` | 0 errors 0 warnings（297 files） |
| `pnpm -C web build` | 成功（只有既有的 `chunks/env.js` 空 chunk 提示） |
| `pnpm -C web test:e2e`（起了 API :8080 與 Vite :5173） | **1 passed（3.4s）** |
| dev DB（計畫 1 資料上）`_sqlx_migrations` | v1、v2 皆 `success = t`；`settings` 五把 key 都在；既有 4 筆訂單無損 |

活體探測（起 API + web dev 後做，做完已 `kill`，`lsof` 確認 :8080／:5173 釋放）：

- SSR `<title>`：`/checkout` → `dog_shop`（錯）；`/cart`、`/account`、`/account/addresses`、`/admin/settings` → 各自的標題（對）。
- `POST /api/cart/validate`，兩列同 `variant_id`、`qty: 2147483647` → API log `panicked at src/domain/orders.rs:198:34: attempt to add with overflow`，curl exit 52（連線被切）；之後 `/api/health` 仍 200（只有該連線的 task 死掉，不是整台掛掉）。
- `qty: -5` → 正常夾成 1（`merge_items` 的 `max(1)`）。
- 以 `svelte/compiler` 直接編 SSR 輸出 + 在 scratch 目錄做的獨立重現（見「`<title>` 診斷」）。

副作用揭露：e2e 在 dev DB 留下一個 `E2E 狗糧 <timestamp>` 商品與一筆已取消訂單（與 Task 16 相同）；我用 admin 登入探測多了一列 session；scratch 的 cookie 檔已 `trash`。工作樹、index、HEAD 未動。

15 分鐘上限內沒查完的：為什麼真實的 `/account/addresses`（同樣被編成 settle loop）標題正確、而我的獨立重現在同樣深度說它會錯 — 模型能解釋（見診斷節），但重現沒完全對齊；dev 模式編譯的重現因 `FILENAME` 佈線問題跑不起來，放棄。

---

### 優點

- **下單交易寫得對。** `create_order`（`api/src/domain/orders.rs:503-676`）先讀設定、驗欄位，再在一個交易內：查門市 → 依 `variant_id` 排序後逐列 `UPDATE … WHERE stock >= $2 AND p.status='active' AND v.is_active` 條件扣庫存（`:520-548`）→ 全部檢查完才一次回 `OUT_OF_STOCK` 並列出 `available` → 重算小計、20,000 上限、運費 → `orders`／`order_items`（快照）／`shipments`／`payments(pending, order_no||'01')`／`jobs` outbox 同交易。任何 `?` 都讓 `tx` drop → rollback；測試 `out_of_stock_lists_shortages_and_rolls_back`、`cvs_order_needs_store_and_respects_limit` 都驗了庫存真的回來。
- **並發測試是真的。** `concurrent_orders_do_not_oversell` 用 6 個 tokio task 對同一個 pool 搶 3 件，斷言正好 3 成功、3 個 `OutOfStock`、庫存 0、訂單 3 筆；靠的是 Postgres READ COMMITTED 下條件 UPDATE 的 EvalPlanQual 重檢，不是應用層鎖。鎖定順序統一排序（`:513`）避免死鎖。
- **取消只還一次、只從 `pending_payment`。** `cancel_in_tx`（`:780-806`）先做狀態條件式 UPDATE，`rows_affected == 0` 就不還庫存；兩個同時取消只會有一個成功，`cancel_returns_stock_once` 驗了第二次是 `VALIDATION` 且庫存不變。
- **訪客 token 隔離修對了。** `get_for_viewer`／`cancel` 的訪客分支加了 `user_id IS NULL`（`:706-707`、`:816`），`member_order_rejects_guest_token` 驗會員訂單拿 token 也是 404；`OrderRow` 用 `#[serde(skip)]` 藏 `guest_token`／`user_id`／`needs_refund`；看不到一律 404 不用 403。
- **忘記密碼照規格 §11。** 32 bytes 隨機（`auth/tokens.rs`），DB 只存 SHA-256 hex，`consume` 用單一 `UPDATE … WHERE used_at IS NULL AND expires_at > now() RETURNING` 做到原子的單次使用（`password_resets.rs:24`），重設後 `delete_all_for_user` 清全部 session；`forgot` 永遠 202、token 由計畫 3 的 job 在寄信當下才產生，DB 裡沒有明文（測試 `forgot_is_quiet_and_enqueues_only_for_known_email` 斷言 payload 沒 token）。
- **CSRF 全覆蓋。** `require_same_origin` 是 `Router` 最外層的 layer（`app.rs`），所有 POST/PUT/DELETE 含 `/api/admin/settings`、`/api/orders/{id}/cancel`、`/api/me/*` 都要 `X-Requested-With: fetch` + Origin 相符；只有 `/api/ecpay/` 前綴例外。`api()` 自動帶 header；e2e 的 `APIRequestContext` 也照帶。
- **限流共用同一個 governor。** 四條 auth 路徑 `.layer(governor_layer.clone())` 共用同一個 `GovernorConfig`（`routes/auth.rs:36-66`），`register_shares_the_login_rate_limit` 驗了跨路徑合計第 11 次 429。
- **設定型別化、壞資料不會讓頁面打不開。** `settings::parse` 把壞 JSON 退回 `Default`（有測試），`put_all` 五把 key 一個交易 upsert，公開端點只回 `shop/shipping/payment_methods`（測試斷言 `sender`／`return_store` 不外洩）。migration 種子用 `ON CONFLICT DO NOTHING`，計畫 1 資料相容（dev DB 實測）。
- **統編檢查碼是對的。** `is_tw_tax_id`（`orders.rs:157-176`）照財政部 2023 年後「總和整除 5，第 7 碼為 7 時 +1 也可」規則，前後端向量一致（`validation.test.ts` 對同一組向量）。
- **前端守衛到位。** `sessionStorage`／`localStorage` 只在 `onMount`／`$effect` 內碰；`untrack` 取初始值；購物車頁的結帳閘門要 `checked !== null && problems.length === 0`（Task 13 fix round 修好的 fail-open）；結帳頁送單用伺服器核對後的 `lines`，會員不會把 `guest_token` 放進網址；訂單頁 `+page.server.ts` 把 404／400 都轉成 404。
- **測試品質。** 每個 `#[sqlx::test]` 都在獨立臨時 DB 跑 migration，經 `Router::oneshot` 走完整 middleware；HTTP 錯誤契約（400／401／403／404／409／429、`CVS_*`）都有斷言；vitest 對 `validation.ts`、`checkout.ts`（含發票三型 payload 形狀）、`cart.update`、`ApiError.fields()`、22 縣市郵遞區號全掃。

---

### 問題

#### Critical（必修）

無。

#### Important（應修）

1. **`merge_items` 整數溢位可從未登入的 `POST /api/cart/validate` 觸發（已實際打出 panic）。**
   `api/src/domain/orders.rs:193-203`：`let qty = item.qty.max(1)` 只夾下界，`Some((_, q)) => *q = (*q + qty).min(MAX_QTY_PER_LINE)` 先加再夾。`cart::check`（`api/src/domain/cart.rs:45-48`）只檢查列數上限就直接呼叫 `merge_items`，**沒有**經過 `validate_input` 的 `1..=99` 檢查（ledger 第 73 行的裁決只推論了 `create_order` 的呼叫順序，漏掉這條路徑）。實測：兩列同 `variant_id`、`qty: 2147483647` → debug build `panicked at src/domain/orders.rs:198:34: attempt to add with overflow`、連線重置；release build（正式環境）會 wrap 成負數 → 回應裡 `qty`／`subtotal` 為負（結帳頁會把負數 `setQty` 當成移除，不會成單，但這是未登入端點回垃圾）。單一連線的 task 死掉、伺服器不倒，所以不是整站 DoS，但仍是可重現的 bug。
   **修法**（1 行 + 測試）：`let qty = item.qty.clamp(1, MAX_QTY_PER_LINE);`，之後兩個 ≤ 99 的數相加不會溢位（保留 `.min(MAX_QTY_PER_LINE)`）。在 `orders.rs` 的 `merge_items_sums_and_caps` 加 `i32::MAX` 兩列 → `(a, 99)`；在 `api/tests/cart.rs` 加 HTTP 測試：同上 body → 200、`items[0].qty == min(99, stock)`。

2. **`POST /api/orders` 沒有任何濫用防護：未登入就能無限制建立 `pending_payment` 訂單並扣庫存。**
   `api/src/routes/orders.rs:23,29-36`：`CurrentUser` 允許訪客，沒有限流、沒有每 Email／IP 的待付款訂單上限。每筆訂單最多 50 種 × 99 件；這支分支上**還沒有**過期歸還（計畫 3）也**沒有**後台取消（計畫 4），被人用腳本把庫存掃到 0 之後，老闆只能下 SQL 救。規格 §11 的限流條文只寫 `/api/auth/*`，所以這不是違規，但控制者的審查清單明列「rate limits on … order creation」，而且這是營收路徑上唯一開放給匿名者的寫入端點。
   **修法**（現在做的部分，≈12 行）：在 `routes/orders.rs` 為 `POST /api/orders` 掛一個**獨立**的 `GovernorLayer`（不要與 auth 共用配額）：`SmartIpKeyExtractor`、`per_second(12)`、`burst_size(10)`，複製 `routes/auth.rs:33-58` 的 config／`retain_recent` 背景 task／`error_handler`；測試：連打 11 次無效 body → 前 10 次 400、第 11 次 429。**注意** `api/tests/orders.rs::order_errors_over_http` 已經對同一個測試 IP 打 5 次 `POST /api/orders`，burst 別低於 6。這只是減速帶；結構性的防線（過期 job、後台取消、每 Email 待付款上限）列在計畫 3／4 交接，是**上線前必做**。

#### Minor（可選）

1. `api/src/domain/users.rs:53` `is_valid_email` 沒有長度上限（body 上限 2 MB），`users.email`／`orders.email`／`shop.contact_email` 都吃得下超長字串。加 `email.len() <= 254`。
2. 密碼沒有上限（`routes/auth.rs:141,214`、`routes/me.rs:57`）：argon2 前置 Blake2b 是線性成本，不是 DoS，但慣例上加 `<= 128` 字。
3. 金額用 `i32` 且 `products::validate`（`api/src/domain/products.rs:190`）只擋負數：`row.price * qty`（`orders.rs:566,631`、`cart.rs`）在單價 > 21,691,754 時溢位。狗用品不會，但這是計畫 1 遺留：在 `products::validate` 加 `price <= 1_000_000`（或改 `i64` 累加）。計畫 5 匯入時一併處理。
4. `cancel_in_tx` 的歸還用一句 `UPDATE product_variants v … FROM order_items oi`（`orders.rs:798-804`），鎖定順序由執行計畫決定，與 `create_order` 的 `variant_id` 排序不一致；兩者同時碰到同兩個規格時 Postgres 會偵測死鎖並讓一邊 500（可重送）。改成逐列 `ORDER BY variant_id` 迴圈就一致；計畫 3 的批次過期取消尤其要用這個版本。
5. 改密碼（`routes/me.rs:88-90`）不會登出其他裝置；規格只要求重設時清 session，但改密碼時 `delete_all_for_user` 排除目前 session 是便宜的加分。
6. 重設成功後（`routes/auth.rs:216-222`）同一使用者其他尚未用過的 reset token 仍有效到過期（多按幾次忘記密碼會有多把）；加 `UPDATE password_resets SET used_at = now() WHERE user_id = $1 AND used_at IS NULL`。
7. `forgot`（`routes/auth.rs:181-200`）`dedupe_key: None`、無每使用者節流：現在只是 `jobs` 表以 10/分/IP 長大；計畫 3 worker 上線後就是對受害者 Email 的轟炸。→ 交接（見下）。
8. `password_resets`、`cvs_store_selections`、做完的 `jobs` 沒有清理 job（migration `0002:2,129,145`）。→ 交接，併入規格 §9 的 `purge_expired_sessions`。
9. `cart::check` 每列一個查詢（最多 50）；改成 `WHERE v.id = ANY($1)` 一次查完是小優化，現在不用。
10. `orders.rs:585-596` 個人載具 `carrier_type == "1"` 時 `carrier_num` 存 `None`，但欄位 `invoice_carrier_type` 存 `"1"`；訂單頁只在 `invoice_carrier_num` 有值時顯示「載具」，否則顯示「發票會寄到 Email」— 行為對，只是資料上「綠界會員載具」與「沒選載具」看起來一樣；計畫 3 開發票時以 `carrier_type` 為準即可。
11. 前端 `checkout/+page.svelte:97-104` 每次變動就把整張表單（含 Email、手機、發票地址）寫進 `sessionStorage`，成單才清；分頁關閉即消失、同源才讀得到，可接受，但要知道。
12. 訂單頁 `PAYMENT_LABELS[o.payment?.method ?? 'credit']`：`payments.method` 允許 `'cod'`（migration `:83`）但 label 表無此 key（擱置事項，接受；第二階段做貨到付款時補）。
13. 錯誤訊息 `<span>` 沒有 `aria-describedby`／`aria-invalid`；購物車灰色「前往結帳」是 `<span>` 無 `aria-disabled`；Toasts 仍缺 `aria-live`（計畫 1 Minor 14）。
14. `account/+layout.server.ts:6` 登入導向只帶 `pathname`，丟 `?page=N`（擱置事項，接受）。
15. e2e：沒有金額斷言、種子商品不清理、多個 spec 時要 `workers: 1`（Task 16 Minor 2–4）。
16. 程式註解引用任務編號（`orders.rs:491`「Task 8 的 domain/cart.rs 也用」、`common/mod.rs`「Task 3 起可用」）— 合併後這些編號沒有意義，順手改成模組名。
17. 計畫 1 最終審查的 Minor 1、4、8、9、11～14 與 `web/README.md` 樣板都還沒動（本範圍沒碰那些檔案）；計畫 1 的兩個 Important（request span、EXIF）已在計畫 1 收尾時修好（`app.rs:55-60`、`storage/mod.rs:58-60` 確認）。

---

### 擱置事項的裁決（parked-findings triage）

依 `parked-findings.md` 的行號：

- L32 `clean_opt`／`len_between` 三處重複 → **Accept**（3 行 helper，抽共用模組反而多一層）。
- L33／L146 Playwright 不進 CI、chromium 裝在使用者快取 → **Accept**（與規格不同之處 17；快取 GC 掉舊版 chromium-1217 已在報告揭露）。
- L40 已停用但保留的規格在下一次存檔會失去 `image_id`（`ON DELETE SET NULL`）→ **Accept**（後台顯示問題；ProductForm 送出時會帶所有規格，實務上只有外部改資料才會發生）。
- L45 `password_resets::create` 用應用端 `Utc::now()`、`consume` 比 DB `now()` → **Accept**（同一台主機；差距是秒級）。
- L51 `forgot` 的 DB 往返時間側通道 → **Accept**（10/分/IP 限流；`register` 本來就會說 Email 已註冊）。
- L52 未補 expired-token／reset 路徑限流的 HTTP 測試 → **Accept**（domain 層 `expired_token_is_rejected` 已蓋；四條路徑的 layer 宣告一模一樣）。
- L56／L59 已在 fix round 處理（測試補回、`settings::get` 移除）→ **已結案**。
- L65 地址上限 count+insert 非原子 → **Accept**（自己跟自己競賽，上限是 UX 限制）。
- L66 可能沒有預設地址 → **Accept**（結帳頁用選單帶入，不要求預設）。
- L73「`merge_items` 先加再夾，但 qty 已先驗證所以不會溢位」→ **Fix-now**：`cart::check` 沒走 `validate_input`，實測 panic（Important 1）。
- L79–82 鎖定順序、超商不寫地址 → **已結案**（407f42a）。L81 沒有死鎖測試 → **Accept**（時序相依；Minor 4 順便統一取消側的順序）。
- L87 訪客 token 缺 `user_id IS NULL` → **已結案**（cbec227，有測試）。
- L90 門市 token 出現在 INFO span 的 `path` → **Accept**：span 只記 `uri().path()`（`app.rs:58`），`?t=` 的 guest token 不會進 log；門市 token 只揭露短暫的門市選擇，且路徑形狀是規格 §10 定的。計畫 4 接地圖時若改成 `?token=` 可一併消掉。
- L91 列數上限在合併前檢查 → **已結案**（540664f）。
- L98 `payments.method` CHECK 含 `'cod'` 但無 label → **Accept**（規格 §1.2 明說要預留；程式拒收 `cod`，不可達）。
- L99 `validation`／`tw-address` 測試沒斷言上界拒絕 → **Accept**（regex 有 `$` 錨定）。
- L104 `/register` 已登入導向 `/account` 早於 Task 11 → **已結案**（Task 11 落地）。
- L111 `/account` 登入導向丟 search → **Accept**（Minor 14）。
- L112 地址表單多帶 `id` → **Accept**（serde 忽略未知欄位）。
- L113 負數 page → **不成立**（`clamp_paging` 夾到 1..10000，本次確認）。
- L119–123 購物車頁 fail-open 等 → **已結案**（c87f794，本次讀碼確認 `canCheckout` 需 `checked !== null`）。
- L122 超商上限提示用本地小計 → **Accept**（結帳頁以伺服器 `checked.subtotal` 為準）。
- L124 下架列舊價格灌水小計、灰色結帳 `<span>` 無 aria → **Accept**（Minor 13；下架列在 `problems` 內會擋住結帳）。
- L134 Task 13 M4（`cvs_sub_type` radio 可能與門市不一致）→ **Plan-4 handoff**（按鈕停用中，計畫 4 接地圖時由門市回傳決定 `sub_type`）；M6 `restoreDraft` 信任草稿形狀 → **Accept**（同源 + 前後端驗證）；M8 同路由 `?store=` 變更要 `{#key}` → **Plan-4 handoff**；M7 其餘覆蓋缺口 → **Accept**（本次補齊的 `checkout.test.ts` 六個案例已涵蓋三種發票 payload 與 cvs 分支）。
- L135／L139 `/checkout` SSR `<title>` → **Fix-now**（已診斷出根因與 3 行修法，見診斷節；嚴重度 Minor，但便宜且已排進修正波）。
- L148 Task 14 M1（取消失敗留舊狀態）與 M2（`if (cancelling) return`）→ **Fix-now**（控制者已排定）；M3 焦點管理 → **Accept**。
- L156 Task 15 M2（`save()` 明確再入守衛）→ **Accept**（單一 submit 按鈕 `disabled` 已足夠）；M3 `aria-describedby` → **Accept**（Minor 13 一併）；M4 錯誤 key 字串耦合 → **Accept**（已逐一核對 13 個 key 一致）。
- Task 16 Important 1（`screenshot`／`trace`）→ **Fix-now**（控制者已排定）；Minor 1–5 → **Plan-3 handoff**（e2e 擴充時一起做）。

---

### 修正波清單（Fix wave）

一次派工、一個或多個 commit，都在本機分支、不 push。每項都要維持 `cargo test`／`clippy -D warnings`／`pnpm check` 全綠。

1. **`api/src/domain/orders.rs:196`** — `merge_items`：`let qty = item.qty.max(1);` → `let qty = item.qty.clamp(1, MAX_QTY_PER_LINE);`（其餘不動）。測試：`orders.rs` 的 `merge_items_sums_and_caps` 加兩列 `qty: i32::MAX` 同 id → `(a, 99)`；`api/tests/cart.rs` 新增 `validate_clamps_huge_quantities`：兩列 `qty: 2147483647` 同 `variant_id`（用 `common::active_product` 建的，stock 5）→ 200、`items.len()==1`、`items[0].qty == 5`。驗證：debug build 不再 panic（`cargo test` 本身就是 debug）。

2. **`api/src/routes/orders.rs:21-26`** — `POST /api/orders` 加獨立限流：照 `routes/auth.rs:33-58` 建第二個 `GovernorConfigBuilder`（`SmartIpKeyExtractor`、`.per_second(12)`、`.burst_size(10)`）+ `retain_recent` 背景 task + 同一個 `error_handler`，`.route("/api/orders", post(create).layer(order_governor))`；`GET /api/orders/{id}`、`cancel` 不限。測試：`api/tests/orders.rs` 新增 `order_creation_is_rate_limited`：連打 11 次 `{}` body → 前 10 次 400（`VALIDATION`）、第 11 次 429 `RATE_LIMITED`；並確認既有 `order_errors_over_http`（同 IP 5 次）仍過。README／`.env.example` 不必改。

3. **`web/src/routes/orders/[id]/+page.svelte:14-27`** — `cancel()`：開頭加 `if (cancelling) return;`；`catch` 內先 `toast.show(...)` 再 `confirming = false; await invalidateAll();`（失敗時重抓狀態，避免留舊畫面與確認鈕）。驗證：`pnpm -C web check` 0/0；手動：把訂單先在 DB 改成 `cancelled` 再按取消 → 出現「這筆訂單已經不能取消」且畫面刷新成已取消。

4. **`web/playwright.config.ts:9-13`** — `use` 加 `screenshot: 'only-on-failure'`、`trace: 'retain-on-failure'`。驗證：`pnpm -C web check` 過；故意讓一個斷言失敗跑一次 `test:e2e`，`web/test-results/` 出現截圖與 trace（跑完還原）。

5. **`/checkout` SSR 標題（3 行）** — `web/src/routes/+layout.svelte:22-25`：`import { page } from '$app/state';`，`<title>{page.data.title ?? data.shop.name}</title>`；`web/src/routes/checkout/+page.server.ts:17`：`return { addresses, store, title: '結帳' };`；`web/src/routes/checkout/+page.svelte:163` 刪掉 `<svelte:head><title>結帳</title></svelte:head>`。在 layout 加一行註解指到本節（Svelte 5.57 settle-loop 路徑問題）。驗證：起 API + web dev，`curl -s localhost:5173/checkout | grep -o '<title>[^<]*</title>'` → `結帳`；`/cart`、`/products` 仍是自己的標題；瀏覽器從 `/checkout` 點到 `/cart` 標題變回「購物車」（其他頁自己的 `<svelte:head>` 路徑較晚、仍會蓋過 layout）。**不要**改用「拿掉 `bind:`」的做法：Svelte 5.57 仍有 `ownership_invalid_mutation` dev 警告（`svelte/src/internal/client/dev/ownership.js:46`），子元件改 `address.city` 會每個鍵擊都警告。

（可選、若同一波有餘裕：Minor 1、2、6 各 1～2 行；不要求。）

---

### 交給計畫 3 的事項（補充計畫 6911–6935 沒寫到的）

1. **上線閘門：庫存佔用濫用。** 修正波的限流只是減速帶。計畫 3 的 `expire_unpaid_orders` 必須在上線前存在；建議討論規格 §5「沒有任何繳費期限就用 `created_at` + 3 天」是否對「連綠界頁都沒去過（沒有任何 `payments` 更新）」的訂單縮短（例如 2 小時）；計畫 4 的後台取消是老闆的手動救援；可考慮每 Email 待付款訂單上限（例如 3 筆）。
2. **`forgot` 的 Email 轟炸防護**（Minor 7）：worker 寄 `password_reset` 前檢查該使用者是否已有未用、未過期的 reset（有就跳過並把 job 標 done），或 `forgot` 改用時間分桶的 `dedupe_key`（`password_reset:{user_id}:{yyyyMMddHHmm/10}`）。`jobs.dedupe_key` 是永久 UNIQUE，**不能**用固定 key，否則使用者一輩子只能重設一次。
3. **清理 job**：與 `purge_expired_sessions` 同一個每日排程刪 `password_resets`（`expires_at < now() - 1 day`）、`cvs_store_selections`（過期）、`jobs`（`done` 超過 30 天；payload 已依計畫清成 `{}`）。
4. **批次取消的鎖定順序**（Minor 4）：過期 job 一次取消多筆時，用逐列 `ORDER BY variant_id` 的歸還，避免與下單交易互相死鎖。
5. **`orders::get_detail(db, id)`**（計畫已列）順便把 `get_for_viewer` 的四個查詢抽成共用；Email 模板要的欄位（`OrderRow` 全欄位 + items + shipment + payment）都已在。
6. **新限流器依賴 `X-Forwarded-For`**：與 auth 相同的部署前提（只有 Caddy 能連 api:8080；Caddy 預設會清掉不受信任來源的 `X-Forwarded-*`）。
7. **`<title>` 陷阱是模式問題**：任何有自己 `<svelte:head><title>` 又對子元件用 `bind:` 的**第一層頁面**都會中（`analysis.uses_component_bindings` → settle loop）。計畫 4 改結帳頁、計畫 5 匯入頁若用元件 `bind:`，沿用修正波 5 的 `page.data.title` 模式。值得回報 Svelte：`Renderer.copy()` 建的 renderer 不在 parent `#out` 裡 → `get_path()` 回 `-1` → `set_title` 排序錯。
8. **e2e 擴充**（Task 16 Minor）：會員流程（登入 → 常用地址 → 結帳「帶入」→ `/account/orders`）、金額斷言（`總計 NT$…`）、種子清理、`workers: 1`。計畫 4 有真門市後補超商流程。
9. **小型安全加分**（Minor 1、2、5、6）：Email ≤ 254、密碼 ≤ 128、改密碼登出其他裝置、重設後作廢同使用者其他 token — 可與計畫 3 的重設信一起做。
10. **計畫 1 遺留**：Minor 1、4、8、9、11～14、`web/README.md`；價格上限（Minor 3）在計畫 5 匯入前補。
11. `payments.method = 'cod'` 第二階段實作時補 `PAYMENT_LABELS` 與 `validate_input`。

---

### 驗收清單對照（items 1–10）

| # | 項目 | 狀態 |
|---|---|---|
| 1 | compose db healthy；`cargo run` 跑完 `0002` 無錯 | **自動驗證**：`#[sqlx::test]` 每個測試都在空 DB 跑兩支 migration；本次另在計畫 1 資料的 dev DB 實跑 `cargo run`（log 無 migration 錯誤，`_sqlx_migrations` v2 `success=t`，既有 4 筆訂單無損） |
| 2 | `cargo test` 全綠含並發 | **自動驗證**：101 passed，16 個整合測試檔（與計畫一致），`concurrent_orders_do_not_oversell` 通過 |
| 3 | `pnpm test`（22）、`check` 0/0、`build` | **自動驗證**：24 passed（Task 13 fix round 多 2 個）、0/0、build 成功 |
| 4 | `test:e2e` 1 passed | **自動驗證**：本次在 HEAD 重跑 1 passed（3.4s） |
| 5 | 瀏覽器手動：註冊→會員中心→地址→購物車→結帳帶入→公司發票→取消→庫存回來→我的訂單 | **需手動**：API 層全部有整合測試（profile、addresses、orders、cancel 歸還、`04595257` 統編向量），e2e 蓋了訪客宅配 + 取消；「常用地址帶入」按鈕、公司發票 UI、後台商品頁看到庫存回來、頁首「會員中心」連結要人工點一次 |
| 6 | 訪客 `?t=`，去掉變 404 | **自動驗證**（API `guest_checkout_view_and_cancel`、web `+page.server.ts` 404 對應）+ 建議手動一次 |
| 7 | `/forgot-password` 文案、`/reset/亂打` 文案 | **需手動**（API 行為有測試；頁面文案本次讀碼確認與計畫一致） |
| 8 | 後台改免運 100、關超商代碼 → 結帳運費／付款方式變 | **需手動**：API 有 `admin_can_read_validate_and_update`（public 跟著變），結帳頁的 `enabledPayments`／`shippingFee` 有 vitest；瀏覽器串起來要人工 |
| 9 | 超商：按鈕停用、送出提示「請先選擇取貨門市」 | **需手動**（`validateForm` 的 `cvs_store` 訊息有 vitest；按鈕 `disabled` 讀碼確認） |
| 10 | commit 都在 `worktree-mvp-design`、樹乾淨、沒 push | **自動驗證**：28 個 commit（16 任務 + fix round）作者皆 Wilson；`git status` 乾淨；`git branch -r` 沒有任何遠端分支 |

未達成：無。

---

### `<title>` 診斷

**現象（本次實測，Vite dev SSR）**：`/checkout` 的 SSR HTML `<title>` 是 layout 的 `dog_shop`；`/cart`、`/account`、`/account/addresses`、`/admin/settings` 都是自己的標題。Head 放置位置各頁相同（已由前次確認，本次也再看過）。

**根因鏈（Svelte 5.57.0）**：

1. `checkout/+page.svelte:231,254` 對子元件用 `bind:address={form.address}`、`bind:invoice={form.invoice}` → 編譯器 `analysis.uses_component_bindings` 為真（`svelte/src/compiler/phases/3-transform/server/transform-server.js:180-210`），整個模板被包進 legacy settle loop：`$$inner_renderer = $$renderer.copy(); $$render_inner($$inner_renderer); … $$renderer.subsume($$inner_renderer)`。用 `svelte/compiler` 直接編 SSR 確認：`/checkout` 與 `/account/addresses` 有這個 loop，`/cart`、根 layout 沒有；把兩個 `bind:` 改成一般 prop 後 loop 消失。
2. `Renderer.copy()`（`svelte/src/internal/server/renderer.js:468-474`）建的新 renderer 帶同一個 `#parent`，但**沒有被 push 進 parent 的 `#out`**；`get_path()`（`:461-463`）是 `[...parent.get_path(), parent.#out.indexOf(this)]` → 得到 `-1`。
3. `<title>` 在 SSR 不是「最後寫的贏」，而是 `Renderer.title()`（`:417-423`）把 `get_path()` 交給 `global.set_title(value, path)`（`:1043-1061`），依路徑字典序決定，只有「更晚／更深」的才蓋掉現值。頁面在 `copy()` 內寫的標題路徑是 `[…, -1, …]`，layout 的 head 是 `[…, h ≥ 0, …]`，在同一個祖先層級比較時 `-1 < h` → 頁面標題被判定為「更早」而丟棄，layout 的留下。
4. 為什麼 `/account/addresses` 沒事（最可能的解釋，未在重現中完全驗證，見下）：它多一層 `/account/+layout.svelte`。元件在 SSR 走 `Renderer.component() → child()`（`renderer.js:323,202`）各有自己的子 renderer，所以 addresses 頁的路徑是 `[…, p, -1, …]`（p = account layout 在根 layout 裡的索引，p > h），比較在 p 對 h 那一層就決定 → 頁面贏。`/checkout` 是根 layout 的第一層子頁，`-1` 直接對上 h。

**證據**：獨立重現（scratch 目錄，仿 SvelteKit 產生的 `root.svelte` 金字塔）：第一層頁 + 元件 `bind:` → `<title>LAYOUT</title>`；同頁去掉 `bind:` → `<title>PAGE</title>`。與真站 `/checkout` 一致。

**沒對齊的部分（15 分鐘上限）**：同一個重現在「第二層 + `bind:`」給的是 LAYOUT，而真站 `/account/addresses` 是自己的標題 — 模型（第 4 點）能解釋真站，但我的極簡 Root/MidLayout 顯然少了 SvelteKit 真實巢狀的某個環節；用 `dev: true` 重編的重現因 `FILENAME` 佈線跑不起來，放棄。已排除：head 放置順序、只有巢狀深度、子元件 SSR 時有無實際渲染（`{#if}` 隱藏）、綁定對象是 identifier 還是 member expression（四種變體在重現裡結果相同）。

**最小修法**：修正波 5（layout 讀 `page.data.title`，checkout 從 `+page.server.ts` 回 `title`），不改資料流、不觸發 `ownership_invalid_mutation`。替代方案「拿掉 `bind:`、靠 `$state` proxy 傳遞」在重現裡有效，但 dev 模式會對子元件的每次 `address.city = …` 印警告，不建議。

---

### 評估

**Ready to merge? With fixes。**

理由：核心承諾都達成 — 規格 §5 的條件式扣庫存與並發不超賣有真測試、§4 的買家取消只還一次、§11 的重設 token／訪客 token／CSRF／admin 閘門都正確，錯誤契約前後端對得上，計畫 1 資料相容。沒有可被利用來拿錢、拿別人資料或繞過權限的洞。兩個 Important 都是小而確定的修正：一個是實測會 panic 的整數溢位（1 行 + 測試），一個是匿名下單缺乏任何減速（沿用既有 governor 模式），加上控制者排定的 Task 14／16 小修與 3 行的 `<title>` 修正，一波派工即可。結構性的庫存佔用防線（過期 job、後台取消）屬計畫 3／4，列為上線閘門而非本計畫的阻擋項。

## 附錄 A：控制者裁決（SDD ledger 全部 `Ruling:` 行，共 42 條，依時間順序）

- Ruling: execution mode subagent-driven without asking (same as Plan 1; user said "implement" once, harness forbids blocking on a question with a reasonable default) — cost if wrong: none.
- Ruling: accept the two 3-line helper duplicates (clean_opt, len_between) rather than a shared util module; reviewers may flag as minor, defer — cost if wrong: trivial refactor later.
- Ruling: Playwright stays out of CI (deviation 17); Task 16 BLOCKED on browser download is acceptable — files still committed — cost if wrong: e2e unverified until owner runs it.
  - Ruling: stale first sentence of write_images_and_variants doc comment (products.rs:295) — carry into Task 2 dispatch as a separate first commit — cost if wrong: none (comment only)
  - Ruling: deactivated-but-kept variant can lose image_id on a later save (ON DELETE SET NULL, brief-mandated code) — park for final-review triage, admin-cosmetic only — cost if wrong: an inactive variant shows no image in admin
  - Ruling: password_resets create uses app Utc::now() while consume compares with DB now() — brief-mandated, both clocks are the same host in deployment; park, informational — cost if wrong: a reset token expires up to clock-skew seconds early/late
  - Ruling: forgot has a DB-round-trip timing side channel (brief-mandated flow) — accept for MVP: endpoint is rate-limited and the body never differs; park for final-review triage — cost if wrong: a patient attacker could infer registered emails by timing
  - Ruling: expired-token and reset-route rate-limit tests not added — expiry is covered by Task 2 domain tests, limiter wiring is identical for all four routes; no fix — cost if wrong: a wiring regression on /reset would go untested
  - Ruling: brief rewrote api/tests/settings.rs verbatim and dropped Plan 1 test migration_creates_catalog_tables (plan defect) — restore it verbatim from 52d0205 as a follow-up commit before review (fix round 1, resume implementer) — cost if wrong: none (test only)
  - Ruling: settings::get(db, key) is dead (no callers in code or in the rest of the plan) — remove it in Task 5 dispatch as a separate first commit — cost if wrong: a later task that needs a single-key read re-adds 6 lines
  - Ruling: address cap count+insert is not race-proof under READ COMMITTED — accept for MVP (a user racing their own address creation is negligible; cap is a UX limit, not a security bound) — cost if wrong: a user ends up with 11 addresses
  - Ruling: a user may end up with zero default addresses after unsetting/deleting the default — accept; checkout (Task 13) offers addresses as a picker and never requires a default — cost if wrong: one extra click at checkout
  - Ruling: is_citizen_cert byte-slices s[..2]/s[2..] and panics on multi-byte input (plan defect; is_mobile_barcode is safe since "/" is 1 byte) — fix round 1 (resume implementer): compare via as_bytes() slices and add a multi-byte negative test — cost if wrong: none
  - Ruling: merge_items adds before clamping — qty is validated (1..=MAX_QTY_PER_LINE) before merge in the documented call order, so no overflow; accept — cost if wrong: none reachable
  - Ruling: lock-order deadlock (lines locked in cart order) — fix round 1: sort merged lines by variant_id inside create_order only (NOT in merge_items: cart::check must keep cart order for the cart page) — cost if wrong: order_items are stored in variant_id order instead of cart order (cosmetic)
  - Ruling: shipments insert binds address unconditionally — fix round 1: bind address only for home delivery, mirror the cvs_store gating; extend the cvs test to send an address and assert the home columns are NULL — cost if wrong: none
  - Ruling: no deterministic deadlock test added (timing-dependent); rely on the sort invariant — cost if wrong: a regression removing the sort goes untested
  - Ruling: Task 7 get_for_viewer/cancel guest branch matches guest_token without a user_id IS NULL guard, so a member order could be viewed/cancelled with its token alone (security) — fix before review (resume Task 8 implementer): add `user_id IS NULL` to the guest branch in both queries + test that a member order 404s via token — cost if wrong: a logged-out member cannot open their own order via ?t= (must log in; acceptable)
  - Ruling: cvs store token is a path segment and lands in the INFO request span — accept for MVP: the token only reveals a short-lived store selection and grants no order access; park for final-review triage (option later: move to ?t= or redact) — cost if wrong: log readers learn which store a shopper picked
  - Ruling: cart::check merges before the MAX_LINES cap (O(n²) on an unauthenticated POST) — fix round 1 (resume implementer): check raw items.len() > MAX_LINES before merge with the same message; add a 51-line → 400 test — cost if wrong: none
  Ruling: payments.method CHECK 允許 'cod' 但 PAYMENT_LABELS 無此 key — 取貨付款是第二段功能，訂單建立驗證目前排除 cod，不可達；等第二段實作 cod 時一併補 label — 若判斷錯：前端某處顯示 undefined 標籤，肉眼可見、一行修
  Ruling: validation/tw-address 測試未斷言上界拒絕（如 6 位郵遞區號）— 測試內容為 brief 逐字規定，regex 有 $ 錨定行為安全；不加測試 — 若判斷錯：未來改寫 regex 時少一道回歸保護
  Ruling: register 頁已登入者導向 /account，但 /account 要到 Task 11 才存在 — 依計畫順序屬暫時缺口，不改 — 若判斷錯：Task 11 落地前已登入者打 /register 會看到 404，一頁、幾分鐘內消失
  Ruling: account/+layout.server.ts 登入導向只帶 pathname 不帶 search（丟失 ?page=N）— brief 逐字規定，MVP 影響僅重新登入後回第一頁；不改，列入最終審查 triage — 若判斷錯：使用者重登後少翻一頁
  Ruling: addresses 頁 form = { ...a } 多帶 id 進 AddressInput body — serde 忽略未知欄位，無害；不改 — 若判斷錯：後端若改成 deny_unknown_fields 會 400，屆時一行修
  Ruling: orders/+page.server.ts 負數 page 可透過手改 URL 傳到 API — 已查 products::clamp_paging 會 clamp 到 1..10000，不會 400/500；finding 不成立 — 若判斷錯：無
  Ruling: [critical] validate() 失敗時 checked=null → problems=[] → canCheckout=true（閘門失效開放）— 規格 §6.1 意圖是伺服器確認後才可結帳；改為 canCheckout 需 checked !== null，且 catch 時 checked=null — 若判斷錯：網路錯誤時結帳鈕灰掉，使用者按「重新確認庫存」即可
  Ruling: [important] 進行中的 validate 回應會用舊 qty 蓋掉使用者剛改的數量 — 改成只在本地 qty > item.stock 時才 setQty(stock)（等同處理 qty_reduced）— 若判斷錯：極端競態下多一次伺服器修正
  Ruling: [important] 單筆「移除」後 problems 仍含已移除列，錯誤擋住結帳 — problems 只算仍在購物車內的列 — 若判斷錯：無（純 derived 過濾）
  Ruling: [important] 超商上限提示用本地 cart.subtotal 而非 checked.cvs_limit_exceeded — 本地小計已由 cart.update 同步伺服器價格且在改數量後即時，checked.subtotal 反而會過期；提示僅供參考，結帳頁以伺服器回應為準；不改 — 若判斷錯：提示在含下架列時略高估，結帳頁會更正
  Ruling: [minor] 「+」鈕無庫存上限 — 一行加 Math.min(…, p.stock)，與競態修正一致 — 若判斷錯：無
  Ruling: [minor] 下架列舊價格灌水小計、灰色結帳用 <span> 無 aria — 接受，列入最終審查 triage
- Ruling: Task 13 fix round 1 = I1 (validate failure → checkFailed flag, persistent error + 重新確認庫存 button that re-runs validateCart, hide 小計/運費/總計 rows while checked===null), I2 (collect qty_reduced lines → persistent yellow notice 「〈品名〉庫存不足，數量已調整為 N」), M3 (goto moved outside try/catch), M5 (VALIDATION toast shows the first server message so unrendered keys still surface), + 2 tests from M7 (carrier_type "1" → carrier_num undefined; donation → love_code) — all plan-template defects on the revenue page, cheap to fix — cost if wrong: ~40-line diff to re-review
- Ruling: park M4 (cvs_sub_type radios can contradict store.sub_type; 選擇門市 button disabled in this plan) → Plan 4 handoff; M6 (restoreDraft trusts parsed shape) accepted — same-origin sessionStorage + client + server validation; M8 ({#key data.store?.token} for same-route ?store= change) → Plan 4 handoff; remaining M7 coverage gaps → final-review triage — cost if wrong: minor robustness gaps until Plan 4
- Ruling: SSR <title> on /checkout renders layout title — reviewer confirmed head placement is not the cause; implementer curls /cart and /account titles in fix round; if repo-wide → pre-existing Plan 1/Svelte issue, park to final review (cosmetic: tab title/SEO) — cost if wrong: wrong tab title until fixed
- Ruling: SSR <title> wrong ONLY on /checkout (cart/products render their own titles) — not caused by head placement; park to final whole-branch review with this evidence, cosmetic (tab title/SEO), no user-facing function affected — cost if wrong: wrong tab title on one page until Plan 3
- Ruling: Task 16 installs chromium into the default Playwright cache (~/Library/Caches/ms-playwright) instead of the brief's PLAYWRIGHT_BROWSERS_PATH=0 — the user already maintains that cache, it is the standard workflow, and it avoids 150 MB inside node_modules and an env var on every run — cost if wrong: one extra chromium build (~150 MB) in the user cache, trivially removable
- Ruling: park Task 14 M1 (failed cancel leaves stale status + confirm buttons: add invalidateAll() and confirming=false in catch — plan-mandated) and M2 (explicit `if (cancelling) return` guard) to the final-review fix wave — 2-line fixes, non-destructive today (backend UPDATE is state-checked); M3 (focus management) accepted — cost if wrong: one stale view after a raced cancel until the final wave
- Ruling: Task 15 fix round 1 = I1 (coerce shipping.cvs_fee/home_fee/free_threshold with Math.round(Number(v) || 0) into the PUT body — empty number input binds null → serde i32 rejection → generic toast instead of field red text; matches ProductForm precedent) + M1 (show 已儲存 toast before invalidateAll so a navigation error cannot mislabel a saved PUT as 儲存失敗); park M2 (explicit re-entrancy guard), M3 (aria-describedby), M4 (string-key coupling) → final-review triage — cost if wrong: ~8-line diff to re-review
- Ruling: park Task 16 I1 (add use: { screenshot: "only-on-failure", trace: "retain-on-failure" } to playwright.config.ts) to the final-review fix wave together with Task 14 M1/M2 — 1-line config, no behaviour change; M1–M5 (retry double-click theory, no money assertion, no product cleanup, workers unset, .first() on 已取消) → final-review triage — cost if wrong: thinner failure artefacts until the wave
- Ruling: fix wave (ONE dispatch, BASE 21098f7) = (1) merge_items clamps each qty to 1..=MAX_QTY_PER_LINE before summing + unit case (i32::MAX ×2 → 99) + HTTP case in tests/cart.rs; (2) dedicated GovernorLayer on POST /api/orders mirroring routes/auth.rs (SmartIpKeyExtractor, per_second(12), burst_size(10), retain_recent task, same 429 envelope) + test 10×400 then 429 — values match auth.rs, a human never places >10 orders in a burst; (3) orders/[id] cancel(): `if (cancelling) return` + catch: confirming=false, invalidateAll(); (4) playwright.config.ts use.screenshot only-on-failure + use.trace retain-on-failure; (5) <title>: root +layout.svelte uses page.data.title ?? data.shop.name, checkout/+page.server.ts returns title 結帳, remove checkout <svelte:head> — cost if wrong: ~60-line diff across api+web to re-review; rate limit too strict for a shared NAT is tunable
- Ruling: parked-findings triage adopted as written in final-review.md §擱置事項裁決 (Fix-now = the wave above; Plan-3/4 handoff = Task 13 M4/M8, Task 16 M1–M5, plus §交給計畫 3 additions; rest Accept) — cost if wrong: items resurface in Plan 3 review

## 附錄 B：修正波結果（2026-09-07）

- 修正 commit：`0b83603` fix(api) 購物車驗證數量先夾限避免整數溢位、下單端點加獨立限流；`be846c2` fix(web) 訂單頁取消失敗後重新載入、Playwright 失敗截圖與 trace、結帳頁標題改由 layout 帶入；`f4ef7e5` docs(api) 修正限流註解。
- 範圍複審（sonnet）：5 項全部 ADDRESSED，無新的 Critical/Important；1 個 Minor（限流註解把 `per_second(12)` 寫成每秒 12）已在 `f4ef7e5` 修正。
- 控制者在 `be846c2` 重跑：`cargo fmt --check`、`cargo clippy --all-targets -D warnings` 乾淨，`cargo test` 103/103；`svelte-check` 0 errors 0 warnings、vitest 24/24、`pnpm build` 成功；Playwright e2e 由修正波 implementer 在 `be846c2` 跑過 `1 passed`。
- 仍未實地驗證：Playwright 失敗時的截圖／trace 檔沒有用故意失敗的測試確認；訂單頁「已取消後再按取消」的流程沒有用瀏覽器手點；驗收清單 5、7、8、9 需要人工用瀏覽器走一次。
- 開發環境副作用：Playwright 安裝 chromium-1243 時，把快取裡沒有套件引用的舊版 chromium-1217 清掉了（其他專案下次 `playwright install` 會重抓）；dev DB 多了 e2e 留下的 `E2E 狗糧 …` 商品與已取消訂單。
