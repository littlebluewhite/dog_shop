# 部署與對帳手冊

給老闆看的正式環境部署與日常維運手冊；也可以交給懂一點指令的朋友代操作。指令都是可以直接複製貼上的樣子，英文代號（環境變數、路徑、指令）維持原文。

## 1. 上線前要準備的東西

- **網域**一個（例如 `shop.example.com`），要能改 DNS 的 A 記錄。
- **VPS** 一台：至少 2 vCPU、4 GB 記憶體，能裝 Docker。
- **綠界（ECPay）正式特店資料三組**：金流（AIO）、物流（超商取貨 C2C）、電子發票（B2C）各自的 MerchantID、HashKey、HashIV。要跟綠界申請並通過正式環境審核後才拿得到；三組不能沿用開發時的測試特店資料。
- **SMTP 寄信帳號**一組：主機、port、帳號、密碼、寄件人。Gmail 應用程式密碼、Resend、Mailgun 都可以。
- **寄件人姓名與手機**：建立物流單要用。姓名 5 個中文字以內，手機是 09 開頭的 10 碼台灣手機號碼。
- **退貨門市代號**：超商取貨的退貨門市，可以先留空，之後在後台補。
- **商店名稱、Logo、聯絡方式（Email、電話）、退換貨說明文字**：這些在後台「設定」頁填。
- **商品資料檔**：蝦皮匯出的 xlsx，或依範本填的 xlsx（見第 8 節）。
- **綠界帳戶要先預存運費**：正式環境每建立一張物流單，綠界會直接從帳戶餘額扣運費；餘額不夠會導致建單失敗（錯誤碼 `10500049`）。上線前先確認帳戶有錢，上線後也要記得定期儲值。

## 2. 第一次部署

1. VPS 上安裝 Docker（含 docker compose）。
2. `git clone` 這個 repo 到 VPS。
3. 複製正式環境設定範本並逐行填好：

   ```
   cp deploy/env.prod.example deploy/.env
   ```

   檔案裡每一行都有中文註解。特別注意：`POSTGRES_PASSWORD` 只能用英數字（它會被組進資料庫連線字串）；`SITE_ADDRESS` 不含 `https://`；`PUBLIC_BASE_URL` 一定要 `https://` 開頭、結尾不帶 `/`。

4. 把網域的 DNS A 記錄指到 VPS 的 IP，**等 DNS 生效**再往下做（Caddy 啟動時會自動跟 Let's Encrypt 要憑證，網域解析不到會拿不到憑證）。
5. 建置並啟動：

   ```
   cd deploy && docker compose up -d --build
   ```

   第一次建置映像檔要花幾分鐘。

6. 建立第一個管理員帳號：

   ```
   docker compose run --rm -e ADMIN_PASSWORD=<密碼> api api create-admin <email>
   ```

7. 瀏覽器打開 `https://<domain>/admin`，用剛剛的帳號密碼登入。
8. 到「設定」頁（`/admin/settings`）填寄件人姓名、寄件人手機、退貨門市代號、運費（免運門檻、超商取貨運費、宅配運費）、商店名稱、Logo、聯絡方式、退換貨說明。
9. 到「商品」頁（`/admin/products`）隨便挑一個商品，上傳一張圖片，存檔後打開確認看得到圖片。**這一步是在確認圖片真的存得進去**（技術上是 `uploads` 這個 Docker volume 能不能寫入），本機煙霧測試沒有測到這件事，第一次上線一定要手動做一次。
10. 到綠界廠商後台，把下面四個網址填進對應的回呼設定（路徑是系統目前實際提供的路徑）：

    | 用途 | 網址 |
    |---|---|
    | 付款結果通知（ReturnURL） | `https://<domain>/api/ecpay/payment/return` |
    | 付款方式資訊通知（ATM／超商代碼，PaymentInfoURL） | `https://<domain>/api/ecpay/payment/info` |
    | 物流狀態通知 | `https://<domain>/api/ecpay/logistics/status` |
    | 物流門市更新通知 | `https://<domain>/api/ecpay/logistics/store-update` |

    （還有一個 `/api/ecpay/logistics/map-reply`，這個不用在後台設定，客人結帳選門市時系統會自動帶著這個網址請綠界地圖回傳，不是固定回呼設定。）

**如果啟動被拒絕**：`docker compose ps` 會看到 `api` 一直在 `restarting`；用 `docker compose logs api` 看最後幾行，會是第 3 節列出的某一句錯誤訊息。照訊息把 `deploy/.env` 改對，再 `docker compose up -d` 一次就會起來。

## 3. 上線檢查清單

每一條都要成立；括號裡是設錯時 `docker compose logs api` 會出現的原文，方便對照。

- [ ] `ECPAY_ENV=prod`（打錯值會擋：「ECPAY_ENV 只能是 stage 或 prod，收到 {打的值}」）。
- [ ] 三組綠界憑證（`ECPAY_AIO_*`、`ECPAY_INVOICE_*`、`ECPAY_LOGISTICS_*`）都有填。任一個變數空白會擋：「ECPAY_ENV=prod 時必須設定 {變數名}」，例如「ECPAY_ENV=prod 時必須設定 ECPAY_AIO_MERCHANT_ID」。
- [ ] 三組憑證都**不是** `.env.example` 裡開發用的測試特店值（就算只有 HashKey 或 HashIV 對到也算）。對到會擋，例如「ECPAY_ENV=prod 時 ECPAY_AIO 不能用測試特店憑證（.env.example 裡的值）」（`ECPAY_AIO` 換成 `ECPAY_INVOICE`／`ECPAY_LOGISTICS` 是同一句）。
- [ ] `PUBLIC_BASE_URL` 有填且是 `https://` 開頭。沒填會擋：「ECPAY_ENV=prod 時必須設定 PUBLIC_BASE_URL（https 的公開網址）」；填了但不是 https 會擋：「ECPAY_ENV=prod 時 PUBLIC_BASE_URL 必須是 https://（綠界回呼與 Secure cookie 都需要）」。
- [ ] `COOKIE_SECURE=true`。設成 false 會擋：「ECPAY_ENV=prod 時 COOKIE_SECURE 不能關」。
- [ ] SMTP 寄得出信：`SMTP_HOST`、`SMTP_FROM` 都要填（沒填會擋：「ECPAY_ENV=prod 時必須設定 SMTP_HOST 與 SMTP_FROM（訂單信、發票信都靠它）」；只填 `SMTP_HOST` 沒填 `SMTP_FROM` 會擋：「有 SMTP_HOST 就必須設定 SMTP_FROM」）；`SMTP_PORT` 要是數字（沒填預設 587，打錯會擋：「SMTP_PORT 要是 1～65535 的數字」）。上線後自己註冊一個測試帳號，確認信收得到。
- [ ] `MAIL_LOG_BODY` 沒有設。設了會擋：「ECPAY_ENV=prod 時不能開 MAIL_LOG_BODY（信件內文含重設連結與訪客訂單網址）」。
- [ ] `RUST_LOG` 至少 `info`（範本預設 `info,tower_http=info`）。不要把它調低，尤其不能把 `dog_shop_api::routes` 降到 warn 以下——第 5 節的物流對帳完全靠這段 log。
- [ ] 用 `docker compose up` 依照 `deploy/docker-compose.yml` 啟動，`DATABASE_URL` 會自動組好不用自己填；只有跳過 compose、直接執行 `api` 這個程式才可能漏掉，錯誤是「缺少環境變數 DATABASE_URL」。
- [ ] `/admin/settings` 的寄件人手機是台灣手機（09 開頭 10 碼）。
- [ ] 退貨門市代號已經填了（或已知先留空、出貨前會補）。
- [ ] 綠界廠商後台的付款、物流回呼網址已經設定（第 2 節第 10 步）。
- [ ] 綠界帳戶餘額夠扣運費（第 1 節）。
- [ ] 備份服務正在跑：`docker compose ps backup` 要顯示 `Up`。
- [ ] 知道備份檔名上的時間是 UTC、不是台灣時間（細節見第 6 節），對帳或還原時不要看錯。

## 4. 限流與反向代理

Caddy 是唯一對外開放的入口；`api` 與 `web` 兩個容器完全不對外開埠，只有 Caddy 連得到它們。api 的限流（登入、下單、選門市）是靠讀取請求裡的來源 IP 判斷的，Caddy 預設會**忽略**客戶端自己送來的 `X-Forwarded-*`，一律用連線的真實來源 IP 覆寫，所以沒有人能靠偽造 header 繞過限流，這部分不用另外設定。

只有一種情況要改：如果之後在 Caddy **前面**又加一層 CDN（例如 Cloudflare），就要讓 Caddy 知道該信任誰，不然所有請求在 api 眼中來源 IP 都會變成 CDN 自己的 IP，限流形同虛設。做法是在 `deploy/Caddyfile` 最上面加一個全域設定區塊（下面這段語法已經用本機的 `caddy:2` 映像實際跑過 `caddy adapt` 驗證過）：

```
{
	servers {
		trusted_proxies static <Cloudflare 公告的 IP 範圍，用空白分隔>
	}
}

{$SITE_ADDRESS} {
	...（原本的內容不動）
}
```

同一個全域設定區塊也可以加一行 `email 你的信箱`（跟 `servers { }` 平行放在同一層），這樣 Let's Encrypt 憑證快過期時會寄信提醒——這個不是必要設定，Caddy 本來就會自動換發憑證，只是沒設就不會收到到期提醒信。

## 5. Log 與孤兒物流單對帳

`docker compose logs api` 印出來的每一行都是 JSON，每行有 `request_id`，可以用來串同一個請求的前後訊息。

以下幾句 log 訊息**一定要留著**（前提是 `RUST_LOG` 沒有把 `dog_shop_api::routes` 降到 warn 以下）：

- 「綠界物流單已建立」（帶 `logistics_id`）：正常建單成功。
- 「綠界建立物流單連線失敗」（帶 `merchant_trade_no`）：api 沒能連上綠界。
- 「綠界建立物流單失敗」（帶 `merchant_trade_no`）：連上了，但綠界拒絕這張單。
- 「綠界已建單，但這次嘗試已被重新認領；單號只留在 log，請到綠界廠商後台對帳」（帶 `merchant_trade_no` 與 `logistics_id`）：綠界說建單成功了，但系統這邊認定這次嘗試已經逾時、被後來的重試接手，所以這張單的單號沒有寫進資料庫——只有這行 log 知道它存在。
- 「連線失敗的結果寫不進去」、「建單失敗的結果寫不進去」（都帶 `merchant_trade_no`）：連線失敗或建單失敗本來要記一筆失敗原因，但這次嘗試也已經被重新認領，什麼都沒寫進資料庫。

**什麼情況要對帳：**

- 訂單頁「出貨區」出現「連線綠界失敗，請先到綠界廠商後台確認這筆是否已建單，再決定要不要重試」。
- 或者 log 裡出現任何一句含「已被重新認領」的訊息。

**對帳步驟：**

1. 到綠界廠商後台「物流管理」→「物流建單及查詢」，用訂單編號（`DS…`）前綴搜尋。
2. 同一筆訂單如果查到兩張以上（結尾 `…L01`、`…L02` 這樣遞增），就是孤兒物流單。
3. 孤兒單要在綠界後台自己處理，本系統沒有「取消物流單」的功能。
4. 訂單頁「出貨區」有一個 `raw.create_requests` 欄位，記錄了每一次建單嘗試送出的內容，可以拿來跟綠界後台核對是哪一次成功的。

## 6. 備份與還原

`backup` 這個容器服務每 24 小時自動跑一次 `pg_dump`，存進 Docker volume `dog_shop_backups`，保留 14 天，超過的自動刪除。

列出目前有哪些備份檔（在 `deploy/` 目錄下執行）：

```
./restore.sh list
```

還原到某個備份檔：

```
./restore.sh <檔名>
```

這會先停掉 `api`／`web`，清掉現有資料庫內容、換成備份檔的內容，還原完再重新啟動 `api`／`web`。

幾件要注意的事：

- **備份檔名上的時間是 UTC，不是台灣時間**：容器內部時鐘是 UTC，比台灣時間慢 8 小時。舉例，檔名 `dog_shop-20260908-171214.dump` 看起來像「9/8 17:12」，但實際是台灣時間 9/9 凌晨 01:12（隔天）建立的。挑檔案時記得加 8 小時換算，不要直接照檔名對照「現在」的台灣時間。
- **還原半路失敗**：如果 `./restore.sh <檔名>` 跑到一半失敗，`api`／`web` 會停在關閉狀態（腳本已經先停了它們，還沒重開）。這時手動執行 `docker compose start api web` 讓服務恢復，再去查失敗原因（例如檔名打錯、備份檔不存在）。
- **還原會清掉所有現有資料**（是「先清空再灌入備份內容」，不是合併），還原前務必先手動備份一次現在的資料：

  ```
  docker compose run --rm --entrypoint sh backup -c '/backup.sh once'
  ```

  沒有先備份，還原後現在的資料就回不來了。

- 商品圖片存在 volume `dog_shop_uploads`，不在資料庫備份範圍內，要另外抄出來：

  ```
  docker run --rm -v dog_shop_uploads:/data -v $PWD:/out alpine tar czf /out/uploads.tgz /data
  ```

- 建議**每週**手動把最新的 dump 與 `uploads.tgz` 抄一份到 VPS 以外的地方（自己的電腦、雲端硬碟都可以），VPS 整台掛掉時才有得救。

## 7. 更新與回滾

更新到最新版本：

```
git pull
docker compose up -d --build
```

資料庫的 migration 會在 api 啟動時自動跑，不用手動操作。

回滾到之前的版本：

```
git checkout <要回滾到的 commit SHA 或 tag>
docker compose up -d --build
```

**migration 不會自動倒退**，回滾前一定要先照第 6 節備份一次，回滾後如果資料庫結構跟舊版本對不上，要有心理準備得手動處理。

## 8. 匯入商品

登入後台後，左側選單「匯入」→ 選擇 xlsx 檔案 → 按「預覽」。

預覽畫面會顯示：幾個商品、幾個規格、對不上範本欄位的欄名（會被忽略，畫面上寫「對不上的欄位（會被忽略）：品牌」這樣）、每一列的錯誤（表格列出第幾列、哪個欄位、什麼問題）。有錯誤時畫面會顯示「N 個錯誤，修正後重新上傳才能匯入」，這時「確認匯入」是關閉的，要照錯誤表改好 Excel、重新選檔、重新按「預覽」。

沒有錯誤才能按「確認匯入」，還會再跳出一次「再按一次確認匯入 N 個商品」的二次確認，避免手滑按到。

匯入送出時，畫面會顯示「匯入中，請不要關閉頁面…」；後端這時會**重新讀一次你選的那個檔案**，跟預覽時的內容做比對（用檔案內容算出的指紋比對是不是同一份）。中間如果檔案被動過，會被擋下來，訊息是「檔案已變更，請重新預覽」——這種情況要整個重來一次「選檔案 → 預覽 → 確認匯入」，不能只按確認鈕重試。

**範本欄位**（第一列標題，一字不改）：`商品編號`、`商品名稱`、`商品描述`、`分類`、`規格名稱1`、`規格選項1`、`規格名稱2`、`規格選項2`、`價格`、`庫存`、`SKU`、`圖片網址`（多張圖用逗號分隔，最多 9 張）。同一個「商品編號」出現在多列時，會合併成同一個商品底下的多個規格。

蝦皮匯出檔的欄位對應是暫定的（程式裡有一份別名表，例如「商品ID」「父SKU」「主商品貨號」都會被當成「商品編號」），**第一次拿真正的蝦皮匯出檔匯入時，對不上的欄位會列在預覽頁「對不上的欄位」**，把這份清單回報給開發者，才能把別名表補齊、之後匯入就不會再漏。

**圖片下載時間**：伺服器會自己去下載「圖片網址」欄裡的網址，每張圖最多等 10 秒、最多 6 張同時下載、單張上限 10 MB。用 500 個商品、每個商品 9 張圖估算，最壞情況大約要 2 小時，商品數量多時**建議分批匯入**（例如一次 100 個），不要一次全部上傳。這個下載過程最多用到 6 個並行連線，在建議的 2 vCPU／4 GB VPS 上沒問題；如果之後換成規格更小的主機，不建議在上面跑大批次匯入。

**匯入期間請不要同時編輯商品或處理出貨**：匯入是先讀一次現有商品資料，接著花時間下載圖片，最後才把結果寫回資料庫；如果在這段時間裡改了商品內容或處理了訂單出貨，那些改動可能會被匯入的結果蓋掉。批次數量大、預期匯入要跑比較久時，這一點特別重要。

匯入進去的商品，不管是新增還是更新既有商品，狀態都是「草稿」，要自己到後台檢查沒問題後再上架。

重複匯入同一個「商品編號」，會拿現有商品當底，**只覆蓋 Excel 裡有填的欄位**：網址代稱（slug）、上架狀態、排序、庫存、圖片這些如果 Excel 對應的欄位是空的，會保留原本的值，不會被清空或重置成預設值。

## 9. 上線後要看的事

**超商取貨 14 天自動完成，不會排除「已到門市」的狀態**：出貨滿 14 天、物流狀態不是「已退回」，訂單就會自動標成「已完成」——就算貨還在超商沒被取走也一樣。實務上綠界通常 7 天內就會發「逾期未取退回」的狀態，所以順序上通常沒問題，但**第一批超商取貨訂單到店之後，建議手動看一次這個時序有沒有跑對**。

綠界的物流狀態代碼會不定期更新，對照表在 `api/src/ecpay/logistics.rs` 的 `shipment_status_for`；如果之後某家超商的到店／取件代碼變了，代碼只會被記錄、不會影響訂單狀態，到時候把新代碼補進對照表就好。

`cvs_map_requests` 這張表的列數，可以當成「有沒有人在亂刷選門市」的便宜指標，這張表每天會自動清一次：

```
docker compose exec db psql -U dog_shop -d dog_shop -c "SELECT count(*) FROM cvs_map_requests"
```

背景工作（寄信、排程）失敗筆數，正常應該是 0 或很小：

```
docker compose exec db psql -U dog_shop -d dog_shop -c "SELECT count(*) FROM jobs WHERE status = 'failed'"
```

有失敗的話，資料庫 `jobs` 表的 `last_error` 欄位有錯誤訊息可以查。

這份部署預設只跑**一份** api（沒有多副本）。如果之後為了效能要開多個 api 副本，要注意兩個「當機保護」用的時間窗口：物流建單有 2 分鐘的「重新認領」窗口、背景工作有 10 分鐘的「卡住重排」窗口，多副本情況下極端時機可能造成重複建單或重複執行同一項工作；真的出狀況時，第 5 節的對帳步驟是目前唯一的救援方式。

## 10. 尚未做的與人工驗收

以下項目自動測試沒有涵蓋，上線前或上線後找時間，建議用瀏覽器手動走一次：

**會員與購買流程**（合併自前期開發的人工驗收清單）：
- 註冊 → 自動登入 → 改姓名手機、改密碼 → 新增／切換預設／刪除地址 → 商品加入購物車 → 結帳時用「常用地址帶入」、選公司發票（填統一編號）→ 訂單頁顯示待付款與明細 → 取消訂單，確認後台商品庫存有回補。
- 忘記密碼：`/forgot-password` 送出後顯示「如果這個 Email 有註冊過…」；用隨便打的字串開 `/reset/隨便打` 顯示「重設連結無效或已過期」。
- 後台「設定」：把免運門檻改成 100、關掉超商代碼付款，確認結帳頁的運費與付款方式選項有跟著變。
- 超商取貨：結帳頁選超商時，沒按過「選擇門市」按鈕會擋住送出，並提示「請先選擇取貨門市」。

**綠界 stage 走查留下的待確認項**（腳本見 `docs/dev/ecpay-stage.md`）：
- 「信用卡」節：公司發票 `CarrierType=1` 不帶 `CustomerID` 能不能成功開立；綠界回應的 `RtnCode` 實際上是數字還是字串。
- 「超商取貨（物流）」節第 4、5 點：真實 `/Express/Create` 回應能不能通過 MD5 簽章驗證；`GoodsAmount`／`ReceiverEmail`／`LogisticsC2CReplyURL` 這三個欄位是不是三家超商（7-ELEVEN、全家、萊爾富）都接受；Safari「列印託運單」開新分頁被瀏覽器當成彈出視窗擋下時要怎麼處理。

**還沒做的功能**：電子發票的 GetIssue（作廢後查詢重開）；前期開發留下的一些小項；後台儀表板把「待出貨」拆成宅配／超商分開統計。

**Playwright e2e 沒有排進 CI**（`.github/workflows/ci.yml` 有 `api`〔fmt／clippy／test〕、`web`〔check／test／build〕、`docker`〔映像建置〕三個 job，都不含 Playwright），要手動跑的話，開三個終端機視窗依序執行、全部跑完再關掉：

```
export PATH="$HOME/.cargo/bin:$PATH" && DATABASE_URL=postgres://dog_shop:dog_shop@localhost:5435/dog_shop cargo run --manifest-path api/Cargo.toml
```

```
pnpm -C web dev
```

```
pnpm -C web test:e2e
```
