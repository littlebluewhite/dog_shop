# 計畫 5 最終審查：蝦皮匯入與部署

日期：2026-09-09
範圍：`96af124..632fdcf`（分支 `worktree-mvp-design`，16 個 commit、44 個檔案、+3964/−47；程式碼基準是計畫 4 結束的 `80509df`）。審查後的修正：修正波 `d1f3e40`（附錄 B，範圍複審乾淨）、codex 第二意見修正 `e650878` ＋ 測試註解修正 `9bf1b2a`（附錄 C，範圍複審乾淨）；程式碼最終 HEAD 為 `9bf1b2a`，本審查文件的 commit 在其之上。
審查者：Senior Code Reviewer（整支分支、只讀）
規格權威：`docs/superpowers/specs/2026-09-06-dog-shop-mvp-design.md`；計畫：`docs/superpowers/plans/2026-09-08-dog-shop-plan-5-import-deploy.md`；逐任務裁決：見附錄 A

---

### 總結

**Ready to merge? With fixes。** 沒有 Critical。這一波最該看對的四件事，三件對、一件對但沒說出口：

1. **「先整批驗證再寫入」是真的。** `import/apply.rs:56` 的 `validate_all` 在建分類、下載圖片、任何一次 INSERT／UPDATE 之前，把每個商品都跑過 `products::validate`，任一個失敗就整批 400。我用 `commit_writes_nothing_when_a_later_product_fails_validation` 之外的路徑實測過同一族行為（列錯誤擋 commit、DB 一列都沒寫）。Task 4 fix 1 消掉的是「驗證錯誤造成半套匯入」這整族問題，這是這個計畫最有價值的一次修正。
2. **「留白就保留」逐條成立，我在活的資料庫上一項一項驗過。** 先匯入 `PROBE5-A`（雞肉／牛肉兩規格＋兩張圖），手動改成 `status=active`、`slug=probe5-dog-food`、`sort_order=7`、雞肉 `stock=3`／`compare_at_price=1500`，再用「只改價格、庫存空白、描述空白、分類空白、圖片網址全部 404」的第二份工作表重匯：**slug／status／sort_order／compare_at_price／SKU／庫存／描述／分類／原圖 9 項全部保留**，價格更新、新規格「魚肉」新增。這正是規格 §13「重複匯入以 `external_ref` 更新」最容易做錯的地方，而它是對的。
3. **部署的攻擊面收得很乾淨。** `deploy/docker-compose.yml` 只有 caddy 開 80/443，api／web 沒有 `ports:`；`Caddyfile` 不設 `trusted_proxies`（限流讀的 `X-Forwarded-For` 因此不可能偽造，Task 7 用「12 個假 IP 打 login、第 11 次就 429」直接證明了）；api 映像跑非 root（uid 10001）；`config.rs` 的正式環境檢查會擋掉九個公開測試特店值（三組常數全覆蓋）、非 https、`COOKIE_SECURE=false`、沒設 SMTP、`MAIL_LOG_BODY`；所有 `bail!` 訊息都只講變數名，不印值。`deploy/`、`docs/deploy.md`、`README.md` 掃過九個 stage 字串，零命中。
4. **沒說出口的那一件：工作表沒列到的規格會被刪掉。** `products::update` 的語意是「payload 就是全部」（`domain/products.rs:419-420`：沒出現的規格有訂單引用就停用、沒有就真刪），匯入沿用它。我實測：`PROBE5-A` 有雞肉／牛肉／魚肉三個規格，上傳一份只有「雞肉」一列的**改價表**、預覽顯示「1 個商品、1 個規格」、沒有任何警告，commit 之後**牛肉與魚肉直接從資料庫消失**。這是老闆最可能做的一種表（「我只想改這幾樣的價錢」），`docs/deploy.md` §8 沒有一個字提到它，預覽頁也不提示。程式行為本身是設計決定（與規格不同之處 51 的「規格以 `(規格選項1, 規格選項2)` 對回既有 id」隱含了這件事），但**沉默**才是問題。

要修的是 3 件 Important（**兩件是手冊文字、一件是手冊文字＋一個回歸測試**，不動風險路徑）加一串很便宜的補洞：

1. 上面第 4 點：§8 要寫清楚「工作表沒列到的規格會被刪／停用」「改選項值＝刪舊建新、庫存不會跟過去」，並補一個把這個行為釘住的回歸測試。
2. **匯入期間的庫存覆寫窗口，手冊只講對一半。** `apply` 在整批圖片下載**之前**就把既有商品拍成快照（`apply.rs:56`），庫存留白時寫回的是快照值。§8 叫老闆「不要同時編輯商品或處理出貨」——但庫存也會被**客人下單**扣掉，而那是老闆停不掉的。500 商品的匯入可能跑幾十分鐘到兩小時，這段時間賣掉的量會被寫回去。
3. **重複匯入會在 `uploads` volume 留下永遠不會被清掉的舊圖檔。** 我實測：同一份檔案 commit 兩次，磁碟上的 jpg 從 6 個變 8 個，資料庫仍然只有 1 列圖片。一次 500 商品 × 9 圖的全量重匯大約留下 1～2 GB 垃圾，VPS 磁碟會被慢慢吃掉，手冊沒有提，也沒有檢查磁碟的指令。

再加上 9 個一到五行的 Minor（`restore.sh` 沒有 trap、`backup.sh` 的 `.tmp` 殘留、SIGTERM 註冊失敗的 `Err` arm、指紋比對順序、CI 的 `mode=max`、前端三個死狀態…）。全部合計 60 行以內，一波派工可以收尾；沒有任何一項需要重做設計。

**整間店可以交付嗎？** 可以，在修正波之後、而且**使用者必須自己做完驗收清單第 7 項**（拿真的蝦皮匯出檔試一次匯入、在 VPS 依 `docs/deploy.md` 部署一次、綠界 stage 走查）。理由：這支分支從來沒有在正式模式（`ECPAY_ENV=prod`）下起過一次容器——`config.rs` 的上線檢查只有單元測試覆蓋，Task 7 的煙霧測試是用 `ECPAY_ENV=stage`、`http://localhost:8081` 跑的；`uploads` volume 的寫入路徑、Let's Encrypt 憑證、真的 SMTP、真的綠界正式憑證，四樣都要等第一次部署才會被走到。手冊第 2 節第 9 步已經明白要求老闆手動補上 uploads 那一項，這是對的處理方式。

---

### 審查方式

五個 pass，照控制者指示切，全部我自己做（沒有派任何 subagent、沒有第二意見審查者）：

1. **匯入正確性與重匯的資料保留**：`domain/products.rs`、`domain/categories.rs`、`import/{mod,columns,parse,apply}.rs`、`migrations/0001_init.sql`。重點在偏離 49–54 的實作、留白欄位保留規則、`(option1, option2)` 對回既有規格、工作表沒列到的規格的下場、標題列啟發式、孤兒列、金額解析、`MAX_ROWS`、`validate_all` 乾跑、交易邊界、分類找或建的競賽、指紋契約。
2. **圖片下載**：`import/images.rs` 全文與其測試假伺服器、`storage/mod.rs`（解碼上限與 `spawn_blocking`）。重點在 SSRF 姿態、scheme 檢查、轉址上限、逾時、串流累加上限是否真的被測到、content sniff、記憶體、log 內容、錯誤對應。
3. **HTTP 介面**：`routes/admin_import.rs`、`extract.rs`、`auth/csrf.rs`、`app.rs`、`web/src/routes/admin/import/+page.svelte`、`web/src/lib/importPreview.ts(.test.ts)`、`web/src/lib/types.ts`。重點在兩條路由的 admin 守衛與 CSRF、multipart 拒絕如何變成 VALIDATION、body 上限、錯誤文案、回應形狀對「共用介面」、前端狀態機。
4. **設定／執行期／部署**：`config.rs`、`main.rs`、`jobs/worker.rs`、`.env.example`、`api/Dockerfile`、`web/Dockerfile`、兩個 `.dockerignore`、`deploy/*`、`.github/workflows/ci.yml`。重點在上線檢查是否完整且只在 prod 生效、`bail!` 訊息不外洩、SIGTERM、worker 守衛、容器安全、範本只有佔位符、備份還原腳本、healthcheck、CI job。
5. **文件與規格覆蓋**：`docs/deploy.md` 全文逐句對程式碼、`README.md`、`docs/dev/ecpay-stage.md`、規格 §13／§16／§17／§11／§2 逐條、計畫驗收清單 1–7、計畫 4 交接 1–9、`api/tests/{admin_import,import_parse,admin_products,categories,jobs_worker}.rs`。

**我自己跑過的**（都在這棵樹上）：

- `cargo fmt --check` → exit 0。
- `cargo clippy --all-targets -- -D warnings` → exit 0，零 warning。
- `cargo test --test admin_import --test import_parse --test admin_products --test categories --test jobs_worker` → 5 個 binary、**34 passed／0 failed**（6＋3＋7＋5＋13）。
- `cargo test --lib -- import:: config::` → **23 passed／0 failed**（columns 4、parse 7、images 2、config 10）。
- `pnpm -C web test` → **8 檔 38 passed**。
- 活體探測：api（`LISTEN_ADDR=127.0.0.1:8090`）＋ 本機假圖床（`127.0.0.1:8099`）＋ web dev（:5199），七份自製 xlsx、十五個請求，詳見「活體探測」。
- SIGTERM 實測：用 `kill <pid>`（SIGTERM）收掉 api，log 最後一行是 `shutting down`、行程乾淨結束。
- 獨立核對 Task 8 的全套測試 log（`/Users/wilson08/.claude/jobs/82e5d2c3/tmp/p5-task8-full-suite.log`）：**29 個 `test result` 行、合計 274 passed、29 行都是 `0 failed`**。

**我沒有跑的**：完整 `cargo test`（控制者指示不重跑，改為核對 Task 8 的 log）、`cargo build --release`、任何 `docker`／`docker compose` 指令（控制者指示只讀判斷 `deploy/`，煙霧證據以 `task-7-report.md` 為準）、`pnpm -C web check`／`build`／Playwright（Task 8 的 log 為準）、任何對外網路請求（探測用的圖片網址全部是 `http://127.0.0.1:8099/…`）。

---

### 計畫 4 交接事項對照

| # | 交接內容 | 狀態 |
|---|---|---|
| 1 | 孤兒物流單對帳程序，log 是唯一輸入；(a) 保留 log、(b) 對帳步驟、(c) 綠界後台自行處理 | **做到**。`docs/deploy.md:100-122` 第 5 節整節。六句必須保留的 log 我逐句 grep 對過原文，全部逐字命中 `api/src/routes/admin_orders.rs`（含控制者註提到、`082103c` 新增的「綠界已建單，但這次嘗試已被重新認領…」）。(a) `docs/deploy.md:71` 明寫 `RUST_LOG` 不得把 `dog_shop_api::routes` 降到 warn 以下；(b) `:119-120` 用訂單編號前綴到「物流建單及查詢」搜尋、`…L01`／`…L02` 判定孤兒；(c) `:121` 明寫本系統沒有取消物流單的功能。額外加了 `:122` 的 `raw.create_requests` 對照法。 |
| 2 | `apply_status` 回填 `ecpay_logistics_id`（已在 `082103c` 做掉，計畫 5 不必再做） | **N/A**，計畫 5 沒有動這條路徑（`git diff` 對 `domain/shipments.rs`、`routes/admin_orders.rs` 零改動）。 |
| 3 | 部署檢查清單要有「`ECPAY_ENV=prod` 且三組憑證都不是 `.env.example` 的值」 | **做到，而且做得比交接要求強**：不只寫進清單（`docs/deploy.md:66`），還在 `config.rs:158-165` 變成**啟動時的硬性檢查**（三組任一項 merchant_id／hash_key／hash_iv 等於 `STAGE_*` 常數就 `bail!`）。九個字串全部在三個常數裡（`config.rs:77,79,81`），所以檢查是全覆蓋的。 |
| 4 | Caddy 後面必須確定 `X-Forwarded-For` 是 Caddy 寫的 | **做到**。`deploy/Caddyfile:1-4` 註解說明依賴拓樸、`docker-compose.yml:1` 註解與 api／web 無 `ports:` 的事實、`docs/deploy.md:80-98` 第 4 節整節（含加 CDN 時要設 `trusted_proxies` 的完整寫法）。Task 7 另做了 12 次偽造 XFF 打 login 的實驗，第 11 次 429，直接證明偽造無效。 |
| 5 | `cvs_map_requests` 列數可當「有人在刷」的便宜指標 | **做到**。`docs/deploy.md:218-222` 給了 `docker compose exec db psql … count(*)` 的指令與解讀。 |
| 6 | 計畫 3 交接第 4 條後半段已完成 | **N/A**。 |
| 7 | `auto_complete_shipped` 不排除 `arrived`，第一批超商訂單要看一次時序 | **做到**。`docs/deploy.md:214` 第 9 節第一段，含「實務上綠界 7 天內會發退回」的順序說明與「建議手動看一次」。 |
| 8 | 儀表板 `pending_shipment` 混算宅配／超商（最便宜的改進） | **明白不做**：計畫 Global Constraints「本計畫不做」列了它，`docs/deploy.md:248` 也列在「還沒做的功能」。同意——計畫 5 的範圍已經夠滿。 |
| 9 | 開發資料庫留著計畫 4 的探測訂單，`PROBE…` 物流單號不能拿去對綠界 | **做到**：計畫「環境事實」最後一行有寫。我本次探測沒有碰任何訂單，三筆訂單狀態原封不動（見「活體探測」的副作用揭露）。 |
| 控制者註 | 第 1 條仍有效、`082103c` 新增一條必須保留的 log | **做到**，見上面第 1 列。 |

---

### 優點

- **`validate_all` 的乾跑是這個計畫最好的一個決定**（`import/apply.rs:140-156`）。它用「同一個 `build_input`、圖片給空陣列、分類給 `None`」跑真正的 `products::validate`，所以乾跑與實跑不會漂移；欄位錯誤加上 `{external_ref}.` 前綴合併成一個 `FieldErrors`（`add_prefixed_fields`），老闆一次看到所有問題而不是修一個跳一個。而且它順手把 `find_by_external_ref` 的結果帶回第二輪重用，沒有多查一次資料庫。
- **圖片下載的串流上限是真的被測到的**（`import/images.rs:94` + 測試 `/big-no-length`）。Task 3 的第一版用「宣告 20 MB、只送 1 KB」的假 header，reqwest／hyper 根本走不到那個分支；現在改用 `http-body-util` 的 `Channel`（真的沒有 `Content-Length`、真的 chunked、真的送 12.5 MB），累加分支才第一次被執行。測試檔裡那段 15 行的註解把「為什麼不能用假 Content-Length」講得比多數 production code 還清楚。
- **`fetch_all` 的順序保證**（`images.rs:139-150`）：用 `JoinSet` 併發但把結果放回 `Vec<Option<…>>` 的原索引，所以圖片順序＝工作表順序，`sort_order` 不會亂；task panic 也只降級成該張圖的 `Store` 錯誤，不會少一筆讓後面錯位。
- **解碼是安全的**：`storage::encode` 有 `Limits`（16384×16384、`max_alloc` 256 MiB）而且額外補了 `limits.reserve(decoder.total_bytes())`（`storage/mod.rs:55-57`），`save` 又整包丟 `spawn_blocking`（`:85`）。所以匯入的 6 併發解碼不會卡住 async 執行緒、也不會被一張惡意大圖爆掉記憶體。
- **`config.rs` 從 `from_env` 拆出 `from_vars(&HashMap)`** 是最小、最對的可測性改動：五個新測試全部是純函式測試，不需要動行程環境變數（那會讓平行測試互相干擾）。`prod_ok()` 用的假憑證（`1234567`／`aaaa…`）刻意不是 stage 值，所以「拒絕 stage 憑證」的測試不會偽陽性。
- **錯誤訊息的紀律**：`FetchError` 六個變體全是固定文案，外部回應內容只有 content-type（截 60 字）與狀態碼會進訊息，回應 body 一個字都不會外洩；`config.rs` 的每一句 `bail!` 只講變數名。`error_messages_never_contain_secret_values` 這個測試存在本身就是對的習慣（雖然它現在只覆蓋一條路徑，見 Minor）。
- **`deploy/` 的每個檔案都有「為什麼」的註解**，而且註解是對的：`Caddyfile` 解釋 `trusted_proxies` 的取捨、`docker-compose.smoke.yml` 解釋為什麼要另一個專案名稱（避免 `down -v` 刪掉開發資料庫——這是控制者裁決，但檔案裡留下了理由）、`api/Dockerfile:12` 解釋為什麼要裝 `curl`（compose healthcheck）。
- **`docs/deploy.md` 是真的寫給老闆看的**：每一條上線檢查都附「設錯時 log 會出現的原文」，我逐句對過 `config.rs`，**九句全部逐字相符**；第 7 節連「`git checkout` 之後 `git pull` 會失敗，因為 detached HEAD」這種只有真的踩過才會寫的細節都有；第 6 節主動揭露「備份檔名是 UTC，比台灣時間慢 8 小時」並給了換算例子。第 2 節第 9 步明白告訴老闆「本機煙霧測試沒有測到 uploads volume 能不能寫，第一次上線一定要手動做一次」——**主動揭露測試沒覆蓋到的地方**，這是這份手冊最值得稱讚的地方。
- **Task 7 的煙霧測試報告沒有粉飾**：第 4 點自己指出「log grep 證明力有限（TraceLayer 本來就不印 IP）」，然後另外設計了一個真的有證明力的限流實驗；`restore.sh` 那個 `$name（` 的位元組邊界 bug 是在測試中發現、修掉、重跑驗證的，而且報告完整記錄了第一次失敗的狀態。

---

### 問題

#### Critical（必修）

無。

#### Important（應修）

**I1. 工作表沒列到的規格會被靜默刪除；預覽不提示、手冊不提。**
`api/src/import/apply.rs:166-191`（只把工作表的列組成 `variants`）→ `api/src/domain/products.rs:420`（`for old in existing.iter().filter(|id| !kept.contains(id))`：沒有訂單引用就 `DELETE`，有就走 `:430` 的 `UPDATE product_variants SET is_active = false`）。

實測（活體探測 P7）：`PROBE5-A` 在資料庫裡有雞肉／牛肉／魚肉三個規格，上傳一份只有「雞肉」一列的改價表，預覽顯示「1 個商品、1 個規格（新增 0、更新 1）」、`errors` 空、沒有任何警告；commit 回 200 `updated: 1`；之後 `product_variants` 只剩雞肉一列，牛肉與魚肉**已從資料庫消失**。

為什麼重要：這是老闆最自然會做的一種表——「我只想調這幾樣的價錢，其他不用動」。他看到的預覽是「1 個規格」，不會意識到那代表「其他規格會被刪掉」。同一族的第二種情況是**改選項值**（「紅」改成「紅色」）：舊規格被刪、新規格被建，庫存不會跟過去（新規格庫存＝工作表值或 0），有訂單引用的舊規格則變成 `is_active=false` 的殭屍列。備份救得回來，但老闆要先發現。

我**同意程式碼不改**：`products::update` 的「payload 就是全部」語意是計畫 2 就定下的，匯入沿用它才不會出現第二套規格同步邏輯；而且「工作表列什麼就是什麼」在批次匯入的語境下是可以理解的契約。問題純粹是**沒有說**。

修法（修正波 1、2）：
1. `docs/deploy.md` §8 加一段：「**工作表沒有列到的規格會被刪掉**（有訂單引用的會改成停用、不會真刪）。所以要改價就必須把該商品**全部的規格都列出來**，不能只列要改的那幾個。另外，改「規格選項1／2」的字（例如「紅」改成「紅色」）等於**刪掉舊規格、建一個新規格**，庫存不會跟著搬過去。」
2. `web/src/routes/admin/import/+page.svelte:85` 那段說明文字加一句同樣意思的話（老闆在頁面上就看得到，不用翻手冊）。
3. `api/tests/admin_import.rs` 加一個回歸測試把這個行為釘住（現在沒有任何測試涵蓋「規格消失」這條路，改壞了不會有人知道）。

（可選、不列入必修：預覽回應多帶一個「這個商品現有 N 個規格、工作表只有 M 個」的警告。約 30 行跨 api＋web，價值真實但屬於新功能，交給老闆用過真檔之後再決定。）

**I2. 匯入期間的庫存覆寫窗口，手冊只講對一半——客人下單也會被蓋掉。**
`api/src/import/apply.rs:56`：`validate_all` 在**整批圖片下載之前**就把每個既有商品的完整狀態拍成快照（`existing_by_product`）；第二輪 `build_input` 用的是這份快照，庫存留白時寫回 `matched.stock`（`apply.rs:186`）。500 商品的匯入依 `docs/deploy.md:204` 自己的估算最壞要跑兩小時。

`docs/deploy.md:206` 現在寫的是「匯入期間請不要同時**編輯商品或處理出貨**」。但庫存的第三個變動來源是**客人下單**（`orders` 扣 `product_variants.stock`），那是老闆停不掉的：匯入跑到一半時賣掉的量，會在寫入時被快照值蓋回去，商品變成超賣。

範圍要講精確：**只影響「庫存」欄留白的規格**——填了數字的以工作表為準，本來就是老闆要的結果，沒有暴露；整份工作表都把庫存填滿的話，這個窗口等於不存在。

修法（修正波 3，純文件）：§8 那一段改成明講三個來源（老闆編輯、出貨、**客人下單**），並給老闆可執行的規避方式：**要嘛把「庫存」欄填滿**（填了就以工作表為準，不存在覆寫問題）、**要嘛挑離峰時間分批匯入**。

（可選、不列入必修：在第二輪寫入前對每個商品重查一次 `find_by_external_ref`，把窗口從「整批」縮成「單一商品的毫秒級」。約 5 行，`apply.rs:104` 那個 `match &existing` 之前多一次查詢。它不能消滅競賽（沒有鎖），只是把窗口縮小三個數量級，所以我列為可選。）

**I3. 每次重複匯入都會在 `uploads` volume 留下永遠不會被回收的舊圖檔。**
`api/src/import/apply.rs:75-88`（每次 commit 都重新下載、`storage::save` 寫新檔）→ `api/src/domain/products.rs:325`（`update` 先 `DELETE FROM product_images WHERE product_id = $1`，只刪資料列，**磁碟上的 jpg 不刪**）。

實測（活體探測 P8）：同一份蝦皮風格工作表 commit 兩次，`UPLOAD_DIR` 裡的 jpg 從 6 個變成 8 個，而 `PROBE5-C` 在資料庫裡仍然只有 1 列圖片——前一次的主圖＋縮圖成了永久孤兒。

規模：一次「500 商品 × 9 張圖」的全量重匯 = 9000 個新檔（主圖 1600px 約 200–400 KB、縮圖約 20 KB）≈ **1–2 GB 垃圾**，而且舊的一份不會消失。老闆每季重匯一次，一年就是好幾 GB；`uploads`、`pgdata`、`backups`（14 份 dump）在同一顆 VPS 磁碟上。磁碟滿了會同時弄倒資料庫與備份。

這件事的**根**是計畫 2 就有的（後台改商品圖片也一樣不刪檔），計畫 5 沒有讓它變壞，但把它從「偶爾一張」變成「一次幾千張」。

修法（修正波 3，純文件；刪檔要判斷路徑有沒有被別的商品共用，不值得在這一波動）：`docs/deploy.md` §6 或 §9 加一段「重複匯入會留下舊圖檔，磁碟會長大」，並給檢查指令：

```
docker system df -v | grep dog_shop_uploads
docker run --rm -v dog_shop_uploads:/data alpine du -sh /data
```

#### Minor（可選）

**M1. `restore.sh` 失敗時把店留在關閉狀態（`deploy/restore.sh:16-21`）。** `set -eu` 下 `pg_restore` 一失敗（檔名打錯、備份檔壞掉）腳本就結束，`docker compose start api web` 那行永遠跑不到——這在 Task 7 的煙霧測試裡**真的發生過一次**。`docs/deploy.md:145` 已經把補救寫進手冊（手動 `docker compose start api web`），所以不是沒人知道；但這是災難復原路徑，老闆執行它的時候正在慌。修法是 `docker compose stop api web` 之後加一行 `trap 'docker compose start api web' EXIT`（1 行）。

**M2. `restore.sh:19` 的 `$name` 沒有大括號也沒有引號地插進 `sh -c` 字串。** `docker compose run --rm --entrypoint sh backup -c "pg_restore … /backups/$name"`：檔名有空白會拆開、有 `;` 會在 backup 容器裡執行任意指令。檔名是操作者自己打的（不是權限邊界），所以是穩健性問題不是安全漏洞。修法：`${name}` ＋ 一個 `case "$name" in dog_shop-*.dump) ;; *) echo "檔名不正確" >&2; exit 2 ;; esac` 的白名單（3 行）。

**M3. `backup.sh:12` 的 `.dump.tmp` 在失敗時會永遠留著。** `pg_dump -Fc -f "$f.tmp" && mv "$f.tmp" "$f"`：`pg_dump` 失敗就沒有 `mv`，而第 13 行的清理只 `find -name 'dog_shop-*.dump'`，配不到 `.dump.tmp`。連續失敗一天累積一個，每個都是不完整的 dump（可能好幾 GB）。修法：`pg_dump … || { rm -f "$f.tmp"; exit 1; }`（1 行）。

**M4. `main.rs:69-76` SIGTERM 註冊失敗時，`Err` arm 會讓 future 立刻完成 → `select!` 觸發 → 剛印完 "api listening" 就關機。** 這是真的邏輯錯誤，只是觸發不了：`#[tokio::main]`（full features）下 `signal(SignalKind::terminate())` 幾乎不可能回 `Err`，而且真的發生時是「第一次部署就重啟迴圈」這種很大聲的失敗，不是靜默的。修法是 `Err` arm 印完 log 之後 `std::future::pending::<()>().await`（1 行）。**動到這裡要重跑 SIGTERM 實測。**

**M5. 指紋比對排在解析之後，空字串指紋給錯訊息（`routes/admin_import.rs:128-135`）。** 實測：`fingerprint=`（空字串）回「檔案已變更，請重新預覽」而不是「請先預覽」（`ok_or_else` 只擋 `None`，`trim()` 後的空字串是 `Some("")`）；上傳一個壞掉的檔案＋舊指紋，回的是「檔案不是 xlsx 或已損壞」而不是「檔案已變更」。兩個都是文案問題不是安全問題（指紋不符一定會被擋下來），但「檔案已變更，請重新預覽」是唯一會叫老闆重來一次的訊息，值得準。修法：`read_upload` 之後先 `let fp = fingerprint(&upload.file);`，把 `fingerprint_field` 用 `.filter(|s| !s.is_empty())` 過濾，比對通過才 `parse_and_count`（約 5 行）。

**M6. CI 的 api 映像用 `cache-to: type=gha,mode=max`（`.github/workflows/ci.yml:74`）。** `mode=max` 會把 Rust build stage 的所有中間層都推進 GitHub Actions 快取；那個 stage 有整棵 `target/`，好幾 GB。GHA 快取每個 repo 只有 10 GB，會把別的 job 的快取擠掉。修法：api 那一個改 `mode=min`（web 的 node_modules 層留 `max` 沒問題）。這個 job **從來沒有在 CI 上執行過**（分支沒 push），所以是紙上判斷。

**M7. `web/.dockerignore` 沒有 `.env.*`（只有 `.env`）。** 目前 `web/` 底下沒有任何 `.env*` 檔（`envDir: '..'`，共用根目錄的），所以現在不會出事；但 `web/Dockerfile:6` 是 `COPY . .`，哪天有人放一個 `web/.env.production` 就會進映像。修法：加一行 `.env.*`。

**M8. 匯入頁三個小狀態問題（`web/src/routes/admin/import/+page.svelte`）。**
(a) `:11` 的 `fieldErrors` 賦值了但模板從來不讀，是死狀態（三處賦值可以整組刪掉）。
(b) commit 成功之後沒有把 `preview` 設回 `null`（`:58-59`），所以「確認匯入」按鈕還在，再按一次會**再跑一次整批匯入**（重新下載全部圖片、舊檔全成孤兒，見 I3）。修法：成功後 `preview = null`（結果區塊照樣顯示，因為它看的是 `result`）。
(c) `doCommit` 開頭沒有清 `result`（`:52`），第二次 commit 失敗時，上一次的「匯入完成」綠字還留在畫面上、旁邊配一句紅色錯誤。修法：`result = null`（1 行）。
另外 `importPreview.ts:12` 的 `summarize()` 回傳的 `errorCount` 沒有任何人用（頁面自己數 `preview.parsed.errors.length`）——留著無害，但測試 `:19` 在斷言一個沒有消費者的欄位。

**M9. 幾個測試的斷言是空的或不存在。**
(a) `api/tests/admin_import.rs:488` 的 `commit_writes_nothing_when_a_later_product_fails_validation`：末尾斷言「乾跑不會建分類」（`categories_after == categories_before`），但兩個商品的 `category` 都是 `None`，所以這個斷言恆真。修法：把 `g1.category` 設成 `Some("乾跑不該建的分類")`（1 行），斷言才有意義。
(b) `api/src/config.rs:522` 的 `error_messages_never_contain_secret_values` 只覆蓋 AIO hash_key 一條路徑，而且 `!err.contains("aaaaaaaaaaaaaaaa")` 斷言的是一個**已經被測試自己覆寫掉**的值。修法：三個 prefix × 三個欄位迴圈跑一遍，斷言錯誤字串不含任何 `STAGE_*` 值也不含 `prod_ok()` 的任何值。
(c) `mark_failed_attempt` 的兩句 UPDATE（`jobs/worker.rs:80` 的 `failed` 分支、`:89` 的 `queued` 分支）各自加了 `AND status = 'running'`，但**兩句都沒有測試**——新加的 `jobs_worker` 測試只覆蓋 `mark_done`（`:68`），證明不了另外兩句。我仍列為**接受**，理由不是「與 `mark_done` 同形所以被測到了」，而是：這是同一個三字尾綴加在兩句既有 SQL 上，diff 一眼看得完（`git show c3dd4c2 -- api/src/jobs/worker.rs`），而要測它得手工佈置「job 已被 `requeue_stale` 搶走、原 worker 才回報失敗」的競賽狀態，成本高於它擋掉的風險。`is_active` 保留、「請選擇 xlsx 檔案」分支也都沒有斷言（後者我已在活體探測 P11 走過，回文正確），同樣接受。

**M10. `config.rs:305` 重複 `use std::collections::HashMap`**（檔案第 1 行已經有，`mod tests` 又 `use super::*`）。clippy 沒有抓（`-D warnings` 通過），刪一行即可。

**M11. `config.rs:177` 的 `std::env::vars()` 遇到非 UTF-8 環境變數會 panic。** 容器裡的環境是自己控制的，開發機上也極少見；真的踩到的話是啟動時一個難懂的 panic 而不是我們自己的錯誤訊息。修法是 `vars_os().filter_map(|(k, v)| Some((k.into_string().ok()?, v.into_string().ok()?)))`（2 行）。列為可選。

**M12. 「檔案有 N 列錯誤」數的是錯誤數不是列數（`routes/admin_import.rs:140`）。** 實測：一份兩列的壞檔（第 2 列有價格錯＋網址錯，第 3 列有編號錯）回「檔案有 3 列錯誤」。老闆去 Excel 找第 3 列會找錯地方。修法：改成「檔案有 N 個錯誤」（改一個字），或去重 `errors` 的 `row` 之後再數。

**M13. 續列（同一商品的第二列以後）的非規格欄位被靜默忽略（`import/parse.rs:227-229, 252`）。** 商品的描述、分類、規格名稱、圖片網址**只從建立該商品的那一列讀**；同一個商品編號後面幾列即使填了這些欄位也不會有任何效果，也不會有警告。範本的用法本來就是這樣，但第三方蝦皮外掛匯出的檔案有可能把圖片放在每一列。手冊 §8 只說「第二列以後可以不填商品編號、也不填商品名稱」，沒說「填了其他欄位也沒用」。修法：§8 補半句（純文件）。

**M14. 匯入圖片的連線失敗會把完整網址寫進 log（`import/images.rs:56`）。** 規格 §11 禁的是 HashKey／HashIV／密碼／連線字串／token／`guest_token`，商品圖片網址不在其中，而且是老闆自己貼的、對排查「為什麼這批圖沒進來」很有用。**接受**，不改。

**M15. 圖片保留時 `alt` 沿用舊的商品名稱（`import/apply.rs:92-100`）。** 商品改名之後保留下來的原圖，`alt` 還是舊名字（實測 `PROBE5-A` 改名後兩張圖的 alt 仍是「審查探測狗糧」）。純美觀，**接受**。

**M16. 5 MB 與 10 MB+64 KB 之間的檔案回「上傳格式錯誤」。** 實測：11 MB 的檔案先撞全域 `DefaultBodyLimit`，`field.bytes()` 失敗 → 「上傳格式錯誤」；5 MB 出頭的才會拿到「檔案超過 5 MB」。文案不一致但兩個都是 400 VALIDATION、都會被擋。**接受**（Caddy 那層還有 12 MB 的 `request_body max_size`）。

**M17. `MAX_PRICE` 的「9,999,999」文案在兩個地方各寫死一次**（`import/parse.rs:325`、`domain/products.rs:204,209`）。常數不會變，把它抽成格式化函式反而多一層。**接受**（＝Task 2 M8 的裁決結果）。

---

### 擱置事項的裁決（`final-review-inputs.md` 逐行）

| 來源 | 項目 | 裁決 | 去向與理由 |
|---|---|---|---|
| T1 | 4 個 Minor 都是簡報文字落差（驗證指令語法、`map_product_db_error` 描述、Files 計數、derive 描述） | 同意 | **接受**。都是簡報／報告的文字，程式碼正確。順帶更正一項給控制者：任務簡報說 T1 有「`products.external_ref` migration」，實際上**沒有新 migration**——這個欄位在 `migrations/0001_init.sql:43` 就存在（`external_ref text UNIQUE`），T1 只是把它接進 `create`／`update` 的 SQL。所以「既有資料庫的 migration 風險」這一項不存在。 |
| T1 | by-design：`find_or_create_by_name` 非原子 | 同意 | **接受**。單一 admin、匯入是序列的；真的撞上只會多一個同名分類，`docs/deploy.md:200` 已經叫老闆匯入後去分類頁看一眼。 |
| T2 fix1 | `parse_amount` 的 N/T 在任意位置被忽略（"1t2"→12） | 同意（已解決） | **接受**：fix round 1 重寫後，非數字字元一律走 `_ => return None`（`parse.rs:134`），"1t2" 現在是 `None`。全形 `ＮＴ＄` 也在前綴表裡（`:120`）。 |
| T2 | I1 空編號但有名稱的列靜默掛接 | 同意（已解決） | **接受**：`parse.rs:285-293` 現在報「這一列有商品名稱但沒有商品編號」並 `last = None`。我實測命中（活體探測 P6）。 |
| T2 | I2 `MAX_ROWS` 在濾空白列前計算 | 同意（已解決） | **接受**：`parse.rs:203-209` 先濾整列空白再比，`import_parse.rs:46` 的 `write_string(6000, 0, " ")` 回歸測試是有效的（有值但空白的格會進 range）。 |
| T2 | M4 `variants.is_empty` 死碼 | 同意（已解決） | **接受**：已不存在。 |
| T2 | M5 靜默丟棄的列仍計入 `row_count` | 同意（已解決） | **接受**：`row_count` 只算真的處理到的列（裁決如此），實測 header-only 檔 `row_count = 0`。 |
| T2 | M6 編號 > 100 字每列重複報 | 同意（已解決） | **接受**：檢查在「新商品」分支內（`parse.rs:231`），一個商品只報一次。 |
| T2 | M7 「商品圖片N」壞網址的錯誤欄名固定寫「圖片網址」 | 同意（已解決） | **接受**：`Row::image_urls()` 現在回 `(Column, String)`，錯誤用 `col.label()`（`parse.rs:257`）。 |
| T2 | M8 價格上限文案硬寫兩處 | 同意 | **接受**（Minor M17）。 |
| T2 | M9 有選項值沒名稱不報錯 | 同意（已解決） | **接受**：`parse.rs:429-449` 兩條對稱檢查都在（「有規格選項1 就要填規格名稱1」「…2」）。 |
| T2 | M10 xlsx 展開記憶體 | 同意裁決 | **接受**。admin-only、5 MB 上限；`worksheet_range` 會先展開整張表才輪到 `MAX_ROWS`，但能觸發的只有管理員自己。 |
| T2 | M11 `TEMPLATE_HEADERS` 無全對應測試 | 同意（已解決） | **接受**：`every_template_header_matches`（`columns.rs:252`）。 |
| T2 | M12 「規格1」裸別名可疑、缺「主圖」 | 同意裁決 | **接受**：裸別名已移除並有負向斷言（`columns.rs:217-218`），`主圖 → Image(1)` 已加。 |
| T2 | M13 calamine 0.36.1／rust_xlsxwriter 0.99.0 乾淨 | 同意 | **接受**（情資）。 |
| T3 | Important：串流上限零覆蓋、`/big-no-length` 名不副實 | 同意（已解決） | **接受**：新的 `/big-no-length` 用 `http_body_util::channel::Channel` 送 200 × 64 KB、不帶 `Content-Length`，`images.rs:94` 的累加分支是真的被執行的。這是本計畫修得最紮實的一項。 |
| T3 | log 含完整 url | 同意 | **接受**（Minor M14）：不是 §11 的機密，且對排查有用。 |
| T3 | body 中途失敗無 log | 同意 | **接受**：老闆看得到每張圖的警告訊息，log 再補一句價值不大。 |
| T3 | 空 Content-Type 訊息 | 同意（已解決） | **接受**：現在是「不是圖片（未標示）」，我實測命中。 |
| T3 | svg/bmp 走 Decode | 同意 | **接受**：`image/svg+xml` 通過 MIME 檢查但解碼失敗 → 「圖片無法讀取」，訊息略不精準但結論正確（本來就不該存 svg）。 |
| T3 | encode 256 MiB × 6 併發要寫進手冊 | 同意 | **修正波 3**：`docs/deploy.md:204` 現在只寫「6 個並行連線在 2 vCPU／4 GB 上沒問題」，沒提最壞情況每張圖可吃到 256 MiB。補半句。 |
| T3 | `Scheme` 在真流程碰不到 | 同意 | **接受**：`parse_grid` 已經先擋掉非 http(s)（`parse.rs:254`），`Scheme` 是 `fetch_and_store` 直接呼叫者的防線，而且有測試。 |
| T4 | M1 指紋比對應在解析前＋空字串指紋 | 同意 | **修正波 4**（Minor M5），兩者一起改。 |
| T4 | M2 existing 快照在圖片下載前拍 | **部分推翻** | **修正波 3（手冊）＋ Important I2**。原本的處置是「進手冊」，但手冊寫出來的版本只講「不要編輯商品或處理出貨」，漏掉**客人下單**這個老闆停不掉的來源——所以它不是一句話就結案的 Minor，文字必須改對。 |
| T4 | M3 偏離 51 措辭與 `build_input` 不符（程式對、措辭改） | 同意 | **接受**（措辭）：見下面 T8 那一列。 |
| T4 | M4 工作表少的既有規格會被刪／停用、停用規格回填 `is_active=false` 無註解 | **推翻（升級）** | **Important I1**。原本停放為 Minor；我實測後認為它是本計畫最可能造成老闆真實損失的行為，而且完全沉默。程式不改，但手冊＋頁面＋回歸測試三件事要做。 |
| T4 | M5 乾跑不建分類的斷言是空的 | 同意 | **修正波（Minor M9a）**，1 行。 |
| T4 | M6 `is_active` 保留無斷言、「請選擇 xlsx 檔案」分支無測試 | 同意 | **接受**：「請選擇 xlsx 檔案」分支我已在活體探測 P11 走過（回文正確）；`is_active` 保留由 `build_input` 的 `matched.map(|m| m.is_active)` 保證，補測試價值不高。 |
| T4 | M7 `validate_all` 只攔 `Validation` 變體 | 同意 | **接受**：這是**正確的**設計——其他變體（`Internal` 等）由 `?` 原樣往上丟，不會被吞成假的 400。`annotate` 也是同樣寫法。 |
| T5 | `fieldErrors` 死狀態 | 同意 | **修正波（Minor M8a）**。 |
| T5 | commit 成功後 `preview` 未設 null | 同意 | **修正波（Minor M8b）**：而且它會觸發 I3 的孤兒圖檔，值得修。 |
| T5 | `doCommit` 開頭未清 `result` | 同意 | **修正波（Minor M8c）**。 |
| T6 | M1 SIGTERM `Err` arm 完成 future | 同意 | **修正波 5（Minor M4）**，1 行，**要重跑 SIGTERM 實測**。 |
| T6 | M2 `std::env::vars()` 對非 UTF-8 panic | 同意 | **修正波（可選，Minor M11）**，2 行。 |
| T6 | M3 `error_messages_never_contain_secret_values` 半空 | 同意 | **修正波（Minor M9b）**。 |
| T6 | M4 沒有單獨拿掉 `SMTP_FROM` 的 prod 測試 | 同意 | **接受**：`SMTP_FROM` 缺少時由 `context("有 SMTP_HOST 就必須設定 SMTP_FROM")` 攔下，手冊 `:69` 也寫了這句原文。 |
| T6 | M5 `MAIL_LOG_BODY=yes` 通過 prod 檢查 | 同意（不是 bug） | **接受**：`flag_enabled` 只認 `1`／`true`，所以 `yes` 代表「沒開」，通過檢查是**正確**的——內文不會進 log。 |
| T6 | M6 `mark_failed_attempt` 競賽輸了靜默無作用 | 同意 | **接受**：`mark_done` 有 warn 就夠了（同一個現象只需要一條線索）；再加一條 warn 會在 `requeue_stale` 之後製造成對的噪音。 |
| T6 | M7 `mod tests` 重複 `use HashMap` | 同意 | **修正波（Minor M10）**，刪 1 行。 |
| T7 | M1 `restore.sh` 沒有 trap | 同意 | **修正波 6（Minor M1）**，1 行，**要重跑備份／還原煙霧**。手冊已有手動補救，但這是災難路徑。 |
| T7 | M2 `restore.sh:19` `$name` 未加大括號／未白名單 | 同意 | **修正波 6（Minor M2）**，3 行，同一次驗證。 |
| T7 | M3 `backup.sh` 失敗留 `.dump.tmp` | 同意 | **修正波 7（Minor M3）**，1 行，**要重跑 `backup.sh once`**。 |
| T7 | M4 備份檔名是容器 UTC | 同意 | **接受（手冊已寫）**：`docs/deploy.md:78,144` 兩處都講了，還附換算例子。加 `TZ=Asia/Taipei` 會讓舊檔名與新檔名混在同一個目錄裡不可比，維持 UTC 是對的。 |
| T7 | M5 dump 與 pgdata 同一台主機 | 同意 | **接受（手冊已寫）**：§6 最後一條建議每週抄一份到 VPS 以外。 |
| T7 | M6 api Dockerfile 沒有依賴預建層 | 同意 | **接受**（簡報逐字）：代價是每次改一行 Rust 就要重編全部依賴。本機實測 50 秒，VPS 上會是幾分鐘——寫進「交給上線後的事項」。 |
| T7 | M7 CI api 快取 `mode=max` | 同意 | **修正波 8（Minor M6）**，改一個字。 |
| T7 | M8 `web/.dockerignore` 缺 `.env.*` | 同意 | **修正波（Minor M7）**，1 行。 |
| T7 | M9 web runtime stage 裝零個依賴 | 同意 | **接受**：`--prod` 在全 devDependencies 的專案上是 no-op（`prepare` 靠 `|| echo ''` 接住），保留簡報逐字內容沒有壞處。 |
| T7 | M10 Caddyfile 沒有 ACME email | 同意 | **接受（手冊已寫）**：`docs/deploy.md:98` 說明怎麼加、以及不加只是收不到到期提醒。 |
| T7 | M11 `restart` 不是健康看門狗 | 同意 | **接受**：`restart: unless-stopped` 只救行程掛掉，不救「活著但不健康」。單店規模不值得加 watchdog。 |
| T7 | M12 backup sidecar 跑 root | 同意 | **接受**：新建的具名 volume 掛載點屬 root，改 `user: postgres` 會寫不進去。它不對外開埠、只連內網 db。 |
| T7 | M13 `.gitignore` 的 `deploy/backups/` 只是防禦性 | 同意 | **接受**：compose 用具名 volume，host 上本來就不會有這個目錄；留著擋「有人改成 bind mount」。 |
| T7 | ⚠️ 煙霧結果是報告文字、CI docker job 沒跑過、uploads 寫入未測、prod 拒絕路徑只有單元測試、部分證據是轉述 | 同意 | **接受，但要在交付時說出口**：見「總結」最後一段與「交給上線後的事項」。`docs/deploy.md:46` 已經主動把 uploads 那一項變成第一次部署的必做步驟，處理方式正確。 |
| T8 | `build_input` 的 `option1_name`／`option2_name` 直接照工作表，留白會清掉；偏離 51 的措辭說「規格名稱留白保留」 | **同意控制者裁決（程式對、改措辭）** | **接受**，而且我要補一個更強的理由：如果留白時回填既有的 `option1_name`，就會變成「有規格名稱、但每一列的規格選項1 都空白」，被 `products::validate` 的「有規格名稱時每一列都要填規格選項1」擋下——**老闆將永遠無法把一個多規格商品改回單規格**。所以現在的行為不只是可接受，是唯一可行的。`docs/deploy.md` §8 的「留白就保留」清單正確地沒有列規格名稱。要改的是偏離 51 的文字（把「規格名稱」從留白保留清單移出，另註「規格名稱以工作表為準」）。 |

---

### 修正波清單（Fix wave）

順序＝價值。1–8 建議必做，9–12 是很便宜的清理。

1. **`docs/deploy.md` §8（`:210` 那段之後）＋ `web/src/routes/admin/import/+page.svelte:85`：寫出「工作表沒列到的規格會被刪／停用」。**
   手冊加一段（措辭見 Important I1），頁面說明加一句「工作表沒有列到的規格會被刪除，改價請把該商品全部規格都列出來」。
   回歸測試：無（純文件）。煙霧：不需要。
2. **`api/tests/admin_import.rs`：新增 `variants_missing_from_the_sheet_are_removed`。**
   建一個兩規格商品 → 上傳只有第一個規格的工作表 → commit → 斷言 `product_variants` 只剩一列、且剩下那列的 `id` 與原本相同（對回既有 id 有效）。
   驗證：`cargo test --test admin_import variants_missing`。煙霧：不需要。
3. **`docs/deploy.md`：三段文件補正。**
   (a) `:206` 那段：庫存的變動來源要包含**客人下單**，建議「把庫存欄填滿」或「離峰分批」（Important I2）。
   (b) `:204` 那段：補一句「單張圖片解碼最多會用到約 256 MB 記憶體、最多 6 張同時，所以不要在 4 GB 以下的機器跑大批匯入」（T3 M5）。
   (c) §6 或 §9 加一段「重複匯入會在 `uploads` 留下舊圖檔」＋ `docker system df -v` / `du -sh` 檢查指令（Important I3）。
   (d) §8 補半句「同一個商品的第二列以後，只有規格選項／價格／庫存／SKU 有作用，其他欄位填了會被忽略」（Minor M13）。
   煙霧：不需要。
4. **`api/src/routes/admin_import.rs:128-135`：指紋先比、空字串當沒帶。**
   `let actual = fingerprint(&upload.file);` → `upload.fingerprint_field.filter(|s| !s.is_empty()).ok_or_else(… "請先預覽")` → 不符就回「檔案已變更，請重新預覽」 → 通過才 `parse_and_count`。
   回歸測試：`commit_is_refused_with_row_errors_or_a_stale_fingerprint` 加兩個斷言（空字串 → 「請先預覽」；壞檔＋舊指紋 → 「檔案已變更，請重新預覽」）。煙霧：不需要。
5. **`api/src/main.rs:74`：SIGTERM 註冊失敗不要讓 future 完成。**
   `Err(e) => { tracing::error!(error = %e, "無法監聽 SIGTERM"); std::future::pending::<()>().await; }`。
   回歸測試：無（不可觸發）。**要重跑 SIGTERM 實測**（起 api、`kill <pid>`、確認 log 有 `shutting down` 且行程結束）。
6. **`deploy/restore.sh:16-21`：加 trap＋收緊檔名。**
   `docker compose stop api web` 之後加 `trap 'docker compose start api web' EXIT`；第 19 行 `$name` → `${name}`，並在 `name="$1"` 之後加 `case` 白名單（只收 `dog_shop-*.dump`）。
   回歸測試：無（shell）。**要重跑 Task 7 的備份／還原往返煙霧**，而且要多測一次「故意打錯檔名 → 腳本失敗 → api／web 仍然自己起來」。
7. **`deploy/backup.sh:12`：失敗時清掉 `.tmp`。**
   `pg_dump -Fc -f "$f.tmp" || { rm -f "$f.tmp"; return 1; }` 之後才 `mv`。
   **要重跑 `backup.sh once` 與一次失敗情境**（例如把 `PGHOST` 指到不存在的主機，確認沒有殘留 `.tmp`）。
8. **`.github/workflows/ci.yml:74`：api 的 `cache-to` 改 `mode=min`。** 煙霧：不需要（CI 從未執行）。
9. **`web/src/routes/admin/import/+page.svelte`：三個狀態清理。** 刪 `fieldErrors`（`:11,21,29,54,62`）；`:59` 之後 `preview = null`；`:52` 之前 `result = null`。
   回歸測試：無新測試（`importPreview.test.ts` 不涵蓋元件）；跑 `pnpm -C web check`＋`test`。煙霧：不需要。
10. **`api/src/config.rs`：`:305` 刪掉重複的 `use std::collections::HashMap`；`:522` 的 `error_messages_never_contain_secret_values` 改成對三個 prefix、三個欄位與 `prod_ok()` 的所有值全掃。** 煙霧：不需要。
11. **`api/tests/admin_import.rs:488`：把 `g1.category` 改成 `Some("乾跑不該建的分類".into())`**，讓「乾跑不建分類」的斷言真的有東西可斷。 煙霧：不需要。
12. **`web/.dockerignore`：加一行 `.env.*`。** 煙霧：不需要（只影響映像內容；下次 `docker compose build` 自然生效）。

**可選（我不建議在這一波做，但列出來讓控制者決定）**：
- `import/apply.rs:104` 之前對每個商品重查一次 `find_by_external_ref`，把庫存覆寫窗口從「整批」縮成「單一商品」（5 行；不能消滅競賽）。
- `config.rs:177` 改 `vars_os` + `filter_map`（2 行）。
- 預覽回應加「現有 N 個規格、工作表只有 M 個」的警告（跨 api＋web 約 30 行）。

---

### 交給上線後的事項

補充計畫 3101–3109 行與 `docs/deploy.md` §9–10：

1. **這支分支從來沒有在 `ECPAY_ENV=prod` 下起過一次容器。** Task 7 的煙霧測試用的是 `stage`／`http://localhost:8081`／`COOKIE_SECURE=false`；`config.rs` 的上線檢查只有單元測試（5 個）覆蓋。第一次 `docker compose up -d --build` 就是這條路徑的首航——照 `docs/deploy.md:58` 的指示看 `docker compose logs api` 的最後幾行，訊息會逐字對得上手冊第 3 節。
2. **CI 的 `docker` job 從來沒有在 GitHub 上跑過**（分支只在本機）。第一次 push 時它是新的、未驗證的 job；本機的 `docker compose build` 成功過（Task 7），所以風險低，但別把它的第一次紅燈當成程式壞掉。
3. **uploads volume 的寫入路徑要手動驗一次**（`docs/deploy.md:46` 第 2 節第 9 步已經寫成必做步驟）。這是煙霧測試唯一沒覆蓋到、又會靜默失敗的東西。
4. **匯入的三件事要在拿到真檔之後回頭調**：(a) 別名表（預覽頁「對不上的欄位」就是回報清單，規格 §13／§17 第 6 點）；(b) 圖片是同步下載的，商品多要分批，若之後改成背景工作就得放棄 §13 的「每列即時警告」；(c) 匯入期間客人下單會被庫存快照蓋掉（修正波 3a）。
5. **磁碟要有人看**：`uploads` 每次重匯都會長大（Important I3）、`backups` 固定 14 份 dump、`pgdata` 只增不減，三者在同一顆 VPS 磁碟上。建議每季看一次 `docker system df -v`。
6. **api 映像沒有依賴預建層**（T7 M6）：每次改一行 Rust，`docker compose up -d --build` 都會重編全部依賴。本機 50 秒，2 vCPU 的 VPS 上會是好幾分鐘，更新時要有心理準備（或在本機 build 好再推 registry——那超出本計畫範圍）。
7. **長時間 commit 的中斷風險**：`apply` 是一個商品一個交易，中途斷線（老闆關掉分頁）會留下「前面幾個商品已經寫進去、畫面沒有結果」的半套狀態。`validate_all` 已經消掉「驗證錯誤造成半套」這一族，剩下的只有斷線與 DB 錯誤。手冊 §8 的「匯入中，請不要關閉頁面…」＋分批建議是目前接受的答案；真的中斷了就重跑同一份檔案（重跑是安全的：既有商品會被更新而不是重複建立）。
8. **worker 多副本**：`mark_done`／`mark_failed_attempt` 已加 `AND status = 'running'` 守衛（並有 `a_requeued_job_is_not_marked_done_by_its_stale_runner` 測試），但物流建單 2 分鐘的重認領窗口仍是設計取捨（計畫 4 交接 2）。`docs/deploy.md:232` 已寫。
9. **Cloudflare／CDN 加在 Caddy 前面時要設 `trusted_proxies`**，否則限流形同虛設（`docs/deploy.md:84-96` 有完整寫法）。（控制者註：codex S3 之後 §4 已改寫——單靠 `trusted_proxies` 不夠，還要防火牆只放 Cloudflare 的 IP、並在 `handle /api/*` 加 `header_up X-Forwarded-For {header.CF-Connecting-IP}`，改完用 §4 附的 curl 測法驗，見附錄 C。）
10. **未做**：發票 GetIssue、計畫 1／2 的小項、儀表板拆宅配／超商、Playwright 不在 CI（`docs/deploy.md:248-250` 已列）。
11. **全新部署的第一份 dump 可能只有半套 schema**（修正波實作者實測、控制者裁決不在本計畫修）：compose 的 `backup` 服務在 `up` 當下就拍第一份 dump，那時 api 的 migration 可能還沒跑完。附錄 C 的 `--single-transaction` 讓拿它還原會整筆 rollback、不再留半套資料庫，但那個檔案仍會躺在 `backups` volume 裡 14 天。還原時挑 api healthy 之後拍的 dump（`restore.sh list` 最舊那份若是部署當天的，別用）；長期修法是把 `backup` 的 `depends_on` 改成等 `api` healthy，或在 `backup.sh loop` 開頭先睡一段。

---

### 驗收清單對照（計畫 3091–3100 行，items 1–7）

| # | 項目 | 狀態與證據 |
|---|---|---|
| 1 | Rust：fmt exit 0、clippy 零輸出、完整 `cargo test` 0 failed（含點名的新測試） | **通過**。我自己重跑：`cargo fmt --check` **exit 0**；`cargo clippy --all-targets -- -D warnings` **exit 0、0 warning**。完整測試依控制者指示不重跑，改為獨立核對 Task 8 的 log：**29 個 `test result` 行、274 passed、29 行都 `0 failed`**（控制者裁決已把「≥ 30 行」改成「0 failed 且 ≥ 260 passed」，實測 29 行是全部的 binary 數）。點名的新測試我逐項對到並自己跑過：`admin_products` +2（`external_ref_is_kept_across_admin_updates`、`price_above_max_is_rejected`）、`categories` +1（`find_or_create_by_name_reuses_the_first_match`）、`import::columns` **4**（清單寫 3）、`import::parse` **7**（清單寫 5）、`import::images` 2、`admin_import` **6**（清單寫 5）、`config::` **+5（該模組共 10，另 5 個是計畫 1–4 就有的）**、`jobs_worker` +1（`a_requeued_job_is_not_marked_done_by_its_stale_runner`）。實際數量都 ≥ 清單，差額是實作者多寫的。 |
| 2 | web：check 0/0、vitest（+2）、build ok、Playwright 2 passed | **通過（部分轉述）**。我自己跑 `pnpm -C web test` → **8 檔 38 passed**（含新的 `importPreview.test.ts` 2 個）。`check`／`build`／Playwright 依 Task 8 報告（check 0 errors、build ok、Playwright 2 passed），我沒有重跑。 |
| 3 | `/admin/import` 上傳 → 預覽 → 確認匯入的煙霧 | **通過，證據比清單要求強**。清單只要求「curl SSR 200 + Task 4 的整合測試」；我兩邊都做了：SSR 頁 `GET /admin/import`（帶 admin session）**200**、頁面內容與側欄「匯入」連結都在；另外用真的 api 跑完 **7 份 xlsx、15 個請求**的完整 preview → commit 流程（見「活體探測」）。 |
| 4 | `deploy/` 煙霧（config 通過且 api／web 無 ports、build 成功、經 Caddy 200、create-admin、備份還原往返、SIGTERM graceful、`.env` 沒被 commit） | **部分通過**。docker 相關七項只有 `task-7-report.md` 的文字證據（控制者指示我不得執行 docker）——報告本身可信度高（有指令、有輸出、有自我揭露的失敗與修正、清理證據完整），但它是轉述。我自己驗到的兩項：**SIGTERM graceful 我實測過**（`kill <pid>` → log `shutting down` → 行程結束）；**`.env` 沒被 commit**（`git ls-files deploy/` 只有 7 個檔案、沒有 `.env`；工作目錄下 `deploy/.env` 不存在）。另外 `git status --short` 全綠、`git log --oneline 80509df..632fdcf` = **17 個 commit**（16 個實作＋1 個計畫）。 |
| 5 | `deploy/env.prod.example`、`docs/deploy.md`、`README.md` 不含測試特店值 | **通過**。`grep -rn` 九個字串（`3002607 2000132 2000933 pwFHCqoQZGmho4w6 ejCk326UnaZWKisg XBERn1YOvpM9nfZc EkRm7iFT261dpevs q9jcZX8Ib9LM8wYk h1ONHk4P4yqbl5LK`）掃 `deploy/`＋`docs/deploy.md`＋`README.md`：**零命中**。 |
| 6 | 與規格不同之處 49–60 都有對應實作或文件 | **通過**。49 指紋（`admin_import.rs:128-135`，實測不符回「檔案已變更，請重新預覽」）；50 任一列錯誤擋 commit（實測「檔案有 3 列錯誤…」，但 N 數的是錯誤數不是列數——Minor M12）；51 留白保留（活體探測逐項驗過；**措辭要改**，規格名稱不在保留清單裡，見 T8 裁決）；52 價格必填／上限／庫存選填／SKU ≤ 60（`parse.rs` + `products::validate`）；53 標題列啟發式與孤兒列（實測 `header_row=3`、蝦皮前兩列說明被跳過）；54 分類找或建（實測建出「審查探測分類」一個、兩個商品共用）；55 prod 檢查（5 個單元測試＋手冊第 3 節逐句對得上）；56 `LISTEN_ADDR`＋SIGTERM（兩者都實測）；57 backup sidecar（`docker-compose.yml:74-89`＋`backup.sh`）；58 `env.prod.example` 全佔位符（item 5 的 grep）；59 smoke compose＋CI docker job（存在；CI job 未執行）；60 不擋內網位址（實測：探測的圖片全部從 `127.0.0.1:8099` 下載成功）。 |
| 7 | 人工（留給使用者）：真蝦皮檔試匯入、VPS 實際部署一次、stage 走查 | **未做，依設計留給使用者**。`docs/deploy.md` §10 已把 stage 走查的待確認項與人工驗收清單寫齊；真檔匯入的回報路徑（預覽頁「對不上的欄位」）也寫進 §8。這三項是交付的前置條件，不是本次審查能代做的。 |

---

### 活體探測

**我起的東西**

| 服務 | 位址 | 說明 |
|---|---|---|
| api | `127.0.0.1:8090` | `LISTEN_ADDR=127.0.0.1:8090`（Task 6 的新功能，順便驗它）、`UPLOAD_DIR=/Users/wilson08/.claude/jobs/82e5d2c3/tmp/p5rev-uploads`（**不寫進 repo**）、`DATABASE_URL` 指開發庫 `:5435`、`ECPAY_ENV=stage`、`COOKIE_SECURE=false`。log 導到 job tmp。 |
| 假圖床 | `127.0.0.1:8099` | `python3 -m http.server 8099 --bind 127.0.0.1 --directory …/p5rev-images`，內容：`a.png`（60×40）、`b.png`（50×50）、`notimage.txt`（`text/plain`）；`missing.png` 故意不存在（404）。所有探測用的圖片網址都是 `http://127.0.0.1:8099/…`，**全程零對外請求**。 |
| web dev | `localhost:5199` | 只為了驗 SSR，`API_INTERNAL_URL=http://127.0.0.1:8090`。 |

xlsx 用自己寫的最小 zip 產生器（`p5rev-xlsx.py`，inlineStr 儲存格；本機沒有 openpyxl），七份檔案都在 job tmp，沒有進 repo。

**請求與結果**

| # | 動作 | 結果 |
|---|---|---|
| P1 | `POST /api/auth/login`（`admin@example.com`／`admin12345`，帶 `X-Requested-With: fetch`） | 200，`role: admin` |
| P2 | `POST /api/admin/import/preview`（第一份：`PROBE5-A` 兩規格＋兩圖、`PROBE5-B` 單規格） | 200。`product_count=2 variant_count=3 new=2 update=0`、`sheet=工作表1 header_row=1 row_count=3`、`errors=[]`、`unmatched=[]`。金額解析實測：`1,200`→1200、`NT$1300`→1300、`99.0`→99 |
| P3 | `POST …/commit`（帶正確指紋） | 200，`created=2 updated=0 warnings=[]`，`PROBE5-A` 2 張圖。DB 確認：兩個商品都是 `draft`、共用新建的「審查探測分類」、`PROBE5-B` 庫存留白 → 0 |
| P4 | 手動 SQL 模擬老闆：`PROBE5-A` 改 `status=active`／`slug=probe5-dog-food`／`sort_order=7`，雞肉 `stock=3`／`compare_at_price=1500` | 2 rows updated |
| P5 | preview＋commit 第二份（改名、價格改、庫存／描述／分類／SKU 留白、圖片網址全部 404、加「魚肉」） | 200，`updated=1`、1 個警告（`圖片 http://127.0.0.1:8099/missing.png 沒有匯入：下載失敗：HTTP 404`）。DB 確認**九項保留**：slug=`probe5-dog-food`、status=`active`、sort_order=7、description=`描述一`、category 不變、2 張原圖不變（path 逐字相同）、雞肉 stock=3／compare_at_price=1500／SKU=`SKU-A1`；價格 1200→1250、牛肉 stock=8、魚肉新增 |
| P6 | preview 一份有錯的檔（價格 `abc`、`ftp://` 網址、一列有名稱沒編號） | 200，三個錯誤：`第2列 價格「價格要是 0 以上的整數」`、`第2列 圖片網址「不是 http(s) 網址：ftp://x/y.png」`、`第3列 商品編號「這一列有商品名稱但沒有商品編號」`。commit → **400** `fields.rows = 檔案有 3 列錯誤，請先修正再匯入`（`PROBE5-D` 因此不存在，一列都沒寫） |
| P7 | **preview＋commit 只列一個規格的改價表**（`PROBE5-A` 只有「雞肉」） | 200，預覽「1 個商品、1 個規格」、無警告；commit 200 `updated=1`。**DB：牛肉與魚肉消失，只剩雞肉一列**（→ Important I1） |
| P8 | 同一份蝦皮風格檔 commit 第二次 | 200 `updated=1`。`UPLOAD_DIR` 的 jpg **6 → 8**，`PROBE5-C` 的圖片列數仍是 1（→ Important I3） |
| P9 | preview＋commit 蝦皮風格檔（兩列說明、`商品ID`／`售價`／`數量`／`規格名稱 1`／`規格選項 1`／`商品圖片 1`／`品牌`） | 200。`header_row=3`（兩列說明被跳過）、`unmatched=['品牌']`、全形 `３５０`→350、`360元`→360、`商品圖片 1` 的圖下載成功。commit `created=1` |
| P10 | preview＋commit 圖片全壞的檔（404＋`text/plain`） | 200，`created=1`、**兩個警告**（`下載失敗：HTTP 404`、`不是圖片（text/plain）`），商品照樣建立、0 張圖（規格 §13「失敗只記警告不擋整批」成立） |
| P11 | 錯誤路徑八連發 | 空字串指紋 → 400「檔案已變更，請重新預覽」（**應為「請先預覽」**，Minor M5）；沒帶指紋 → 400「請先預覽」；指紋不符 → 400「檔案已變更，請重新預覽」；壞檔＋舊指紋 → 400「檔案不是 xlsx 或已損壞」（**指紋比對排在解析之後**，Minor M5）；multipart 沒有 file 欄位 → 400「請選擇 xlsx 檔案」；JSON body → 400「請用 multipart/form-data 上傳」；**沒帶 `X-Requested-With` → 403「缺少 X-Requested-With header」**；**沒登入 → 401「請先登入」**；5 MB+1 → 400「檔案超過 5 MB」；11 MB → 400「上傳格式錯誤」（撞全域 body 上限） |
| P12 | preview 只有標題列的檔 | 200，`product_count=0 row_count=0 products=[]`，不 panic（前端 `canCommit` 會擋住） |
| P13 | `GET /admin/import`（web SSR，帶 admin session） | **200**，27,701 bytes，內容含「上傳 xlsx（依範本或蝦皮匯出檔）」與側欄 `href="/admin/import"` 的「匯入」連結（active 樣式）。無 session → 303（導去登入） |
| P14 | **SIGTERM 實測**：`kill $(lsof -ti :8090)` | log 最後一行 `{"level":"INFO","fields":{"message":"shutting down"},"target":"api"}`，行程結束、埠釋放。Task 6 的 SIGTERM graceful shutdown 在真行程上成立 |
| P15 | 副作用檢查 | 計畫 4 的三筆探測訂單狀態原封不動（`DS260908UHSU=completed`、`DS2609087Z2Y=shipped`、`DS2609083TN4=shipped`）；22 個 `E2E 狗糧 …` 商品未動 |

**留在開發資料庫的東西（沒有刪任何既有資料）**

- 商品 4 個，`external_ref`：**`PROBE5-A`**（審查探測狗糧（改名），探測結束後我把它從 `active` 改回 `draft`，避免壞掉的圖片路徑出現在開發前台；現在只剩「雞肉」一個規格，那是 P7 的預期結果）、**`PROBE5-B`**（審查探測玩具）、**`PROBE5-C`**（審查探測零食，2 規格）、**`PROBE5-E`**（審查探測圖片壞掉，0 圖）。`PROBE5-D` 因為列錯誤被擋，**不存在**。
- 分類 1 個：`審查探測分類`（slug `c81d28a3`，`find_or_create_by_name` 建的）。
- 這些商品的圖片路徑指向我的 job tmp 目錄（`/Users/wilson08/.claude/jobs/82e5d2c3/tmp/p5rev-uploads`），**不在 repo 也不在開發用的 `api/uploads`**，所以在開發環境打開它們會看到圖片 404——這是刻意的（不往 repo 寫檔）。四個商品都是 `draft`，不會出現在前台。

**清理**

- `lsof -ti :8090 | xargs kill`（同時當成 SIGTERM 實測）、`lsof -ti :8099 | xargs kill`、`lsof -ti :5199 | xargs kill` — 三個埠都已確認空。
- 資料庫的列全部保留（依指示不刪）。
- repo：`git status --short` **全空**（除了本審查文件之外沒有任何檔案異動）。

---

### 評估

**Ready to merge? With fixes。**

沒有 Critical。這一波的核心——「preview → commit 的指紋契約」「先整批驗證再寫入」「重匯只覆蓋工作表有填的欄位」「Caddy 是唯一入口＋正式環境拒絕測試憑證」——四件都做對了，而且我在活的資料庫與活的行程上一項一項驗過，不是靠讀程式碼推論。測試品質也是四個計畫以來最好的一次：Task 3 那個「假的 Content-Length 測不到串流上限」的修正，是把一個**看起來綠、其實零覆蓋**的測試改成真的會執行到那行程式碼，這種修正比多加十個測試更有價值。

擋在「可以直接合併」前面的是三件 Important，而它們**全部不需要改風險路徑**：兩件是手冊文字（庫存覆寫窗口要包含客人下單、重匯會留下孤兒圖檔），一件是手冊文字＋一個回歸測試（工作表沒列到的規格會被刪）。第三件是我這次審查最有價值的發現：老闆最可能做的「只列要改的那幾樣」改價表，會靜默刪掉其他規格，而預覽頁與手冊都不說。程式碼行為我同意不改（`products::update` 的「payload 就是全部」語意是計畫 2 定的，匯入沿用它才不會有兩套規格同步邏輯），但沉默必須打破。

九個 Minor 全部是一到五行，其中三個（`restore.sh` 的 trap 與檔名、`backup.sh` 的 `.tmp`）動到災難復原路徑，修完要重跑 Task 7 的備份／還原煙霧；一個（SIGTERM 的 `Err` arm）要重跑 SIGTERM 實測。其餘不需要任何煙霧。

**整間店的交付判斷**：修正波做完就可以交付，但交付的定義必須包含「使用者自己完成驗收清單第 7 項」。這支分支從頭到尾沒有在 `ECPAY_ENV=prod` 下起過一次容器，`uploads` volume 的寫入、Let's Encrypt、真 SMTP、真綠界憑證四樣都要等第一次部署才會被走到——`docs/deploy.md` 已經把這四樣分別寫成必做步驟或檢查清單，處理方式是對的，但它們是**尚未驗證的路徑**，交付說明裡要講清楚，不能讓老闆以為「本機測過了所以上線一定會過」。

---

## 附錄 A：控制者裁決（SDD ledger 全部 `Ruling` 行，共 28 條，依時間順序；前 4 條是開工前預檢衝突掃描的裁決，第 15–17 條是 Task 7／8 派工前的裁決，最後 11 條是最終審查修正波與 codex 第二意見階段的裁決）

- Ruling: 整合測試檔各自帶 `png()`／`multipart()`／`xlsx()` helper（與 tests/uploads.rs 重複）— 本 repo 慣例（每個整合測試檔自給自足，`common/mod.rs` 只放跨檔共用的）；不視為邏輯區塊逐字重複 — 代價：三處 helper 要一起維護。
- Ruling: `ApiError::Validation` 變體名與 `FieldErrors` 是否可迭代未在計畫裡驗證（error.rs 第 15–30、134–160 行）— T4 實作者先看再決定要不要加 `into_map`，不算偏離 — 代價：一次小改。
- Ruling: T5 的 `confirm()` 若與計畫 4 後台頁的兩步確認慣例不同，以既有慣例為準（計畫 Step 2 已註明）— 代價：無。
- Ruling: 圖片下載不擋內網位址（與規格不同之處 60）— 管理員自己貼的網址、測試靠 127.0.0.1；最終審查若判定要擋，只擋非 loopback 的私有網段並保留 127.0.0.1 給測試 — 代價：一次修正波項目。
- Ruling (Task 2): 計畫的 parse_amount 邏輯有缺陷（先抽數字再看小數點，"99.0" 會變 990）— 進 fix round 1 改成「整數部分／小數部分分開，小數部分必須全 0；只忽略空白、千分位逗號、NT$／＄／元；其他字元一律無效」並加 11 個單元斷言 — 若錯：多一輪複審
- Ruling (Task 2): I1 改為錯誤「這一列有商品名稱但沒有商品編號」不掛接 — 與規格不同之處 50 的「要失敗得大聲」優先於 53 的掛接規則（掛接只給沒有名稱的規格列）— 若錯：老闆多修一列
- Ruling (Task 2): I2 改為濾掉整列空白後再比 MAX_ROWS，並加 rust_xlsxwriter write_blank 的回歸測試 — 若錯：無
- Ruling (Task 2): M8 停放（Task 1 的 products::validate 同樣硬寫「9,999,999」，兩處要一起改，留最終審查）；M10 接受（admin-only、5 MB 上限）；M12 拿掉「規格1」「規格2」兩個裸別名（會落到 unmatched 讓老闆回報）、加「主圖」→Image(1)
- Ruling (Task 2 fix 2): 既有測試「孤兒列」的商品名稱格改成空字串（方案 a），第 9 列另測「有名稱沒編號」— 兩條路徑都保住覆蓋 — 若錯：無
- Ruling (Task 2 fix 2): row_count 只算真的處理到的列 — 空編號、空名稱、無規格資料但描述／分類有字的列不計、不處理（靜默丟棄是預期；老闆在預覽看不到它，但那種列沒有任何可用資訊）— 若錯：預覽列數少 1
- Ruling (Task 2): fix round 3 = (a) I2 test uses write_string(6000, 0, " ")（有值但空白的格會進 range，舊碼 data_rows.len()=6000 會 TooManyRows、新碼 non_blank=2）；(b) I1 分支加 `last = None`，讓漏編號商品後面的規格列報「接不到上一個商品」而不是掛到更前面的商品 — 若錯：多幾個孤兒錯誤，仍是大聲失敗
- Ruling (Task 3): fix round 1 = 把 /big-no-length 換成真 chunked（http-body-util channel feature；備案：裸 TcpListener 手寫 chunked 回應）讓 :91 被測到；順手 Minor 3（空 Content-Type → 「未標示」）；其餘 Minor 停放，M5 記進 Task 8 手冊（VPS 記憶體）— 若錯：無
- Ruling (Task 4): fix round 1 = (a) parse_grid 加對稱檢查「有規格名稱2 時每一列都要填規格選項2」（column 規格選項2）；(b) apply() 先對全部商品 build_input（images 先給空 vec）跑 products::validate 並用 annotate 收集，任何錯誤就在寫入或下載前整批回 400 — 消除「驗證錯誤造成部分匯入」這一族；DB 唯一鍵衝突等極少數仍可能部分匯入，記進最終審查 — 若錯：多一次 validate 的 CPU
- Ruling (Task 4): 7 Minor 全部停放（reviewer Approved 無阻擋項；M1/M5/M6 進最終修正波候選，M2 進手冊，M3 改審查文件措辭）— 避免每個 Minor 都開一輪複審 — 若錯：最終修正波多幾個小項
- Ruling (pre-Task 7): api/Dockerfile build stage must also COPY build.rs and templates/ (askama reads api/templates/mail/* at compile time — plan Step 1 omits them, build would fail) and must NOT copy rust-toolchain.toml (channel = "stable" would make rustup download a toolchain inside the image; image default 1.98 is the pinned version) — add rust-toolchain.toml and tests/ to api/.dockerignore — cost if wrong: one failed docker build, visible in the smoke log.
- Ruling (pre-Task 7): the dev DB container dog_shop-db-1 belongs to compose project `dog_shop` (deploy/docker-compose.dev.yml), the same name the plan gives the production compose file — the local smoke test must pass `-p dog_shop_smoke` on EVERY docker compose invocation (config/build/up/ps/logs/exec/run/stop/down) and the smoke override file also sets `name: dog_shop_smoke`; assert `config` prints `name: dog_shop_smoke` before `up`; `down -v` only ever with `-p dog_shop_smoke`. Production keeps `name: dog_shop` (only one compose file runs on the VPS). — why: `down -v` under project dog_shop would delete the dev DB volume (probe orders, admin) — cost if wrong: an extra flag; a missed flag destroys the dev DB, which is why it is asserted.
- Ruling (pre-Task 8): plan Task 8 Step 3.1 expects ≥ 30 `test result` lines, but the suite has exactly 29 test binaries (Task 6 full run: 29 lines, 274 passed) and Task 8 adds no tests — the binding gate is 0 failed and passed ≥ 260; the line count is informational (report the real number) — why: the 30 was a plan estimate, not a spec requirement — cost if wrong: none (the count is reported verbatim in the review doc).
- Ruling (fix wave): do all 12 items, skip the three optionals (re-query existing before write; vars_os; preview variant-count warning) — why: user Simplicity/Surgical guidelines, reviewer recommends skipping, the vars_os panic cannot occur inside the Docker image env — cost if wrong: a later 2–30 line change, no data risk.
- Ruling (fix wave re-smoke): items 6/7 are re-verified with the cached dog_shop_smoke-* images (no --build) because the wave changes no Dockerfile/compose and the backup/restore stage exercises scripts + db, not api code; api code changes are covered by the full suite + the SIGTERM live run — cost if wrong: none for the scripts; an api-image regression would surface on the first VPS build.
- Ruling (fix wave concern 1): the compose backup service dumps at first up while api runs migrations, so the very first dump of a fresh deploy can be half-schema (implementer saw pg_restore errors ignored: 4 + api restart loop when restoring it) — parked as a post-launch item in Appendix B and memory, not fixed in this wave — why: pre-existing Task 7 design, only the first minute of a fresh deploy, pruned after 14 days, fixing needs a compose change + re-smoke and the wave is closed — cost if wrong: a first-day restore from that one file fails loudly (§6 還原半路失敗 covers recovery); recommended fix later: backup depends_on api service_healthy or a sleep before the first dump.
- Ruling (codex timing): codex second-opinion review launched in parallel with the scoped re-review over 80509df..d1f3e40 — why: both are read-only on the checkout and the wave is the last planned code change; any re-review breakage and any real codex findings will go into ONE post-wave fix dispatch followed by one scoped re-review — cost if wrong: if that fix dispatch changes code, codex will not have seen the last small diff (the scoped re-review covers it).
- Ruling (codex Sp3): blank 規格名稱 with filled 規格選項 stays a loud parse error 「有規格選項1 就要填規格名稱1」, not a silent preserve — why: Task 4 M3 / Task 8 ruling (variants are rebuilt from the sheet; a multi-variant product must be able to revert to single-variant) and deviation 50 fail-loud beats the plan wording of deviation 51 — cost if wrong: the boss types the option name once more.
- Ruling (codex S4, reverses plan deviation 60): image URLs must be public — literal or DNS-resolved private (RFC1918), link-local (169.254/16, fe80::/10), unspecified, multicast, and unique-local addresses are refused for the first URL and for every redirect hop; loopback stays allowed (tests use 127.0.0.1 and loopback inside the api container is the api itself) — why: two reviews flagged it, the fix is ~60 lines, and a VPS has cloud-metadata and Docker-network neighbours — cost if wrong: a legitimate image host on a private IP cannot be used (none exists for a Shopee export).
- Ruling (codex Sp4, overturns the fix-wave optional skip): apply re-reads the existing product right before build_input (after image downloads) instead of using the pre-download snapshot, with a deterministic regression test (image server hook decrements stock mid-import) — why: two reviews (Important I2, codex P1) and the true fix (stock: Option in the domain) is far more invasive — cost if wrong: a millisecond-wide race remains between the re-read and the UPDATE (documented).
- Ruling (codex Sp5): guard memory before calamine allocates — zip entry uncompressed-size budget checked with the zip crate (already a calamine dependency, same version) before open, then a streaming worksheet_cells_reader pass bounds the sheet rectangle before worksheet_range; the business MAX_ROWS rule stays where it is (blank-row filtering unchanged) — cost if wrong: an admin sheet with cells scattered beyond the cell budget is refused with a clear message.
- Ruling (residual, zip budget): the zip budget trusts the central-directory uncompressed_size (zip-8.6.0 deflate ignores it), so a hand-crafted archive lying about its size can still make calamine load an oversized sharedStrings — accepted as residual: admin-only upload capped at 5 MB, the cell budget still bounds the dense Range (the crash codex found), and a streaming decompressed-byte counter would need a custom Read wrapper around calamine — cost if wrong: an admin uploading a deliberately lying zip can push api memory up; recorded in Appendix C for the post-launch list.
- Ruling (residual, v6 mappings): is_public_ip unwraps only IPv4-mapped IPv6; IPv4-compatible ::a.b.c.d, NAT64 64:ff9b::/96 and 6to4 2002::/16 are not translated — accepted: deprecated or need a relay absent from this deployment; the Docker network and cloud metadata are v4 and refused — cost if wrong: none in the compose deployment.
- Ruling (residual, CDN bypass): with the documented Cloudflare setup, traffic that reaches the origin directly (firewall step skipped) carries no CF-Connecting-IP, XFF becomes empty and SmartIpKeyExtractor falls back to X-Real-IP / peer IP so bypass traffic shares one bucket — informational; docs step 1 (firewall to Cloudflare ranges) forecloses it.

## 附錄 B：修正波結果與範圍複審

修正波：一次派工（opus，因第 4、5、6、7、10 項動到路由、行程訊號、部署腳本、config 測試與 CI），基底 `632fdcf`，1 個 commit `d1f3e40`（10 個檔案、+259/−37），全在本機分支、沒有 push。控制者兩條裁決：12 項全做、三個「可選」不做（寫入前重查既有商品、`vars_os`、預覽規格數警告——第一個後來被 codex 階段推翻，見附錄 C Sp4）；第 6／7 項用 Task 7 的快取映像重跑煙霧（不 `--build`，本波沒改 Dockerfile／compose）。

| # | 對應 | 內容 | 位置／測試 |
|---|---|---|---|
| 1 | Important I1 | §8 新段落寫出「工作表沒列到的規格會被刪掉（有訂單引用的改停用）」「改選項值＝刪舊建新、庫存不跟過去」；匯入頁說明加一句同義警語（原文一個位元組沒動） | `docs/deploy.md:214`、`web/src/routes/admin/import/+page.svelte:83` |
| 2 | I1 回歸測試 | `variants_missing_from_the_sheet_are_removed`：兩規格商品 → 只上傳雞肉那一列 → 直接查 `product_variants`（不帶 `is_active` 過濾，分得出真刪與停用）只剩雞肉，且剩下那列的 id 與原本相同；前置斷言兩個規格（防恆真） | `api/tests/admin_import.rs:356-385` |
| 3 | Important I2、T3 M5、Important I3、Minor M13 | (a) 庫存覆寫窗口的變動來源補上「客人下單」，建議填滿庫存欄或離峰分批；(b) 單張圖解碼約 256 MB × 最多 6 張，4 GB 以下別跑大批；(c) §9 加「重複匯入會在 uploads 留下舊圖檔」＋ `docker system df -v | grep dog_shop_uploads`／`docker run --rm -v dog_shop_uploads:/data alpine du -sh /data`；(d) 同一商品第二列以後只有規格選項／價格／庫存／SKU 有作用 | `docs/deploy.md:196,204,206-208,236` |
| 4 | Minor M5 | commit 路由指紋先比再解析：空字串指紋 → 「請先預覽」；不符 → 「檔案已變更，請重新預覽」（文案逐字未改，回應形狀不變）；`commit_is_refused_with_row_errors_or_a_stale_fingerprint` 加兩段斷言（舊碼下分別得到「檔案已變更」與 `null`，確認是真回歸測試） | `api/src/routes/admin_import.rs:126-139`、`api/tests/admin_import.rs:512-542` |
| 5 | Minor M4 | SIGTERM 註冊失敗的 `Err` arm 改成 `tracing::error!` ＋ `std::future::pending::<()>().await`，不再一啟動就關機；SIGTERM 實測重跑（`kill` 後 log 有 `shutting down`、行程結束） | `api/src/main.rs:76-77`；`p5-fixwave-sigterm-run.log` |
| 6 | Minor M1／M2 | `restore.sh`：檔名白名單擋在 `stop` 之前、`trap 'docker compose start api web' EXIT` 緊接在 `stop` 之後、`${name}`；保留結尾顯式 `start`（對已在跑的容器是 no-op）；煙霧：往返成功、不存在的檔名 → 腳本 exit 1 且 api／web 由 trap 開回、`evil.txt` 在停機前被拒 | `deploy/restore.sh:18-29`；`p5-fixwave-smoke-restore-*.log` |
| 7 | Minor M3 | `backup.sh`：`pg_dump … \|\| { rm -f "$f.tmp"; return 1; }` 之後才 `mv`；`loop` 模式失敗只印 `backup failed` 不會讓容器退出，`once` 模式 exit 非 0；順帶修掉舊碼「pg_dump 失敗仍印 `backup ok` 並 exit 0」（`&&` 串列讓 `set -e` 失效，等於每日備份可能長期假成功）；煙霧：`once` 成功、`PGHOST=no-such-host PGCONNECT_TIMEOUT=5` 失敗後沒有任何 `.tmp` | `deploy/backup.sh:9-17`；`p5-fixwave-smoke-backup.log` |
| 8 | Minor M6 | CI 的 api 映像 `cache-to` 改 `mode=min`（web 不動） | `.github/workflows/ci.yml` |
| 9 | Minor M8 | 匯入頁刪死狀態 `fieldErrors`、commit 成功後 `preview = null`（結果區塊與預覽區塊是兄弟節點，不會被藏起來）、`doCommit` 開頭 `result = null` | `web/src/routes/admin/import/+page.svelte:50-57` |
| 10 | Minor M9b／M10 | 刪 `mod tests` 重複的 `use std::collections::HashMap`；`error_messages_never_contain_secret_values` 對 AIO／INVOICE／LOGISTICS 各觸發一次拒絕，掃 8 個機密欄位（三組 HASH_KEY／HASH_IV、SMTP_PASS、DATABASE_URL）與 9 個 stage 字串，刻意不掃 `ECPAY_ENV=prod`（每句訊息都合法含它）；`prod_ok()` 假值改成獨一無二並新增 `SMTP_PASS`；臨時洩漏實驗證明兩半掃描都會紅 | `api/src/config.rs:409,531-580` |
| 11 | Minor M9a | `g1.category` 給值，「乾跑不建分類」的斷言有東西可斷 | `api/tests/admin_import.rs:641` |
| 12 | Minor M7 | `web/.dockerignore` 加 `.env.*`（web 只用 `$env/dynamic/*`，build 不需要任何 `.env` 檔，純未來防護） | `web/.dockerignore:7` |

閘門（`d1f3e40`）：`cargo fmt --check` exit 0；`cargo clippy --all-targets -- -D warnings` 零輸出；完整 `cargo test` 29 個 `test result` 行、275 passed、0 failed（基準 274 ＋ 新測試 1）；`pnpm check` 0 errors、vitest 38 passed、`pnpm build` 成功；Playwright 刻意略過（沒有 e2e 碰 `/admin/import`，web 改動只有匯入頁狀態與一句文案）。R2：`dog_shop-db-1` 仍 healthy、`dog_shop_pgdata_dev` 仍在、`dog_shop_smoke` 專案 `down -v` 後無殘留、`deploy/.env` 已 trash、`git status` 只剩本審查文件。

範圍複審（sonnet，`review-632fdcf..d1f3e40.diff`）：**12 項全部 ADDRESSED，沒有新的 Critical／Important**。逐項另外確認了：指紋雜湊只對已過 5 MB 上限的內容計算（沒有新的無上限路徑）；預覽路由未動；config 測試沒有弱化其他 4 個 `prod_ok()` 測試（拒絕 stage 憑證的測試仍自己餵 stage 值）；`.env.*` 忽略不影響 web build。一個新 Minor：`restore.sh:16-17` 的註解宣稱白名單擋得住空白與分號，但 `case` 的 `*` 什麼都配——併入附錄 C 的 C2 一起修。六個偏離全部接受：§6「還原半路失敗」改寫是清理第 6 項造成的失準；3(c) 放 §9 是清單允許的；保留結尾 `start` 無害；第 2 項的「先紅」用錯誤期望值證明斷言非恆真（程式本就不改）；`prod_ok()` 換值＋`SMTP_PASS` 是掃描所需；煙霧 happy path 改用 api healthy 之後拍的 dump 是測試佈置修正。

實作者提出、控制者裁決的新事項：compose 的 `backup` 服務在 `up` 當下就拍第一份 dump，會與 api 的 migration 打對台——全新部署的第一份 dump 可能只有半套 schema（實測拿它還原會 `errors ignored on restore: 4` 並讓 api 進重啟迴圈）。裁決：不在本波修（既有 Task 7 設計、只發生在全新部署的第一分鐘、14 天後被清；改 compose 要重建重測）；列為「交給上線後的事項」第 11 條：建議 `backup` 的 `depends_on` 改成等 `api` healthy（或 `loop` 開頭先睡一段），還原時挑 api healthy 之後拍的 dump。附錄 C 的 C1（`--single-transaction`）讓這份半套 dump 的還原會整個 rollback、不再留半套資料庫。其餘實作者疑慮（CI `docker` job 從未在 GitHub 跑過、SIGTERM `Err` arm 無法觸發、`ApiError.fields()` 仍有六個呼叫者、`ECPAY_ENV=prod` 從未起過容器）皆與正文「交給上線後的事項」一致，無新動作。

## 附錄 C：codex 第二意見審查與修正（`/codex-review-fix`）

依使用者指示，計畫收尾時用 `codex exec -s read-only`（codex CLI 0.153.4、model `gpt-6-astra`、reasoning `xhigh`、sandbox read-only）對 `80509df..d1f3e40`（計畫 5 全部 commit，含修正波）做一次只讀的第二意見審查，與範圍複審並行（兩者都唯讀）；codex 用既有編譯產物跑了 24 個測試與 shell 語法檢查，沒有改任何檔案、沒有跑 docker、沒有網路請求，約 5 分鐘。codex 回報 **10 條（安全 5：P1×2、P2×3；規格 5：P1×2、P2×3）**，並確認 api／web 非 root、沒有開埠、正式範本沒有 stage 憑證、multipart／指紋／admin／CSRF 沒有額外問題。控制者逐條打開引用的 file:line 對照程式碼、規格與既有裁決後，**9 條判定為真、1 條維持既有裁決**，一次派工（opus）修正並附回歸測試與備份／還原煙霧，commit `e650878`：

| # | 優先 | codex 的發現 | 控制者判定 | 修正（`e650878`） |
|---|---|---|---|---|
| S1 | P1 | `restore.sh:25` 的 trap 在 `pg_restore --clean` 失敗後無條件重啟 api／web，而 `--clean` 沒有交易保護：半路失敗會讓店對著「DROP 了一半」的資料庫營業 | 真。修正波第 6 項只解了「別把店留在關閉狀態」，沒想到半套資料庫比關店更糟 | `pg_restore` 加 `--single-transaction`（隱含 `--exit-on-error`：失敗整個 rollback，資料維持還原前狀態，trap 重啟因此安全）；檔名改成直接當 `pg_restore` 的參數，不再經過 `sh -c`；§6「還原半路失敗」改寫 |
| S2 | P2 | `restore.sh:19` 的白名單 `dog_shop-*.dump` 接受空白與 shell 中繼字元，再被塞進 `sh -c` 字串 | 真（範圍複審也抓到同一處註解高估）。操作者自己輸入的參數，不是攻擊面，但便宜且註解不實 | `case` 改成精確對應 `backup.sh` 產生的形狀 `dog_shop-YYYYMMDD-HHMMSS.dump`（拒絕訊息不變）；配合 S1 不再進任何 shell 字串；煙霧加三個白名單探測（`evil.txt`、含 `;` 的檔名、位數不對的檔名） |
| S3 | P2 | `docs/deploy.md` §4 的 Cloudflare 建議只加 `trusted_proxies`：Cloudflare 是附加型 CDN，客戶端先塞的假 IP 會排在 `X-Forwarded-For` 第一個，而 api 的 `SmartIpKeyExtractor` 正好取第一個 | 真（`auth.rs:37`、`rate_limit.rs:20` 都用 `SmartIpKeyExtractor`）。預設部署（沒有 CDN）不受影響 | §4 改寫：防火牆只放 Cloudflare 的 IP 連 80／443；`handle /api/*` 的 `reverse_proxy` 加 `header_up X-Forwarded-For {header.CF-Connecting-IP}`；改完用 §4 附的 curl 限流測法（12 個不同假 IP 打登入，第 11 次要 429）實際驗一次；`Caddyfile:4` 註解指向 §4（指令本身不動） |
| S4 | P1 | `images.rs:51` 只檢查 scheme：私有網段、loopback、link-local、DNS 解到內網的目的地都連得到，轉址也不檢查；MIME 檢查在請求之後 | 真。計畫「與規格不同之處 60」明文允許（管理員自己貼的網址），最終審查也接受；但 VPS 上有 cloud metadata 與 Docker 網路鄰居，兩份審查都點名、修法約 60 行——**推翻 60**（裁決見附錄 A）。loopback 保留：測試靠 127.0.0.1，而容器裡的 loopback 就是 api 自己 | `is_public_ip` 純函式（拒 RFC1918、169.254/16、100.64/10、fe80::/10、fc00::/7、unspecified／multicast／broadcast，IPv4-mapped 轉 v4 判）；第一個網址與每一跳轉址都先 `lookup_host` 全部解析結果必須公開；轉址改成手動迴圈（最多 3 跳，「轉址超過 3 次」）；新 `FetchError::NotPublic`「圖片網址不是公開網址」；§8 一句；DNS 重綁接受為已知殘餘風險 |
| S5 | P2 | `images.rs:55` 把完整網址與 `reqwest::Error`（Display 含網址）寫進 log；帶簽名 token 的圖片網址會進正式 log，違反 §11 | 真 | log 只記 host，錯誤用 `without_url()`；`import/` 底下 grep 確認沒有其他把網址寫進 `tracing::` 的地方 |
| Sp1 | P2 | `apply.rs:188` 一律 `image_path: None`：`products::update` 每次重建圖片列，連「原圖保留」的純改價匯入也把老闆在後台設的規格圖片對應清掉 | 真。「留白就保留」的原則漏了這一項（最終審查活體探測 9 項保留清單沒含規格圖片） | 對回既有規格時，若它的 `image_id` 對應的 path 也在這次要寫入的圖片裡就保留（整組換新圖時自然變 `None`，換圖後要回後台重指定）；回歸測試；§8 半句 |
| Sp2 | P2 | `apply.rs:146` 的 `validate_all` 只驗商品不驗分類：第 2 個商品的分類名太長時第 1 個已寫入，違反「先全部驗證再寫入」 | 真 | `categories::validate_name` 改 pub，`validate_all` 對每個有分類的商品呼叫並併進同一個 `FieldErrors`；回歸測試（`products`／`categories` 都零新列） |
| Sp3 | P2 | `parse.rs:429` 對「有規格選項1、沒規格名稱1」直接報錯，沒有用既有商品的名稱補——與規格不同之處 51 說的「保留」不同 | **不採**。Task 4 M3／Task 8 的裁決：規格由工作表整組重建，名稱跟著工作表（否則多規格商品永遠變不回單規格）；錯誤訊息「有規格選項1 就要填規格名稱1」明確，屬「要失敗得大聲」（與規格不同之處 50），不是靜默資料流失；審查文件已把 51 的措辭修正 | 無 |
| Sp4 | P1 | `apply.rs:186` 用整批下載圖片**之前**的快照寫庫存：結帳把 10 扣成 9，匯入 commit 又寫回 10 | 真。最終審查 Important I2 只要求手冊說明（修正波 3a），「寫入前重查」列為可選未做；兩份審查同指一處、5 行可解——**推翻可選不做**（裁決見附錄 A）。完整解法（庫存 `Option` 進 domain）改動面太大，不做 | `validate_all` 不再回傳快照；每個商品在圖片下載完、`build_input` 之前重讀既有商品；殘餘只有重讀到 UPDATE 之間的毫秒級間隙；確定性回歸測試（圖片伺服器被打到時先扣一個庫存，斷言寫回的是 9 不是 10）；§8 窗口說明改寫 |
| Sp5 | P1 | `parse.rs:97` 在檢查任何上限之前就 `worksheet_range`：calamine 的 `Range` 是稠密矩形，一個只有 `A1` 與 `XFD1048576` 兩格的迷你檔會嘗試配 170 億格，api 行程 abort、容器重啟 | 真。5 MB 上傳上限擋不住高壓縮比與稀疏遠格；管理員專用但一發就讓正在營業的 api 重啟 | 兩道預算都在既有邏輯之前：`zip` crate（calamine 的既有依賴、同版）讀 central directory 的解壓後大小（單一 entry 32 MiB、總和 64 MiB）；`worksheet_cells_reader` 串流量宣告矩形與實際最大 row／col（200 萬格）；超過回新變體 `ImportError::TooBig`「檔案太大或格子太多（解壓後超過 64 MB，或範圍超過 200 萬格），請刪掉多餘的欄與列再上傳」；MAX_ROWS 與空白列規則不動；測試含遠格檔 5 秒內拒絕 |

修正派工：一次（opus），基底 `d1f3e40`，1 個 commit `e650878`（11 個檔案、+703/−53：`Cargo.toml`／`Cargo.lock` 新增直接依賴 `zip = { version = "8.6", default-features = false }`；`images.rs` +273、`parse.rs` +85、`apply.rs`、`categories.rs`、兩個測試檔、`restore.sh`、`Caddyfile` 註解、`docs/deploy.md`）。派工後 advisor 抓到 brief 四處缺陷，控制者以更正令補上並由實作者照做：Sp5 的「先紅」不得拿 `XFD1048576` 遠格檔打舊碼（舊碼 `vec![Data::Empty; rows*cols]` 會讓 macOS 先給幾百 GB 懶配置、逐格初始化時把整台機器拖進 swap——改用約 25 000 × 101 的矩形，舊碼配約 80 MB 後回 `TooManyRows`，`TooBig` 斷言乾淨地紅；極端檔只在新碼當綠燈）；Sp1 的斷言改成「`image_id` 非 NULL 且 JOIN 出的 path 等於原圖」（`products::update` 會重建全部圖片列、id 一定變）；Sp2 的欄位鍵明確用 `<商品編號>.category`（`validate_name` 回的是 `name`，會撞商品名稱的鍵）；S4 手動轉址後整段 `fetch()` 包一個 `tokio::time::timeout(10 s)`（否則 10 秒變成每一跳 10 秒，違反 §13）。

閘門（`e650878`）：`cargo fmt --check` exit 0；clippy 零 warning；聚焦 `admin_import` 10／0、`import_parse` 5／0、`--lib import::` 19／0；完整 `cargo test` 29 個 `test result` 行、286 passed、0 failed（基準 275 ＋ 新測試 11）；`sh -n` 兩個腳本 exit 0；九個 stage 字串在 `deploy/`、`docs/deploy.md`、`README.md` 零命中；web 未改動、不跑 pnpm 閘門。煙霧（Task 7 快取映像、`-p dog_shop_smoke`）五段全過：`--single-transaction` happy path 往返成功；不存在但形狀合法的檔名 → 腳本 exit 1、標記列仍在（失敗沒動到資料）、api／web 由 trap 開回並 healthy；`evil.txt`、含 `;` 的檔名、位數不對的檔名 → 三個都 exit 2、含「檔名不正確」、api／web 全程沒被停過、`/backups/pwned` 不存在。R2：`dog_shop-db-1` healthy、`dog_shop_pgdata_dev` 仍在、`dog_shop_smoke` 容器與 volume 都不見、`deploy/.env` 已 trash。

實作者的偏離（全部接受）：§3 沒有可引用的限流測法，§4 自帶完整 curl 指令與判讀；§8 原本沒有 5 MB 那句，改成新增「檔案大小限制」一段；`ImportError::TooBig` 變體在先紅之前先加（測試才編得過，先紅時沒有任何預算檢查）；三個新整合測試共用新 helper `preview_then_commit`（既有測試未改）；S4 沒有跑先紅——舊碼會真的往私有 IP 送 SYN，違反「只打 127.0.0.1」；`cargo fmt --all` 只動到本波新增的行。

範圍複審（opus，`review-d1f3e40..e650878.diff`）：**9 項與四條更正全部 ADDRESSED，沒有新的 Critical／Important**。複審自己對照 crate 原始碼確認：zip-8.6.0 的 `by_index_raw` 不解壓、`size()` 讀的是 central directory 的 uncompressed size；calamine 0.36.1 的 `worksheet_cells_reader` 串流、不配置稠密矩形，`dimensions()` 缺 `<dimension>` 時回 (0,0)–(0,0) 由串流那一道補上；`is_public_ip` 拒絕的範圍完整、沒有誤拒公開位址；轉址迴圈只在 301／302／303／307／308 續跳、轉址回應的 body 永遠不會被當成圖片讀、所有錯誤文案都是常數；外層 10 秒 timeout 涵蓋串流讀 body；C9／C6／C7 三個回歸測試在舊碼下都會紅（實作者的紅燈輸出與程式邏輯相符）；§4 的 Caddyfile 片段語法正確、curl 限流測法不需要真帳密（401 → 第 11 次 429，帶 `X-Requested-With: fetch` 避開 CSRF 403）；手冊每一句改動都對得上 `e650878` 的程式碼，「檔案超過 5 MB」與 `TooBig` 文案逐字一致；stage 字串 grep 零命中。兩個 Minor 備註：`api/tests/import_parse.rs:71` 註解說舊碼回 `TooManyRows`，但紅燈輸出其實是 `Ok(row_count 1)`（遠格是空白、被濾掉）——控制者自己改了那一行註解，commit `9bf1b2a`；`add_category_error` 從 `details.fields` 取第一個訊息，今天 `validate_name` 只回一個欄位所以無歧義，將來若回兩個會靜默丟一個（記錄，不改）。

殘餘項（附錄 A 有對應裁決，皆不改程式）：(1) zip 預算信任 central directory 宣告的 uncompressed size（zip 的 deflate 路徑不核對它），刻意說謊的壓縮檔仍可能讓 calamine 載入過大的 sharedStrings——管理員專用、5 MB 上限、格數預算仍擋住 codex 發現的稠密配置崩潰；要根治得在 calamine 外包一層計數的 `Read`。(2) `is_public_ip` 只還原 IPv4-mapped 的 IPv6；`::a.b.c.d`、NAT64 `64:ff9b::/96`、6to4 `2002::/16` 不轉——都已棄用或需要本部署沒有的中繼。(3) 照 §4 設了 Cloudflare 但跳過防火牆那一步時，直連來源沒有 `CF-Connecting-IP`、`X-Forwarded-For` 變空，`SmartIpKeyExtractor` 退回 X-Real-IP／連線 IP，繞過流量共用一個桶——退化不是壞掉，§4 第 1 步就是為了堵它。(4) DNS 重綁：檢查與 reqwest 自己解析之間位址可能被換掉（程式註解已寫；根治需自訂 resolver／connector）。(5) `--single-transaction` 隱含 `--exit-on-error`：若有人改用非 superuser 還原，以前「有警告但成功」的語句會讓整筆還原失敗（資料仍安全）；compose 部署用的 `POSTGRES_USER` 是 superuser，煙霧 happy path 也證實不受影響。(6) 附錄 B 記的「首次部署第一份 dump 半套 schema」：現在拿它還原會整筆 rollback、不再留半套資料庫，但那份 dump 本身仍會存在——上線後事項第 11 條照舊。
