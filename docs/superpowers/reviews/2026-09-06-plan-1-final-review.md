### Strengths

- Auth is correct end-to-end. argon2id with crate defaults (`api/src/auth/password.rs:6-11`), sessions are random UUID v4 stored server-side with a 30-day `expires_at` checked in SQL (`api/src/auth/session.rs:10-25`), cookie is `HttpOnly; SameSite=Lax; Path=/; Max-Age=2592000` plus `Secure` unless `COOKIE_SECURE=false`, and an unset `COOKIE_SECURE` defaults to secure (`api/src/config.rs:24-30`). `/me` is 401 without a valid session; logout deletes the row and clears the cookie. Every `/api/admin/*` handler takes `AdminUser` (admin_products, categories, uploads) and, because extractors run in order, the multipart body is never read for a non-admin.
- CSRF is defence-in-depth: SameSite=Lax + a mandatory `X-Requested-With: fetch` (forces a CORS preflight cross-origin) + exact Origin match against `PUBLIC_BASE_URL` (`api/src/auth/csrf.rs:16-44`). The browser client sets the marker on every non-GET (`web/src/lib/api.ts:50`); every `serverApi` call site in Plan 1 is a GET, so the SSR path never needs it.
- Uploads trust nothing from the client: decode with a minimal decoder set (`api/Cargo.toml:22`, gif/jpeg/png/webp only), always re-encode to JPEG, uuid v7 filenames under `yyyy/mm`, no user-controlled path segment, `ServeDir` for `/uploads`, `Cache-Control` only on 2xx (tested in `tests/uploads.rs`).
- SQL: every user value is a bind parameter. The single dynamic query (`api/src/domain/products.rs:552-569`) interpolates a match-arm literal, and the route validates `sort` first. Product create/update writes product + images + variants in one transaction; unique/FK violations become field errors; variant ids from other products cannot be hijacked (`v.id.filter(|id| existing.contains(id))`).
- No N+1 and the indexes cover every query path: both list endpoints are single statements (LATERAL price/stock aggregate + correlated thumb subquery + `COUNT(*) OVER ()`), `get_public` is four fixed queries. `0001_init.sql` provides `products(status, created_at DESC)` (newest listing), `products(category_id, status)`, unique `products.slug`, `product_variants(product_id)`, `product_images(product_id, sort_order)`, `sessions(expires_at)`, `sessions(user_id)` and unique `lower(email)`; ILIKE on `name` is an intentional sequential scan at catalog scale. The largest files, `domain/products.rs` (756 lines) and `ProductForm.svelte` (427 lines), are cohesive single-concern files.
- One error envelope everywhere: handlers, extractor rejections (Json/Query/Path/Multipart), the governor error handler, and `web/src/lib/api.ts` parses exactly that shape (tested).
- Cross-task contracts line up: `web/src/lib/types.ts` matches the Rust serialisations field-for-field (including `#[serde(flatten)]` on `AdminProduct` and `#[serde(skip)] total`), `serverApi` forwards the cookie and uses `API_INTERNAL_URL`, `/uploads/...` paths are rendered as returned, `cart.add` is called with the `Omit<CartLine,'qty'>` shape and qty as the second argument, `Page.total` semantics are shared.
- Tests run through the full `app::router` stack (all middleware) via `Router::oneshot` (`api/tests/common/mod.rs:60`); 54 green; negative paths 400/401/403/404/429 and both CSRF failure modes are covered; CI runs the identical commands against a Postgres 17 service.
- Svelte 5 is used correctly: runes only, `untrack` initial snapshots + `{#key updated_at}` remount for the product form, module-level `cart`/`toast` singletons are never mutated during SSR (`load()` only in `onMount`).
- Hygiene: none of the 103 files is a `.env`; the only `HashKey`/`HashIV` strings on the branch are the spec §8.5 public stage values in `.env.example:22-23`; no `tracing::`/`println!` line touches a password, hash or `DATABASE_URL`; `.gitignore` covers `.env`, `.env.*`, `uploads/`, `api/target/`, `web/node_modules/`, `web/build/`, `web/.svelte-kit/`.

### Issues

#### Critical (Must Fix)

None found.

#### Important (Should Fix)

1. **The request id never reaches the logs, so an `INTERNAL` error cannot be correlated with anything.** `api/src/app.rs:47` uses `TraceLayer::new_for_http()`, whose default span is `debug_span!("request", ...)` under target `tower_http`; `api/src/main.rs:15` filters `info,tower_http=info`, so that span is never created. The one place internals are logged, `api/src/error.rs:69` (`tracing::error!(error = ?err, "internal error")`), therefore carries no method, path or request id. Deviation 6 moved the id from the JSON body to the `x-request-id` header but did not decide that logs omit it, and spec §14 ("內部錯誤只回 INTERNAL 與 request id，細節在 log") only works if the log line can be found by that id. Fix (about 10 lines): `TraceLayer::new_for_http().make_span_with(|req: &Request<_>| tracing::info_span!("request", method = %req.method(), path = %req.uri().path(), request_id = ?req.headers().get("x-request-id")))`. `SetRequestIdLayer` is outside `TraceLayer` (comment at `app.rs:18-19` is accurate), so the header is present when the span is made; the JSON subscriber includes span fields by default. Add an assertion or a doc line so `RUST_LOG=info` keeps the span.

2. **EXIF orientation is discarded on re-encode, so portrait phone/camera JPEGs will display rotated.** `api/src/storage/mod.rs:44` decodes with `image::load_from_memory`, which does not apply the Orientation tag, and `storage/mod.rs:59-63` writes a fresh JPEG with no EXIF. Browsers render the original correctly (CSS default `image-orientation: from-image`) but the stored derivative has raw sensor orientation, so any file whose Orientation ≠ 1 (Android camera originals, DSLR/mirrorless files; iOS often bakes rotation in on upload) appears turned 90°/180° on the product page and thumbnail. This is deterministic pipeline behaviour, not a browser quirk, and product photos are the primary admin flow. One-minute check for the owner: upload a portrait JPEG taken with an Android phone and open the product page. Fix (image is at 0.25.10, so the API exists): `let mut decoder = ImageReader::new(Cursor::new(bytes)).with_guessed_format()?.into_decoder()?; let orientation = decoder.orientation()?; let mut decoded = DynamicImage::from_decoder(decoder)?; decoded.apply_orientation(orientation);` then continue with the existing resize/encode. Report `width`/`height` after applying orientation.

#### Minor (Nice to Have)

1. Unknown routes and wrong methods return axum's empty-body 404/405 instead of the JSON envelope (`tests/csrf.rs` `exempt_ecpay_path_skips_check` shows the bare 404). Add `.fallback(|| async { ApiError::NotFound })` and `.method_not_allowed_fallback(...)` in `api/src/app.rs`.
2. No decode limits on uploads (`storage/mod.rs:44`): `load_from_memory` uses `Limits::default()` (512 MiB `max_alloc`, no width/height cap), so a 10 MB PNG of a 12,000×9,000 image allocates ~430 MB plus a second buffer for `to_rgb8`. Admin-only, so not exploitable by the public, but a couple of large phone photos in flight can OOM a 4 GB VPS. `image::Limits` is `#[non_exhaustive]`, so build it as `let mut limits = Limits::default(); limits.max_image_width = Some(16_384); limits.max_image_height = Some(16_384); limits.max_alloc = Some(256 << 20); reader.limits(limits);` (map `LimitsExceeded` to the same "圖片無法讀取" field error).
3. `page` has no upper bound: `(page - 1) * per_page` at `products.rs:472` and `:573` wraps on `?page=9223372036854775807` (release build) into a negative OFFSET, which Postgres rejects, producing a 500 `INTERNAL` and an error log from an unauthenticated request (debug build: panic). Clamp `page` in `clamp_paging` (e.g. max 10_000) or use `checked_mul`.
4. 413 collapses to 400: an over-limit body surfaces as a `MultipartError` and `api/src/routes/uploads.rs:22-28,37-40` maps every multipart error to `VALIDATION` with the raw multer text in `details.detail`. Use `e.status()` to preserve 413, and put a fixed message in `detail`. The in-handler `bytes.len() > MAX_UPLOAD_BYTES` check (`uploads.rs:41`) is only reachable in the 64 KiB window between the two limits.
5. `safeRedirect` (`web/src/routes/login/+page.svelte:12-15`) accepts `/\evil.com`, which the URL parser resolves to `https://evil.com/`. SvelteKit 2's `goto` rejects cross-origin URLs, so this is not an open redirect, but the login then fails with the generic "登入失敗" message. Use `/^\/(?![\/\\])/`.
6. `Config` derives `Debug` (`api/src/config.rs:6-8`) and contains `database_url`, which carries the DB password. It is not logged today; implement `Debug` manually (redact `database_url`) so a future `?config` cannot violate spec §11.
7. `/products` drops `per_page` (`web/src/routes/products/+page.server.ts:8` forwards only `q, category, sort, page`) although spec §6.1 lists it as a page parameter; the API supports it. Add `'per_page'` to the forwarded keys.
8. Categories admin page surfaces field errors via toast (`web/src/routes/admin/categories/+page.svelte:21-24`) rather than beside the field (spec §14); ProductForm and login comply. Low impact (two fields).
9. Files removed from a product (or uploaded and never attached) are never deleted from `UPLOAD_DIR`; disk grows monotonically. Note for a later cleanup job (list `product_images.path` ∪ thumb paths, delete the rest older than N days).
10. Sessions deliberately use UUID v4 (`session.rs:10`) against the spec's "主鍵一律 UUID v7". This is the right call (v7 is time-ordered and partly predictable) but it is not in the deviations list; record it as deviation 9.
11. Product page: when the selected variant has its own image, `shownImage` (`web/src/lib/components/ProductView.svelte:34`) ignores thumbnail clicks. Consider letting an explicit thumbnail click override the variant image.
12. `admin/+layout.server.ts:7` encodes only `url.pathname` into `redirect=`, so `/admin/products?status=draft` returns to `/admin/products` after login.
13. `web/README.md` is the untouched `sv create` boilerplate; replace with the real dev recipe (compose db, `cargo run`, `create-admin`, `pnpm dev`, env vars).
14. `Toasts.svelte:5` container lacks `role="status" aria-live="polite"` (T11 minor; trivial).

### Deferred-minor triage

- T1 `.gitignore` redundant `api/uploads/` line — CAN SHIP (harmless duplicate).
- T2 `Config.cookie_secure` parsing untested; `ApiError::field` untested directly — CAN SHIP (`field()` is exercised through every validation test; parsing is 5 lines with a safe default).
- T2 `app.rs` layer-order comment — resolved in 3f4609b; comment now matches the code — CAN SHIP.
- T3 settings test asserts only `name`/`contact_email`; migration test checks existence not constraints — CAN SHIP (constraints are exercised indirectly by FK/unique tests).
- T4 Postgres NOTICE `_sqlx_migrations already exists` at INFO on startup — CAN SHIP (add `sqlx=warn` to `RUST_LOG`).
- T4 async hash/verify wrappers have no direct unit test — CAN SHIP (covered by login integration tests).
- T5 30-day constant duplicated (`cookie.rs` vs `session.rs`) — CAN SHIP (fold into one `pub const`).
- T5 `AdminUser` 403 path untested until Task 7 — closed: `customer_is_forbidden`, `customer_and_guest_cannot_manage_categories`, `customer_cannot_upload` — CAN SHIP.
- T5 no invalid/expired `sid` tests — CAN SHIP, but add one test that inserts a session with `expires_at = now() - interval '1 day'` and asserts 401 on `/me`: the `expires_at > now()` check at `session.rs:24` is the only security-relevant check on the branch with no test, and 413 is the only negative status in the brief with no test (its current behaviour is Minor 4).
- T5 per-`router()` `retain_recent` thread — CAN SHIP (one per process in production; test-only leak).
- T5 login timing side-channel (verify only when the user exists) — CAN SHIP (10/min/IP limit, and Plan 2's register endpoint reveals existence anyway); cheap fix when convenient: verify against a constant dummy PHC string when the user is missing.
- T5 429 lacks `Retry-After` — CAN SHIP.
- T5 `sid` parse duplicated in `extract.rs` and `routes/auth.rs` — CAN SHIP.
- T7 PUT categories is full replacement (omitted slug → random, omitted sort_order → 0) — CAN SHIP; verified `admin/categories/+page.svelte:44` always sends `name, slug, sort_order`.
- T7 name/slug errors not aggregated; random-slug collision reads as "slug taken" — CAN SHIP.
- T8 #1 PUT without slug rotates the product slug — ruling stands; verified ProductForm pre-fills and sends `slug` — CAN SHIP.
- T8 #2 slug resolution duplicates `categories::resolve_slug` — CAN SHIP.
- T8 #3 DB violation mapping by SQLSTATE class, not constraint name — CAN SHIP for Plan 1 (only `products.slug` is writable), but MUST be revisited before Plan 5 makes `external_ref` writable, otherwise an `external_ref` conflict is reported as "這個網址代稱已經有人用了" (see Recommendations).
- T8 #4 `list_admin` offset arithmetic unbounded — CAN SHIP; same root cause as Minor 3 (also affects the public list), fix both together.
- T8 #5 `total` = 0 on a past-the-end page — CAN SHIP (Pagination then hides the nav; cosmetic).
- T9 #1 upload filenames are UUID v7 not v4 — CAN SHIP (filenames need uniqueness, not unpredictability; the directory is public anyway).
- T9 #2 thumbnails upscale images smaller than 400 px — CAN SHIP (skip `resize` when `max(w,h) <= THUMB_MAX_EDGE`).
- T9 #3 was the review's Important (bare `Multipart` rejection bypassing the JSON envelope), not a minor; fixed in bce2389 as `AppMultipart` with test `non_multipart_upload_is_validation_error` — closed.
- T9 #4 `save()` writes main then thumb with no cleanup on partial failure, no tmp+rename — CAN SHIP (worst case one orphan file).
- T9 #5 duplicated `png()` test helper — CAN SHIP.
- T10 ILIKE `%`/`_` in `q` unescaped — CAN SHIP: the value is a bind parameter (no injection), the only effects are broader matches and, on a large catalog, a slower scan; escape with `replace('%','\\%')`/`_` and `ESCAPE '\'` when the catalog grows or the import lands.
- T10 `total` = 0 when OFFSET is past all rows — CAN SHIP.
- T11 #1 scaffold `web/.gitignore`, `web/tsconfig.json` committed — CAN SHIP (required).
- T11 #2 JSON content-type logic duplicated in `api.ts`/`server/api.ts` — CAN SHIP.
- T11 #3 Toasts lacks `aria-live` — CAN SHIP (Minor 14).
- T12 categories page has no in-flight/double-submit guard — CAN SHIP (a double click can create two categories with random slugs; add a `submitting` flag like the login page).
- T12 `$effect` re-sync replaces rows and drops unsaved edits in other rows — CAN SHIP (single-admin tool).
- T12 `web/static/` removed — CAN SHIP (route-based robots.txt exists).
- T13 unkeyed `{#each variants as v, i}` — CAN SHIP (Svelte 5 unkeyed blocks rebind by index correctly with `$state` proxies; key on a local id if rows get per-row async state).
- T13 `generateVariants` pushes the same object twice for duplicated option values — CAN SHIP (server rejects with "規格重複"; dedupe `splitValues` with a `Set`).
- T13 archive confirm has no in-flight guard — CAN SHIP (archive is idempotent).
- T13/T14/T15 `Generated an empty chunk: chunks/env.js` — CAN SHIP (informational).
- T14 `add()` `Object.assign` after qty — CAN SHIP (`Omit<CartLine,'qty'>` prevents it at the type level; ProductView passes no qty).
- T14 qty input one-way value can show out-of-range keystrokes at 99 — CAN SHIP.
- T15 search inputs lack labels — CAN SHIP (add `aria-label`).
- T15 invalid `sort` → API 400 → SvelteKit error page — CAN SHIP (optionally default to `newest` in the page load).
- T16 `sitemap.xml` has no try/catch when the API is down — CAN SHIP.
- T17 CI runs twice on PR branches (push + pull_request) — CAN SHIP (add `branches: [main]` under `push` or a `concurrency` group).

### Recommendations

Deployment (owner / Plan 5):
- Never publish the api container's port 8080 on the host; only Caddy may reach it. Ruling 7 (`SmartIpKeyExtractor`) keys the login limiter on the leftmost `X-Forwarded-For` entry, which is only trustworthy while Caddy is the sole peer. Do not set Caddy `trusted_proxies` unless a real upstream proxy exists; if Cloudflare or another CDN is ever put in front, switch the extractor to the rightmost hop or `CF-Connecting-IP`.
- `PUBLIC_BASE_URL` must equal the browser origin exactly (scheme, lowercase host, port; apex vs `www`), or every POST/PUT/DELETE fails with 403 "Origin 不符". Consider logging `config.public_origin()` at startup (not secret) to make misconfiguration obvious.
- `COOKIE_SECURE` unset means secure; set `false` only for plain-http development.
- `UPLOAD_DIR` should be an absolute path on a mounted volume in the api container, and the Plan 5 `backup.sh` should back up `uploads/` alongside `pg_dump` (the spec only mentions the database).
- web container env: `API_INTERNAL_URL=http://api:8080`, `PUBLIC_BASE_URL`, and `ORIGIN=https://<domain>` for adapter-node (it defaults to `https://` + Host, which works behind Caddy, but explicit is safer for `page.url.origin`, used by OG/JSON-LD).
- Migrations run at startup via `sqlx::migrate!`: never edit `0001_init.sql` after it has run in production (checksum mismatch aborts startup); add new numbered files.
- `RUST_LOG=info,tower_http=info,sqlx=warn` silences the migration NOTICE; after fixing Important 1, `info` must keep the request span.
- `create-admin`: prefer the interactive prompt on the VPS; the `ADMIN_PASSWORD` env path exists for scripts but leaves the password in shell history / `docker inspect`.
- Browser flows not exercised in this session and worth a 10-minute manual pass before go-live: image upload UI, drag-and-drop ordering, "依選項產生規格", add-to-cart toast, cart persistence across reload, a two-option product page, and a portrait phone photo (Important 2).

Plan 2 hand-off (in addition to the plan's own list):
- Variant deletion in `write_images_and_variants` (`products.rs:297-403`) must become deactivate-if-referenced once `order_items` exists; with an FK in place a PUT that drops a sold variant would otherwise 500.
- `GET /api/settings/public` returns the entire `shop` JSON (`routes/settings.rs:11-16`). When Plan 2 stores sender phone / return store / payment toggles in `settings`, keep them under separate keys or whitelist fields; do not extend the `shop` value.
- `map_product_db_error` (`products.rs:228-238`) maps any unique violation to the slug field; before Plan 5 makes `external_ref` writable, switch to matching `e.constraint()` names.
- Any SSR-side mutating call (form actions) must send `x-requested-with: fetch`; if an SSR-called endpoint is ever rate limited, `serverApi` must also forward the client IP as `X-Forwarded-For`.
- Routes under `/api/ecpay/*` are CSRF-exempt by prefix (`csrf.rs:11`); they must never take `AuthUser`/`AdminUser` or rely on cookies. Consider a test that asserts no exempt route uses a session extractor.
- Add the expired-session test and, if convenient, the dummy-hash verify for unknown emails.

### Assessment

**Ready to merge?** With fixes

**Reasoning:** No exploitable hole, data-loss path or broken core flow was found; the auth/CSRF/upload/SQL layers are sound and the cross-task contracts line up. Two small, well-scoped defects should land first: request-scoped logging so `x-request-id` is actually correlatable (spec §14), and EXIF orientation handling so product photos are not stored rotated. Everything in the deferred ledger can ship as is.
