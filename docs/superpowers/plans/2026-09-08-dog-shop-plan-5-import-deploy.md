# 計畫 5：Shopee 匯入與部署 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 老闆能把蝦皮匯出（或依範本填寫）的 xlsx 上傳到後台、預覽、確認匯入成商品；整個系統能用 `deploy/` 裡的 Dockerfile、docker compose、Caddyfile、備份腳本部署到一台 VPS，並有一份老闆看得懂的部署與對帳手冊。

**Architecture:** 匯入是 `api/src/import/`（欄位對應 → 解析成 `ParsedImport` → 下載圖片 → 用既有的 `products::create/update` 寫入）加兩條 admin 路由（`preview`／`commit`，commit 重新上傳同一個檔案並比對 sha256 指紋）加一頁 `/admin/import`。部署是三個容器（caddy、web、api）+ db + backup sidecar；Caddy 是唯一對外的入口，api／web 不對外開埠；正式環境啟動時 `config.rs` 拒絕測試特店憑證、非 https 網址、沒設 SMTP。程式碼改動集中在 `import/`、`routes/admin_import.rs`、`config.rs`、`main.rs`、`jobs/worker.rs`、`products.rs`（加 `external_ref`、`MAX_PRICE`）與 `categories.rs`（依名稱找或建）。

**Tech Stack:** Rust 1.98（edition 2024）、axum 0.8、sqlx 0.9、`calamine`（xlsx 解析，新依賴）、`rust_xlsxwriter`（測試用產生 xlsx，dev 依賴）、reqwest 0.13（`rustls-no-provider`）、`image` 0.25（既有 `storage::save`）、SvelteKit 2／Svelte 5 runes、Tailwind 4、vitest、Playwright 1.63、Docker／docker compose、Caddy 2、PostgreSQL 17。

**Spec:** `docs/superpowers/specs/2026-09-06-dog-shop-mvp-design.md`（§13 商品匯入、§16 部署、§17 使用者要準備的東西、§18 建置順序第 10、11 步；§2 系統架構；§11 安全；§15 測試策略）。輸入還包括：計畫 4 計畫文件結尾「交給計畫 5 的事項」1–9（`docs/superpowers/plans/2026-09-08-dog-shop-plan-4-logistics-admin-orders-dashboard.md:5871-5883`）、計畫 4 審查文件「交給計畫 5 的事項」1–9 與控制者註（`docs/superpowers/reviews/2026-09-08-plan-4-final-review.md:323-337`）、計畫 3 審查文件「交給計畫 4」第 9 條（stage 憑證是公開的）、計畫 2 審查文件對價格上限的交接。

## Global Constraints

- Rust 1.98、edition 2024、axum 0.8、sqlx 0.9（`sqlx::query` 執行期字串，不用 `query!` 巨集，所以 Docker 建置不需要 `DATABASE_URL`）；SvelteKit 2、Svelte 5 runes、Tailwind 4；Node 24、pnpm 10；PostgreSQL 17。
- 規格 §11：HashKey、HashIV、SMTP 密碼、DB 連線字串、reset token、`guest_token` 不得進 log、錯誤文字、`jobs.last_error`、git。`Config` 的 `Debug` 已把這些遮掉，新增欄位照做。
- 規格 §13：只收 `.xlsx`；範本 12 個欄位標題逐字為 `商品編號`、`商品名稱`、`商品描述`、`分類`、`規格名稱1`、`規格選項1`、`規格名稱2`、`規格選項2`、`價格`、`庫存`、`SKU`、`圖片網址`；圖片網址逗號分隔最多 9 個；同一 `商品編號` 多列合併成多規格；`preview` 回商品數、規格數、每列錯誤、缺欄位；圖片由伺服器下載（10 秒 timeout、≤ 10 MB、只收圖片 MIME），失敗只記該列警告不擋整批；重複匯入以 `products.external_ref = 商品編號` 更新；匯入的商品預設 `draft`。
- 規格 §16：`deploy/docker-compose.yml` 有 `caddy`（80/443）、`web`（adapter-node、Node 24）、`api`（多階段：rust 1.98 → debian-slim）、`db`（postgres:17，volume `pgdata`）；`api` 另掛 volume `uploads`；`Caddyfile` 依 §2 反向代理；migration 在 api 啟動時執行（已是如此）；`deploy/backup.sh` 每日 `pg_dump`、保留 14 天；`.env.example` 列全規格所列的環境變數。
- 規格 §2：Caddy 把 `/api/*`、`/uploads/*` 轉給 api（:8080），其餘轉給 web（:3000）；web SSR 用內網 `http://api:8080`；綠界回呼打 `https://<domain>/api/ecpay/...`。
- 規格 §10：後台端點 `POST /api/admin/import/preview`、`POST /api/admin/import/commit`，需 admin；錯誤格式 `{ "error": { "code", "message", "details" } }`；驗證錯誤 `VALIDATION` 帶 `details.fields`。
- 規格 §15：Rust 單元＋整合（`sqlx::test`）、vitest、Playwright 主流程、CI 跑 fmt／clippy／test／check／test／build。
- 既有裁決（計畫 3／4）：鎖序 payments → orders → product_variants、orders → shipments；`ApiError::EcpayError(String)` 固定文案；綠界回呼路由 64 KB body 上限 + 100 欄位上限；`MAIL_LOG_BODY=1` 是唯一讓信件內文進 log 的方式。
- **測試與探測不得對外部網路發請求**：圖片下載的測試只打 `127.0.0.1` 的本機假伺服器；任何測試都不得碰 `*.ecpay.com.tw` 或 `*.shopee.tw`。
- **本計畫不做**：實際購買／設定 VPS 與網域、把映像推到 registry、真的執行正式部署（那些是使用者的步驟，寫進手冊）；發票 GetIssue 與計畫 1／2 的小項（計畫 4 交接 7）；儀表板拆宅配／超商（計畫 4 審查交接 8）。
- 全程只在本機分支 `worktree-mvp-design` commit，不 push、不開 PR、不碰 main。

---

## 環境事實（每個任務開始前都要知道）

- worktree：`/Users/wilson08/IdeaProjects/dog_shop/.claude/worktrees/mvp-design`，分支 `worktree-mvp-design`，計畫 4 結束於 `80509df`。api crate 名 `dog-shop-api`，lib 名 `dog_shop_api`，binary 名 `api`（`api/Cargo.toml` 的 `[[bin]] name = "api"`）。
- 指令食譜（沙盒 shell 拒絕 `source`、heredoc、`$(...)`、`for`／`while`、前景 `sleep`、`timeout`；`rm -rf` 被擋，用 `trash`；需要腳本就用 Write 工具寫檔再 `bash` 執行）：
  - Rust：`export PATH="$HOME/.cargo/bin:$PATH"`；一律 `cargo … --manifest-path /Users/wilson08/IdeaProjects/dog_shop/.claude/worktrees/mvp-design/api/Cargo.toml`，不要 `cd api`。每個 `cargo test`／`cargo run` 都要 `DATABASE_URL=postgres://dog_shop:dog_shop@localhost:5435/dog_shop`（Docker 容器 `dog_shop-db-1`，PostgreSQL 17，已在跑）。
  - Web：永遠 `pnpm -C /Users/wilson08/IdeaProjects/dog_shop/.claude/worktrees/mvp-design/web <script>`，不要 `cd web`。SSR 頁都會打 `/api/settings/public`，curl :5173 前要先起 api（:8080）。
  - 伺服器：api `cargo run`（:8080）、web `pnpm -C web dev`（:5173），都用 `run_in_background`；停止用 `lsof -ti :8080`／`lsof -ti :5173` 再 `kill <pid>`。
  - 完整 `cargo test`（27 個以上的 `test result` 行，約 5–10 分鐘）用 `run_in_background`（上限 600000 ms）導到 log；`docker compose build` 會超過 10 分鐘，要用 `nohup … &` 再用控制者提供的 `wait-pid.sh` 等。
  - 開發 admin：`admin@example.com`／`admin12345`；用 curl 登入要帶 `-H 'X-Requested-With: fetch'`。
  - git 指令單獨執行（`git -C <worktree> …`），不要 `git stash`；commit 訊息最後一行 `Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>`。
- **商品 domain（`api/src/domain/products.rs`）**：
  - `ProductInput { name: String, slug: Option<String>, description: Option<String>, category_id: Option<Uuid>, status: String, option1_name: Option<String>, option2_name: Option<String>, sort_order: Option<i32>, variants: Vec<VariantInput>, images: Vec<ImageInput> }`（`#[derive(Debug, Serialize)]`，但路由是用 `Deserialize` 的 — 實際上 `ProductInput` derive 的是 `Serialize` 給測試、`Deserialize` 在路由層：本計畫 Task 1 會加 `external_ref: Option<String>` 並保持兩邊都能用，見該任務）。
  - `VariantInput { id: Option<Uuid>, option1_value: Option<String>, option2_value: Option<String>, sku: Option<String>, price: i32, compare_at_price: Option<i32>, stock: i32, is_active: Option<bool>, image_path: Option<String> }`（`#[derive(Debug, Deserialize)]`）。
  - `ImageInput { path: String, thumb_path: String, alt: Option<String> }`（`#[derive(Debug, Deserialize)]`）。
  - `AdminProduct { #[serde(flatten)] product: ProductRow, variants: Vec<VariantRow>, images: Vec<ImageRow> }`；`ProductRow { id, slug, name, description: String, category_id: Option<Uuid>, status, option1_name, option2_name, external_ref: Option<String>, sort_order: i32, created_at, updated_at }`；`VariantRow { id, product_id, option1_value, option2_value, sku, price: i32, compare_at_price, stock: i32, is_active: bool, image_id, sort_order }`；`ImageRow { id, product_id, path, thumb_path, alt: String, sort_order }`。
  - `create(db, ProductInput) -> Result<AdminProduct, ApiError>`、`update(db, id, ProductInput) -> Result<AdminProduct, ApiError>`、`get_admin(db, id) -> Result<Option<AdminProduct>, ApiError>`、`validate(&ProductInput)`、`clean(&Option<String>) -> Option<String>`（去頭尾空白、空字串當 None）。
  - **`update` 不是 patch**：`slug: None` → `random_slug()`（網址會換）、`description: None` → 空字串、`category_id: None` → 清掉、`sort_order: None` → 0、`images` 整組重建（先 DELETE 再 INSERT）、`variants` 有 `id` 的更新、沒 `id` 的新增、沒出現的刪除（有訂單引用的改停用）。所以匯入更新既有商品時，**一定要先 `get_admin` 拿現值當底，只覆蓋工作表有填的欄位，規格用 `(option1_value, option2_value)` 對回既有 `VariantRow.id`**。
  - `validate` 現況：name 1～120 字、status ∈ {draft, active, archived}、slug 合法、至少一個規格、規格 ≤ `MAX_VARIANTS = 100`、圖片 ≤ `MAX_IMAGES = 9`、有規格 2 必先有規格 1、沒規格名稱只能一個規格、價格／庫存／原價不能負、規格組合不重複。價格沒有上限（計畫 2 審查交接：`price * qty` 是 `i32`，本計畫加上限）。
  - `products.external_ref text UNIQUE`（`api/migrations/0001_init.sql:43`）已存在，`ProductRow.external_ref` 已對應，但 `create`／`update` 的 SQL 沒寫它。
- **分類 domain（`api/src/domain/categories.rs`）**：`Category { id, slug, name, sort_order }`；`create(db, slug: Option<&str>, name: &str, sort_order: i32) -> Result<Category, ApiError>`（slug None → `random_slug()`）；`list(db)`；`categories.name` **不是 unique**。
- **圖片儲存（`api/src/storage/mod.rs`）**：`save(upload_dir: &Path, bytes: Vec<u8>) -> Result<StoredImage, StorageError>`，`StoredImage { path: String /* /uploads/yyyy/mm/{uuid}.jpg */, thumb_path: String, width: u32, height: u32 }`；解碼失敗回 `StorageError::Decode`；`MAX_UPLOAD_BYTES = 10 * 1024 * 1024`；`ALLOWED_MIME = ["image/jpeg", "image/png", "image/webp", "image/gif"]`；存的是 JPEG（計畫 2 的決定，不是規格說的 WebP）。
- **錯誤（`api/src/error.rs`）**：`ApiError::field(field, msg)` → 400 `VALIDATION` 帶 `details.fields[field]`；`FieldErrors::new()/add()/into_result()`；`ApiError::NotFound`、`Forbidden(&'static str)`、`Internal(anyhow::Error)`（`?` 可從 anyhow 轉）。
- **路由**：`api/src/app.rs::router` 用 `.merge(routes::xxx::router())` 組裝（第 35–49 行），全域 `DefaultBodyLimit::max(BODY_LIMIT_BYTES)`，`BODY_LIMIT_BYTES = 10 MB + 64 KB`（xlsx 上傳 ≤ 5 MB 不用另外調）。admin 權限用 extractor `AdminUser`（`crate::extract::AdminUser`），路徑參數用 `AppPath<T>`。multipart 用 `axum::extract::Multipart`（`api/src/routes/uploads.rs` 是範例：逐 field 讀 `name()`、`file_name()`、`content_type()`、`bytes()`）。
- **測試 helper（`api/tests/common/mod.rs`）**：`app(pool) -> Router`、`app_with_state(pool) -> (Router, AppState)`、`state(pool)`、`req(method, uri, cookie: Option<&str>, body: Option<Value>) -> Request<Body>`、`send(&app, req) -> (StatusCode, Value, HeaderMap)`、`admin_cookie(&app, &pool)`、`customer_cookie(&app, &pool)`、`active_product(&pool, name, price, stock) -> (Uuid, slug)`。multipart 手工組法在 `api/tests/uploads.rs:19-40`（`multipart(cookie, filename, content_type, data)`，boundary `XxDogShopBoundaryxX`），`png(width, height)` 產生器在同檔第 9 行 — 新測試檔照抄這兩個 helper（本 repo 慣例：整合測試檔自給自足）。整合測試可 `use dog_shop_api::domain::{products, categories}` 與 `dog_shop_api::import::…`。
- **設定（`api/src/config.rs`）**：`Config { database_url, public_base_url, cookie_secure, upload_dir, ecpay: EcpayConfig { env: EcpayEnv, aio, invoice, logistics: EcpayCredentials }, smtp: Option<SmtpConfig>, mail_log_body }`；`Config::from_env()` 直接讀 `std::env`；`credentials(prefix, env, stage)`：stage 空值退回 `STAGE_AIO`／`STAGE_INVOICE`／`STAGE_LOGISTICS` 常數，prod 缺值 `bail!`；**沒有**「憑證 ≠ stage 值」的檢查；`PUBLIC_BASE_URL` 沒設預設 `http://localhost:5173`；`COOKIE_SECURE` 沒設預設 true；`SMTP_HOST` 沒設就 `smtp = None`（prod 也不強制）；`Config::for_tests(upload_dir)` 在第 218 行。`.env.example` 的九個 `ECPAY_*` 值與三個 `STAGE_*` 常數逐字相同（公開測試特店）。
- **啟動（`api/src/main.rs`）**：`dotenvy`、JSON tracing（`EnvFilter` 預設 `info,tower_http=info`）、`Config::from_env`、`db::connect`、`db::migrate`（`sqlx::migrate!("./migrations")` 編譯期內嵌，四個 migration 檔）、子指令只有 `create-admin <email>`（`ADMIN_PASSWORD` 可非互動）、`jobs::start(state)` 同行程、監聽寫死 `0.0.0.0:8080`、`into_make_service_with_connect_info::<SocketAddr>()`、`shutdown_signal` 只接 `ctrl_c`（**沒接 SIGTERM，`docker stop` 不會 graceful**）。
- **jobs（`api/src/jobs/worker.rs`）**：`mark_done`（第 66 行）與 `mark_failed_attempt`（第 78／86 行）的 UPDATE 只有 `WHERE id = $1`，沒有 `AND status = 'running'`（計畫 4 交接 2）；`requeue_stale`（第 191 行）把跑超過 `STALE_RUNNING_MINUTES` 的 running 改回 queued。
- **限流**：`tower_governor 0.8` 的 `SmartIpKeyExtractor` 依序看 `x-forwarded-for`（取逗號分隔後**最左邊**能解析的 IP）→ `x-real-ip` → `Forwarded` → `ConnectInfo`。`POST /api/orders`、`POST /api/checkout/cvs-map`（`rate_limit::anonymous_write`）與 `/api/auth/*` 有限流。**Caddy 預設會忽略客戶端送來的 `X-Forwarded-*` 並自己寫入真實來源 IP（只有設了 `trusted_proxies` 才會信任上游的值）**；所以防偽造靠拓樸：Caddyfile 不設 `trusted_proxies`、compose 不對 api／web 開 `ports:`。
- **web**：`@sveltejs/adapter-node`（`web/svelte.config.js`），`package.json` scripts 只有 dev/build/preview/prepare/check/test/test:e2e（沒有 `start`）；瀏覽器端 `api<T>(path, init)`（`web/src/lib/api.ts:51`）非 GET 自動帶 `X-Requested-With: fetch`，**body 是 `FormData` 時交給瀏覽器設 content-type**；伺服器端 `serverApi<T>(event, path)`（`web/src/lib/server/api.ts`）讀 `$env/dynamic/private` 的 `API_INTERNAL_URL`（預設 `http://localhost:8080`）；`robots.txt`／`sitemap.xml` 用 `$env/dynamic/public` 的 `PUBLIC_BASE_URL`；後台側欄是 `web/src/routes/admin/+layout.svelte:8-12` 的 `{ href, label }` 陣列（儀表板／訂單／商品／分類／設定）；後台頁的樣式與 `act()`／`busy` 寫法看 `web/src/routes/admin/orders/+page.svelte` 與 `admin/orders/[id]/+page.svelte`；型別集中在 `web/src/lib/types.ts`。Vite dev proxy（`web/vite.config.ts:9-12`）把 `/api`、`/uploads` 轉到 :8080，`envDir: '..'`（api／web 共用根目錄 `.env`）。
- **CI（`.github/workflows/ci.yml`）**：`api` job（postgres service、fmt、clippy、test）與 `web` job（pnpm 10、Node 24、check、test、build）；沒有 docker 建置。
- **calamine**（新依賴）：`use calamine::{open_workbook_from_rs, Reader, Xlsx, Data};`，`let mut wb: Xlsx<_> = open_workbook_from_rs(Cursor::new(bytes))?;`，`wb.sheet_names()`（`Vec<String>`），`wb.worksheet_range(&name)?`（`Range<Data>`），`range.rows()` 逐列給 `&[Data]`；`Data` 變體：`Int(i64)`、`Float(f64)`、`String(String)`、`Bool(bool)`、`DateTime(ExcelDateTime)`、`DateTimeIso(String)`、`DurationIso(String)`、`Error(CellErrorType)`、`Empty`。若安裝到的版本把 `Data` 叫 `DataType`（0.23 以前），用 `cargo add calamine` 裝最新版即可。
- **rust_xlsxwriter**（dev 依賴，測試產生 xlsx）：`let mut wb = Workbook::new(); let ws = wb.add_worksheet(); ws.write_string(row, col, "文字")?; ws.write_number(row, col, 1200.0)?; let bytes: Vec<u8> = wb.save_to_buffer()?;`。
- **Docker**：本機有 Docker（`dog_shop-db-1` 在跑）；本機的 80 埠可能被占用，煙霧測試用 `deploy/docker-compose.smoke.yml` 把 caddy 改成 `8081:80`。
- 開發資料庫裡有計畫 4 審查者留下的探測訂單 `DS260908UHSU`／`DS2609083TN4`／`DS2609087Z2Y`（`PROBE…` 假物流單號），不要當真、不要拿去對綠界。

## 檔案結構

新增：
- `api/src/import/mod.rs` — 模組根：常數、re-export。
- `api/src/import/columns.rs` — 欄位列舉 `Column`、標題正規化、範本標題與蝦皮別名表、`map_headers`。
- `api/src/import/parse.rs` — 字串格 → `ParsedImport`（分組、驗證、每列錯誤）；`parse_xlsx` 用 calamine 把 xlsx 變成字串格再交給 `parse_grid`。
- `api/src/import/images.rs` — `ImageFetcher`（reqwest、10 秒、≤ 10 MB、只收 `image/*`、≤ 3 次轉址、只收 http/https）、`fetch_and_store`、`fetch_all`（Semaphore 限 6 併發）。
- `api/src/import/apply.rs` — 把 `ParsedImport` 寫進資料庫：分類找或建、既有商品當底覆蓋、圖片下載、`products::create/update`；回 `ImportResult`。
- `api/src/routes/admin_import.rs` — `POST /api/admin/import/preview`、`POST /api/admin/import/commit`。
- `api/tests/admin_import.rs` — 路由與 apply 的整合測試（含本機圖片假伺服器）。
- `web/src/routes/admin/import/+page.svelte`、`web/src/lib/importPreview.ts`（+ `.test.ts`）。
- `api/Dockerfile`、`api/.dockerignore`、`web/Dockerfile`、`web/.dockerignore`、`deploy/docker-compose.yml`、`deploy/docker-compose.smoke.yml`、`deploy/Caddyfile`、`deploy/env.prod.example`、`deploy/backup.sh`、`deploy/restore.sh`、`docs/deploy.md`、`README.md`。

修改：
- `api/src/domain/products.rs` — `ProductInput.external_ref`、`MAX_PRICE`、`find_by_external_ref`、`existing_external_refs`。
- `api/src/domain/categories.rs` — `find_or_create_by_name`。
- `api/src/lib.rs` — `pub mod import;`；`api/src/routes/mod.rs` — `pub mod admin_import;`；`api/src/app.rs` — merge 新路由。
- `api/src/config.rs` — `from_vars`、prod 檢查、`listen_addr`；`api/src/main.rs` — SIGTERM、`listen_addr`；`api/src/jobs/worker.rs` — `AND status = 'running'`。
- `api/Cargo.toml` — `calamine`（dep）、`rust_xlsxwriter`（dev-dep）。
- `web/src/lib/types.ts`、`web/src/routes/admin/+layout.svelte`（側欄加「匯入」）、`web/package.json`（`start` script）。
- `.env.example`（加註解與 `LISTEN_ADDR`）、`.gitignore`（`deploy/backups/`）、`.github/workflows/ci.yml`（docker build job）、`docs/dev/ecpay-stage.md`（交叉引用一句）。

## 共用介面（每個任務都以這裡的簽章為準；後面的任務逐字重複時以此為準）

```rust
// api/src/import/mod.rs
pub mod apply;
pub mod columns;
pub mod images;
pub mod parse;

/// xlsx 上限 5 MB（規格沒定；蝦皮匯出通常 < 1 MB）
pub const MAX_XLSX_BYTES: usize = 5 * 1024 * 1024;
/// 資料列上限（標題列之後）
pub const MAX_ROWS: usize = 5000;

pub use apply::{ImportResult, ImportWarning, ImportedProduct, apply};
pub use images::{FetchError, ImageFetcher, fetch_all, fetch_and_store};
pub use parse::{ImportError, ImportProduct, ImportVariant, ParsedImport, RowError, parse_grid, parse_xlsx};
```

```rust
// api/src/import/columns.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Column {
    ExternalRef, Name, Description, Category,
    Option1Name, Option1Value, Option2Name, Option2Value,
    Price, Stock, Sku,
    /// 「圖片網址」：一格裡逗號分隔多個
    ImageUrls,
    /// 「商品圖片 1」～「商品圖片 9」：一格一個
    Image(u8),
}
pub const TEMPLATE_HEADERS: [&str; 12] = ["商品編號", "商品名稱", "商品描述", "分類", "規格名稱1", "規格選項1", "規格名稱2", "規格選項2", "價格", "庫存", "SKU", "圖片網址"];
pub fn normalize_header(raw: &str) -> String;          // 去空白（含全形）、全形數字→半形、全形括號→半形、小寫
pub fn match_header(raw: &str) -> Option<Column>;      // 正規化後查別名表
pub struct HeaderMap { pub columns: Vec<Option<Column>>, pub unmatched: Vec<String> }
pub fn map_headers(row: &[String]) -> HeaderMap;       // 同一 Column 出現兩次 → 第二次進 unmatched
pub fn is_header_row(row: &[String]) -> bool;          // 有 Name 且對到 ≥ 3 個 Column
```

```rust
// api/src/import/parse.rs
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RowError { pub row: u32, pub column: Option<String>, pub message: String }   // row = 工作表 1-based 列號；column = 範本欄名
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ImportVariant { pub row: u32, pub option1_value: Option<String>, pub option2_value: Option<String>, pub sku: Option<String>, pub price: i32, pub stock: Option<i32> }
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ImportProduct { pub external_ref: String, pub first_row: u32, pub name: String, pub description: Option<String>, pub category: Option<String>, pub option1_name: Option<String>, pub option2_name: Option<String>, pub image_urls: Vec<String>, pub variants: Vec<ImportVariant> }
#[derive(Debug, Clone, Serialize, Default)]
pub struct ParsedImport { pub sheet: String, pub header_row: u32, pub row_count: u32, pub products: Vec<ImportProduct>, pub errors: Vec<RowError>, pub unmatched_columns: Vec<String> }
impl ParsedImport { pub fn variant_count(&self) -> usize; }
#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("檔案不是 xlsx 或已損壞")] Unreadable,
    #[error("找不到標題列（需要「商品名稱」以及至少兩個其他範本欄位）")] NoHeader,
    #[error("缺少必要欄位：{0}")] MissingColumns(String),   // 例：「商品編號、價格」
    #[error("資料列超過 {0} 列")] TooManyRows(usize),
}
pub fn parse_grid(sheet: &str, grid: &[Vec<String>]) -> Result<ParsedImport, ImportError>;
pub fn parse_xlsx(bytes: &[u8]) -> Result<ParsedImport, ImportError>;   // 第一個工作表；Data → String 的規則見 Task 2
```

```rust
// api/src/import/images.rs
pub const FETCH_TIMEOUT_SECS: u64 = 10;
pub const MAX_IMAGE_BYTES: usize = crate::storage::MAX_UPLOAD_BYTES;   // 10 MB
pub const MAX_REDIRECTS: usize = 3;
pub const FETCH_CONCURRENCY: usize = 6;
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum FetchError {
    #[error("只接受 http／https 網址")] Scheme,
    #[error("下載失敗：{0}")] Http(String),        // 固定文案 + 狀態碼或「連線失敗」，不含回應內容
    #[error("不是圖片（{0}）")] NotImage(String),   // content-type，截 60 字
    #[error("圖片超過 10 MB")] TooLarge,
    #[error("圖片無法讀取")] Decode,
    #[error("存檔失敗")] Store,
}
#[derive(Clone)]
pub struct ImageFetcher { client: reqwest::Client }
impl ImageFetcher {
    pub fn new() -> anyhow::Result<Self>;
    pub async fn fetch(&self, url: &str) -> Result<Vec<u8>, FetchError>;
}
pub async fn fetch_and_store(fetcher: &ImageFetcher, upload_dir: &Path, url: &str) -> Result<StoredImage, FetchError>;
/// 依 urls 順序回傳；最多 FETCH_CONCURRENCY 個同時下載
pub async fn fetch_all(fetcher: &ImageFetcher, upload_dir: &Path, urls: &[String]) -> Vec<(String, Result<StoredImage, FetchError>)>;
```

```rust
// api/src/import/apply.rs
#[derive(Debug, Clone, Serialize)]
pub struct ImportWarning { pub external_ref: String, pub row: u32, pub message: String }
#[derive(Debug, Clone, Serialize)]
pub struct ImportedProduct { pub external_ref: String, pub id: Uuid, pub name: String, pub created: bool, pub images: usize }
#[derive(Debug, Clone, Serialize, Default)]
pub struct ImportResult { pub created: usize, pub updated: usize, pub products: Vec<ImportedProduct>, pub warnings: Vec<ImportWarning> }
pub async fn apply(db: &PgPool, upload_dir: &Path, fetcher: &ImageFetcher, parsed: &ParsedImport) -> Result<ImportResult, ApiError>;
```

```rust
// api/src/domain/products.rs（Task 1 新增）
pub const MAX_PRICE: i32 = 9_999_999;
// ProductInput 多一個欄位：pub external_ref: Option<String>（serde default；UPDATE 用 COALESCE，None 不會清掉舊值）
pub async fn find_by_external_ref(db: &PgPool, external_ref: &str) -> Result<Option<AdminProduct>, ApiError>;
pub async fn existing_external_refs(db: &PgPool, refs: &[String]) -> Result<HashSet<String>, ApiError>;
// api/src/domain/categories.rs（Task 1 新增）
pub async fn find_or_create_by_name(db: &PgPool, name: &str) -> Result<Category, ApiError>;   // 同名取 ORDER BY sort_order, id 第一個；沒有就 create(db, None, name, 0)
```

```rust
// api/src/routes/admin_import.rs（Task 4）
// POST /api/admin/import/preview   multipart: file=<xlsx>
// POST /api/admin/import/commit    multipart: file=<xlsx>, fingerprint=<sha256 hex>
#[derive(Serialize)] pub struct PreviewResponse { pub fingerprint: String, pub product_count: usize, pub variant_count: usize, pub new_count: usize, pub update_count: usize, pub parsed: ParsedImport }
#[derive(Serialize)] pub struct CommitResponse { pub fingerprint: String, pub result: ImportResult }
```

```ts
// web/src/lib/types.ts（Task 5 新增）
export interface ImportRowError { row: number; column: string | null; message: string }
export interface ImportVariant { row: number; option1_value: string | null; option2_value: string | null; sku: string | null; price: number; stock: number | null }
export interface ImportProduct { external_ref: string; first_row: number; name: string; description: string | null; category: string | null; option1_name: string | null; option2_name: string | null; image_urls: string[]; variants: ImportVariant[] }
export interface ImportParsed { sheet: string; header_row: number; row_count: number; products: ImportProduct[]; errors: ImportRowError[]; unmatched_columns: string[] }
export interface ImportPreview { fingerprint: string; product_count: number; variant_count: number; new_count: number; update_count: number; parsed: ImportParsed }
export interface ImportWarning { external_ref: string; row: number; message: string }
export interface ImportedProduct { external_ref: string; id: string; name: string; created: boolean; images: number }
export interface ImportResult { created: number; updated: number; products: ImportedProduct[]; warnings: ImportWarning[] }
export interface ImportCommit { fingerprint: string; result: ImportResult }
```

## 與規格不同之處（接續計畫 4 的 48 條，從 49 起編號）

49. **commit 重新上傳同一個檔案並帶 sha256 指紋**，伺服器不保存預覽狀態（規格只說「preview → 老闆確認 → commit」）。指紋不符 → 400 `VALIDATION` `fields.fingerprint`「檔案已變更，請重新預覽」。
50. **有任何列錯誤就拒絕 commit**（400 `VALIDATION` `fields.rows`「檔案有 N 列錯誤，請先修正再匯入」）；圖片下載失敗只是警告（規格原文），其他錯誤規格沒說，這裡選「全部修好才進」而非「跳過壞列」，避免老闆以為全進了。
51. **更新既有商品時以現有商品為底、只覆蓋工作表有填的欄位**：slug、status、sort_order、`compare_at_price`、`is_active` 保留；描述／分類／規格名稱工作表空白就保留；`庫存` 空白保留現值（新商品為 0）；圖片全部下載失敗或工作表沒填網址時保留原圖；規格以 `(規格選項1, 規格選項2)` 對回既有 `id`。規格只說「更新既有商品」。
52. **`價格` 為必填、上限 `MAX_PRICE = 9_999_999`**（`products::validate` 一併套用到後台表單）；`庫存` 選填；`SKU` 選填（≤ 60 字）。
53. **標題列偵測用啟發式**：第一個「對到 `商品名稱` 且對到 ≥ 3 個範本欄位」的列是標題列，之前的列忽略（蝦皮範本前幾列是說明）；`商品編號` 空白但有規格選項／價格的列，掛到前一個商品；整列空白跳過。別名表是暫定的，要對著使用者的真檔調（規格 §13 原文、§17 第 6 點）。
54. **分類依名稱找或建**：同名取 `ORDER BY sort_order, id` 第一個；沒有就用 `random_slug()` 建（`categories.name` 不是 unique）。
55. **正式環境啟動檢查**：`ECPAY_ENV=prod` 時三組憑證任一等於 `STAGE_*` 常數（含只有 HashKey 或 HashIV 相同）就拒絕啟動；`PUBLIC_BASE_URL` 必填且必須 `https://`；`COOKIE_SECURE` 不得為 false；`SMTP_HOST`＋`SMTP_FROM` 必填；`MAIL_LOG_BODY` 不得開（規格只列變數，沒寫檢查）。
56. **`LISTEN_ADDR`** 環境變數（預設 `0.0.0.0:8080`）與 SIGTERM graceful shutdown（規格沒列；`docker stop` 需要）。
57. **備份是 compose 的 `backup` sidecar**（`postgres:17` 映像跑 `backup.sh loop`，每 24 小時 `pg_dump -Fc` 到 `backups` volume、刪超過 14 天的），不是 host cron（老闆不用會 cron）；另有 `restore.sh`。
58. **`deploy/env.prod.example` 與根目錄 `.env.example` 分開**：正式範本全是佔位符，不含任何測試特店值；根目錄 `.env.example` 維持開發用（含公開 stage 值，加註解）。
59. **`deploy/docker-compose.smoke.yml`** 與 CI 的 docker build job（只 build 不 push）不在規格內，是驗證 Dockerfile 的唯一自動化手段。
60. **圖片下載不擋內網位址**（管理員自己貼的網址；測試也靠 127.0.0.1 假伺服器）；只擋非 http/https。

---

### Task 1: 商品與分類的匯入地基（`external_ref`、`MAX_PRICE`、依名稱找分類）

**Files:**
- Modify: `api/src/domain/products.rs`（`ProductInput`、`validate`、`create`、`update`、`map_product_db_error`；新增 `MAX_PRICE`、`find_by_external_ref`、`existing_external_refs`）
- Modify: `api/src/domain/categories.rs`（新增 `find_or_create_by_name`）
- Modify: `api/tests/common/mod.rs:169-190`（`active_product` 建構 `ProductInput` 時加 `external_ref: None`）
- Test: `api/tests/admin_products.rs`（新增 3 條）、`api/tests/categories.rs`（新增 1 條）

**Interfaces:**
- Consumes: 既有 `ProductInput`／`AdminProduct`／`create`／`update`／`get_admin`、`categories::create(db, None, name, 0)`、`categories::Category`。
- Produces: `products::MAX_PRICE: i32 = 9_999_999`；`ProductInput.external_ref: Option<String>`（`#[serde(default)]`）；`products::find_by_external_ref(db, &str) -> Result<Option<AdminProduct>, ApiError>`；`products::existing_external_refs(db, &[String]) -> Result<HashSet<String>, ApiError>`；`categories::find_or_create_by_name(db, &str) -> Result<Category, ApiError>`。

- [ ] **Step 1: 寫失敗的測試（商品）**

在 `api/tests/admin_products.rs` 檔尾加：

```rust
#[sqlx::test(migrations = "./migrations")]
async fn external_ref_is_kept_across_admin_updates(pool: PgPool) {
    use dog_shop_api::domain::products::{self, ImageInput, ProductInput, VariantInput};
    let created = products::create(
        &pool,
        ProductInput {
            name: "匯入狗糧".to_string(),
            slug: None,
            description: None,
            category_id: None,
            status: "draft".to_string(),
            option1_name: None,
            option2_name: None,
            sort_order: None,
            variants: vec![VariantInput {
                id: None,
                option1_value: None,
                option2_value: None,
                sku: None,
                price: 100,
                compare_at_price: None,
                stock: 1,
                is_active: None,
                image_path: None,
            }],
            images: Vec::<ImageInput>::new(),
            external_ref: Some("SHOPEE-1".to_string()),
        },
    )
    .await
    .unwrap();
    assert_eq!(created.product.external_ref.as_deref(), Some("SHOPEE-1"));

    // 後台表單 PUT 不帶 external_ref → 不能被清掉
    let app = common::app(pool.clone());
    let cookie = common::admin_cookie(&app, &pool).await;
    let mut body = sample_product("draft");
    body["name"] = json!("改名");
    let (status, _, _) = common::send(
        &app,
        common::req(
            "PUT",
            &format!("/api/admin/products/{}", created.product.id),
            Some(&cookie),
            Some(body),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let found = products::find_by_external_ref(&pool, "SHOPEE-1")
        .await
        .unwrap()
        .expect("external_ref 還在");
    assert_eq!(found.product.id, created.product.id);
    assert_eq!(found.product.name, "改名");

    // 同一個 external_ref 不能給第二個商品
    let (status, body, _) = common::send(
        &app,
        common::req(
            "POST",
            "/api/admin/products",
            Some(&cookie),
            Some({
                let mut b = sample_product("draft");
                b["external_ref"] = json!("SHOPEE-1");
                b
            }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["details"]["fields"]["external_ref"], json!("這個商品編號已經有別的商品在用"));

    let refs = products::existing_external_refs(
        &pool,
        &["SHOPEE-1".to_string(), "SHOPEE-2".to_string()],
    )
    .await
    .unwrap();
    assert!(refs.contains("SHOPEE-1") && !refs.contains("SHOPEE-2"));
}

#[sqlx::test(migrations = "./migrations")]
async fn price_above_max_is_rejected(pool: PgPool) {
    let app = common::app(pool.clone());
    let cookie = common::admin_cookie(&app, &pool).await;
    let mut body = sample_product("draft");
    body["variants"][0]["price"] = json!(10_000_000);
    let (status, body, _) = common::send(
        &app,
        common::req("POST", "/api/admin/products", Some(&cookie), Some(body)),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["details"]["fields"]["variants.0.price"], json!("價格最多 9,999,999"));
}
```

（`sample_product` 是該檔第 8 行既有的 helper；確認它的 `variants[0]` 是物件，若它的鍵名不同就照它的結構改索引。）

在 `api/tests/categories.rs` 檔尾加：

```rust
#[sqlx::test(migrations = "./migrations")]
async fn find_or_create_by_name_reuses_the_first_match(pool: PgPool) {
    use dog_shop_api::domain::categories;
    let a = categories::find_or_create_by_name(&pool, " 狗糧 ").await.unwrap();
    assert_eq!(a.name, "狗糧");
    let b = categories::find_or_create_by_name(&pool, "狗糧").await.unwrap();
    assert_eq!(a.id, b.id, "同名不重複建");
    // 已有兩個同名時取 sort_order 最小、id 最小的那個
    let older = categories::create(&pool, None, "零食", 0).await.unwrap();
    let _newer = categories::create(&pool, None, "零食", 5).await.unwrap();
    let c = categories::find_or_create_by_name(&pool, "零食").await.unwrap();
    assert_eq!(c.id, older.id);
    let err = categories::find_or_create_by_name(&pool, "   ").await.unwrap_err();
    assert_eq!(err.code(), "VALIDATION");
}
```

- [ ] **Step 2: 跑測試確認失敗**

Run: `DATABASE_URL=postgres://dog_shop:dog_shop@localhost:5435/dog_shop cargo test --manifest-path <worktree>/api/Cargo.toml --test admin_products external_ref --test categories find_or_create`
Expected: 編譯錯誤（`external_ref` 欄位、`find_by_external_ref`、`find_or_create_by_name` 不存在）。

- [ ] **Step 3: 實作**

`api/src/domain/products.rs`：

```rust
pub const MAX_PRICE: i32 = 9_999_999;
```

`ProductInput` 加欄位（放在 `images` 之後）：

```rust
    /// 匯入用：對應 products.external_ref（蝦皮商品編號）。後台表單不送；UPDATE 時 None 不會清掉舊值
    #[serde(default)]
    pub external_ref: Option<String>,
```

`ProductInput` 目前 `#[derive(Debug, Serialize)]`——路由層反序列化用的是哪個型別，先 `grep -n 'ProductInput' api/src/routes/admin_products.rs` 確認；若路由直接 `Json<ProductInput>`，那 `ProductInput` 應該已同時 derive `Deserialize`（否則編譯不過），照既有 derive 加 `#[serde(default)]` 即可。

`validate` 的價格迴圈裡加：

```rust
        if v.price > MAX_PRICE {
            errors.add(&format!("variants.{i}.price"), "價格最多 9,999,999");
        }
        if v.compare_at_price.is_some_and(|p| p > MAX_PRICE) {
            errors.add(&format!("variants.{i}.compare_at_price"), "原價最多 9,999,999");
        }
```

並加 `external_ref` 長度檢查（1～100 字、去頭尾空白）：

```rust
    if let Some(r) = clean(&input.external_ref)
        && r.chars().count() > 100
    {
        errors.add("external_ref", "商品編號最多 100 字");
    }
```

`create` 的 INSERT 加 `external_ref`（第 10 個參數）：

```rust
        "INSERT INTO products (id, slug, name, description, category_id, status, option1_name, option2_name, sort_order, external_ref)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
    // …既有 .bind 之後：
    .bind(clean(&input.external_ref))
```

`update` 的 UPDATE 加 `external_ref = COALESCE($10, external_ref)`，同樣 `.bind(clean(&input.external_ref))`。

`map_product_db_error`：既有分支是看 unique violation 的 constraint 名（slug）；加一個分支：constraint 名含 `external_ref`（`products_external_ref_key`）→ `ApiError::field("external_ref", "這個商品編號已經有別的商品在用")`。用 `grep -n 'fn map_product_db_error' -A 14 api/src/domain/products.rs` 看既有寫法照抄。

新函式（放在 `get_admin` 後面）：

```rust
/// 匯入用：依蝦皮商品編號找既有商品（含規格與圖片）
pub async fn find_by_external_ref(
    db: &PgPool,
    external_ref: &str,
) -> Result<Option<AdminProduct>, ApiError> {
    let id: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM products WHERE external_ref = $1")
            .bind(external_ref.trim())
            .fetch_optional(db)
            .await?;
    match id {
        Some(id) => get_admin(db, id).await,
        None => Ok(None),
    }
}

/// 匯入預覽用：這些商品編號裡哪些已經存在
pub async fn existing_external_refs(
    db: &PgPool,
    refs: &[String],
) -> Result<HashSet<String>, ApiError> {
    let found: Vec<String> =
        sqlx::query_scalar("SELECT external_ref FROM products WHERE external_ref = ANY($1)")
            .bind(refs)
            .fetch_all(db)
            .await?;
    Ok(found.into_iter().collect())
}
```

`api/src/domain/categories.rs` 加：

```rust
/// 匯入用：依名稱找分類，同名取 sort_order／id 最小的；沒有就建一個（slug 隨機）
pub async fn find_or_create_by_name(db: &PgPool, name: &str) -> Result<Category, ApiError> {
    let name = name.trim();
    validate_name(name)?;
    let found = sqlx::query_as::<_, Category>(
        "SELECT id, slug, name, sort_order FROM categories WHERE name = $1 ORDER BY sort_order, id LIMIT 1",
    )
    .bind(name)
    .fetch_optional(db)
    .await?;
    match found {
        Some(c) => Ok(c),
        None => create(db, None, name, 0).await,
    }
}
```

（`validate_name` 是該檔既有的私有函式，回 `ApiError::field("name", …)`；若它的簽章不是 `&str` 就配合。）

`api/tests/common/mod.rs` 的 `active_product` 建構 `ProductInput` 時加 `external_ref: None,`；全 repo `grep -rn 'ProductInput {' api/` 找出其他建構處（例如 `api/tests/products_public.rs`、`api/tests/orders.rs`）一併補上。

- [ ] **Step 4: 跑測試確認通過**

Run: `DATABASE_URL=… cargo test --manifest-path <worktree>/api/Cargo.toml --test admin_products --test categories --test products_public`
Expected: 全部 ok（含新增 3 條）。

- [ ] **Step 5: 全部閘門與 commit**

Run: `cargo fmt --all --check`、`cargo clippy --all-targets -- -D warnings`、完整 `cargo test`（背景、log）。
Commit：`feat(api): 商品 external_ref 寫入與查詢、價格上限、分類依名稱找或建（計畫 5 Task 1）`

---

### Task 2: xlsx 欄位對應與解析（`import/columns.rs`、`import/parse.rs`）

**Files:**
- Create: `api/src/import/mod.rs`、`api/src/import/columns.rs`、`api/src/import/parse.rs`
- Modify: `api/src/lib.rs`（`pub mod import;`）、`api/Cargo.toml`（`calamine` 依賴；`rust_xlsxwriter` dev 依賴）
- Test: 兩個檔案各自的 `#[cfg(test)] mod tests`；`api/tests/import_parse.rs`（真 xlsx 走 calamine）

**Interfaces:**
- Consumes: `crate::domain::products::{MAX_IMAGES, MAX_PRICE, MAX_VARIANTS}`。
- Produces: 共用介面裡 `columns.rs`／`parse.rs` 的全部項目（`Column`、`TEMPLATE_HEADERS`、`normalize_header`、`match_header`、`HeaderMap`、`map_headers`、`is_header_row`、`RowError`、`ImportVariant`、`ImportProduct`、`ParsedImport`、`ImportError`、`parse_grid`、`parse_xlsx`）、`import::MAX_XLSX_BYTES`、`import::MAX_ROWS`。

- [ ] **Step 1: 加依賴與模組骨架**

Run: `cargo add --manifest-path <worktree>/api/Cargo.toml calamine` 與 `cargo add --manifest-path <worktree>/api/Cargo.toml --dev rust_xlsxwriter`（記下裝到的版本，寫進報告）。

`api/src/lib.rs` 加 `pub mod import;`。`api/src/import/mod.rs`：

```rust
//! 商品匯入（規格 §13）：xlsx → 欄位對應 → 解析 → 下載圖片 → 寫入商品

pub mod apply;
pub mod columns;
pub mod images;
pub mod parse;

/// xlsx 上限 5 MB（規格沒定；蝦皮匯出通常 < 1 MB）
pub const MAX_XLSX_BYTES: usize = 5 * 1024 * 1024;
/// 資料列上限（標題列之後）
pub const MAX_ROWS: usize = 5000;

pub use apply::{ImportResult, ImportWarning, ImportedProduct, apply};
pub use images::{FetchError, ImageFetcher, fetch_all, fetch_and_store};
pub use parse::{
    ImportError, ImportProduct, ImportVariant, ParsedImport, RowError, parse_grid, parse_xlsx,
};
```

本任務先只建 `columns.rs` 與 `parse.rs`；`apply.rs`、`images.rs` 先放空殼讓它編譯：

```rust
// api/src/import/images.rs（Task 3 會整個換掉）
//! 圖片下載（Task 3）
pub const FETCH_TIMEOUT_SECS: u64 = 10;
```

```rust
// api/src/import/apply.rs（Task 4 會整個換掉）
//! 寫入商品（Task 4）
```

對應地 `mod.rs` 的兩行 `pub use apply::…`／`pub use images::…` 本任務先註解掉，Task 3／4 再打開。

- [ ] **Step 2: 寫失敗的單元測試（columns）**

`api/src/import/columns.rs` 檔尾：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_fullwidth_and_spaces() {
        assert_eq!(normalize_header(" 規格名稱 １ "), "規格名稱1");
        assert_eq!(normalize_header("商品圖片　２"), "商品圖片2");
        assert_eq!(normalize_header("SKU"), "sku");
        assert_eq!(normalize_header("商品ID（父）"), "商品id(父)");
    }

    #[test]
    fn matches_template_and_shopee_aliases() {
        for (h, c) in [
            ("商品編號", Column::ExternalRef),
            ("商品ID", Column::ExternalRef),
            ("父SKU", Column::ExternalRef),
            ("主商品貨號", Column::ExternalRef),
            ("商品名稱", Column::Name),
            ("商品描述", Column::Description),
            ("分類", Column::Category),
            ("類別", Column::Category),
            ("規格名稱1", Column::Option1Name),
            ("規格名稱 1", Column::Option1Name),
            ("規格選項1", Column::Option1Value),
            ("規格選項 2", Column::Option2Value),
            ("價格", Column::Price),
            ("售價", Column::Price),
            ("庫存", Column::Stock),
            ("數量", Column::Stock),
            ("SKU", Column::Sku),
            ("商品選項貨號", Column::Sku),
            ("圖片網址", Column::ImageUrls),
            ("商品圖片 1", Column::Image(1)),
            ("商品圖片9", Column::Image(9)),
            ("圖片3", Column::Image(3)),
        ] {
            assert_eq!(match_header(h), Some(c), "{h}");
        }
        assert_eq!(match_header("商品圖片 10"), None);
        assert_eq!(match_header("品牌"), None);
        assert_eq!(match_header(""), None);
    }

    #[test]
    fn maps_headers_and_reports_unmatched_and_duplicates() {
        let row: Vec<String> = ["商品編號", "商品名稱", "品牌", "價格", "價格", "商品圖片 1", "商品圖片 2"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let m = map_headers(&row);
        assert_eq!(m.columns[0], Some(Column::ExternalRef));
        assert_eq!(m.columns[1], Some(Column::Name));
        assert_eq!(m.columns[2], None);
        assert_eq!(m.columns[3], Some(Column::Price));
        assert_eq!(m.columns[4], None, "重複的欄位第二個不對應");
        assert_eq!(m.columns[5], Some(Column::Image(1)));
        assert_eq!(m.unmatched, vec!["品牌".to_string(), "價格".to_string()]);
        assert!(is_header_row(&row));
        let not: Vec<String> = ["請依範本填寫", "", "商品名稱"].iter().map(|s| s.to_string()).collect();
        assert!(!is_header_row(&not));
    }
}
```

- [ ] **Step 3: 實作 columns.rs**

```rust
//! 匯入欄位：範本標題（規格 §13）與蝦皮匯出檔的別名（暫定，拿到真檔後補齊）

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Column {
    ExternalRef,
    Name,
    Description,
    Category,
    Option1Name,
    Option1Value,
    Option2Name,
    Option2Value,
    Price,
    Stock,
    Sku,
    /// 「圖片網址」：一格裡逗號分隔多個
    ImageUrls,
    /// 「商品圖片 1」～「商品圖片 9」：一格一個
    Image(u8),
}

impl Column {
    /// 範本欄名（錯誤訊息與前端顯示用）
    pub fn label(self) -> String {
        match self {
            Column::ExternalRef => "商品編號".into(),
            Column::Name => "商品名稱".into(),
            Column::Description => "商品描述".into(),
            Column::Category => "分類".into(),
            Column::Option1Name => "規格名稱1".into(),
            Column::Option1Value => "規格選項1".into(),
            Column::Option2Name => "規格名稱2".into(),
            Column::Option2Value => "規格選項2".into(),
            Column::Price => "價格".into(),
            Column::Stock => "庫存".into(),
            Column::Sku => "SKU".into(),
            Column::ImageUrls => "圖片網址".into(),
            Column::Image(n) => format!("商品圖片{n}"),
        }
    }
}

pub const TEMPLATE_HEADERS: [&str; 12] = [
    "商品編號", "商品名稱", "商品描述", "分類", "規格名稱1", "規格選項1", "規格名稱2", "規格選項2",
    "價格", "庫存", "SKU", "圖片網址",
];

/// 標題正規化：去所有空白（含全形空白）、全形英數與括號轉半形、英文小寫
pub fn normalize_header(raw: &str) -> String {
    raw.chars()
        .filter(|c| !c.is_whitespace() && *c != '\u{3000}')
        .map(|c| match c {
            '\u{FF10}'..='\u{FF19}' => char::from_u32(c as u32 - 0xFF10 + '0' as u32).unwrap_or(c),
            '\u{FF21}'..='\u{FF3A}' => char::from_u32(c as u32 - 0xFF21 + 'A' as u32).unwrap_or(c),
            '\u{FF41}'..='\u{FF5A}' => char::from_u32(c as u32 - 0xFF41 + 'a' as u32).unwrap_or(c),
            '（' => '(',
            '）' => ')',
            other => other,
        })
        .flat_map(|c| c.to_lowercase())
        .collect()
}

/// 別名表（正規化後比對）。同一個 Column 可有多個別名；「商品圖片N」用前綴＋數字處理
const ALIASES: &[(&str, Column)] = &[
    ("商品編號", Column::ExternalRef),
    ("商品id", Column::ExternalRef),
    ("商品id(父)", Column::ExternalRef),
    ("父sku", Column::ExternalRef),
    ("主商品貨號", Column::ExternalRef),
    ("商品貨號(父)", Column::ExternalRef),
    ("商品名稱", Column::Name),
    ("名稱", Column::Name),
    ("商品描述", Column::Description),
    ("描述", Column::Description),
    ("商品說明", Column::Description),
    ("分類", Column::Category),
    ("類別", Column::Category),
    ("商品分類", Column::Category),
    ("規格名稱1", Column::Option1Name),
    ("規格1名稱", Column::Option1Name),
    ("選項名稱1", Column::Option1Name),
    ("規格選項1", Column::Option1Value),
    ("規格1", Column::Option1Value),
    ("選項1", Column::Option1Value),
    ("規格名稱2", Column::Option2Name),
    ("規格2名稱", Column::Option2Name),
    ("選項名稱2", Column::Option2Name),
    ("規格選項2", Column::Option2Value),
    ("規格2", Column::Option2Value),
    ("選項2", Column::Option2Value),
    ("價格", Column::Price),
    ("售價", Column::Price),
    ("單價", Column::Price),
    ("庫存", Column::Stock),
    ("數量", Column::Stock),
    ("庫存數量", Column::Stock),
    ("sku", Column::Sku),
    ("商品選項貨號", Column::Sku),
    ("規格貨號", Column::Sku),
    ("貨號", Column::Sku),
    ("圖片網址", Column::ImageUrls),
    ("圖片", Column::ImageUrls),
    ("商品圖片", Column::ImageUrls),
];

pub fn match_header(raw: &str) -> Option<Column> {
    let key = normalize_header(raw);
    if key.is_empty() {
        return None;
    }
    if let Some((_, c)) = ALIASES.iter().find(|(alias, _)| *alias == key) {
        return Some(*c);
    }
    for prefix in ["商品圖片", "圖片", "主圖"] {
        if let Some(rest) = key.strip_prefix(prefix)
            && let Ok(n) = rest.parse::<u8>()
            && (1..=9).contains(&n)
        {
            return Some(Column::Image(n));
        }
    }
    None
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeaderMap {
    /// index = 工作表欄位；None = 對不上或重複
    pub columns: Vec<Option<Column>>,
    /// 對不上的原始標題（給預覽頁列出）
    pub unmatched: Vec<String>,
}

pub fn map_headers(row: &[String]) -> HeaderMap {
    let mut seen: std::collections::HashSet<Column> = std::collections::HashSet::new();
    let mut columns = Vec::with_capacity(row.len());
    let mut unmatched = Vec::new();
    for raw in row {
        let trimmed = raw.trim();
        match match_header(trimmed) {
            Some(c) if seen.insert(c) => columns.push(Some(c)),
            Some(_) => {
                unmatched.push(trimmed.to_string());
                columns.push(None);
            }
            None => {
                if !trimmed.is_empty() {
                    unmatched.push(trimmed.to_string());
                }
                columns.push(None);
            }
        }
    }
    HeaderMap { columns, unmatched }
}

/// 標題列：對到「商品名稱」且對到至少 3 個欄位
pub fn is_header_row(row: &[String]) -> bool {
    let m = map_headers(row);
    let matched = m.columns.iter().flatten().count();
    matched >= 3 && m.columns.contains(&Some(Column::Name))
}
```

- [ ] **Step 4: 跑 columns 測試**

Run: `cargo test --manifest-path <worktree>/api/Cargo.toml --lib import::columns`
Expected: 3 passed。

- [ ] **Step 5: 寫失敗的單元測試（parse）**

`api/src/import/parse.rs` 檔尾：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn grid(rows: &[&[&str]]) -> Vec<Vec<String>> {
        rows.iter()
            .map(|r| r.iter().map(|s| s.to_string()).collect())
            .collect()
    }

    const HEADER: &[&str] = &[
        "商品編號", "商品名稱", "商品描述", "分類", "規格名稱1", "規格選項1", "規格名稱2", "規格選項2",
        "價格", "庫存", "SKU", "圖片網址",
    ];

    #[test]
    fn groups_rows_by_external_ref_and_skips_instruction_rows() {
        let g = grid(&[
            &["請依範本填寫，第一列是說明"],
            &[],
            HEADER,
            &["A1", "狗糧 5kg", "很好吃\n第二行", "狗糧", "口味", "雞肉", "", "", "1,200", "10", "DOG-C", "https://img.example/a.jpg, https://img.example/b.jpg"],
            &["", "", "", "", "", "牛肉", "", "", "1300", "", "DOG-B", ""],
            &["A1", "", "", "", "", "魚肉", "", "", "1400", "3", "", ""],
            &["", "", "", "", "", "", "", "", "", "", "", ""],
            &["B2", "玩具球", "", "", "", "", "", "", "99", "0", "", ""],
        ]);
        let p = parse_grid("工作表1", &g).unwrap();
        assert_eq!(p.header_row, 3);
        assert_eq!(p.row_count, 4);
        assert!(p.errors.is_empty(), "{:?}", p.errors);
        assert_eq!(p.products.len(), 2);
        let a = &p.products[0];
        assert_eq!(a.external_ref, "A1");
        assert_eq!(a.first_row, 4);
        assert_eq!(a.name, "狗糧 5kg");
        assert_eq!(a.description.as_deref(), Some("很好吃\n第二行"));
        assert_eq!(a.category.as_deref(), Some("狗糧"));
        assert_eq!(a.option1_name.as_deref(), Some("口味"));
        assert_eq!(a.option2_name, None);
        assert_eq!(a.image_urls, vec!["https://img.example/a.jpg", "https://img.example/b.jpg"]);
        assert_eq!(a.variants.len(), 3);
        assert_eq!(a.variants[0], ImportVariant { row: 4, option1_value: Some("雞肉".into()), option2_value: None, sku: Some("DOG-C".into()), price: 1200, stock: Some(10) });
        assert_eq!(a.variants[1].price, 1300);
        assert_eq!(a.variants[1].stock, None, "庫存空白 = None");
        assert_eq!(a.variants[2].row, 6);
        let b = &p.products[1];
        assert_eq!(b.name, "玩具球");
        assert_eq!(b.option1_name, None);
        assert_eq!(b.variants.len(), 1);
        assert_eq!(p.variant_count(), 4);
        assert!(p.unmatched_columns.is_empty());
    }

    #[test]
    fn reports_row_errors_with_column_names() {
        // 孤兒列要放在任何商品之前，否則會被接到前一個商品（那是刻意的行為，見第一個測試）
        let g = grid(&[
            HEADER,
            &["", "孤兒列", "", "", "", "紅", "", "", "50", "", "", ""],
            &["A1", "", "", "", "", "", "", "", "100", "", "", ""],
            &["C3", "壞價格", "", "", "顏色", "紅", "", "", "abc", "-1", "", "not-a-url"],
            &["C3", "", "", "", "", "紅", "", "", "60", "", "", ""],
            &["D4", "沒規格名卻多列", "", "", "", "紅", "", "", "60", "", "", ""],
            &["D4", "", "", "", "", "藍", "", "", "60", "", "", ""],
            &["E5", "太貴", "", "", "", "", "", "", "10000000", "", "", ""],
        ]);
        let p = parse_grid("s", &g).unwrap();
        let msgs: Vec<(u32, Option<String>, String)> = p
            .errors
            .iter()
            .map(|e| (e.row, e.column.clone(), e.message.clone()))
            .collect();
        assert!(msgs.contains(&(2, Some("商品編號".into()), "這一列沒有商品編號，也接不到上一個商品".into())), "{msgs:?}");
        assert!(msgs.contains(&(3, Some("商品名稱".into()), "商品名稱必填".into())), "{msgs:?}");
        assert!(msgs.contains(&(4, Some("價格".into()), "價格要是 0 以上的整數".into())), "{msgs:?}");
        assert!(msgs.contains(&(4, Some("庫存".into()), "庫存要是 0 以上的整數".into())), "{msgs:?}");
        assert!(msgs.contains(&(4, Some("圖片網址".into()), "不是 http(s) 網址：not-a-url".into())), "{msgs:?}");
        assert!(msgs.contains(&(5, Some("規格選項1".into()), "規格組合重複".into())), "{msgs:?}");
        assert!(msgs.contains(&(7, Some("規格名稱1".into()), "沒有規格名稱時只能有一列".into())), "{msgs:?}");
        assert!(msgs.contains(&(8, Some("價格".into()), "價格最多 9,999,999".into())), "{msgs:?}");
        // 有錯的商品仍會出現在 products（預覽要列出來），但錯誤數 > 0 就不能 commit；孤兒列不是商品
        assert_eq!(p.products.len(), 4);
    }

    #[test]
    fn missing_required_columns_and_no_header() {
        let g = grid(&[&["商品名稱", "價格", "庫存"], &["x", "1", "2"]]);
        match parse_grid("s", &g) {
            Err(ImportError::MissingColumns(cols)) => assert_eq!(cols, "商品編號"),
            other => panic!("{other:?}"),
        }
        let g = grid(&[&["a", "b"], &["c", "d"]]);
        assert!(matches!(parse_grid("s", &g), Err(ImportError::NoHeader)));
    }

    #[test]
    fn shopee_style_image_columns_and_fullwidth_headers() {
        let g = grid(&[
            &["商品ID", "商品名稱", "規格名稱 1", "規格選項 1", "售價", "數量", "商品圖片 1", "商品圖片 2", "品牌"],
            &["S1", "蝦皮商品", "尺寸", "S", "３５０", "5", "https://a/1.jpg", "https://a/2.jpg", "無"],
            &["S1", "", "", "M", "350", "5", "", "", ""],
        ]);
        let p = parse_grid("s", &g).unwrap();
        assert!(p.errors.is_empty(), "{:?}", p.errors);
        assert_eq!(p.unmatched_columns, vec!["品牌".to_string()]);
        let s = &p.products[0];
        assert_eq!(s.image_urls, vec!["https://a/1.jpg", "https://a/2.jpg"]);
        assert_eq!(s.variants[0].price, 350, "全形數字也要能讀");
        assert_eq!(s.variants.len(), 2);
    }

    #[test]
    fn too_many_rows_and_too_many_images() {
        let mut rows: Vec<Vec<String>> = vec![HEADER.iter().map(|s| s.to_string()).collect()];
        for i in 0..(crate::import::MAX_ROWS + 1) {
            rows.push(vec![format!("R{i}"), "x".into(), "".into(), "".into(), "".into(), "".into(), "".into(), "".into(), "1".into(), "".into(), "".into(), "".into()]);
        }
        assert!(matches!(parse_grid("s", &rows), Err(ImportError::TooManyRows(_))));
        let urls = (0..10).map(|i| format!("https://a/{i}.jpg")).collect::<Vec<_>>().join(",");
        let g = grid(&[HEADER, &["A", "x", "", "", "", "", "", "", "1", "", "", &urls]]);
        let p = parse_grid("s", &g).unwrap();
        assert_eq!(p.errors[0].message, "圖片最多 9 張");
    }
}
```

- [ ] **Step 6: 實作 parse.rs**

```rust
//! xlsx → 字串格 → ParsedImport（規格 §13；與規格不同之處 52、53）

use std::collections::HashMap;
use std::io::Cursor;

use calamine::{Data, Reader, Xlsx, open_workbook_from_rs};
use serde::Serialize;

use crate::domain::products::{MAX_IMAGES, MAX_PRICE, MAX_VARIANTS};
use crate::import::MAX_ROWS;
use crate::import::columns::{Column, HeaderMap, is_header_row, map_headers};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RowError {
    /// 工作表 1-based 列號
    pub row: u32,
    /// 範本欄名；整列的錯誤是 None
    pub column: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ImportVariant {
    pub row: u32,
    pub option1_value: Option<String>,
    pub option2_value: Option<String>,
    pub sku: Option<String>,
    pub price: i32,
    /// 空白 = 保留現值（新商品為 0）
    pub stock: Option<i32>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ImportProduct {
    pub external_ref: String,
    pub first_row: u32,
    pub name: String,
    pub description: Option<String>,
    pub category: Option<String>,
    pub option1_name: Option<String>,
    pub option2_name: Option<String>,
    pub image_urls: Vec<String>,
    pub variants: Vec<ImportVariant>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct ParsedImport {
    pub sheet: String,
    pub header_row: u32,
    /// 標題列之後、非整列空白的列數
    pub row_count: u32,
    pub products: Vec<ImportProduct>,
    pub errors: Vec<RowError>,
    pub unmatched_columns: Vec<String>,
}

impl ParsedImport {
    pub fn variant_count(&self) -> usize {
        self.products.iter().map(|p| p.variants.len()).sum()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("檔案不是 xlsx 或已損壞")]
    Unreadable,
    #[error("找不到標題列（需要「商品名稱」以及至少兩個其他範本欄位）")]
    NoHeader,
    #[error("缺少必要欄位：{0}")]
    MissingColumns(String),
    #[error("資料列超過 {0} 列")]
    TooManyRows(usize),
}

/// 儲存格 → 字串：數字去掉 .0；日期／錯誤當空白；去頭尾空白
fn cell_to_string(d: &Data) -> String {
    match d {
        Data::String(s) => s.trim().to_string(),
        Data::Int(i) => i.to_string(),
        Data::Float(f) if f.fract() == 0.0 && f.abs() < 1e15 => format!("{}", *f as i64),
        Data::Float(f) => f.to_string(),
        Data::Bool(b) => b.to_string(),
        Data::DateTimeIso(s) | Data::DurationIso(s) => s.trim().to_string(),
        Data::DateTime(_) | Data::Error(_) | Data::Empty => String::new(),
    }
}

/// 第一個工作表 → 字串格 → parse_grid
pub fn parse_xlsx(bytes: &[u8]) -> Result<ParsedImport, ImportError> {
    let mut wb: Xlsx<_> =
        open_workbook_from_rs(Cursor::new(bytes.to_vec())).map_err(|_| ImportError::Unreadable)?;
    let name = wb.sheet_names().first().cloned().ok_or(ImportError::Unreadable)?;
    let range = wb.worksheet_range(&name).map_err(|_| ImportError::Unreadable)?;
    let grid: Vec<Vec<String>> = range
        .rows()
        .map(|r| r.iter().map(cell_to_string).collect())
        .collect();
    parse_grid(&name, &grid)
}

fn blank(s: &str) -> bool {
    s.trim().is_empty()
}

fn opt(s: &str) -> Option<String> {
    let t = s.trim();
    (!t.is_empty()).then(|| t.to_string())
}

/// 「1,200」「NT$1200」「３５０」→ 1200；非負整數才算合法
fn parse_amount(raw: &str) -> Option<i64> {
    let digits: String = raw
        .chars()
        .filter_map(|c| match c {
            '0'..='9' => Some(c),
            '\u{FF10}'..='\u{FF19}' => char::from_u32(c as u32 - 0xFF10 + '0' as u32),
            _ => None,
        })
        .collect();
    let has_minus = raw.contains('-') || raw.contains('－');
    if digits.is_empty() || has_minus {
        return None;
    }
    // 「1.5」這種小數不算整數
    let cleaned: String = raw.chars().filter(|c| !c.is_whitespace() && *c != ',').collect();
    if cleaned.contains('.') && !cleaned.ends_with(".0") && !cleaned.ends_with(".00") {
        return None;
    }
    digits.parse::<i64>().ok()
}

fn split_urls(raw: &str) -> Vec<String> {
    raw.split(|c| c == ',' || c == '，' || c == '\n' || c == ';' || c == '；')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

struct Row<'a> {
    map: &'a HeaderMap,
    cells: &'a [String],
}

impl Row<'_> {
    fn get(&self, col: Column) -> &str {
        self.map
            .columns
            .iter()
            .position(|c| *c == Some(col))
            .and_then(|i| self.cells.get(i))
            .map(String::as_str)
            .unwrap_or("")
    }
    fn is_blank(&self) -> bool {
        self.cells.iter().all(|c| blank(c))
    }
    fn image_urls(&self) -> Vec<String> {
        let mut urls = split_urls(self.get(Column::ImageUrls));
        for n in 1..=9u8 {
            if let Some(u) = opt(self.get(Column::Image(n))) {
                urls.push(u);
            }
        }
        urls
    }
}

pub fn parse_grid(sheet: &str, grid: &[Vec<String>]) -> Result<ParsedImport, ImportError> {
    let header_idx = grid.iter().position(|r| is_header_row(r)).ok_or(ImportError::NoHeader)?;
    let map = map_headers(&grid[header_idx]);
    let missing: Vec<String> = [Column::ExternalRef, Column::Name, Column::Price]
        .into_iter()
        .filter(|c| !map.columns.contains(&Some(*c)))
        .map(Column::label)
        .collect();
    if !missing.is_empty() {
        return Err(ImportError::MissingColumns(missing.join("、")));
    }
    let data_rows = &grid[header_idx + 1..];
    if data_rows.len() > MAX_ROWS {
        return Err(ImportError::TooManyRows(MAX_ROWS));
    }

    let mut out = ParsedImport {
        sheet: sheet.to_string(),
        header_row: header_idx as u32 + 1,
        unmatched_columns: map.unmatched.clone(),
        ..Default::default()
    };
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut last: Option<usize> = None;

    for (offset, cells) in data_rows.iter().enumerate() {
        let row_no = (header_idx + 1 + offset) as u32 + 1;
        let row = Row { map: &map, cells };
        if row.is_blank() {
            continue;
        }
        out.row_count += 1;
        let external_ref = row.get(Column::ExternalRef).trim().to_string();
        let target: Option<usize> = if !external_ref.is_empty() {
            if external_ref.chars().count() > 100 {
                out.errors.push(RowError { row: row_no, column: Some(Column::ExternalRef.label()), message: "商品編號最多 100 字".into() });
            }
            match index.get(&external_ref) {
                Some(&i) => Some(i),
                None => {
                    let name = row.get(Column::Name).trim().to_string();
                    if name.is_empty() {
                        out.errors.push(RowError { row: row_no, column: Some(Column::Name.label()), message: "商品名稱必填".into() });
                    } else if name.chars().count() > 120 {
                        out.errors.push(RowError { row: row_no, column: Some(Column::Name.label()), message: "商品名稱最多 120 字".into() });
                    }
                    let image_urls = row.image_urls();
                    for u in &image_urls {
                        if !(u.starts_with("http://") || u.starts_with("https://")) {
                            out.errors.push(RowError { row: row_no, column: Some(Column::ImageUrls.label()), message: format!("不是 http(s) 網址：{u}") });
                        }
                    }
                    if image_urls.len() > MAX_IMAGES {
                        out.errors.push(RowError { row: row_no, column: Some(Column::ImageUrls.label()), message: format!("圖片最多 {MAX_IMAGES} 張") });
                    }
                    out.products.push(ImportProduct {
                        external_ref: external_ref.clone(),
                        first_row: row_no,
                        name,
                        description: opt(row.get(Column::Description)),
                        category: opt(row.get(Column::Category)),
                        option1_name: opt(row.get(Column::Option1Name)),
                        option2_name: opt(row.get(Column::Option2Name)),
                        image_urls,
                        variants: Vec::new(),
                    });
                    let i = out.products.len() - 1;
                    index.insert(external_ref.clone(), i);
                    Some(i)
                }
            }
        } else {
            let has_variant_data = !blank(row.get(Column::Option1Value))
                || !blank(row.get(Column::Option2Value))
                || !blank(row.get(Column::Price));
            if !has_variant_data {
                continue;
            }
            match last {
                Some(i) => Some(i),
                None => {
                    out.errors.push(RowError { row: row_no, column: Some(Column::ExternalRef.label()), message: "這一列沒有商品編號，也接不到上一個商品".into() });
                    None
                }
            }
        };
        let Some(i) = target else { continue };
        last = Some(i);

        // 規格
        let price = match parse_amount(row.get(Column::Price)) {
            Some(p) if p <= MAX_PRICE as i64 => p as i32,
            Some(_) => {
                out.errors.push(RowError { row: row_no, column: Some(Column::Price.label()), message: "價格最多 9,999,999".into() });
                0
            }
            None => {
                let msg = if blank(row.get(Column::Price)) { "價格必填" } else { "價格要是 0 以上的整數" };
                out.errors.push(RowError { row: row_no, column: Some(Column::Price.label()), message: msg.into() });
                0
            }
        };
        let stock = if blank(row.get(Column::Stock)) {
            None
        } else {
            match parse_amount(row.get(Column::Stock)) {
                Some(s) if s <= i32::MAX as i64 => Some(s as i32),
                _ => {
                    out.errors.push(RowError { row: row_no, column: Some(Column::Stock.label()), message: "庫存要是 0 以上的整數".into() });
                    None
                }
            }
        };
        let sku = opt(row.get(Column::Sku));
        if sku.as_ref().is_some_and(|s| s.chars().count() > 60) {
            out.errors.push(RowError { row: row_no, column: Some(Column::Sku.label()), message: "SKU 最多 60 字".into() });
        }
        let variant = ImportVariant {
            row: row_no,
            option1_value: opt(row.get(Column::Option1Value)),
            option2_value: opt(row.get(Column::Option2Value)),
            sku,
            price,
            stock,
        };
        let product = &mut out.products[i];
        if product
            .variants
            .iter()
            .any(|v| v.option1_value == variant.option1_value && v.option2_value == variant.option2_value)
        {
            out.errors.push(RowError { row: row_no, column: Some(Column::Option1Value.label()), message: "規格組合重複".into() });
        }
        product.variants.push(variant);
    }

    // 商品層級的一致性檢查
    for p in &out.products {
        if p.option1_name.is_none() && p.variants.len() > 1 {
            out.errors.push(RowError { row: p.variants[1].row, column: Some(Column::Option1Name.label()), message: "沒有規格名稱時只能有一列".into() });
        }
        if p.option2_name.is_some() && p.option1_name.is_none() {
            out.errors.push(RowError { row: p.first_row, column: Some(Column::Option2Name.label()), message: "要先有規格名稱1 才能有規格名稱2".into() });
        }
        if p.option1_name.is_some() && p.variants.iter().any(|v| v.option1_value.is_none()) {
            let r = p.variants.iter().find(|v| v.option1_value.is_none()).map(|v| v.row).unwrap_or(p.first_row);
            out.errors.push(RowError { row: r, column: Some(Column::Option1Value.label()), message: "有規格名稱時每一列都要填規格選項1".into() });
        }
        if p.variants.len() > MAX_VARIANTS {
            out.errors.push(RowError { row: p.first_row, column: None, message: format!("規格最多 {MAX_VARIANTS} 個") });
        }
        if p.variants.is_empty() {
            out.errors.push(RowError { row: p.first_row, column: Some(Column::Price.label()), message: "至少要有一列規格（價格）".into() });
        }
    }
    out.errors.sort_by_key(|e| (e.row, e.column.clone()));
    Ok(out)
}
```

（第一個測試裡 `A1` 的第 3 列 `庫存` 空白、第 4 列 `SKU` 空白等都靠 `opt`／`stock: None` 處理；「規格組合重複」在同一商品內比較 `clean` 後的值。）

- [ ] **Step 7: 跑 parse 單元測試**

Run: `cargo test --manifest-path <worktree>/api/Cargo.toml --lib import::`
Expected: columns 3 + parse 5 全 ok。若「reports_row_errors」對訊息文字有出入，以本計畫的文字為準修實作，不改測試。

- [ ] **Step 8: 真 xlsx 走 calamine 的整合測試**

`api/tests/import_parse.rs`：

```rust
use dog_shop_api::import::{ImportError, parse_xlsx};
use rust_xlsxwriter::Workbook;

fn sheet(rows: &[&[&str]]) -> Vec<u8> {
    let mut wb = Workbook::new();
    let ws = wb.add_worksheet();
    for (r, row) in rows.iter().enumerate() {
        for (c, cell) in row.iter().enumerate() {
            // 純數字寫成數字格，測 Float → 整數字串
            if let Ok(n) = cell.parse::<f64>() {
                ws.write_number(r as u32, c as u16, n).unwrap();
            } else {
                ws.write_string(r as u32, c as u16, *cell).unwrap();
            }
        }
    }
    wb.save_to_buffer().unwrap()
}

#[test]
fn parses_a_real_xlsx_with_numeric_cells() {
    let bytes = sheet(&[
        &["商品編號", "商品名稱", "價格", "庫存", "圖片網址"],
        &["A1", "狗糧", "1200", "10", "https://img.example/a.jpg"],
        &["B2", "玩具", "99.0", "", ""],
    ]);
    let p = parse_xlsx(&bytes).unwrap();
    assert_eq!(p.products.len(), 2);
    assert_eq!(p.products[0].variants[0].price, 1200);
    assert_eq!(p.products[0].variants[0].stock, Some(10));
    assert_eq!(p.products[1].variants[0].price, 99);
    assert_eq!(p.products[1].variants[0].stock, None);
    assert!(p.errors.is_empty(), "{:?}", p.errors);
}

#[test]
fn garbage_is_unreadable() {
    assert!(matches!(parse_xlsx(b"not an xlsx"), Err(ImportError::Unreadable)));
    assert!(matches!(parse_xlsx(b""), Err(ImportError::Unreadable)));
}
```

Run: `cargo test --manifest-path <worktree>/api/Cargo.toml --test import_parse`
Expected: 2 passed。（`rust_xlsxwriter` 若沒有 `save_to_buffer`，用 `save(path)` 寫到 `std::env::temp_dir()` 再 `std::fs::read`；把實際用法寫進報告。）

- [ ] **Step 9: 閘門與 commit**

Run: `cargo fmt --all --check`、`cargo clippy --all-targets -- -D warnings`、完整 `cargo test`（背景）。
Commit：`feat(api): 匯入欄位對應（範本＋蝦皮別名）與 xlsx 解析（計畫 5 Task 2）`

---

### Task 3: 圖片下載（`import/images.rs`）

**Files:**
- Create: `api/src/import/images.rs`（取代 Task 2 的空殼；`mod.rs` 打開 `pub use images::…`）
- Test: 同檔 `#[cfg(test)]`（本機 axum 假伺服器，只綁 `127.0.0.1`）

**Interfaces:**
- Consumes: `crate::storage::{save, StoredImage, StorageError, MAX_UPLOAD_BYTES}`；reqwest 0.13（`rustls-no-provider`：**測試裡建 client 前要 `let _ = rustls::crypto::ring::default_provider().install_default();`**，正式程式在 `main.rs` 已裝）。
- Produces: 共用介面裡 `images.rs` 的全部項目。

- [ ] **Step 1: 寫失敗的測試**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, body::Bytes, http::{HeaderValue, StatusCode, header}, response::IntoResponse, routing::get};
    use std::net::SocketAddr;

    fn png(width: u32, height: u32) -> Vec<u8> {
        let img = image::RgbImage::from_pixel(width, height, image::Rgb([200, 120, 40]));
        let mut out = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut out, image::ImageFormat::Png)
            .unwrap();
        out.into_inner()
    }

    /// 假圖床：/ok.png 真圖、/html 文字、/big 假裝 20 MB、/500 伺服器錯、/redirect → /ok.png
    async fn serve() -> SocketAddr {
        async fn ok() -> impl IntoResponse {
            ([(header::CONTENT_TYPE, HeaderValue::from_static("image/png"))], Bytes::from(png(64, 48)))
        }
        async fn html() -> impl IntoResponse {
            ([(header::CONTENT_TYPE, HeaderValue::from_static("text/html"))], "<html>")
        }
        async fn big() -> impl IntoResponse {
            (
                [
                    (header::CONTENT_TYPE, HeaderValue::from_static("image/png")),
                    (header::CONTENT_LENGTH, HeaderValue::from_static("20971520")),
                ],
                Bytes::from(vec![0u8; 1024]),
            )
        }
        async fn big_no_length() -> impl IntoResponse {
            ([(header::CONTENT_TYPE, HeaderValue::from_static("image/png"))], Bytes::from(vec![0u8; MAX_IMAGE_BYTES + 1]))
        }
        async fn fail() -> impl IntoResponse {
            (StatusCode::INTERNAL_SERVER_ERROR, "boom")
        }
        async fn redirect() -> impl IntoResponse {
            (StatusCode::FOUND, [(header::LOCATION, HeaderValue::from_static("/ok.png"))])
        }
        async fn corrupt() -> impl IntoResponse {
            ([(header::CONTENT_TYPE, HeaderValue::from_static("image/png"))], "not really a png")
        }
        let app = Router::new()
            .route("/ok.png", get(ok))
            .route("/html", get(html))
            .route("/big", get(big))
            .route("/big-no-length", get(big_no_length))
            .route("/500", get(fail))
            .route("/redirect", get(redirect))
            .route("/corrupt", get(corrupt));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        addr
    }

    #[tokio::test]
    async fn fetches_stores_and_classifies_failures() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let addr = serve().await;
        let base = format!("http://{addr}");
        let dir = tempfile::tempdir().unwrap();
        let fetcher = ImageFetcher::new().unwrap();

        let stored = fetch_and_store(&fetcher, dir.path(), &format!("{base}/ok.png")).await.unwrap();
        assert!(stored.path.starts_with("/uploads/"));
        assert!(dir.path().join(stored.path.trim_start_matches("/uploads/")).exists());

        let via_redirect = fetch_and_store(&fetcher, dir.path(), &format!("{base}/redirect")).await.unwrap();
        assert_ne!(via_redirect.path, stored.path);

        assert_eq!(fetch_and_store(&fetcher, dir.path(), "ftp://x/y.png").await.unwrap_err(), FetchError::Scheme);
        assert_eq!(fetch_and_store(&fetcher, dir.path(), "/relative.png").await.unwrap_err(), FetchError::Scheme);
        assert_eq!(fetch_and_store(&fetcher, dir.path(), &format!("{base}/html")).await.unwrap_err(), FetchError::NotImage("text/html".into()));
        assert_eq!(fetch_and_store(&fetcher, dir.path(), &format!("{base}/big")).await.unwrap_err(), FetchError::TooLarge);
        assert_eq!(fetch_and_store(&fetcher, dir.path(), &format!("{base}/big-no-length")).await.unwrap_err(), FetchError::TooLarge);
        assert_eq!(fetch_and_store(&fetcher, dir.path(), &format!("{base}/500")).await.unwrap_err(), FetchError::Http("HTTP 500".into()));
        assert_eq!(fetch_and_store(&fetcher, dir.path(), &format!("{base}/corrupt")).await.unwrap_err(), FetchError::Decode);
        let unreachable = fetch_and_store(&fetcher, dir.path(), "http://127.0.0.1:1/x.png").await.unwrap_err();
        assert_eq!(unreachable, FetchError::Http("連線失敗".into()));
    }

    #[tokio::test]
    async fn fetch_all_keeps_order_and_isolates_failures() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let addr = serve().await;
        let base = format!("http://{addr}");
        let dir = tempfile::tempdir().unwrap();
        let fetcher = ImageFetcher::new().unwrap();
        let urls: Vec<String> = (0..8)
            .map(|i| if i == 3 { format!("{base}/500") } else { format!("{base}/ok.png") })
            .collect();
        let results = fetch_all(&fetcher, dir.path(), &urls).await;
        assert_eq!(results.len(), 8);
        for (i, (url, r)) in results.iter().enumerate() {
            assert_eq!(url, &urls[i]);
            if i == 3 { assert!(r.is_err()); } else { assert!(r.is_ok()); }
        }
    }
}
```

`tempfile` 若不是 dev 依賴，`cargo add --dev tempfile`（先 `grep -n tempfile api/Cargo.toml`）。`image` 是既有依賴（`png` feature 已開）。

- [ ] **Step 2: 跑測試確認失敗**

Run: `cargo test --manifest-path <worktree>/api/Cargo.toml --lib import::images`
Expected: 編譯錯誤（型別不存在）。

- [ ] **Step 3: 實作**

```rust
//! 匯入圖片下載（規格 §13：10 秒 timeout、≤ 10 MB、只收圖片 MIME；失敗只當該列警告）。
//! 只擋非 http/https；不擋內網位址（與規格不同之處 60）

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Semaphore;
use tokio::task::JoinSet;

use crate::storage::{self, StorageError, StoredImage};

pub const FETCH_TIMEOUT_SECS: u64 = 10;
pub const MAX_IMAGE_BYTES: usize = storage::MAX_UPLOAD_BYTES;
pub const MAX_REDIRECTS: usize = 3;
pub const FETCH_CONCURRENCY: usize = 6;

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum FetchError {
    #[error("只接受 http／https 網址")]
    Scheme,
    /// 固定文案＋狀態碼或「連線失敗」；不放回應內容
    #[error("下載失敗：{0}")]
    Http(String),
    #[error("不是圖片（{0}）")]
    NotImage(String),
    #[error("圖片超過 10 MB")]
    TooLarge,
    #[error("圖片無法讀取")]
    Decode,
    #[error("存檔失敗")]
    Store,
}

#[derive(Clone)]
pub struct ImageFetcher {
    client: reqwest::Client,
}

impl ImageFetcher {
    pub fn new() -> anyhow::Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(FETCH_TIMEOUT_SECS))
            .redirect(reqwest::redirect::Policy::limited(MAX_REDIRECTS))
            .user_agent("dog-shop-import/1.0")
            .build()?;
        Ok(Self { client })
    }

    /// 下載並檢查 MIME 與大小；不解碼
    pub async fn fetch(&self, url: &str) -> Result<Vec<u8>, FetchError> {
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            return Err(FetchError::Scheme);
        }
        let resp = self.client.get(url).send().await.map_err(|e| {
            tracing::info!(url, error = %e, "匯入圖片下載連線失敗");
            FetchError::Http("連線失敗".to_string())
        })?;
        let status = resp.status();
        if !status.is_success() {
            return Err(FetchError::Http(format!("HTTP {}", status.as_u16())));
        }
        let content_type = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .split(';')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        if !content_type.starts_with("image/") {
            return Err(FetchError::NotImage(content_type.chars().take(60).collect()));
        }
        if resp.content_length().is_some_and(|n| n > MAX_IMAGE_BYTES as u64) {
            return Err(FetchError::TooLarge);
        }
        let mut resp = resp;
        let mut buf: Vec<u8> = Vec::new();
        while let Some(chunk) = resp.chunk().await.map_err(|_| FetchError::Http("連線失敗".to_string()))? {
            if buf.len() + chunk.len() > MAX_IMAGE_BYTES {
                return Err(FetchError::TooLarge);
            }
            buf.extend_from_slice(&chunk);
        }
        Ok(buf)
    }
}

/// 下載 → 用既有的 storage::save 重新解碼、縮圖、存 JPEG
pub async fn fetch_and_store(
    fetcher: &ImageFetcher,
    upload_dir: &Path,
    url: &str,
) -> Result<StoredImage, FetchError> {
    let bytes = fetcher.fetch(url).await?;
    storage::save(upload_dir, bytes).await.map_err(|e| match e {
        StorageError::Decode(_) => FetchError::Decode,
        StorageError::Io(err) => {
            tracing::error!(error = %err, "匯入圖片存檔失敗");
            FetchError::Store
        }
    })
}

/// 依 urls 順序回傳；最多 FETCH_CONCURRENCY 個同時下載。
/// 最壞情況：每張 10 秒 timeout，N 張要 N/6 × 10 秒 —— 500 個商品各 9 張約 2 小時，所以 commit 的
/// 回應時間由圖片數決定（手冊要提醒老闆分批匯入）
pub async fn fetch_all(
    fetcher: &ImageFetcher,
    upload_dir: &Path,
    urls: &[String],
) -> Vec<(String, Result<StoredImage, FetchError>)> {
    let sem = Arc::new(Semaphore::new(FETCH_CONCURRENCY));
    let mut set = JoinSet::new();
    for (i, url) in urls.iter().cloned().enumerate() {
        let sem = sem.clone();
        let fetcher = fetcher.clone();
        let dir = upload_dir.to_path_buf();
        set.spawn(async move {
            let _permit = sem.acquire_owned().await.expect("semaphore 不會關閉");
            let r = fetch_and_store(&fetcher, &dir, &url).await;
            (i, url, r)
        });
    }
    let mut out: Vec<Option<(String, Result<StoredImage, FetchError>)>> = (0..urls.len()).map(|_| None).collect();
    while let Some(joined) = set.join_next().await {
        match joined {
            Ok((i, url, r)) => out[i] = Some((url, r)),
            Err(e) => tracing::error!(error = %e, "匯入圖片工作 panic"),
        }
    }
    out.into_iter()
        .enumerate()
        .map(|(i, slot)| slot.unwrap_or_else(|| (urls[i].clone(), Err(FetchError::Store))))
        .collect()
}
```

`mod.rs` 打開 `pub use images::{FetchError, ImageFetcher, fetch_all, fetch_and_store};`。

- [ ] **Step 4: 跑測試**

Run: `cargo test --manifest-path <worktree>/api/Cargo.toml --lib import::images`
Expected: 2 passed。若 `resp.chunk()` 在 reqwest 0.13 不存在，改用 `resp.bytes().await` 後檢查長度（Content-Length 缺席時才會多讀一點，仍受 10 MB 上限保護），並在報告說明。

- [ ] **Step 5: 閘門與 commit**

Run: fmt、clippy、完整 `cargo test`（背景）。
Commit：`feat(api): 匯入圖片下載（10 秒、10 MB、只收 image/*、6 併發）（計畫 5 Task 3）`

---

### Task 4: 寫入商品與 preview／commit 路由（`import/apply.rs`、`routes/admin_import.rs`）

**Files:**
- Create: `api/src/import/apply.rs`（取代空殼）、`api/src/routes/admin_import.rs`
- Modify: `api/src/import/mod.rs`（打開 `pub use apply::…`）、`api/src/routes/mod.rs`（`pub mod admin_import;`）、`api/src/app.rs`（`.merge(routes::admin_import::router())`，放在 `admin_products` 之後）
- Test: `api/tests/admin_import.rs`

**Interfaces:**
- Consumes: Task 1 的 `products::{find_by_external_ref, existing_external_refs, create, update, ProductInput, VariantInput, ImageInput, AdminProduct, clean, STATUS_DRAFT}`、`categories::find_or_create_by_name`；Task 2 的 `ParsedImport`／`ImportProduct`／`ImportVariant`／`parse_xlsx`／`ImportError`；Task 3 的 `ImageFetcher`／`fetch_all`；`crate::extract::AdminUser`；`sha2::{Digest, Sha256}` + `hex`（既有依賴）。
- Produces: 共用介面的 `apply.rs` 與 `routes/admin_import.rs` 項目；`AppState` **不變**（`ImageFetcher::new()` 在 handler 內建，成本很低）。

- [ ] **Step 1: 寫失敗的整合測試**

`api/tests/admin_import.rs`（helper 照 `api/tests/uploads.rs` 抄，但 multipart 要能多帶一個文字欄位）：

```rust
mod common;

use axum::{Router, body::{Body, Bytes}, http::{HeaderValue, Request, StatusCode, header}, response::IntoResponse, routing::get};
use rust_xlsxwriter::Workbook;
use serde_json::{Value, json};
use sqlx::PgPool;
use std::net::SocketAddr;

fn png(width: u32, height: u32) -> Vec<u8> {
    let img = image::RgbImage::from_pixel(width, height, image::Rgb([10, 200, 90]));
    let mut out = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(img).write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

/// 本機假圖床：/ok.png 與 /500
async fn image_server() -> String {
    async fn ok() -> impl IntoResponse {
        ([(header::CONTENT_TYPE, HeaderValue::from_static("image/png"))], Bytes::from(png(40, 30)))
    }
    async fn fail() -> impl IntoResponse { (StatusCode::INTERNAL_SERVER_ERROR, "boom") }
    let app = Router::new().route("/ok.png", get(ok)).route("/500", get(fail));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr: SocketAddr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}")
}

fn xlsx(rows: &[Vec<String>]) -> Vec<u8> {
    let mut wb = Workbook::new();
    let ws = wb.add_worksheet();
    for (r, row) in rows.iter().enumerate() {
        for (c, cell) in row.iter().enumerate() {
            ws.write_string(r as u32, c as u16, cell).unwrap();
        }
    }
    wb.save_to_buffer().unwrap()
}

fn s(v: &[&str]) -> Vec<String> { v.iter().map(|x| x.to_string()).collect() }

/// multipart：file=<xlsx> 再加零個或一個文字欄位
fn multipart(cookie: &str, path: &str, data: &[u8], extra: Option<(&str, &str)>) -> Request<Body> {
    let boundary = "XxDogShopBoundaryxX";
    let mut body: Vec<u8> = Vec::new();
    if let Some((name, value)) = extra {
        body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n").as_bytes());
    }
    body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"import.xlsx\"\r\nContent-Type: application/vnd.openxmlformats-officedocument.spreadsheetml.sheet\r\n\r\n").as_bytes());
    body.extend_from_slice(data);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    Request::builder()
        .method("POST")
        .uri(path)
        .header("cookie", cookie)
        .header("x-requested-with", "fetch")
        .header("content-type", format!("multipart/form-data; boundary={boundary}"))
        .body(Body::from(body))
        .unwrap()
}

const HEADER: &[&str] = &["商品編號", "商品名稱", "商品描述", "分類", "規格名稱1", "規格選項1", "規格名稱2", "規格選項2", "價格", "庫存", "SKU", "圖片網址"];

#[sqlx::test(migrations = "./migrations")]
async fn preview_then_commit_creates_products_with_images(pool: PgPool) {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let base = image_server().await;
    let (app, state) = common::app_with_state(pool.clone());
    let cookie = common::admin_cookie(&app, &pool).await;
    let file = xlsx(&[
        s(HEADER),
        s(&["A1", "狗糧 5kg", "很好吃", "狗糧", "口味", "雞肉", "", "", "1200", "10", "DOG-C", &format!("{base}/ok.png, {base}/500")]),
        s(&["A1", "", "", "", "", "牛肉", "", "", "1300", "", "DOG-B", ""]),
        s(&["B2", "玩具球", "", "", "", "", "", "", "99", "0", "", ""]),
    ]);

    let (status, body, _) = common::send(&app, multipart(&cookie, "/api/admin/import/preview", &file, None)).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["product_count"], json!(2));
    assert_eq!(body["variant_count"], json!(3));
    assert_eq!(body["new_count"], json!(2));
    assert_eq!(body["update_count"], json!(0));
    assert_eq!(body["parsed"]["errors"], json!([]));
    assert_eq!(body["parsed"]["products"][0]["external_ref"], json!("A1"));
    let fingerprint = body["fingerprint"].as_str().unwrap().to_string();
    assert_eq!(fingerprint.len(), 64);
    // 預覽不寫資料庫
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM products").fetch_one(&pool).await.unwrap();
    assert_eq!(n, 0);

    let (status, body, _) = common::send(&app, multipart(&cookie, "/api/admin/import/commit", &file, Some(("fingerprint", &fingerprint)))).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["created"], json!(2));
    assert_eq!(body["result"]["updated"], json!(0));
    assert_eq!(body["result"]["products"][0]["created"], json!(true));
    assert_eq!(body["result"]["products"][0]["images"], json!(1));
    let warnings = body["result"]["warnings"].as_array().unwrap();
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0]["external_ref"], json!("A1"));
    assert!(warnings[0]["message"].as_str().unwrap().contains("HTTP 500"), "{warnings:?}");

    let a = dog_shop_api::domain::products::find_by_external_ref(&pool, "A1").await.unwrap().unwrap();
    assert_eq!(a.product.status, "draft");
    assert_eq!(a.product.name, "狗糧 5kg");
    assert_eq!(a.product.description, "很好吃");
    assert!(a.product.category_id.is_some());
    assert_eq!(a.product.option1_name.as_deref(), Some("口味"));
    assert_eq!(a.variants.len(), 2);
    assert_eq!(a.variants[0].stock, 10);
    assert_eq!(a.variants[1].stock, 0, "庫存空白的新商品是 0");
    assert_eq!(a.images.len(), 1);
    assert!(state.config.upload_dir.join(a.images[0].path.trim_start_matches("/uploads/")).exists());
    let cat: (String,) = sqlx::query_as("SELECT name FROM categories WHERE id = $1").bind(a.product.category_id).fetch_one(&pool).await.unwrap();
    assert_eq!(cat.0, "狗糧");
}

#[sqlx::test(migrations = "./migrations")]
async fn reimport_updates_in_place_and_keeps_what_the_sheet_leaves_blank(pool: PgPool) {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let base = image_server().await;
    let (app, _state) = common::app_with_state(pool.clone());
    let cookie = common::admin_cookie(&app, &pool).await;
    let first = xlsx(&[
        s(HEADER),
        s(&["A1", "狗糧", "描述一", "狗糧", "口味", "雞肉", "", "", "1200", "10", "", &format!("{base}/ok.png")]),
        s(&["A1", "", "", "", "", "牛肉", "", "", "1300", "5", "", ""]),
    ]);
    let (_, body, _) = common::send(&app, multipart(&cookie, "/api/admin/import/preview", &first, None)).await;
    let fp = body["fingerprint"].as_str().unwrap().to_string();
    let (status, _, _) = common::send(&app, multipart(&cookie, "/api/admin/import/commit", &first, Some(("fingerprint", &fp)))).await;
    assert_eq!(status, StatusCode::OK);
    let before = dog_shop_api::domain::products::find_by_external_ref(&pool, "A1").await.unwrap().unwrap();
    // 老闆上架、改 slug、賣掉一些
    sqlx::query("UPDATE products SET status = 'active', slug = 'dog-food', sort_order = 7 WHERE id = $1").bind(before.product.id).execute(&pool).await.unwrap();
    sqlx::query("UPDATE product_variants SET stock = 3, compare_at_price = 1500 WHERE id = $1").bind(before.variants[0].id).execute(&pool).await.unwrap();

    // 第二次：只改價格、雞肉庫存空白、牛肉庫存 8、描述與分類空白、圖片網址全部壞掉
    let second = xlsx(&[
        s(HEADER),
        s(&["A1", "狗糧（新包裝）", "", "", "口味", "雞肉", "", "", "1250", "", "", &format!("{base}/500")]),
        s(&["A1", "", "", "", "", "牛肉", "", "", "1350", "8", "", ""]),
        s(&["A1", "", "", "", "", "魚肉", "", "", "1400", "2", "", ""]),
    ]);
    let (_, body, _) = common::send(&app, multipart(&cookie, "/api/admin/import/preview", &second, None)).await;
    assert_eq!(body["new_count"], json!(0));
    assert_eq!(body["update_count"], json!(1));
    let fp = body["fingerprint"].as_str().unwrap().to_string();
    let (status, body, _) = common::send(&app, multipart(&cookie, "/api/admin/import/commit", &second, Some(("fingerprint", &fp)))).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["result"]["updated"], json!(1));
    assert_eq!(body["result"]["warnings"].as_array().unwrap().len(), 1);

    let after = dog_shop_api::domain::products::find_by_external_ref(&pool, "A1").await.unwrap().unwrap();
    assert_eq!(after.product.id, before.product.id);
    assert_eq!(after.product.name, "狗糧（新包裝）");
    assert_eq!(after.product.status, "active", "狀態保留");
    assert_eq!(after.product.slug, "dog-food", "slug 保留");
    assert_eq!(after.product.sort_order, 7);
    assert_eq!(after.product.description, "描述一", "描述空白就保留");
    assert_eq!(after.product.category_id, before.product.category_id, "分類空白就保留");
    assert_eq!(after.images.len(), 1, "圖片全部失敗 → 保留原圖");
    assert_eq!(after.images[0].path, before.images[0].path);
    let chicken = after.variants.iter().find(|v| v.option1_value.as_deref() == Some("雞肉")).unwrap();
    assert_eq!(chicken.id, before.variants[0].id, "規格對回既有 id");
    assert_eq!(chicken.price, 1250);
    assert_eq!(chicken.stock, 3, "庫存空白 → 保留現值");
    assert_eq!(chicken.compare_at_price, Some(1500), "原價保留");
    let beef = after.variants.iter().find(|v| v.option1_value.as_deref() == Some("牛肉")).unwrap();
    assert_eq!(beef.id, before.variants[1].id);
    assert_eq!(beef.stock, 8);
    assert!(after.variants.iter().any(|v| v.option1_value.as_deref() == Some("魚肉")), "新規格新增");
}

#[sqlx::test(migrations = "./migrations")]
async fn commit_is_refused_with_row_errors_or_a_stale_fingerprint(pool: PgPool) {
    let (app, _state) = common::app_with_state(pool.clone());
    let cookie = common::admin_cookie(&app, &pool).await;
    let bad = xlsx(&[s(HEADER), s(&["A1", "", "", "", "", "", "", "", "abc", "", "", ""])]);
    let (status, body, _) = common::send(&app, multipart(&cookie, "/api/admin/import/preview", &bad, None)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["parsed"]["errors"].as_array().unwrap().len(), 2);
    let fp = body["fingerprint"].as_str().unwrap().to_string();
    let (status, body, _) = common::send(&app, multipart(&cookie, "/api/admin/import/commit", &bad, Some(("fingerprint", &fp)))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["details"]["fields"]["rows"], json!("檔案有 2 列錯誤，請先修正再匯入"));

    let good = xlsx(&[s(HEADER), s(&["A1", "ok", "", "", "", "", "", "", "10", "", "", ""])]);
    let (status, body, _) = common::send(&app, multipart(&cookie, "/api/admin/import/commit", &good, Some(("fingerprint", "0000")))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["details"]["fields"]["fingerprint"], json!("檔案已變更，請重新預覽"));
    let (status, body, _) = common::send(&app, multipart(&cookie, "/api/admin/import/commit", &good, None)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM products").fetch_one(&pool).await.unwrap();
    assert_eq!(n, 0);
}

#[sqlx::test(migrations = "./migrations")]
async fn bad_files_and_missing_columns_are_validation_errors(pool: PgPool) {
    let (app, _state) = common::app_with_state(pool.clone());
    let cookie = common::admin_cookie(&app, &pool).await;
    let (status, body, _) = common::send(&app, multipart(&cookie, "/api/admin/import/preview", b"not an xlsx", None)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["details"]["fields"]["file"], json!("檔案不是 xlsx 或已損壞"));
    let no_ref = xlsx(&[s(&["商品名稱", "價格", "庫存"]), s(&["x", "1", "1"])]);
    let (status, body, _) = common::send(&app, multipart(&cookie, "/api/admin/import/preview", &no_ref, None)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["details"]["fields"]["file"], json!("缺少必要欄位：商品編號"));
    // 沒有 file 欄位
    let (status, body, _) = common::send(&app, common::req("POST", "/api/admin/import/preview", Some(&cookie), Some(json!({})))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    // 超過 5 MB
    let huge = vec![0u8; dog_shop_api::import::MAX_XLSX_BYTES + 1];
    let (status, body, _) = common::send(&app, multipart(&cookie, "/api/admin/import/preview", &huge, None)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["details"]["fields"]["file"], json!("檔案超過 5 MB"));
}

#[sqlx::test(migrations = "./migrations")]
async fn import_routes_require_admin(pool: PgPool) {
    let app = common::app(pool.clone());
    let file = xlsx(&[s(HEADER), s(&["A1", "x", "", "", "", "", "", "", "1", "", "", ""])]);
    for path in ["/api/admin/import/preview", "/api/admin/import/commit"] {
        let (status, _, _) = common::send(&app, multipart("", path, &file, None)).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{path}");
        let customer = common::customer_cookie(&app, &pool).await;
        let (status, _, _) = common::send(&app, multipart(&customer, path, &file, None)).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{path}");
    }
}
```

（`multipart("", …)` 帶空 cookie header：若 `Request::builder().header("cookie", "")` 造成 401 以外的結果，改成沒有 cookie 就不加 header。`common::app_with_state` 的 `state.config.upload_dir` 是測試用暫存目錄——先看 `common::state()` 怎麼建，圖片會存到那裡。）

- [ ] **Step 2: 跑測試確認失敗**

Run: `DATABASE_URL=… cargo test --manifest-path <worktree>/api/Cargo.toml --test admin_import`
Expected: 編譯錯誤或 404。

- [ ] **Step 3: 實作 apply.rs**

```rust
//! 把解析結果寫進商品（規格 §13；與規格不同之處 51、54）。
//! 一個商品一個交易（既有 products::create/update）；圖片在交易外先下載

use std::collections::HashMap;
use std::path::Path;

use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::categories;
use crate::domain::products::{self, AdminProduct, ImageInput, ProductInput, STATUS_DRAFT, VariantInput, clean};
use crate::error::ApiError;
use crate::import::images::{ImageFetcher, fetch_all};
use crate::import::parse::{ImportProduct, ParsedImport};

#[derive(Debug, Clone, Serialize)]
pub struct ImportWarning {
    pub external_ref: String,
    pub row: u32,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportedProduct {
    pub external_ref: String,
    pub id: Uuid,
    pub name: String,
    pub created: bool,
    pub images: usize,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct ImportResult {
    pub created: usize,
    pub updated: usize,
    pub products: Vec<ImportedProduct>,
    pub warnings: Vec<ImportWarning>,
}

/// 全部商品逐一寫入。呼叫者保證 parsed.errors 是空的。
pub async fn apply(
    db: &PgPool,
    upload_dir: &Path,
    fetcher: &ImageFetcher,
    parsed: &ParsedImport,
) -> Result<ImportResult, ApiError> {
    let mut result = ImportResult::default();
    let mut category_cache: HashMap<String, Uuid> = HashMap::new();
    for p in &parsed.products {
        let category_id = match &p.category {
            None => None,
            Some(name) => match category_cache.get(name) {
                Some(id) => Some(*id),
                None => {
                    let c = categories::find_or_create_by_name(db, name).await?;
                    category_cache.insert(name.clone(), c.id);
                    Some(c.id)
                }
            },
        };
        let existing = products::find_by_external_ref(db, &p.external_ref).await?;

        // 圖片：先下載，失敗只記警告；更新時全部失敗（或沒填）就保留原圖
        let mut images: Vec<ImageInput> = Vec::new();
        for (url, r) in fetch_all(fetcher, upload_dir, &p.image_urls).await {
            match r {
                Ok(stored) => images.push(ImageInput { path: stored.path, thumb_path: stored.thumb_path, alt: Some(p.name.clone()) }),
                Err(e) => result.warnings.push(ImportWarning { external_ref: p.external_ref.clone(), row: p.first_row, message: format!("圖片 {url} 沒有匯入：{e}") }),
            }
        }
        if images.is_empty()
            && let Some(ex) = &existing
        {
            images = ex.images.iter().map(|i| ImageInput { path: i.path.clone(), thumb_path: i.thumb_path.clone(), alt: Some(i.alt.clone()) }).collect();
        }

        let input = build_input(p, existing.as_ref(), category_id, images);
        let (saved, created) = match &existing {
            Some(ex) => (products::update(db, ex.product.id, input).await.map_err(|e| annotate(e, p))?, false),
            None => (products::create(db, input).await.map_err(|e| annotate(e, p))?, true),
        };
        if created { result.created += 1 } else { result.updated += 1 }
        result.products.push(ImportedProduct {
            external_ref: p.external_ref.clone(),
            id: saved.product.id,
            name: saved.product.name.clone(),
            created,
            images: saved.images.len(),
        });
    }
    Ok(result)
}

/// 既有商品當底，只覆蓋工作表有填的欄位（與規格不同之處 51）
fn build_input(
    p: &ImportProduct,
    existing: Option<&AdminProduct>,
    category_id: Option<Uuid>,
    images: Vec<ImageInput>,
) -> ProductInput {
    let ex = existing.map(|e| &e.product);
    let variants = p
        .variants
        .iter()
        .map(|v| {
            let matched = existing.and_then(|e| {
                e.variants.iter().find(|r| clean(&r.option1_value) == clean(&v.option1_value) && clean(&r.option2_value) == clean(&v.option2_value))
            });
            VariantInput {
                id: matched.map(|m| m.id),
                option1_value: v.option1_value.clone(),
                option2_value: v.option2_value.clone(),
                sku: v.sku.clone().or_else(|| matched.and_then(|m| m.sku.clone())),
                price: v.price,
                compare_at_price: matched.and_then(|m| m.compare_at_price),
                stock: v.stock.or_else(|| matched.map(|m| m.stock)).unwrap_or(0),
                is_active: matched.map(|m| m.is_active),
                image_path: None,
            }
        })
        .collect();
    ProductInput {
        name: p.name.clone(),
        slug: ex.map(|e| e.slug.clone()),
        description: p.description.clone().or_else(|| ex.map(|e| e.description.clone())),
        category_id: category_id.or_else(|| ex.and_then(|e| e.category_id)),
        status: ex.map(|e| e.status.clone()).unwrap_or_else(|| STATUS_DRAFT.to_string()),
        option1_name: p.option1_name.clone(),
        option2_name: p.option2_name.clone(),
        sort_order: ex.map(|e| e.sort_order),
        variants,
        images,
        external_ref: Some(p.external_ref.clone()),
    }
}

/// products::validate 的欄位錯誤加上商品編號，老闆才知道是哪一列
fn annotate(err: ApiError, p: &ImportProduct) -> ApiError {
    match err {
        ApiError::Validation(fields) => {
            let mut out = crate::error::FieldErrors::new();
            for (k, v) in fields.into_iter() {
                out.add(&format!("{}.{k}", p.external_ref), &v);
            }
            out.into_error()
        }
        other => other,
    }
}
```

（`ApiError::Validation` 的實際變體名與 `FieldErrors` 是否可迭代要看 `api/src/error.rs:15-30`、`:134-160`；若 `FieldErrors` 沒有 `into_iter`，加一個 `pub fn into_map(self) -> BTreeMap<String, String>`。若變體名不是 `Validation`，照實際名稱。）

`mod.rs` 打開 `pub use apply::{ImportResult, ImportWarning, ImportedProduct, apply};`。

- [ ] **Step 4: 實作 routes/admin_import.rs**

```rust
//! 商品匯入（規格 §10、§13；與規格不同之處 49、50）

use axum::{Json, Router, extract::{Multipart, State}, routing::post};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::error::{ApiError, ApiResult};
use crate::extract::AdminUser;
use crate::import::{self, ImageFetcher, ImportResult, ParsedImport, parse_xlsx};
use crate::domain::products;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/admin/import/preview", post(preview))
        .route("/api/admin/import/commit", post(commit))
}

#[derive(Serialize)]
pub struct PreviewResponse {
    pub fingerprint: String,
    pub product_count: usize,
    pub variant_count: usize,
    pub new_count: usize,
    pub update_count: usize,
    pub parsed: ParsedImport,
}

#[derive(Serialize)]
pub struct CommitResponse {
    pub fingerprint: String,
    pub result: ImportResult,
}

struct Upload {
    file: Vec<u8>,
    fingerprint_field: Option<String>,
}

/// 讀 multipart：file 必填（≤ MAX_XLSX_BYTES）、fingerprint 選填
async fn read_upload(mut multipart: Multipart) -> Result<Upload, ApiError> {
    let mut file: Option<Vec<u8>> = None;
    let mut fingerprint_field: Option<String> = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| ApiError::field("file", "上傳格式錯誤"))?
    {
        match field.name().unwrap_or("") {
            "file" => {
                let bytes = field.bytes().await.map_err(|_| ApiError::field("file", "上傳格式錯誤"))?;
                if bytes.len() > import::MAX_XLSX_BYTES {
                    return Err(ApiError::field("file", "檔案超過 5 MB"));
                }
                file = Some(bytes.to_vec());
            }
            "fingerprint" => {
                fingerprint_field = Some(field.text().await.map_err(|_| ApiError::field("fingerprint", "格式錯誤"))?.trim().to_string());
            }
            _ => {}
        }
    }
    let file = file.filter(|f| !f.is_empty()).ok_or_else(|| ApiError::field("file", "請選擇 xlsx 檔案"))?;
    Ok(Upload { file, fingerprint_field })
}

fn fingerprint(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

async fn parse_and_count(state: &AppState, bytes: &[u8]) -> Result<PreviewResponse, ApiError> {
    let parsed = parse_xlsx(bytes).map_err(|e| ApiError::field("file", &e.to_string()))?;
    let refs: Vec<String> = parsed.products.iter().map(|p| p.external_ref.clone()).collect();
    let existing = products::existing_external_refs(&state.db, &refs).await?;
    let update_count = refs.iter().filter(|r| existing.contains(*r)).count();
    Ok(PreviewResponse {
        fingerprint: fingerprint(bytes),
        product_count: parsed.products.len(),
        variant_count: parsed.variant_count(),
        new_count: refs.len() - update_count,
        update_count,
        parsed,
    })
}

async fn preview(
    _admin: AdminUser,
    State(state): State<AppState>,
    multipart: Multipart,
) -> ApiResult<Json<PreviewResponse>> {
    let upload = read_upload(multipart).await?;
    Ok(Json(parse_and_count(&state, &upload.file).await?))
}

async fn commit(
    _admin: AdminUser,
    State(state): State<AppState>,
    multipart: Multipart,
) -> ApiResult<Json<CommitResponse>> {
    let upload = read_upload(multipart).await?;
    let expected = upload.fingerprint_field.clone().ok_or_else(|| ApiError::field("fingerprint", "請先預覽"))?;
    let preview = parse_and_count(&state, &upload.file).await?;
    if preview.fingerprint != expected {
        return Err(ApiError::field("fingerprint", "檔案已變更，請重新預覽"));
    }
    if !preview.parsed.errors.is_empty() {
        return Err(ApiError::field("rows", &format!("檔案有 {} 列錯誤，請先修正再匯入", preview.parsed.errors.len())));
    }
    let fetcher = ImageFetcher::new()?;
    let result = import::apply(&state.db, &state.config.upload_dir, &fetcher, &preview.parsed).await?;
    tracing::info!(created = result.created, updated = result.updated, warnings = result.warnings.len(), "商品匯入完成");
    Ok(Json(CommitResponse { fingerprint: preview.fingerprint, result }))
}
```

`Multipart` 對非 multipart 請求會被 extractor 拒絕成 axum 的裸 400 文字；照 `routes/uploads.rs` 的做法（它是怎麼把 `MultipartRejection` 變成 `VALIDATION` 的，`grep -n 'Multipart' api/src/routes/uploads.rs`），若 uploads 用的是 `Result<Multipart, MultipartRejection>` 就照抄，測試 `bad_files_and_missing_columns_are_validation_errors` 的「沒有 file 欄位」那段只斷言 400。

`api/src/routes/mod.rs` 加 `pub mod admin_import;`；`api/src/app.rs` 在 `.merge(routes::admin_products::router())` 之後加 `.merge(routes::admin_import::router())`。

- [ ] **Step 5: 跑測試**

Run: `DATABASE_URL=… cargo test --manifest-path <worktree>/api/Cargo.toml --test admin_import`
Expected: 5 passed。

- [ ] **Step 6: 閘門與 commit**

Run: fmt、clippy、完整 `cargo test`（背景）。
Commit：`feat(api): 商品匯入 preview／commit 路由與寫入（既有商品為底、指紋比對、列錯誤擋 commit）（計畫 5 Task 4）`

---

### Task 5: 後台匯入頁（`/admin/import`）

**Files:**
- Create: `web/src/routes/admin/import/+page.svelte`、`web/src/lib/importPreview.ts`、`web/src/lib/importPreview.test.ts`
- Modify: `web/src/lib/types.ts`（共用介面的 8 個 `Import*` 型別）、`web/src/routes/admin/+layout.svelte:8-12`（側欄加 `{ href: '/admin/import', label: '匯入' }`，放在「商品」之後）

**Interfaces:**
- Consumes: `api<T>(path, init)`（`$lib/api`；body 是 `FormData` 時瀏覽器自設 content-type，非 GET 自動帶 `X-Requested-With`）、`ApiError`（`.field(name)`、`.fields()`）、Task 4 的兩條路由。
- Produces: `importPreview.ts` 的 `canCommit(preview: ImportPreview): boolean` 與 `summarize(preview: ImportPreview): { line: string; errorCount: number }`。

- [ ] **Step 1: 型別與失敗的 vitest**

`web/src/lib/types.ts` 加共用介面裡的 8 個 `Import*` 型別（逐字）。

`web/src/lib/importPreview.ts`：

```ts
import type { ImportPreview } from './types';

/** 有錯誤或沒有商品就不能匯入 */
export function canCommit(preview: ImportPreview): boolean {
	return preview.parsed.errors.length === 0 && preview.product_count > 0;
}

/** 一行摘要給預覽頁頂端 */
export function summarize(preview: ImportPreview): { line: string; errorCount: number } {
	const errorCount = preview.parsed.errors.length;
	const line = `${preview.product_count} 個商品、${preview.variant_count} 個規格（新增 ${preview.new_count}、更新 ${preview.update_count}）`;
	return { line, errorCount };
}
```

`web/src/lib/importPreview.test.ts`：

```ts
import { describe, expect, it } from 'vitest';
import { canCommit, summarize } from './importPreview';
import type { ImportPreview } from './types';

function preview(overrides: Partial<ImportPreview> = {}): ImportPreview {
	return {
		fingerprint: 'abc',
		product_count: 2,
		variant_count: 3,
		new_count: 1,
		update_count: 1,
		parsed: { sheet: 's', header_row: 1, row_count: 3, products: [], errors: [], unmatched_columns: [] },
		...overrides
	};
}

describe('importPreview', () => {
	it('summarizes counts', () => {
		expect(summarize(preview())).toEqual({ line: '2 個商品、3 個規格（新增 1、更新 1）', errorCount: 0 });
	});
	it('blocks commit on errors or empty file', () => {
		expect(canCommit(preview())).toBe(true);
		expect(canCommit(preview({ parsed: { ...preview().parsed, errors: [{ row: 2, column: '價格', message: 'x' }] } }))).toBe(false);
		expect(canCommit(preview({ product_count: 0 }))).toBe(false);
	});
});
```

Run: `pnpm -C <worktree>/web test`
Expected: 新測試失敗（模組不存在）→ 寫完 `importPreview.ts` 後通過。

- [ ] **Step 2: 頁面**

`web/src/routes/admin/import/+page.svelte`（Svelte 5 runes；樣式沿用 `admin/orders/+page.svelte` 的表格 class）：

```svelte
<script lang="ts">
	import { api, ApiError } from '$lib/api';
	import { canCommit, summarize } from '$lib/importPreview';
	import type { ImportCommit, ImportPreview } from '$lib/types';

	let file = $state<File | null>(null);
	let preview = $state<ImportPreview | null>(null);
	let result = $state<ImportCommit | null>(null);
	let busy = $state<'preview' | 'commit' | null>(null);
	let error = $state('');
	let fieldErrors = $state<Record<string, string>>({});

	function onFile(e: Event) {
		const input = e.currentTarget as HTMLInputElement;
		file = input.files?.[0] ?? null;
		preview = null;
		result = null;
		error = '';
		fieldErrors = {};
	}

	async function doPreview() {
		if (!file) return;
		busy = 'preview';
		error = '';
		fieldErrors = {};
		result = null;
		try {
			const form = new FormData();
			form.append('file', file);
			preview = await api<ImportPreview>('/api/admin/import/preview', { method: 'POST', body: form });
		} catch (e) {
			preview = null;
			if (e instanceof ApiError) {
				fieldErrors = e.fields();
				error = e.field('file') ?? e.message;
			} else {
				error = '預覽失敗，請再試一次';
			}
		} finally {
			busy = null;
		}
	}

	async function doCommit() {
		if (!file || !preview || !canCommit(preview)) return;
		if (!confirm(`確定匯入 ${preview.product_count} 個商品？圖片會從網址下載，可能需要幾分鐘。`)) return;
		busy = 'commit';
		error = '';
		fieldErrors = {};
		try {
			const form = new FormData();
			form.append('fingerprint', preview.fingerprint);
			form.append('file', file);
			result = await api<ImportCommit>('/api/admin/import/commit', { method: 'POST', body: form });
		} catch (e) {
			if (e instanceof ApiError) {
				fieldErrors = e.fields();
				error = e.field('rows') ?? e.field('fingerprint') ?? e.field('file') ?? e.message;
			} else {
				error = '匯入失敗，請再試一次';
			}
		} finally {
			busy = null;
		}
	}

	const summary = $derived(preview ? summarize(preview) : null);
	const shown = $derived(preview ? preview.parsed.products.slice(0, 50) : []);
</script>

<svelte:head><title>匯入商品</title></svelte:head>

<h1 class="text-xl font-semibold">匯入商品</h1>
<p class="mt-1 text-sm text-gray-600">
	上傳 xlsx（依範本或蝦皮匯出檔）。第一列是標題：商品編號、商品名稱、商品描述、分類、規格名稱1、規格選項1、規格名稱2、規格選項2、價格、庫存、SKU、圖片網址（逗號分隔，最多 9 個）。同一個商品編號的多列會合併成多規格；已存在的商品編號會更新既有商品。匯入的新商品是「草稿」，檢查後再上架。
</p>

<div class="mt-4 flex flex-wrap items-center gap-3">
	<input type="file" accept=".xlsx,application/vnd.openxmlformats-officedocument.spreadsheetml.sheet" onchange={onFile} disabled={busy !== null} />
	<button class="btn" onclick={doPreview} disabled={!file || busy !== null}>{busy === 'preview' ? '解析中…' : '預覽'}</button>
</div>

{#if error}
	<p class="mt-3 text-sm text-red-700" role="alert">{error}</p>
{/if}

{#if preview && summary}
	<section class="mt-6">
		<p class="font-medium">{summary.line}</p>
		<p class="text-sm text-gray-600">工作表「{preview.parsed.sheet}」，標題在第 {preview.parsed.header_row} 列，共 {preview.parsed.row_count} 列資料。</p>
		{#if preview.parsed.unmatched_columns.length > 0}
			<p class="mt-2 text-sm text-amber-700">對不上的欄位（會被忽略）：{preview.parsed.unmatched_columns.join('、')}</p>
		{/if}
		{#if preview.parsed.errors.length > 0}
			<h2 class="mt-4 font-medium text-red-700">{preview.parsed.errors.length} 個錯誤，修正後重新上傳才能匯入</h2>
			<table class="mt-2 w-full text-sm">
				<thead><tr><th class="text-left">列</th><th class="text-left">欄位</th><th class="text-left">問題</th></tr></thead>
				<tbody>
					{#each preview.parsed.errors as e (e.row + (e.column ?? '') + e.message)}
						<tr class="text-red-700"><td>{e.row}</td><td>{e.column ?? '—'}</td><td>{e.message}</td></tr>
					{/each}
				</tbody>
			</table>
		{/if}
		<h2 class="mt-4 font-medium">商品（前 50 筆）</h2>
		<table class="mt-2 w-full text-sm">
			<thead><tr><th class="text-left">商品編號</th><th class="text-left">名稱</th><th class="text-left">分類</th><th class="text-right">規格</th><th class="text-right">圖片</th></tr></thead>
			<tbody>
				{#each shown as p (p.external_ref)}
					<tr><td>{p.external_ref}</td><td>{p.name}</td><td>{p.category ?? '—'}</td><td class="text-right">{p.variants.length}</td><td class="text-right">{p.image_urls.length}</td></tr>
				{/each}
			</tbody>
		</table>
		<div class="mt-4">
			<button class="btn btn-primary" onclick={doCommit} disabled={!canCommit(preview) || busy !== null}>
				{busy === 'commit' ? '匯入中，請不要關閉頁面…' : '確認匯入'}
			</button>
		</div>
	</section>
{/if}

{#if result}
	<section class="mt-6">
		<h2 class="font-medium">匯入完成：新增 {result.result.created}、更新 {result.result.updated}</h2>
		{#if result.result.warnings.length > 0}
			<h3 class="mt-3 text-amber-700">{result.result.warnings.length} 個警告（商品已匯入，只是圖片沒抓到）</h3>
			<ul class="mt-1 list-disc pl-5 text-sm">
				{#each result.result.warnings as w (w.external_ref + w.row + w.message)}
					<li>{w.external_ref}（第 {w.row} 列）：{w.message}</li>
				{/each}
			</ul>
		{/if}
		<ul class="mt-3 text-sm">
			{#each result.result.products as p (p.id)}
				<li><a class="underline" href={`/admin/products/${p.id}`}>{p.name}</a>（{p.created ? '新增' : '更新'}，{p.images} 張圖）</li>
			{/each}
		</ul>
	</section>
{/if}
```

`btn`／`btn-primary` 若不是既有 class，改用 `admin/orders/[id]/+page.svelte` 裡按鈕實際用的 Tailwind class。`confirm()` 是瀏覽器原生對話框——後台既有頁面用兩步確認（`CONFIRM` 常數），若 Task 9 的慣例是不用 `confirm`，改成同樣的兩步按鈕。

側欄：`web/src/routes/admin/+layout.svelte` 的陣列加 `{ href: '/admin/import', label: '匯入' }`。

- [ ] **Step 3: 檢查與煙霧測試**

Run: `pnpm -C <worktree>/web check`（0 errors）、`pnpm -C <worktree>/web test`（新增 2 條通過）、`pnpm -C <worktree>/web build`。
煙霧：起 api（`cargo run`，背景）與 web dev（背景），用 curl 登入拿 cookie（`POST /api/auth/login`，`-H 'X-Requested-With: fetch'`，`admin@example.com`／`admin12345`），`curl -s -b 'sid=…' http://localhost:5173/admin/import` 要 200 且含「匯入商品」；`curl -s -b 'sid=…' http://localhost:5173/admin` 側欄含「匯入」。做完 `kill` 兩個伺服器。

- [ ] **Step 4: Commit**

Commit：`feat(web): 後台匯入頁（上傳 → 預覽 → 確認匯入）（計畫 5 Task 5）`

---

### Task 6: 正式環境啟動檢查、`LISTEN_ADDR`、SIGTERM、jobs 守衛

**Files:**
- Modify: `api/src/config.rs`（`from_env` → `from_vars`、prod 檢查、`listen_addr`、`for_tests`）、`api/src/main.rs`（`listen_addr`、SIGTERM）、`api/src/jobs/worker.rs`（`mark_done`／`mark_failed_attempt` 加 `AND status = 'running'`）
- Modify: `.env.example`（加 `LISTEN_ADDR` 與正式環境註解）
- Test: `api/src/config.rs` 的 `#[cfg(test)]`、`api/tests/jobs_worker.rs`（新增 1 條）

**Interfaces:**
- Produces: `Config::from_vars(vars: &HashMap<String, String>) -> anyhow::Result<Config>`（`from_env` 變成 `Self::from_vars(&std::env::vars().collect())`）；`Config.listen_addr: String`（預設 `0.0.0.0:8080`）；`worker::mark_done(db, id) -> Result<bool, sqlx::Error>`（rows_affected > 0）。

- [ ] **Step 1: 失敗的單元測試（config）**

`api/src/config.rs` 檔尾：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn vars(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    fn prod_ok() -> HashMap<String, String> {
        vars(&[
            ("DATABASE_URL", "postgres://x"),
            ("ECPAY_ENV", "prod"),
            ("PUBLIC_BASE_URL", "https://shop.example.com/"),
            ("ECPAY_AIO_MERCHANT_ID", "1234567"), ("ECPAY_AIO_HASH_KEY", "aaaaaaaaaaaaaaaa"), ("ECPAY_AIO_HASH_IV", "bbbbbbbbbbbbbbbb"),
            ("ECPAY_INVOICE_MERCHANT_ID", "1234567"), ("ECPAY_INVOICE_HASH_KEY", "cccccccccccccccc"), ("ECPAY_INVOICE_HASH_IV", "dddddddddddddddd"),
            ("ECPAY_LOGISTICS_MERCHANT_ID", "1234567"), ("ECPAY_LOGISTICS_HASH_KEY", "eeeeeeeeeeeeeeee"), ("ECPAY_LOGISTICS_HASH_IV", "ffffffffffffffff"),
            ("SMTP_HOST", "smtp.example.com"), ("SMTP_FROM", "shop@example.com"),
        ])
    }

    #[test]
    fn stage_defaults_fill_in_public_test_credentials() {
        let c = Config::from_vars(&vars(&[("DATABASE_URL", "postgres://x")])).unwrap();
        assert_eq!(c.ecpay.env, EcpayEnv::Stage);
        assert_eq!(c.ecpay.logistics.merchant_id, STAGE_LOGISTICS.0);
        assert_eq!(c.public_base_url, "http://localhost:5173");
        assert_eq!(c.listen_addr, "0.0.0.0:8080");
        assert!(c.cookie_secure);
    }

    #[test]
    fn prod_accepts_a_complete_real_configuration() {
        let c = Config::from_vars(&prod_ok()).unwrap();
        assert_eq!(c.ecpay.env, EcpayEnv::Prod);
        assert_eq!(c.public_base_url, "https://shop.example.com");
        assert!(c.smtp.is_some());
    }

    #[test]
    fn prod_rejects_stage_credentials_even_partially() {
        let mut v = prod_ok();
        v.insert("ECPAY_LOGISTICS_HASH_KEY".into(), STAGE_LOGISTICS.1.into());
        let err = Config::from_vars(&v).unwrap_err().to_string();
        assert!(err.contains("ECPAY_LOGISTICS") && err.contains("測試特店"), "{err}");
        let mut v = prod_ok();
        v.insert("ECPAY_AIO_MERCHANT_ID".into(), STAGE_AIO.0.into());
        v.insert("ECPAY_AIO_HASH_KEY".into(), STAGE_AIO.1.into());
        v.insert("ECPAY_AIO_HASH_IV".into(), STAGE_AIO.2.into());
        assert!(Config::from_vars(&v).unwrap_err().to_string().contains("ECPAY_AIO"));
        let mut v = prod_ok();
        v.insert("ECPAY_INVOICE_HASH_IV".into(), STAGE_INVOICE.2.into());
        assert!(Config::from_vars(&v).unwrap_err().to_string().contains("ECPAY_INVOICE"));
    }

    #[test]
    fn prod_requires_https_base_url_secure_cookie_and_smtp() {
        let mut v = prod_ok();
        v.insert("PUBLIC_BASE_URL".into(), "http://shop.example.com".into());
        assert!(Config::from_vars(&v).unwrap_err().to_string().contains("PUBLIC_BASE_URL"));
        let mut v = prod_ok();
        v.remove("PUBLIC_BASE_URL");
        assert!(Config::from_vars(&v).unwrap_err().to_string().contains("PUBLIC_BASE_URL"));
        let mut v = prod_ok();
        v.insert("COOKIE_SECURE".into(), "false".into());
        assert!(Config::from_vars(&v).unwrap_err().to_string().contains("COOKIE_SECURE"));
        let mut v = prod_ok();
        v.remove("SMTP_HOST");
        assert!(Config::from_vars(&v).unwrap_err().to_string().contains("SMTP_HOST"));
        let mut v = prod_ok();
        v.insert("MAIL_LOG_BODY".into(), "1".into());
        assert!(Config::from_vars(&v).unwrap_err().to_string().contains("MAIL_LOG_BODY"));
        let mut v = prod_ok();
        v.insert("LISTEN_ADDR".into(), "127.0.0.1:9090".into());
        assert_eq!(Config::from_vars(&v).unwrap().listen_addr, "127.0.0.1:9090");
    }

    #[test]
    fn error_messages_never_contain_secret_values() {
        let mut v = prod_ok();
        v.insert("ECPAY_AIO_HASH_KEY".into(), STAGE_AIO.1.into());
        let err = Config::from_vars(&v).unwrap_err().to_string();
        assert!(!err.contains(STAGE_AIO.1) && !err.contains("aaaaaaaaaaaaaaaa"), "{err}");
    }
}
```

- [ ] **Step 2: 實作 config**

把 `env_trimmed`／`credentials`／`from_env` 改成吃 `&HashMap<String, String>`：

```rust
use std::collections::HashMap;

fn trimmed(vars: &HashMap<String, String>, name: &str) -> Option<String> {
    vars.get(name)
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn credentials(
    vars: &HashMap<String, String>,
    prefix: &str,
    env: EcpayEnv,
    stage: (&str, &str, &str),
) -> anyhow::Result<EcpayCredentials> {
    let read = |suffix: &str, fallback: &str| -> anyhow::Result<String> {
        let name = format!("{prefix}_{suffix}");
        match (trimmed(vars, &name), env) {
            (Some(value), _) => Ok(value),
            (None, EcpayEnv::Stage) => Ok(fallback.to_string()),
            (None, EcpayEnv::Prod) => anyhow::bail!("ECPAY_ENV=prod 時必須設定 {name}"),
        }
    };
    let creds = EcpayCredentials {
        merchant_id: read("MERCHANT_ID", stage.0)?,
        hash_key: read("HASH_KEY", stage.1)?,
        hash_iv: read("HASH_IV", stage.2)?,
    };
    // 正式環境不能用公開的測試特店憑證（與規格不同之處 55；計畫 3 審查交接 9、計畫 4 審查交接 3）。
    // 只比對、不印值（規格 §11）
    if env == EcpayEnv::Prod
        && (creds.merchant_id == stage.0 || creds.hash_key == stage.1 || creds.hash_iv == stage.2)
    {
        anyhow::bail!("ECPAY_ENV=prod 時 {prefix} 不能用測試特店憑證（.env.example 裡的值）");
    }
    Ok(creds)
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let vars: HashMap<String, String> = std::env::vars().collect();
        Self::from_vars(&vars)
    }

    /// 從一組變數建設定；正式環境（ECPAY_ENV=prod）多做上線檢查
    pub fn from_vars(vars: &HashMap<String, String>) -> anyhow::Result<Self> {
        let database_url = trimmed(vars, "DATABASE_URL").context("缺少環境變數 DATABASE_URL")?;
        let ecpay_env = match trimmed(vars, "ECPAY_ENV").as_deref() {
            None | Some("stage") => EcpayEnv::Stage,
            Some("prod") => EcpayEnv::Prod,
            Some(other) => anyhow::bail!("ECPAY_ENV 只能是 stage 或 prod，收到 {other}"),
        };
        let is_prod = ecpay_env == EcpayEnv::Prod;

        let public_base_url = match trimmed(vars, "PUBLIC_BASE_URL") {
            Some(u) => u.trim_end_matches('/').to_string(),
            None if is_prod => anyhow::bail!("ECPAY_ENV=prod 時必須設定 PUBLIC_BASE_URL（https 的公開網址）"),
            None => "http://localhost:5173".to_string(),
        };
        if is_prod && !public_base_url.starts_with("https://") {
            anyhow::bail!("ECPAY_ENV=prod 時 PUBLIC_BASE_URL 必須是 https://（綠界回呼與 Secure cookie 都需要）");
        }
        let cookie_secure = match trimmed(vars, "COOKIE_SECURE") {
            Some(value) => !matches!(value.to_ascii_lowercase().as_str(), "false" | "0" | "no"),
            None => true,
        };
        if is_prod && !cookie_secure {
            anyhow::bail!("ECPAY_ENV=prod 時 COOKIE_SECURE 不能關");
        }
        let upload_dir = PathBuf::from(trimmed(vars, "UPLOAD_DIR").unwrap_or_else(|| "./uploads".to_string()));
        let listen_addr = trimmed(vars, "LISTEN_ADDR").unwrap_or_else(|| "0.0.0.0:8080".to_string());

        let ecpay = EcpayConfig {
            env: ecpay_env,
            aio: credentials(vars, "ECPAY_AIO", ecpay_env, STAGE_AIO)?,
            invoice: credentials(vars, "ECPAY_INVOICE", ecpay_env, STAGE_INVOICE)?,
            logistics: credentials(vars, "ECPAY_LOGISTICS", ecpay_env, STAGE_LOGISTICS)?,
        };

        let smtp = match trimmed(vars, "SMTP_HOST") {
            None if is_prod => anyhow::bail!("ECPAY_ENV=prod 時必須設定 SMTP_HOST 與 SMTP_FROM（訂單信、發票信都靠它）"),
            None => None,
            Some(host) => Some(SmtpConfig {
                host,
                port: trimmed(vars, "SMTP_PORT")
                    .map(|p| p.parse::<u16>().context("SMTP_PORT 要是 1～65535 的數字"))
                    .transpose()?
                    .unwrap_or(587),
                user: trimmed(vars, "SMTP_USER"),
                pass: trimmed(vars, "SMTP_PASS"),
                from: trimmed(vars, "SMTP_FROM").context("有 SMTP_HOST 就必須設定 SMTP_FROM")?,
            }),
        };
        let mail_log_body = flag_enabled(trimmed(vars, "MAIL_LOG_BODY").as_deref());
        if is_prod && mail_log_body {
            anyhow::bail!("ECPAY_ENV=prod 時不能開 MAIL_LOG_BODY（信件內文含重設連結與訪客訂單網址）");
        }

        Ok(Self { database_url, public_base_url, cookie_secure, upload_dir, listen_addr, ecpay, smtp, mail_log_body })
    }
}
```

`Config` 加欄位 `pub listen_addr: String`（放 `upload_dir` 之後；`Debug` impl 加 `.field("listen_addr", &self.listen_addr)`；`for_tests` 加 `listen_addr: "127.0.0.1:0".to_string()`）。刪掉舊的 `env_trimmed`（沒有其他呼叫者才刪；`grep -rn env_trimmed api/src`）。

`api/src/main.rs`：

```rust
    let listener = tokio::net::TcpListener::bind(&config.listen_addr).await?;
    tracing::info!(addr = %config.listen_addr, "api listening");
    // …
async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut sig) => {
                sig.recv().await;
            }
            Err(e) => tracing::error!(error = %e, "無法監聽 SIGTERM"),
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    tracing::info!("shutting down");
}
```

`.env.example`：在檔頭加註解「這是開發用；正式環境用 `deploy/env.prod.example`」，加一行 `LISTEN_ADDR=0.0.0.0:8080`（註解：api 監聽位址）。

- [ ] **Step 3: jobs 守衛與測試**

`api/src/jobs/worker.rs`：`mark_done` 的 SQL 改成 `… WHERE id = $1 AND status = 'running'`，回傳 `Ok(result.rows_affected() > 0)`；`mark_failed_attempt` 的兩個 UPDATE 同樣加 `AND status = 'running'`。呼叫者若忽略回傳值就 `let _ =`；`mark_done` 回 false 時 `tracing::warn!(job_id, "job 已不是 running（可能被 requeue_stale 重排），不改狀態")`。

`api/tests/jobs_worker.rs` 檔尾加（照該檔既有的排 job／跑 worker helper）：

```rust
#[sqlx::test(migrations = "./migrations")]
async fn a_requeued_job_is_not_marked_done_by_its_stale_runner(pool: PgPool) {
    use dog_shop_api::jobs::worker;
    // 排一個 job，手動把它變成 running（模擬第一個 worker 拿走）
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO jobs (kind, payload, dedupe_key, status, attempts, max_attempts, run_at)
         VALUES ('send_email', '{}'::jsonb, 'test:stale', 'running', 1, 3, now()) RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    // requeue_stale 把它排回 queued（第二個 worker 會再拿）
    sqlx::query("UPDATE jobs SET status = 'queued', run_at = now() WHERE id = $1").bind(id).execute(&pool).await.unwrap();
    // 第一個 worker 這時才回來說做完了 → 不能改
    assert!(!worker::mark_done(&pool, id).await.unwrap());
    let status: String = sqlx::query_scalar("SELECT status FROM jobs WHERE id = $1").bind(id).fetch_one(&pool).await.unwrap();
    assert_eq!(status, "queued");
}
```

（`jobs` 表的欄位與 `mark_done` 是否 `pub` 看 `api/src/jobs/worker.rs:60-95` 與 `api/migrations/0002_members_orders.sql`；若 `mark_done` 是私有的，改成 `pub` 或透過既有的 pub 入口測。）

- [ ] **Step 4: 跑測試、閘門、commit**

Run: `cargo test --manifest-path <worktree>/api/Cargo.toml --lib config::`（5 passed）、`--test jobs_worker`、完整套件（背景）、fmt、clippy。
Commit：`feat(api): 正式環境啟動檢查（拒絕測試憑證、https、SMTP）、LISTEN_ADDR、SIGTERM、jobs 完成守衛（計畫 5 Task 6）`

---

### Task 7: 部署檔案（Dockerfile ×2、compose、Caddyfile、備份／還原）與本機煙霧測試

**Files:**
- Create: `api/Dockerfile`、`api/.dockerignore`、`web/Dockerfile`、`web/.dockerignore`、`deploy/docker-compose.yml`、`deploy/docker-compose.smoke.yml`、`deploy/Caddyfile`、`deploy/env.prod.example`、`deploy/backup.sh`、`deploy/restore.sh`
- Modify: `web/package.json`（`"start": "node build"`）、`.gitignore`（`deploy/backups/`）、`.github/workflows/ci.yml`（`docker` job）
- Test: 本機煙霧（`docker compose config` → `build` → `up` → curl → 備份／還原往返 → `down -v`），輸出寫進報告

**Interfaces:**
- Consumes: Task 6 的 `LISTEN_ADDR`、SIGTERM；`GET /api/health`（既有）；web 的 adapter-node（`node build`，讀 `PORT`、`HOST`、`ORIGIN`、`PROTOCOL_HEADER`、`HOST_HEADER`、`API_INTERNAL_URL`、`PUBLIC_BASE_URL`）。
- Produces: 下列檔案，內容逐字。

- [ ] **Step 1: api 映像**

`api/.dockerignore`：

```
target/
uploads/
.env
```

`api/Dockerfile`：

```dockerfile
# 多階段（規格 §16）：rust 1.98 建置 → debian-slim 執行
FROM rust:1.98-bookworm AS build
WORKDIR /src
# migrations 在編譯期被 sqlx::migrate! 內嵌，要在 build 前 COPY 進來
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY migrations ./migrations
RUN cargo build --release --locked

FROM debian:bookworm-slim
# ca-certificates：對綠界／SMTP 的 TLS（計畫 3 交接 9）；curl：compose healthcheck
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --uid 10001 --create-home app \
 && mkdir -p /data/uploads && chown -R app:app /data
COPY --from=build /src/target/release/api /usr/local/bin/api
USER app
ENV UPLOAD_DIR=/data/uploads \
    LISTEN_ADDR=0.0.0.0:8080 \
    RUST_LOG=info,tower_http=info
EXPOSE 8080
VOLUME ["/data/uploads"]
CMD ["api"]
```

- [ ] **Step 2: web 映像**

`web/package.json` 的 scripts 加 `"start": "node build"`。

`web/.dockerignore`：

```
node_modules/
build/
.svelte-kit/
test-results/
playwright-report/
.env
```

`web/Dockerfile`：

```dockerfile
FROM node:24-alpine AS build
WORKDIR /app
RUN npm install -g pnpm@10
COPY package.json pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile
COPY . .
RUN pnpm build

FROM node:24-alpine
WORKDIR /app
ENV NODE_ENV=production PORT=3000 HOST=0.0.0.0
RUN npm install -g pnpm@10
COPY package.json pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile --prod
COPY --from=build /app/build ./build
USER node
EXPOSE 3000
CMD ["node", "build"]
```

（若 `pnpm install --prod` 因 `prepare` script 跑 `svelte-kit sync` 失敗，加 `--ignore-scripts`。）

- [ ] **Step 3: compose、Caddyfile、env 範本**

`deploy/docker-compose.yml`：

```yaml
# 正式環境（規格 §16）：caddy 是唯一對外的入口；api／web 不開 ports（限流靠 X-Forwarded-For，只能信 Caddy 寫的）
name: dog_shop

services:
  caddy:
    image: caddy:2
    restart: unless-stopped
    ports:
      - "80:80"
      - "443:443"
      - "443:443/udp"
    environment:
      SITE_ADDRESS: "${SITE_ADDRESS:?請在 deploy/.env 設定 SITE_ADDRESS（例如 shop.example.com）}"
    volumes:
      - ./Caddyfile:/etc/caddy/Caddyfile:ro
      - caddy_data:/data
      - caddy_config:/config
    depends_on:
      - web
      - api

  web:
    build:
      context: ../web
    restart: unless-stopped
    environment:
      API_INTERNAL_URL: http://api:8080
      PUBLIC_BASE_URL: "${PUBLIC_BASE_URL:?請在 deploy/.env 設定 PUBLIC_BASE_URL}"
      ORIGIN: "${PUBLIC_BASE_URL}"
      PROTOCOL_HEADER: x-forwarded-proto
      HOST_HEADER: x-forwarded-host
      PORT: "3000"
    depends_on:
      api:
        condition: service_healthy

  api:
    build:
      context: ../api
    restart: unless-stopped
    env_file:
      - .env
    environment:
      DATABASE_URL: "postgres://dog_shop:${POSTGRES_PASSWORD:?請在 deploy/.env 設定 POSTGRES_PASSWORD}@db:5432/dog_shop"
      UPLOAD_DIR: /data/uploads
      LISTEN_ADDR: 0.0.0.0:8080
    volumes:
      - uploads:/data/uploads
    depends_on:
      db:
        condition: service_healthy
    healthcheck:
      test: ["CMD", "curl", "-fsS", "http://localhost:8080/api/health"]
      interval: 10s
      timeout: 5s
      retries: 12
      start_period: 20s

  db:
    image: postgres:17
    restart: unless-stopped
    environment:
      POSTGRES_USER: dog_shop
      POSTGRES_PASSWORD: "${POSTGRES_PASSWORD:?}"
      POSTGRES_DB: dog_shop
    volumes:
      - pgdata:/var/lib/postgresql/data
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U dog_shop -d dog_shop"]
      interval: 5s
      timeout: 5s
      retries: 12

  backup:
    image: postgres:17
    restart: unless-stopped
    environment:
      PGHOST: db
      PGUSER: dog_shop
      PGPASSWORD: "${POSTGRES_PASSWORD:?}"
      PGDATABASE: dog_shop
      BACKUP_KEEP_DAYS: "14"
    volumes:
      - backups:/backups
      - ./backup.sh:/backup.sh:ro
    entrypoint: ["/bin/sh", "/backup.sh", "loop"]
    depends_on:
      db:
        condition: service_healthy

volumes:
  pgdata: {}
  uploads: {}
  caddy_data: {}
  caddy_config: {}
  backups: {}
```

`deploy/docker-compose.smoke.yml`（本機煙霧用的覆蓋檔：不占 80/443）：

```yaml
services:
  caddy:
    ports: !override
      - "8081:80"
```

（`!override` 需要 compose v2.24+；`docker compose version` 確認。若太舊，改成 `ports: ["8081:80"]` 並在 smoke 檔把 443 拿掉的方式寫進報告。）

`deploy/Caddyfile`：

```
# 反向代理（規格 §2）。
# 不設 trusted_proxies：Caddy 會忽略客戶端送來的 X-Forwarded-* 並自己寫入真實來源 IP，
# api 的限流（X-Forwarded-For）因此不可能被偽造 —— 前提是 api／web 不對外開埠（見 docker-compose.yml）。
# 若前面還有 CDN（Cloudflare），才在 global options 加 trusted_proxies，並只信任它的 IP 範圍。
{$SITE_ADDRESS} {
	encode zstd gzip

	# 圖片上傳 10 MB、匯入 xlsx 5 MB（api 自己還有 10 MB + 64 KB 的上限）
	request_body {
		max_size 12MB
	}

	handle /api/* {
		reverse_proxy api:8080
	}

	handle /uploads/* {
		reverse_proxy api:8080
	}

	handle {
		reverse_proxy web:3000
	}
}
```

`deploy/env.prod.example`（全部佔位符；**不含任何測試特店值**）：

```
# 正式環境設定範本：複製成 deploy/.env 再填。每一行都要填；沒填 api 會拒絕啟動（config.rs 的上線檢查）。
# 密碼只用英數字（DATABASE_URL 是用它組出來的）

# 網域（Caddy 用；不含 https://）
SITE_ADDRESS=shop.example.com
# 對外網址（必須 https://，結尾不帶 /）
PUBLIC_BASE_URL=https://shop.example.com
# 資料庫密碼（compose 同時用在 db 與 api）
POSTGRES_PASSWORD=CHANGE_ME_ALNUM_ONLY

COOKIE_SECURE=true
RUST_LOG=info,tower_http=info

# 綠界正式特店（三組都不能是測試值；HashKey／HashIV 只放這裡，不進 git、不進 log）
ECPAY_ENV=prod
ECPAY_AIO_MERCHANT_ID=
ECPAY_AIO_HASH_KEY=
ECPAY_AIO_HASH_IV=
ECPAY_LOGISTICS_MERCHANT_ID=
ECPAY_LOGISTICS_HASH_KEY=
ECPAY_LOGISTICS_HASH_IV=
ECPAY_INVOICE_MERCHANT_ID=
ECPAY_INVOICE_HASH_KEY=
ECPAY_INVOICE_HASH_IV=

# SMTP（Gmail 應用程式密碼、Resend、Mailgun 任一）
SMTP_HOST=
SMTP_PORT=587
SMTP_USER=
SMTP_PASS=
SMTP_FROM=

# 不要設 MAIL_LOG_BODY
```

`.gitignore` 加 `deploy/backups/`（還原時暫放用）。

- [ ] **Step 4: 備份與還原腳本**

`deploy/backup.sh`：

```sh
#!/bin/sh
# 用法：backup.sh once | loop
# pg_dump -Fc 到 $BACKUP_DIR/dog_shop-YYYYmmdd-HHMMSS.dump，刪掉超過 $BACKUP_KEEP_DAYS 天的（規格 §16：每日、保留 14 天）
# 連線資訊來自 PGHOST／PGUSER／PGPASSWORD／PGDATABASE（compose 的 backup 服務有設）
set -eu
DIR="${BACKUP_DIR:-/backups}"
KEEP="${BACKUP_KEEP_DAYS:-14}"

run_once() {
  mkdir -p "$DIR"
  f="$DIR/dog_shop-$(date +%Y%m%d-%H%M%S).dump"
  pg_dump -Fc -f "$f.tmp" && mv "$f.tmp" "$f"
  find "$DIR" -name 'dog_shop-*.dump' -mtime +"$KEEP" -delete
  echo "backup ok: $f"
}

case "${1:-once}" in
  once) run_once ;;
  loop)
    while true; do
      run_once || echo "backup failed"
      sleep 86400
    done
    ;;
  *) echo "用法：backup.sh once | loop" >&2; exit 2 ;;
esac
```

`deploy/restore.sh`（在主機上、`deploy/` 目錄執行）：

```sh
#!/bin/sh
# 用法：
#   ./restore.sh list                 列出 backups volume 裡的備份
#   ./restore.sh <dog_shop-….dump>    還原（會先停 api／web，還原完再啟動）
set -eu
cd "$(dirname "$0")"
case "${1:-}" in
  list)
    docker compose run --rm --entrypoint sh backup -c 'ls -1 /backups'
    ;;
  "")
    echo "用法：restore.sh list | restore.sh <檔名>" >&2; exit 2
    ;;
  *)
    name="$1"
    echo "停止 api 與 web…"
    docker compose stop api web
    echo "還原 $name（--clean --if-exists：會先清掉現有資料）…"
    docker compose run --rm --entrypoint sh backup -c "pg_restore --clean --if-exists --no-owner -d dog_shop /backups/$name"
    echo "重新啟動…"
    docker compose start api web
    echo "完成"
    ;;
esac
```

`chmod +x deploy/backup.sh deploy/restore.sh`。

- [ ] **Step 5: CI 加 docker build job**

`.github/workflows/ci.yml` 加：

```yaml
  docker:
    name: docker images build
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: docker/setup-buildx-action@v3
      - uses: docker/build-push-action@v6
        with:
          context: ./api
          push: false
          cache-from: type=gha,scope=api
          cache-to: type=gha,mode=max,scope=api
      - uses: docker/build-push-action@v6
        with:
          context: ./web
          push: false
          cache-from: type=gha,scope=web
          cache-to: type=gha,mode=max,scope=web
```

- [ ] **Step 6: 本機煙霧測試（唯一的真實驗證）**

全部在 `deploy/` 目錄（用 Write 工具寫一支 `smoke.sh` 到 `$CLAUDE_JOB_DIR/tmp` 再 `bash` 跑，因為沙盒不能 `cd` 也不能寫迴圈）。步驟：

1. 寫 `deploy/.env`（**不要 commit**；`.gitignore` 已擋 `.env`）：
   ```
   SITE_ADDRESS=http://localhost
   PUBLIC_BASE_URL=http://localhost:8081
   POSTGRES_PASSWORD=smoke123
   COOKIE_SECURE=false
   ECPAY_ENV=stage
   RUST_LOG=info,tower_http=info
   ```
   （stage 模式 + `http://` 站點位址：Caddy 不會去申請憑證；綠界憑證空白會退回 stage 常數；不會對外發任何請求。）
2. `docker compose --project-directory <worktree>/deploy -f <worktree>/deploy/docker-compose.yml -f <worktree>/deploy/docker-compose.smoke.yml config` → 存到 log，**斷言** `api` 與 `web` 服務下沒有 `ports:`（`grep -n 'ports' 輸出，只允許出現在 caddy 下），且 `SITE_ADDRESS`／`DATABASE_URL` 有被代入。
3. `nohup docker compose … build > build.log 2>&1 &`，用控制者的 `wait-pid.sh` 等（Rust 映像約 10～20 分鐘）。exit 0。
4. `docker compose … up -d`；等 api healthy（`docker compose … ps` 每 10 秒看一次，最多 2 分鐘 —— 寫在 smoke.sh 裡）。
5. 驗證（都要寫進報告）：
   - `curl -fsS http://localhost:8081/api/health` → 200。
   - `curl -fsS http://localhost:8081/api/settings/public` → JSON。
   - `curl -fsS -o /dev/null -w '%{http_code}' http://localhost:8081/` → 200（web SSR 經 Caddy 打到 api 內網）。
   - `curl -fsS -o /dev/null -w '%{http_code}' -H 'X-Forwarded-For: 1.2.3.4' http://localhost:8081/api/health` → 200，然後 `docker compose … logs api | grep -c '1.2.3.4'` 應為 0（Caddy 沒把偽造的 XFF 傳進來；api 的 tower_http trace 若不印 IP 就寫「無法從 log 驗證，改看 Caddy 文件的預設行為」）。
   - `docker compose … run --rm api api create-admin smoke@example.com`（`ADMIN_PASSWORD=smoke12345` 環境變數）→ 成功；用 curl 登入拿 cookie → `GET /api/auth/me` 200。
   - 備份往返：`docker compose … run --rm --entrypoint sh backup -c '/backup.sh once'` → 「backup ok」；`./restore.sh list` 看到檔名；記下 `SELECT count(*) FROM users`（`docker compose … exec -T db psql -U dog_shop -d dog_shop -tAc '…'`）；插一列垃圾（`INSERT INTO categories …`）再 `./restore.sh <檔名>` → 垃圾列消失、users 數不變、api 重新 healthy。
   - `docker compose … stop api` 後 `docker compose … logs api | tail -n 3` 含 `shutting down`（SIGTERM 有接到）。
6. `docker compose … down -v`；`trash deploy/.env`；確認 `git status` 沒有多出檔案（`build.log` 等放在 job tmp）。

- [ ] **Step 7: Commit**

Commit：`feat(deploy): Dockerfile、docker compose（caddy/web/api/db/backup）、Caddyfile、備份還原腳本、CI 映像建置（計畫 5 Task 7）`

---

### Task 8: 部署與對帳手冊、README、總驗收

**Files:**
- Create: `docs/deploy.md`、`README.md`
- Modify: `docs/dev/ecpay-stage.md`（在「前置」加一句：正式部署見 `docs/deploy.md`）
- Test: 全部閘門 + Playwright e2e；本計畫驗收清單

**Interfaces:** 無新程式碼。

- [ ] **Step 1: `docs/deploy.md`**

用繁體中文寫給老闆（可以由懂一點指令的朋友代操作），章節與內容必須包含：

1. **上線前要準備的東西**（規格 §17 逐條：網域＋VPS 2 vCPU／4 GB、綠界正式特店三組 HashKey／HashIV、SMTP、寄件人姓名手機＋退貨門市、商店名稱／Logo／聯絡方式／退換貨說明；商品資料檔）。加一條：**綠界帳戶要先預存運費**（錯誤碼 10500049，計畫 4 交接 6）。
2. **第一次部署**：VPS 裝 Docker；`git clone`；`cp deploy/env.prod.example deploy/.env` 逐行填；DNS A 記錄指到 VPS；`cd deploy && docker compose up -d --build`；`docker compose run --rm api api create-admin <email>`（`ADMIN_PASSWORD` 環境變數）；登入 `/admin/settings` 填寄件人、退貨門市、運費；到綠界廠商後台把付款結果、物流狀態、更新門市通知的網址設成 `https://<domain>/api/ecpay/payment/return`、`/payment/info`、`/logistics/status`、`/logistics/store-update`（照 `api/src/routes/ecpay_*.rs` 的實際路徑列，`grep -n 'route(' api/src/routes/ecpay_payment.rs api/src/routes/ecpay_logistics.rs`）。
3. **上線檢查清單**（每條一個勾）：`ECPAY_ENV=prod`；三組憑證都不是 `.env.example` 的值（api 啟動會自己擋，但要寫清楚錯誤訊息長什麼樣）；`PUBLIC_BASE_URL` 是 https；`COOKIE_SECURE=true`；SMTP 寄得出信（註冊一個測試帳號收信）；`RUST_LOG` 至少 `info`（不得把 `dog_shop_api::routes` 降到 warn 以下，否則對帳 log 會消失）；`/admin/settings` 寄件人手機是台灣手機；退貨門市代號；綠界後台回呼網址；綠界帳戶餘額；備份服務在跑（`docker compose ps backup`）。
4. **限流與反向代理**：說明 Caddy 預設忽略客戶端的 `X-Forwarded-*`、api／web 不開埠，所以不用改；若加 Cloudflare 才要 `trusted_proxies`（給範例）。
5. **Log 與孤兒物流單對帳**（計畫 4 審查交接 1）：`docker compose logs api` 是 JSON、每行有 `request_id`；**必須保留的 log 訊息**（引訊息文字，不引行號）：「綠界物流單已建立」（含 `logistics_id`）、「綠界建立物流單連線失敗」與「綠界建立物流單失敗」（含 `merchant_trade_no`）、「綠界已建單，但這次嘗試已被重新認領；單號只留在 log，請到綠界廠商後台對帳」（含 `merchant_trade_no` 與 `logistics_id`）、「連線失敗的結果寫不進去」／「建單失敗的結果寫不進去」。對帳步驟：什麼情況要對（訂單頁出現「連線綠界失敗，請先到綠界廠商後台確認」或 log 有「已被重新認領」）；到綠界廠商後台「物流建單及查詢」用訂單編號（`DS…`）前綴搜尋；同一筆訂單有兩張以上 `…L01`／`…L02` 就是孤兒；孤兒在綠界後台處理（本系統沒有取消物流單的 API）；訂單頁「出貨區」的 `raw.create_requests` 有每次嘗試的欄位。
6. **備份與還原**：`backup` 服務每 24 小時 `pg_dump`，保留 14 天，放在 Docker volume `dog_shop_backups`；`./restore.sh list`／`./restore.sh <檔名>`（會先停 api／web、清掉現有資料再還原）；圖片在 volume `dog_shop_uploads`，另外用 `docker run --rm -v dog_shop_uploads:/data -v $PWD:/out alpine tar czf /out/uploads.tgz /data` 抄走；建議每週把 dump 與 uploads 抄到 VPS 以外的地方。
7. **更新與回滾**：`git pull && docker compose up -d --build`；migration 在 api 啟動時自動跑；回滾 = `git checkout <前一個 tag>` 再 `up -d --build`（migration 不會自動倒退，所以先備份）。
8. **匯入商品**：`/admin/import` 的操作；範本欄位；蝦皮匯出檔的對應是暫定的，第一次拿真檔匯入時對不上的欄位會列在預覽頁，回報後補別名；圖片下載時間估算（每張最多 10 秒、6 張同時，500 個商品 × 9 張最壞約 2 小時 → 分批匯入）；匯入的商品是草稿。
9. **上線後要看的事**：`auto_complete_shipped` 對 `arrived` 不排除（計畫 4 審查交接 7）—— 第一批超商訂單到店後看一次時序；綠界貨態代碼會更新（計畫 4 交接 3，對照表在 `ecpay/logistics.rs::shipment_status_for`）；`cvs_map_requests` 列數是「有人在刷」的便宜指標（`SELECT count(*) FROM cvs_map_requests`）；jobs 表 `failed` 列數；worker 多副本的注意事項（計畫 4 交接 2）。
10. **尚未做的與人工驗收**（合併計畫 2／3／4 的清單）：計畫 2 items 5／7／8／9；計畫 3 stage 走查（含 `CarrierType=1` 不帶 `CustomerID`、`RtnCode` 型別）；計畫 4 stage 走查（`docs/dev/ecpay-stage.md` 超商取貨節：`/Express/Create` 真實回應的 MD5 驗證、三家超商是否都接受 `LogisticsC2CReplyURL`、Safari 列印彈出視窗）；發票 GetIssue、計畫 1／2 小項（計畫 4 交接 7）；Playwright 不在 CI（怎麼手動跑）。

- [ ] **Step 2: `README.md`**

根目錄 README（約 40 行）：專案是什麼、目錄（api／web／deploy／docs）、開發快速開始（`docker compose -f deploy/docker-compose.dev.yml up -d db`、`cp .env.example .env`、`cargo run`、`pnpm dev`、`create-admin`）、測試指令、部署 → `docs/deploy.md`、綠界 stage 走查 → `docs/dev/ecpay-stage.md`、規格與計畫在 `docs/superpowers/`。

`docs/dev/ecpay-stage.md` 的「前置」節加一句「正式部署與上線檢查見 `docs/deploy.md`」。

- [ ] **Step 3: 總驗收（全部要綠，輸出寫進報告）**

1. `cargo fmt --all --check`；`cargo clippy --all-targets -- -D warnings`；完整 `cargo test`（背景；預期 `test result` 行數 ≥ 30、0 failed、passed ≥ 260）。
2. `pnpm -C web check`（0 errors）、`pnpm -C web test`（≥ 38 passed）、`pnpm -C web build`。
3. Playwright：起 api 與 web dev（背景），`pnpm -C web test:e2e`（2 passed），關掉伺服器。
4. `git status` 乾淨；`git log --oneline 80509df..HEAD` 列出本計畫 8 個以上的 commit。
5. 用 `grep -rn 'XBERn1YOvpM9nfZc\|pwFHCqoQZGmho4w6\|ejCk326UnaZWKisg' deploy/ docs/deploy.md README.md` 確認正式範本與手冊沒有測試特店值（只允許出現在 `.env.example`、`config.rs`、`docs/dev/ecpay-stage.md`、規格／計畫／審查文件）。

- [ ] **Step 4: Commit**

Commit：`docs: 部署與對帳手冊、README、計畫 5 總驗收（計畫 5 Task 8）`

---

## 驗收清單（控制者在最終審查前逐項核對）

1. Rust：fmt exit 0、clippy 零輸出、完整 `cargo test` 0 failed（含新測試：`admin_products` +2、`categories` +1、`import::columns` 3、`import::parse` 5、`import_parse` 2、`import::images` 2、`admin_import` 5、`config::` 5、`jobs_worker` +1）。
2. web：check 0/0、vitest（+2）、build ok、Playwright 2 passed。
3. `/admin/import` 上傳 → 預覽 → 確認匯入的煙霧（curl SSR 200 + Task 4 的整合測試）。
4. `deploy/` 煙霧：`docker compose config` 通過且 api／web 無 `ports`、build 成功、`/api/health` 與首頁經 Caddy 200、`create-admin` 可用、備份→還原往返、SIGTERM graceful；`.env` 沒被 commit。
5. `deploy/env.prod.example`、`docs/deploy.md`、`README.md` 不含測試特店值。
6. 與規格不同之處 49–60 都在計畫裡有對應實作或文件。
7. 人工（留給使用者）：拿真的蝦皮匯出檔試匯入並回報對不上的欄位；在 VPS 依 `docs/deploy.md` 部署一次；stage 走查（計畫 3／4 的清單）。

## 交給上線後的事項（寫進 `docs/deploy.md` 第 9、10 節；這裡是給控制者的對照）

1. 蝦皮別名表要對著真檔調（規格 §13、§17 第 6 點）。
2. 圖片匯入是同步的，商品多要分批；若之後要背景化，得放棄 §13 的「每列即時警告」。
3. worker 多副本：`mark_done`／`mark_failed_attempt` 已加守衛，但 2 分鐘的建單重認領窗口仍是設計取捨（計畫 4 交接 2、審查附錄 C）。
4. Cloudflare 或其他 CDN 加在 Caddy 前面時要設 `trusted_proxies`，否則所有請求的來源 IP 都是 CDN。
5. 發票 GetIssue、計畫 1／2 小項、儀表板拆宅配／超商：未做。
6. Playwright 不在 CI。

<!-- PLAN5-END -->
