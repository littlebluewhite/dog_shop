# 前端視覺重新設計 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把 dog_shop 前端（SvelteKit）換成「白底、橘主色、深藍墨字、膠囊按鈕」的雜貨小物店外觀：前台全部重新設計、後台套同一套元件，功能與文案一字不改。

**Architecture:** Tailwind 4 `@theme` token ＋ `web/src/app.css` 裡少量全域 class（`.input`、`.card`、`.table`、`.option-card`、`.chip`、`.link`）＋ 11 個自寫的 Svelte 5 元件（`web/src/lib/components/ui/`）。每一頁只改 markup：把原本的 class 與原生按鈕換成元件，`<script>` 邏輯不動（只加 import、刪掉因此沒人用的樣式字串常數）。Task 2 做完先給使用者看圖，使用者說可以才做其他頁。

**Tech Stack:** SvelteKit 2.70、Svelte 5.57（runes、`$bindable`、`$props.id()`、snippet）、Tailwind CSS 4.3（`@theme`、`@layer components`、`@apply`）、vitest 5、Playwright 1.63（chromium，只用來截圖與跑既有 e2e）、ImageMagick 7（只用來產示意圖片）。

**Spec:** `docs/superpowers/specs/2026-09-09-ui-redesign-design.md`（本計畫從它出發；執行者兩份都要讀）

## Global Constraints

1. **不引入任何套件**：沒有元件庫、沒有 icon 套件、沒有網路字型、沒有新的 npm 依賴。`web/package.json` 與 `pnpm-lock.yaml` 不變。字型只用系統字 `'PingFang TC', 'Noto Sans TC', 'Microsoft JhengHei', system-ui, sans-serif`。
2. **不改文案**：按鈕字、標籤字、提示字、錯誤字、空狀態字全部照舊（可以移動位置、換成元件，但字串一模一樣）。
3. **不改 `<script>` 邏輯**：`$state`、`$derived`、事件處理、API 呼叫、`bind:` 目標全部照舊。只允許（a）新增 import、（b）刪掉因改版而沒人用的樣式字串常數（`const input = 'mt-1 w-full rounded …'`、`const btn/primary/secondary/danger = …`）、（c）本計畫明講的少數例外（Task 8 儀表板 tile 的 `alert: true` 旗標）。
4. **Playwright 定位契約（`web/e2e/checkout.spec.ts`，一字不能改）**：
   - 按鈕（`getByRole('button')`）：`加入購物車`、`送出訂單`、`取消訂單`、`確定取消這筆訂單`、`選擇門市`（exact）。每一頁同名按鈕只能有一顆。
   - 連結（`getByRole('link')`）：`前往結帳`（可結帳時必須是 `<a>`）。
   - 標籤（`getByLabel`）：`/^Email/`、`收件人`、`手機`（exact）、`縣市`、`鄉鎮市區`、`郵遞區號`、`地址`：必須是真的 `<label>` 包住控制項，label 文字本身不含其他字（錯誤字放 label 外）。
   - radio（`getByRole('radio')`）：`/超商取貨/`、`全家`：名稱來自 `<label>` 文字。
   - 文字（`getByText`）：`已加入購物車`（toast）、`小計`、`總計`、`待付款`、`已取消`。
   - 標題（`getByRole('heading')`）：商品頁 `h1` = 商品名稱；訂單頁 `h1` 含訂單編號。
5. **根 layout 的 `<svelte:head>`** 裡 `<title>{page.data.title ?? data.shop.name}</title>` 與它上面那段關於 SSR settle loop 的註解原封不動。
6. **顏色 token**（只用這些，不再用 `gray-*`、`red-*`、`green-*`、`yellow-*`）：`ground #ffffff`、`ink #16274a`、`ink-soft #5b6b85`、`brand #ff8a00`、`brand-deep #d96a00`、`brand-soft #ffe9cf`、`brand-light #ffa640`（只給看板圖案）、`surface #f1f5fa`、`line #d9e1ec`、`success #15734a` / `success-soft #e3f6ec`、`warning #b45309` / `warning-soft #fff4db`、`danger #b91c1c` / `danger-soft #fde8e8`。
7. **橘色永遠不用來寫字**（對比只有 2.4:1）。橘底一律配 `ink` 深藍字。價格用 `ink` 粗體。
8. **數字用 `tabular-nums`**（body 已全域設定）。標題 800、`tracking-tight`；內文 16px。
9. **圓角層級**：按鈕／膠囊／標籤 `rounded-full`；輸入框／圖片格 `rounded-control`（12px）；卡片 `rounded-card`（16px）；看板 `rounded-hero`（24px）；縮圖 `rounded-lg`（8px）。
10. **動態**：只有 150ms 顏色變化與按下 `scale(.98)`；沒有卡片 hover 位移、沒有進場動畫。`prefers-reduced-motion` 由 `app.css` 全域處理。焦點框由 `app.css` 全域處理。
11. **RWD**：375px～1280px 不得出現水平捲軸；寬表格自己 `overflow-x-auto`。
12. **每個 task 的 gate**：`pnpm -C web check` 0 errors 0 warnings；`pnpm -C web test` 全過；`pnpm -C web build` 成功；該 task 負責的頁面各截 375×812 與 1280×800 一張，存到 `/Users/wilson08/.claude/jobs/82e5d2c3/tmp/shots/`，回報路徑與 `overflow=false`。
13. **Git**：分支 `worktree-ui-redesign`，工作區 `/Users/wilson08/IdeaProjects/dog_shop/.claude/worktrees/ui-redesign`。每個 task 一個 commit。**不 push、不開 PR**。commit 訊息結尾加 `Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>`。
14. **示意資料只進開發 DB**（`localhost:5435`、api `localhost:8080`）；永遠不對正式環境跑；任何測試、腳本只連 `localhost`，不得連 `*.ecpay.com.tw` 或任何外部主機。
15. **後台版面不動**（側欄＋內容），只套顏色、字級、按鈕、輸入框、卡片、表格。

## 測試策略

- 純邏輯（`qty.ts`、`status.ts`、`icons.ts`）用 vitest TDD。
- 元件與頁面是 markup，沒有 DOM 快照測試；gate 是 `svelte-check`（型別）、`build`（編譯）、截圖（控制者用眼睛看）、最後 Task 9 的 Playwright e2e（功能）。
- 每個 task 結束都跑三個指令：`pnpm -C web check`、`pnpm -C web test`、`pnpm -C web build`。

## 開發環境（要跑伺服器或截圖的 task 照抄）

工作區是新的 checkout，沒有 `node_modules`、`api/target`、`.env`：

```bash
# 1. 前端套件（約 6 秒）
pnpm -C web install
# 2. .env（開發 DB、stage 綠界、開發 SMTP）只存在主 checkout，複製一份進工作區；只 cp，不 mv，不 commit（.gitignore 已排除）
test -f .env || cp /Users/wilson08/IdeaProjects/dog_shop/.env .env
# 3. 開發 DB（compose 專案 dog_shop，container dog_shop-db-1，port 5435；跟主 checkout 共用同一個 DB）
docker compose -f deploy/docker-compose.dev.yml up -d --wait db
# 4. 後端：從工作區根目錄啟動（dotenvy 讀 ./.env）。UPLOAD_DIR 覆寫成主 checkout 的絕對路徑，圖片才不會留在工作區裡
export PATH="$HOME/.cargo/bin:$PATH"
UPLOAD_DIR=/Users/wilson08/IdeaProjects/dog_shop/uploads cargo run --manifest-path api/Cargo.toml
#    （用 run_in_background 跑；第一次要編譯，約 30 秒～數分鐘；看到 listening on 0.0.0.0:8080 才算好）
# 5. 前端 dev server（用 run_in_background 跑）
pnpm -C web dev
# 6. 健康檢查：一定用 localhost，不要用 127.0.0.1（Vite 8 只聽 IPv6 ::1）
curl -s http://localhost:8080/api/health
curl -s -o /dev/null -w '%{http_code}\n' http://localhost:5173/
```

- 啟動前先確認 port 沒人用：`lsof -ti :8080 -sTCP:LISTEN`、`lsof -ti :5173 -sTCP:LISTEN` 都要沒有輸出。
- 停伺服器：`lsof -ti :5173 -sTCP:LISTEN` 拿到 pid 再 `kill <pid>`；8080 同理。`pkill -f "vite dev"` 抓不到。
- 開發用 admin：`admin@example.com` / `admin12345`。
- 沙盒 shell 不能用 heredoc、`$(...)`、`for`、`cd`、前景 `sleep`、`timeout`、`rm -rf`（用 `trash`）。要寫腳本就用 Write 工具寫成檔案再 `sh` / `node` 執行。
- 截圖腳本（Task 2 建立，之後每個 task 共用）：`node /Users/wilson08/.claude/jobs/82e5d2c3/tmp/shot.mjs <url> <輸出前綴> [--login] [--cart <slug>]`。

---

### Task 1: Token、全域 class、11 個 ui 元件（含純邏輯測試）

**Files:**
- Modify: `web/src/app.css`（整個換掉）
- Modify: `web/src/app.html`（只拿掉 body 的 class）
- Create: `web/src/lib/components/ui/qty.ts`、`web/src/lib/components/ui/qty.test.ts`
- Create: `web/src/lib/components/ui/status.ts`、`web/src/lib/components/ui/status.test.ts`
- Create: `web/src/lib/components/ui/icons.ts`、`web/src/lib/components/ui/icons.test.ts`
- Create: `web/src/lib/components/ui/Icon.svelte`、`Logo.svelte`、`Button.svelte`、`Field.svelte`、`Card.svelte`、`Badge.svelte`、`StatusBadge.svelte`、`Alert.svelte`、`PageHeader.svelte`、`EmptyState.svelte`、`QtyStepper.svelte`

**Interfaces:**
- Consumes: `OrderStatus`、`ProductStatus`（`web/src/lib/types.ts:158`、`:27`）、`ORDER_STATUS_LABELS`（`web/src/lib/labels.ts`）。
- Produces（之後每個 task 都靠這些）：
  - `clampQty(next: number, min: number, max: number, fallback: number): number`
  - `type BadgeTone = 'brand' | 'neutral' | 'success' | 'warning' | 'danger'`；`orderStatusTone(status: OrderStatus): BadgeTone`；`productStatusTone(status: ProductStatus): BadgeTone`
  - `type IconName`（11 個名稱）；`ICON_PATHS: Record<IconName, string>`；`ICON_NAMES: IconName[]`
  - `<Icon name size? class? />`、`<Logo name size?='md'|'lg' inverted? />`
  - `<Button variant?='primary'|'secondary'|'ghost'|'danger' size?='sm'|'md'|'lg' href? disabled? type?='button'|'submit'|'reset' class? ...rest>children</Button>`：有 `href` 且沒 disabled → `<a>`；有 `href` 且 disabled → `<span aria-disabled="true">`；否則 `<button>`
  - `<Field label value=$bindable error? hint? class? ...inputAttrs />` 或 `<Field label error?>{children：自己的 select/textarea，加 class="input mt-1"}</Field>`
  - `<Card title? step? class? actions?={snippet}>children</Card>`
  - `<Badge tone?>children</Badge>`、`<StatusBadge status />`
  - `<Alert tone?='info'|'success'|'warning'|'danger' title? class?>children</Alert>`
  - `<PageHeader title subtitle?>children（右側動作）</PageHeader>`
  - `<EmptyState message>children（動作）</EmptyState>`
  - `<QtyStepper bind:value min?=1 max?=99 onchange?=(v)=>void label?='數量' />`
  - 全域 class：`.input`、`.field-label`、`.field-hint`、`.field-error`、`.card`、`.link`、`.chip` `.chip-idle` `.chip-selected` `.chip-off`、`.option-card`、`.table`

- [ ] **Step 1: 裝套件、確認基準**

```bash
pnpm -C web install
pnpm -C web check
pnpm -C web test
```
Expected: `check` 0 errors 0 warnings；`test` 全過（目前 38 個測試）。

- [ ] **Step 2: 寫三個會失敗的測試**

`web/src/lib/components/ui/qty.test.ts`：
```ts
import { describe, expect, it } from 'vitest';
import { clampQty } from './qty';

describe('clampQty', () => {
	it('範圍內原樣回傳（會去掉小數）', () => {
		expect(clampQty(5, 1, 99, 1)).toBe(5);
		expect(clampQty(2.7, 1, 99, 1)).toBe(2);
	});
	it('低於 min 夾到 min、高於 max 夾到 max', () => {
		expect(clampQty(0, 1, 99, 1)).toBe(1);
		expect(clampQty(200, 1, 99, 1)).toBe(99);
	});
	it('購物車用 min=0：減到 0 要回 0（外面會把該列移除）', () => {
		expect(clampQty(0, 0, 99, 1)).toBe(0);
	});
	it('不是數字就回 fallback（不動原值）', () => {
		expect(clampQty(Number.NaN, 1, 99, 3)).toBe(3);
		expect(clampQty(Number.POSITIVE_INFINITY, 1, 99, 3)).toBe(3);
	});
});
```

`web/src/lib/components/ui/status.test.ts`：
```ts
import { describe, expect, it } from 'vitest';
import { orderStatusTone, productStatusTone } from './status';

describe('orderStatusTone（規格 §4 StatusBadge）', () => {
	it('六個訂單狀態各對到規格說的 tone', () => {
		expect(orderStatusTone('pending_payment')).toBe('warning');
		expect(orderStatusTone('paid')).toBe('success');
		expect(orderStatusTone('shipped')).toBe('brand');
		expect(orderStatusTone('completed')).toBe('success');
		expect(orderStatusTone('cancelled')).toBe('neutral');
		expect(orderStatusTone('refunded')).toBe('neutral');
	});
});

describe('productStatusTone（後台商品狀態）', () => {
	it('上架綠、草稿黃、下架灰', () => {
		expect(productStatusTone('active')).toBe('success');
		expect(productStatusTone('draft')).toBe('warning');
		expect(productStatusTone('archived')).toBe('neutral');
	});
});
```

`web/src/lib/components/ui/icons.test.ts`：
```ts
import { describe, expect, it } from 'vitest';
import { ICON_NAMES, ICON_PATHS } from './icons';

describe('icons', () => {
	it('規格 §3.4 的 11 個名稱都有 path，且都是合法的 d 字串（M 或 m 開頭）', () => {
		expect(ICON_NAMES).toHaveLength(11);
		for (const name of ICON_NAMES) {
			expect(ICON_PATHS[name]).toMatch(/^[Mm]/);
		}
	});
	it('沒有寵物意象（規格 §1：這不是寵物店）', () => {
		expect(ICON_NAMES).not.toContain('paw');
		expect(ICON_NAMES).not.toContain('dog');
	});
});
```

- [ ] **Step 3: 跑測試，確認失敗**

Run: `pnpm -C web test`
Expected: 三個新檔 FAIL，原因是找不到模組 `./qty`、`./status`、`./icons`。

- [ ] **Step 4: 寫三個純邏輯檔**

`web/src/lib/components/ui/qty.ts`：
```ts
/** 把數量夾在 [min, max]，去掉小數；不是有限數字就回 fallback（呼叫端會傳目前的值，等於不動） */
export function clampQty(next: number, min: number, max: number, fallback: number): number {
	if (!Number.isFinite(next)) return fallback;
	return Math.min(Math.max(Math.trunc(next), min), max);
}
```

`web/src/lib/components/ui/status.ts`：
```ts
import type { OrderStatus, ProductStatus } from '$lib/types';

export type BadgeTone = 'brand' | 'neutral' | 'success' | 'warning' | 'danger';

/** 訂單狀態 → 膠囊顏色（規格 §4 StatusBadge） */
export function orderStatusTone(status: OrderStatus): BadgeTone {
	switch (status) {
		case 'pending_payment':
			return 'warning';
		case 'paid':
		case 'completed':
			return 'success';
		case 'shipped':
			return 'brand';
		case 'cancelled':
		case 'refunded':
			return 'neutral';
	}
}

/** 後台商品狀態 → 膠囊顏色 */
export function productStatusTone(status: ProductStatus): BadgeTone {
	switch (status) {
		case 'active':
			return 'success';
		case 'draft':
			return 'warning';
		case 'archived':
			return 'neutral';
	}
}
```

`web/src/lib/components/ui/icons.ts`：
```ts
export type IconName =
	| 'tag'
	| 'cart'
	| 'user'
	| 'search'
	| 'check'
	| 'x'
	| 'minus'
	| 'plus'
	| 'chevron-left'
	| 'chevron-right'
	| 'image';

/** 手寫的 24×24、2px 線、currentColor 圖示（規格 §3.4）；每個是一條 path 的 d（可含多段） */
export const ICON_PATHS: Record<IconName, string> = {
	tag: 'M3 3h8.6a2 2 0 0 1 1.4.6l7.4 7.4a2 2 0 0 1 0 2.8l-6.6 6.6a2 2 0 0 1-2.8 0L3.6 13A2 2 0 0 1 3 11.6V3zM7.5 6.5a1 1 0 1 0 0 2 1 1 0 1 0 0-2',
	cart: 'M3 4h2l2.4 11.2a2 2 0 0 0 2 1.6h7.8a2 2 0 0 0 2-1.5L21 8H7M9 21a1 1 0 1 0 0-2 1 1 0 0 0 0 2M18 21a1 1 0 1 0 0-2 1 1 0 0 0 0 2',
	user: 'M20 21a8 8 0 0 0-16 0M12 13a4 4 0 1 0 0-8 4 4 0 0 0 0 8',
	search: 'm21 21-4.3-4.3M11 18a7 7 0 1 0 0-14 7 7 0 0 0 0 14',
	check: 'M20 6 9 17l-5-5',
	x: 'M18 6 6 18M6 6l12 12',
	minus: 'M5 12h14',
	plus: 'M12 5v14M5 12h14',
	'chevron-left': 'm15 18-6-6 6-6',
	'chevron-right': 'm9 18 6-6-6-6',
	image: 'M5 3h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2M9 9a1 1 0 1 0 0-2 1 1 0 0 0 0 2m12 8-5-5L5 21'
};

export const ICON_NAMES = Object.keys(ICON_PATHS) as IconName[];
```

- [ ] **Step 5: 跑測試，確認通過**

Run: `pnpm -C web test`
Expected: 全過，測試數比基準多 6 個（`qty` 4、`status` 2、`icons` 2 → 共 8 個新測試；總數 46）。

- [ ] **Step 6: 換掉 `web/src/app.css`（整檔）**

```css
@import 'tailwindcss';

/* 設計 token（規格 §3）。Tailwind 4：--color-* 變成 bg-* / text-* / border-*，--radius-* 變成 rounded-* */
@theme {
	--font-sans: 'PingFang TC', 'Noto Sans TC', 'Microsoft JhengHei', system-ui, sans-serif;

	--color-ground: #ffffff;
	--color-ink: #16274a;
	--color-ink-soft: #5b6b85;
	--color-brand: #ff8a00;
	--color-brand-deep: #d96a00;
	--color-brand-soft: #ffe9cf;
	--color-brand-light: #ffa640;
	--color-surface: #f1f5fa;
	--color-line: #d9e1ec;
	--color-success: #15734a;
	--color-success-soft: #e3f6ec;
	--color-warning: #b45309;
	--color-warning-soft: #fff4db;
	--color-danger: #b91c1c;
	--color-danger-soft: #fde8e8;

	--radius-control: 12px;
	--radius-card: 16px;
	--radius-hero: 24px;
}

@layer base {
	body {
		background-color: var(--color-ground);
		color: var(--color-ink);
		font-variant-numeric: tabular-nums;
	}
	h1,
	h2,
	h3 {
		letter-spacing: -0.01em;
		line-height: 1.2;
	}
	:focus-visible {
		outline: 2px solid var(--color-brand);
		outline-offset: 2px;
	}
	@media (prefers-reduced-motion: reduce) {
		*,
		*::before,
		*::after {
			transition-duration: 0s !important;
			animation-duration: 0s !important;
		}
	}
}

/* 共用 class（規格 §4 末段）。元件與原生 select/textarea/radio 共用 */
@layer components {
	.input {
		@apply w-full rounded-control border border-line bg-ground px-3 py-2.5 text-base text-ink transition-colors duration-150 placeholder:text-ink-soft/70 focus:border-brand focus:ring-2 focus:ring-brand/40 focus:outline-none disabled:bg-surface disabled:text-ink-soft aria-[invalid=true]:border-danger;
	}
	.field-label {
		@apply block text-sm font-semibold text-ink;
	}
	.field-hint {
		@apply mt-1 text-xs text-ink-soft;
	}
	.field-error {
		@apply mt-1 text-xs font-medium text-danger;
	}
	.card {
		@apply rounded-card border border-line bg-ground p-4 md:p-5;
	}
	.link {
		@apply font-medium text-ink underline decoration-brand decoration-2 underline-offset-4 hover:decoration-4;
	}
	.chip {
		@apply inline-flex h-10 items-center rounded-full border px-4 text-sm font-medium transition-colors duration-150;
	}
	.chip-idle {
		@apply border-line bg-ground text-ink hover:border-ink;
	}
	.chip-selected {
		@apply border-ink bg-ink text-ground;
	}
	.chip-off {
		@apply border-dashed opacity-50;
	}
	.option-card {
		@apply flex cursor-pointer items-center gap-3 rounded-control border border-line bg-ground px-4 py-3 text-sm text-ink transition-colors duration-150 has-[:checked]:border-brand has-[:checked]:bg-brand-soft has-[:disabled]:cursor-not-allowed has-[:disabled]:opacity-50;
	}
	.option-card input {
		@apply accent-brand;
	}
	.table {
		@apply w-full text-sm;
	}
	.table th {
		@apply bg-surface px-3 py-2 text-left text-xs font-semibold text-ink-soft;
	}
	.table td {
		@apply border-t border-line px-3 py-2 align-top;
	}
	.table tbody tr:nth-child(even) {
		@apply bg-surface/50;
	}
	.table tbody tr:hover {
		@apply bg-brand-soft/40;
	}
}
```

- [ ] **Step 7: `web/src/app.html` 拿掉 body 的 class**

把 `<body data-sveltekit-preload-data="hover" class="bg-gray-50 text-gray-900">` 改成：
```html
		<body data-sveltekit-preload-data="hover">
```
其他一行都不動。

- [ ] **Step 8: 寫 11 個元件**

`web/src/lib/components/ui/Icon.svelte`：
```svelte
<script lang="ts">
	import { ICON_PATHS, type IconName } from './icons';

	let { name, size = 20, class: className = '' }: { name: IconName; size?: number; class?: string } = $props();
</script>

<svg
	xmlns="http://www.w3.org/2000/svg"
	width={size}
	height={size}
	viewBox="0 0 24 24"
	fill="none"
	stroke="currentColor"
	stroke-width="2"
	stroke-linecap="round"
	stroke-linejoin="round"
	aria-hidden="true"
	class="shrink-0 {className}"
>
	<path d={ICON_PATHS[name]} />
</svg>
```

`web/src/lib/components/ui/Logo.svelte`（吊牌記號＋店名，規格 §3.4）：
```svelte
<script lang="ts">
	let { name, size = 'md', inverted = false }: { name: string; size?: 'md' | 'lg'; inverted?: boolean } = $props();

	const mark = $derived(size === 'lg' ? 40 : 28);
</script>

<span class="inline-flex items-center gap-2 {inverted ? 'text-ground' : 'text-ink'}">
	<svg width={mark} height={mark} viewBox="0 0 24 24" aria-hidden="true" class="shrink-0">
		<path
			d="M3 3h8.6a2 2 0 0 1 1.4.6l7.4 7.4a2 2 0 0 1 0 2.8l-6.6 6.6a2 2 0 0 1-2.8 0L3.6 13A2 2 0 0 1 3 11.6V3z"
			class="fill-brand"
		/>
		<circle cx="7.5" cy="7.5" r="1.6" class={inverted ? 'fill-ink' : 'fill-ground'} />
	</svg>
	<span class="font-extrabold tracking-tight {size === 'lg' ? 'text-2xl' : 'text-lg'}">{name}</span>
</span>
```

`web/src/lib/components/ui/Button.svelte`：
```svelte
<script lang="ts">
	import type { Snippet } from 'svelte';
	import type { HTMLAttributes } from 'svelte/elements';

	type Variant = 'primary' | 'secondary' | 'ghost' | 'danger';
	type Size = 'sm' | 'md' | 'lg';

	// rest 用 HTMLAttributes<HTMLElement>（不是 HTMLButtonAttributes）：同一份 rest 會展開到 <a>、<span>、<button> 三種元素，
	// 事件型別才不會互相打架
	let {
		variant = 'primary',
		size = 'md',
		href,
		disabled = false,
		type = 'button',
		class: className = '',
		children,
		...rest
	}: {
		variant?: Variant;
		size?: Size;
		href?: string;
		disabled?: boolean;
		type?: 'button' | 'submit' | 'reset';
		class?: string;
		children: Snippet;
	} & Omit<HTMLAttributes<HTMLElement>, 'class'> = $props();

	const VARIANTS: Record<Variant, string> = {
		primary: 'bg-brand text-ink hover:bg-brand-deep',
		secondary: 'border border-line bg-ground text-ink hover:bg-surface',
		ghost: 'text-ink hover:bg-surface',
		danger: 'bg-danger text-ground hover:bg-danger/90'
	};
	const SIZES: Record<Size, string> = {
		sm: 'h-9 px-3.5 text-sm',
		md: 'h-11 px-5 text-base',
		lg: 'h-12 px-6 text-base'
	};
	const classes = $derived(
		`inline-flex items-center justify-center gap-2 rounded-full font-semibold whitespace-nowrap transition-colors duration-150 active:scale-[.98] disabled:pointer-events-none disabled:opacity-50 aria-disabled:pointer-events-none aria-disabled:opacity-50 ${VARIANTS[variant]} ${SIZES[size]} ${className}`
	);
</script>

{#if href && !disabled}
	<a {href} class={classes} {...rest}>{@render children()}</a>
{:else if href}
	<span class={classes} aria-disabled="true" {...rest}>{@render children()}</span>
{:else}
	<button {type} {disabled} class={classes} {...rest}>{@render children()}</button>
{/if}
```

`web/src/lib/components/ui/Field.svelte`：
```svelte
<script lang="ts">
	import type { Snippet } from 'svelte';
	import type { HTMLInputAttributes } from 'svelte/elements';

	// value 刻意用 any：bind:value 是雙向的，父層可能綁 string、number、null；用 unknown 或 string | number 都會讓其中一個方向型別不合
	let {
		label,
		value = $bindable(),
		error,
		hint,
		class: className = '',
		children,
		...rest
	}: {
		label: string;
		// eslint-disable-next-line @typescript-eslint/no-explicit-any
		value?: any;
		error?: string;
		hint?: string;
		class?: string;
		children?: Snippet;
	} & Omit<HTMLInputAttributes, 'value' | 'class'> = $props();

	const uid = $props.id();
	const errorId = $derived(error ? `${uid}-error` : undefined);
</script>

<!-- label 包住控制項（Playwright getByLabel 靠這個）；hint / error 放 label 外面，label 的可讀名稱才只有標籤字 -->
<div class={className}>
	<label class="block">
		<span class="field-label">{label}</span>
		{#if children}
			{@render children()}
		{:else}
			<input class="input mt-1" bind:value aria-invalid={error ? 'true' : undefined} aria-describedby={errorId} {...rest} />
		{/if}
	</label>
	{#if hint}<p class="field-hint">{hint}</p>{/if}
	{#if error}<p id={errorId} class="field-error">{error}</p>{/if}
</div>
```

`web/src/lib/components/ui/Card.svelte`：
```svelte
<script lang="ts">
	import type { Snippet } from 'svelte';

	let {
		title,
		step,
		class: className = '',
		children,
		actions
	}: { title?: string; step?: number; class?: string; children: Snippet; actions?: Snippet } = $props();
</script>

<section class="card {className}">
	{#if title || actions}
		<div class="mb-3 flex flex-wrap items-center justify-between gap-2">
			{#if title}
				<h2 class="flex items-center gap-2 text-lg font-bold">
					{#if step !== undefined}
						<span class="flex h-7 w-7 items-center justify-center rounded-full bg-brand text-sm font-extrabold text-ink" aria-hidden="true">{step}</span>
					{/if}
					{title}
				</h2>
			{/if}
			{#if actions}<div class="flex flex-wrap items-center gap-2">{@render actions()}</div>{/if}
		</div>
	{/if}
	{@render children()}
</section>
```

`web/src/lib/components/ui/Badge.svelte`：
```svelte
<script lang="ts">
	import type { Snippet } from 'svelte';
	import type { BadgeTone } from './status';

	let { tone = 'neutral', class: className = '', children }: { tone?: BadgeTone; class?: string; children: Snippet } = $props();

	const TONES: Record<BadgeTone, string> = {
		brand: 'bg-brand text-ink',
		neutral: 'bg-surface text-ink-soft',
		success: 'bg-success-soft text-success',
		warning: 'bg-warning-soft text-warning',
		danger: 'bg-danger-soft text-danger'
	};
</script>

<span class="inline-flex items-center rounded-full px-2.5 py-0.5 text-xs font-semibold whitespace-nowrap {TONES[tone]} {className}">{@render children()}</span>
```

`web/src/lib/components/ui/StatusBadge.svelte`：
```svelte
<script lang="ts">
	import { ORDER_STATUS_LABELS } from '$lib/labels';
	import type { OrderStatus } from '$lib/types';
	import Badge from './Badge.svelte';
	import { orderStatusTone } from './status';

	let { status }: { status: OrderStatus } = $props();
</script>

<Badge tone={orderStatusTone(status)}>{ORDER_STATUS_LABELS[status]}</Badge>
```

`web/src/lib/components/ui/Alert.svelte`：
```svelte
<script lang="ts">
	import type { Snippet } from 'svelte';

	type Tone = 'info' | 'success' | 'warning' | 'danger';

	let {
		tone = 'info',
		title,
		class: className = '',
		children
	}: { tone?: Tone; title?: string; class?: string; children: Snippet } = $props();

	const BOX: Record<Tone, string> = {
		info: 'border-ink bg-surface',
		success: 'border-success bg-success-soft',
		warning: 'border-warning bg-warning-soft',
		danger: 'border-danger bg-danger-soft'
	};
	const TITLE: Record<Tone, string> = {
		info: 'text-ink',
		success: 'text-success',
		warning: 'text-warning',
		danger: 'text-danger'
	};
</script>

<div role={tone === 'danger' ? 'alert' : undefined} class="rounded-control border-l-4 px-4 py-3 text-sm text-ink {BOX[tone]} {className}">
	{#if title}<p class="font-semibold {TITLE[tone]}">{title}</p>{/if}
	{@render children()}
</div>
```

`web/src/lib/components/ui/PageHeader.svelte`：
```svelte
<script lang="ts">
	import type { Snippet } from 'svelte';

	let { title, subtitle, children }: { title: string; subtitle?: string; children?: Snippet } = $props();
</script>

<div class="flex flex-wrap items-end justify-between gap-3">
	<div>
		<h1 class="text-2xl font-extrabold tracking-tight md:text-[28px]">{title}</h1>
		{#if subtitle}<p class="mt-1 text-sm text-ink-soft">{subtitle}</p>{/if}
	</div>
	{#if children}<div class="flex flex-wrap items-center gap-2">{@render children()}</div>{/if}
</div>
```

`web/src/lib/components/ui/EmptyState.svelte`：
```svelte
<script lang="ts">
	import type { Snippet } from 'svelte';
	import Icon from './Icon.svelte';

	let { message, children }: { message: string; children?: Snippet } = $props();
</script>

<div class="flex flex-col items-center gap-3 rounded-card bg-surface px-6 py-12 text-center">
	<Icon name="tag" size={32} class="text-ink-soft" />
	<p class="text-ink-soft">{message}</p>
	{#if children}<div class="mt-1">{@render children()}</div>{/if}
</div>
```

`web/src/lib/components/ui/QtyStepper.svelte`：
```svelte
<script lang="ts">
	import Icon from './Icon.svelte';
	import { clampQty } from './qty';

	let {
		value = $bindable(1),
		min = 1,
		max = 99,
		onchange,
		label = '數量'
	}: { value?: number; min?: number; max?: number; onchange?: (v: number) => void; label?: string } = $props();

	function set(next: number) {
		const v = clampQty(next, min, max, value);
		value = v;
		onchange?.(v);
	}
</script>

<div class="inline-flex h-11 shrink-0 items-center overflow-hidden rounded-full border border-line bg-ground">
	<button type="button" class="flex h-full w-10 items-center justify-center text-ink hover:bg-surface" onclick={() => set(value - 1)} aria-label="減少">
		<Icon name="minus" size={16} />
	</button>
	<input
		type="number"
		{min}
		{max}
		{value}
		onchange={(e) => {
			set(Number(e.currentTarget.value));
			e.currentTarget.value = String(value);
		}}
		aria-label={label}
		class="h-full w-12 border-x border-line bg-transparent text-center text-base font-semibold [appearance:textfield] focus:outline-none [&::-webkit-inner-spin-button]:appearance-none [&::-webkit-outer-spin-button]:appearance-none"
	/>
	<button type="button" class="flex h-full w-10 items-center justify-center text-ink hover:bg-surface" onclick={() => set(value + 1)} aria-label="增加">
		<Icon name="plus" size={16} />
	</button>
</div>
```

- [ ] **Step 9: 三個 gate**

```bash
pnpm -C web check
pnpm -C web test
pnpm -C web build
```
Expected: `check` 0 errors 0 warnings（元件沒人用也會被檢查型別）；`test` 46 個全過；`build` 成功。若 `check` 對 `Field.svelte` 的 `any` 抱怨 eslint 註解（專案沒裝 eslint，不會），拿掉那行註解即可；若對 `Button.svelte` 的 `{...rest}` 展開到 `<a>` 報型別錯，把 rest 的型別改成 `Record<string, unknown>`，其他不動。

- [ ] **Step 10: Commit**

```bash
git add web/src/app.css web/src/app.html web/src/lib/components/ui
git commit -m "feat(web): 設計 token 與 11 個 ui 元件（Button/Field/Card/Badge/StatusBadge/Alert/PageHeader/EmptyState/QtyStepper/Logo/Icon）" -m "Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>"
```

---
### Task 2: favicon、外框（頁首頁尾）、首頁、商品卡、示意資料、截圖 → 看圖關卡

**Files:**
- Modify: `web/src/lib/assets/favicon.svg`（整檔覆蓋）
- Modify: `web/src/routes/+layout.svelte`（整檔）
- Modify: `web/src/routes/+page.svelte`（整檔）
- Modify: `web/src/lib/components/ProductCard.svelte`（整檔）
- Create（工作區外、不 commit）：`/Users/wilson08/.claude/jobs/82e5d2c3/tmp/make-images.sh`、`/Users/wilson08/.claude/jobs/82e5d2c3/tmp/seed-demo.mjs`、`/Users/wilson08/.claude/jobs/82e5d2c3/tmp/shot.mjs`

**Interfaces:**
- Consumes: Task 1 的 `Logo`、`Icon`、`Badge`、`EmptyState`；`data.shop.{name,description,contact_email,contact_phone}`（根 layout 的 `+layout.server.ts` 回 `shop`，首頁 `data` 會合併父層 data）；`cart.count`。
- Produces: 截圖腳本 `shot.mjs`（之後每個 task 用）；開發 DB 的 8 個示意商品（slug `demo-ui-1`～`demo-ui-8`）。

- [ ] **Step 1: favicon（整檔覆蓋 `web/src/lib/assets/favicon.svg`）**

```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32"><rect width="32" height="32" rx="8" fill="#ff8a00"/><path d="M8 8h9.2a2 2 0 0 1 1.4.6l6.8 6.8a2 2 0 0 1 0 2.8l-6.2 6.2a2 2 0 0 1-2.8 0L8.6 16.6A2 2 0 0 1 8 15.2V8z" fill="#ffffff"/><circle cx="12.5" cy="12.5" r="1.6" fill="#ff8a00"/></svg>
```

- [ ] **Step 2: 根 layout（整檔覆蓋 `web/src/routes/+layout.svelte`）**

`<script>` 只多了兩個 import 與一個樣式常數 `navLink`；`<svelte:head>` 那段（含註解）一字不改。

```svelte
<script lang="ts">
	import '../app.css';
	import { onMount } from 'svelte';
	import { goto, invalidateAll } from '$app/navigation';
	import { page } from '$app/state';
	import { api } from '$lib/api';
	import favicon from '$lib/assets/favicon.svg';
	import { cart } from '$lib/cart.svelte';
	import Toasts from '$lib/components/Toasts.svelte';
	import Icon from '$lib/components/ui/Icon.svelte';
	import Logo from '$lib/components/ui/Logo.svelte';
	import type { LayoutProps } from './$types';

	let { data, children }: LayoutProps = $props();

	onMount(() => cart.load());

	async function logout() {
		await api('/api/auth/logout', { method: 'POST' });
		await invalidateAll();
		await goto('/');
	}

	const navLink =
		'inline-flex h-10 items-center gap-1.5 rounded-full px-3 text-sm font-medium text-ink transition-colors duration-150 hover:bg-surface';
</script>

<svelte:head>
	<link rel="icon" href={favicon} />
	<!-- 第一層頁若用元件 bind:（如 /checkout）走 SSR settle loop，自己的 <title> 會被這裡蓋掉；
	     那些頁改由 load 回傳 title，見 fix-wave 修正波第 5 項的 <title> 診斷 -->
	<title>{page.data.title ?? data.shop.name}</title>
</svelte:head>

<header class="sticky top-0 z-40 border-b border-line bg-ground/95 backdrop-blur">
	<div class="mx-auto flex h-16 max-w-6xl items-center gap-3 px-4 md:px-6">
		<a href="/" class="rounded-full"><Logo name={data.shop.name} /></a>
		<nav class="ml-auto flex items-center gap-0.5 md:gap-1" aria-label="主選單">
			<a href="/products" class={navLink}>全部商品</a>
			<a href="/cart" class="{navLink} relative">
				<Icon name="cart" />
				<span class="sr-only md:not-sr-only">購物車</span>
				{#if cart.count > 0}
					<span class="absolute top-0 right-0 min-w-5 rounded-full bg-brand px-1.5 text-center text-xs leading-5 font-bold text-ink md:static md:ml-0.5">{cart.count}</span>
				{/if}
			</a>
			{#if data.user}
				{#if data.user.role === 'admin'}
					<a href="/admin" class={navLink}><Icon name="user" /><span class="sr-only md:not-sr-only">後台</span></a>
				{/if}
				{#if data.user.role !== 'admin'}
					<a href="/account" class={navLink}><Icon name="user" /><span class="sr-only md:not-sr-only">會員中心</span></a>
				{/if}
				<button type="button" class={navLink} onclick={logout}>登出</button>
			{:else}
				<a href="/login" class={navLink}>登入</a>
			{/if}
		</nav>
	</div>
</header>

<main class="mx-auto min-h-[60vh] max-w-6xl px-4 py-6 md:px-6 md:py-8">
	{@render children()}
</main>

<footer class="mt-16 bg-ink text-ground">
	<div class="mx-auto flex max-w-6xl flex-col gap-8 px-4 py-10 md:flex-row md:items-start md:justify-between md:px-6">
		<div>
			<Logo name={data.shop.name} inverted />
			<div class="mt-3 space-y-1 text-sm text-ground/80">
				{#if data.shop.contact_email}<p>{data.shop.contact_email}</p>{/if}
				{#if data.shop.contact_phone}<p>{data.shop.contact_phone}</p>{/if}
			</div>
		</div>
		<nav class="flex flex-col gap-2 text-sm text-ground/80 md:items-end" aria-label="頁尾選單">
			<a href="/products" class="hover:text-ground hover:underline">全部商品</a>
			<a href="/cart" class="hover:text-ground hover:underline">購物車</a>
			<a href="/account" class="hover:text-ground hover:underline">會員中心</a>
		</nav>
	</div>
</footer>

<Toasts />
```

- [ ] **Step 3: 首頁（整檔覆蓋 `web/src/routes/+page.svelte`）**

```svelte
<script lang="ts">
	import ProductCard from '$lib/components/ProductCard.svelte';
	import EmptyState from '$lib/components/ui/EmptyState.svelte';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
</script>

<!-- 橘色看板（規格 §5.2）：整站唯一搶眼的地方 -->
<section class="relative overflow-hidden rounded-hero bg-brand px-5 py-8 md:px-10 md:py-12">
	<!-- 散落的小圓點、小方塊、吊牌：表示「很多小東西」（規格 §3.4）；手機只露右半邊 -->
	<svg
		class="pointer-events-none absolute top-0 right-0 h-full w-1/2 text-brand-light opacity-40 md:w-2/5"
		viewBox="0 0 400 300"
		preserveAspectRatio="xMaxYMid slice"
		fill="currentColor"
		aria-hidden="true"
	>
		<circle cx="330" cy="40" r="18" />
		<rect x="240" y="24" width="30" height="30" rx="9" />
		<circle cx="385" cy="130" r="28" />
		<rect x="290" y="150" width="24" height="24" rx="7" />
		<circle cx="215" cy="215" r="14" />
		<path d="M320 200h34l22 22-22 22h-34z" />
		<circle cx="270" cy="95" r="10" />
		<rect x="360" y="230" width="20" height="20" rx="6" />
		<circle cx="190" cy="60" r="8" />
		<rect x="150" y="260" width="16" height="16" rx="5" />
	</svg>
	<div class="relative max-w-2xl">
		<h1 class="text-4xl font-extrabold tracking-tight text-ink md:text-[2.75rem]">{data.shop.name}</h1>
		{#if data.shop.description}<p class="mt-3 max-w-prose text-lg text-ink/80">{data.shop.description}</p>{/if}
		<div class="mt-6 flex flex-wrap gap-2">
			<a href="/products" class="rounded-full bg-ground px-4 py-2 text-sm font-semibold text-ink transition-colors duration-150 hover:bg-brand-soft">全部</a>
			{#each data.categories as c (c.id)}
				<a href={`/products?category=${c.slug}`} class="rounded-full bg-ground px-4 py-2 text-sm font-semibold text-ink transition-colors duration-150 hover:bg-brand-soft">{c.name}</a>
			{/each}
		</div>
	</div>
</section>

<section class="mt-10">
	<div class="flex items-baseline justify-between">
		<h2 class="text-[22px] font-extrabold tracking-tight">最新商品</h2>
		<a href="/products" class="link text-sm">看全部</a>
	</div>
	{#if data.latest.length === 0}
		<div class="mt-4"><EmptyState message="商品準備中，請稍後再來。" /></div>
	{:else}
		<div class="mt-4 grid grid-cols-2 gap-4 md:grid-cols-3 lg:grid-cols-4">
			{#each data.latest as item (item.slug)}
				<ProductCard {item} />
			{/each}
		</div>
	{/if}
</section>
```

- [ ] **Step 4: 商品卡（整檔覆蓋 `web/src/lib/components/ProductCard.svelte`）**

```svelte
<script lang="ts">
	import Badge from '$lib/components/ui/Badge.svelte';
	import Icon from '$lib/components/ui/Icon.svelte';
	import { priceRange } from '$lib/format';
	import type { ProductListItem } from '$lib/types';

	let { item }: { item: ProductListItem } = $props();
</script>

<!-- 沒有邊框、沒有陰影（規格 §5.3）：只有圖、名稱、價格 -->
<a href={`/products/${item.slug}`} class="block rounded-card">
	<div class="relative aspect-square overflow-hidden rounded-control bg-surface">
		{#if item.image_thumb}
			<img src={item.image_thumb} alt={item.name} class="h-full w-full object-cover {item.in_stock ? '' : 'opacity-60'}" loading="lazy" />
		{:else}
			<div class="flex h-full w-full items-center justify-center text-ink-soft"><Icon name="image" size={32} /></div>
		{/if}
		{#if !item.in_stock}<span class="absolute top-2 right-2"><Badge tone="danger">已售完</Badge></span>{/if}
	</div>
	<div class="px-1 pt-3 pb-1">
		<h3 class="line-clamp-2 text-sm leading-snug text-ink">{item.name}</h3>
		<p class="mt-1 text-base font-bold tabular-nums">{priceRange(item.price_min, item.price_max)}</p>
	</div>
</a>
```

- [ ] **Step 5: 三個 gate**

```bash
pnpm -C web check
pnpm -C web test
pnpm -C web build
```
Expected: 0/0、46 全過、build 成功。

- [ ] **Step 6: 產示意圖片（只在本機、只進開發 DB）**

用 Write 工具寫 `/Users/wilson08/.claude/jobs/82e5d2c3/tmp/make-images.sh`：
```sh
#!/bin/sh
# 8 張 800×800 純色 PNG，中間一行字；只給開發 DB 的示意商品用
set -e
OUT=/Users/wilson08/.claude/jobs/82e5d2c3/tmp/demo-img
mkdir -p "$OUT"
i=1
for c in '#FFE9CF' '#F1F5FA' '#E3F6EC' '#FFF4DB' '#EDE9FE' '#FDE8E8' '#E0F2FE' '#F5F5F4'; do
  magick -size 800x800 "xc:$c" -fill '#16274A' -gravity center -pointsize 72 -annotate 0 "DEMO $i" "$OUT/demo-$i.png"
  i=$((i+1))
done
ls -la "$OUT"
```
Run: `sh /Users/wilson08/.claude/jobs/82e5d2c3/tmp/make-images.sh`
Expected: 列出 `demo-1.png`～`demo-8.png`，每張約 10～30 KB。

- [ ] **Step 7: 啟動 DB、api、web（照「開發環境」那段）**

Expected: `curl -s http://localhost:8080/api/health` 有回應；`curl -s -o /dev/null -w '%{http_code}\n' http://localhost:5173/` 印 `200`。

- [ ] **Step 8: 建示意商品**

用 Write 工具寫 `/Users/wilson08/.claude/jobs/82e5d2c3/tmp/seed-demo.mjs`（可重跑：已存在的 slug 會跳過）：
```js
// 只對開發 DB（localhost:8080）建示意商品；永遠不要對正式環境跑
import { readFileSync } from 'node:fs';
import { request } from '/Users/wilson08/IdeaProjects/dog_shop/.claude/worktrees/ui-redesign/web/node_modules/@playwright/test/index.mjs';

const API = 'http://localhost:8080';
const IMG = '/Users/wilson08/.claude/jobs/82e5d2c3/tmp/demo-img';
// api 的 CSRF 檢查：變更請求要帶這兩個 header（同 e2e）
const ctx = await request.newContext({
	baseURL: API,
	extraHTTPHeaders: { 'x-requested-with': 'fetch', origin: 'http://localhost:5173' }
});
async function ok(res, what) {
	if (!res.ok()) throw new Error(`${what}: ${res.status()} ${await res.text()}`);
	return res.json();
}

await ok(await ctx.post('/api/auth/login', { data: { email: 'admin@example.com', password: 'admin12345' } }), 'login');

const existing = await ok(await ctx.get('/api/admin/categories'), 'categories');
async function category(name, sort_order) {
	const found = existing.find((c) => c.name === name);
	if (found) return found.id;
	const created = await ok(await ctx.post('/api/admin/categories', { data: { name, slug: null, sort_order } }), `category ${name}`);
	return created.id;
}
const cat1 = await category('生活雜貨', 0);
const cat2 = await category('文具小物', 1);

const PRODUCTS = [
	{ n: 1, name: '木質收納盒', cat: cat1, price: 480, stock: 12 },
	{ n: 2, name: '帆布托特包', cat: cat1, price: 690, stock: 8, options: ['米白', '深藍'] },
	{ n: 3, name: '陶瓷馬克杯', cat: cat1, price: 320, stock: 20, compare: 380 },
	{ n: 4, name: '棉麻餐墊（2 入）', cat: cat1, price: 260, stock: 15 },
	{ n: 5, name: '玻璃調味罐', cat: cat1, price: 150, stock: 30 },
	{ n: 6, name: '原子筆組（5 色）', cat: cat2, price: 120, stock: 40 },
	{ n: 7, name: '便條紙磚', cat: cat2, price: 90, stock: 0 },
	{ n: 8, name: '金屬書籤', cat: cat2, price: 180, stock: 25 }
];

for (const p of PRODUCTS) {
	const slug = `demo-ui-${p.n}`;
	const probe = await ctx.get(`/api/products/${slug}`);
	if (probe.ok()) {
		console.log('skip（已存在）', slug);
		continue;
	}
	const stored = await ok(
		await ctx.post('/api/admin/uploads', {
			multipart: { file: { name: `demo-${p.n}.png`, mimeType: 'image/png', buffer: readFileSync(`${IMG}/demo-${p.n}.png`) } }
		}),
		`upload ${p.n}`
	);
	const variant = (option1_value, i) => ({
		id: null,
		option1_value,
		option2_value: null,
		sku: `DEMO-UI-${p.n}${option1_value ? `-${i + 1}` : ''}`,
		price: p.price,
		compare_at_price: p.compare ?? null,
		stock: p.stock,
		is_active: true,
		image_path: null
	});
	const body = {
		name: p.name,
		slug,
		description: `示意商品（DEMO-UI）\n${p.name}，用來看新版外觀，不是真的商品。`,
		category_id: p.cat,
		status: 'active',
		option1_name: p.options ? '顏色' : null,
		option2_name: null,
		sort_order: p.n,
		images: [{ path: stored.path, thumb_path: stored.thumb_path, alt: p.name }],
		variants: p.options ? p.options.map(variant) : [variant(null, 0)]
	};
	const created = await ok(await ctx.post('/api/admin/products', { data: body }), `product ${p.name}`);
	console.log('created', created.slug ?? created.id);
}
await ctx.dispose();
```
Run: `node /Users/wilson08/.claude/jobs/82e5d2c3/tmp/seed-demo.mjs`
Expected: 8 行 `created demo-ui-N`（重跑會印 `skip`）。之後 `curl -s http://localhost:8080/api/products/demo-ui-1` 回 JSON、`images[0].thumb_path` 有值。

- [ ] **Step 9: 截圖腳本**

用 Write 工具寫 `/Users/wilson08/.claude/jobs/82e5d2c3/tmp/shot.mjs`：
```js
// 用法：node shot.mjs <url> <輸出前綴> [--login] [--cart <slug>]
//   --login       先用開發 admin 登入（cookie 共用給 5173，SSR 看得到）
//   --cart <slug> 先到商品頁按「加入購物車」，購物車／結帳頁才有東西
// 每個尺寸各存一張整頁截圖，並印出是否有水平捲軸
import { chromium } from '/Users/wilson08/IdeaProjects/dog_shop/.claude/worktrees/ui-redesign/web/node_modules/@playwright/test/index.mjs';

const args = process.argv.slice(2);
const url = args[0];
const out = args[1];
const login = args.includes('--login');
const cartSlug = args.includes('--cart') ? args[args.indexOf('--cart') + 1] : null;
const HEADERS = { 'x-requested-with': 'fetch', origin: 'http://localhost:5173' };

const browser = await chromium.launch();
for (const [name, width, height] of [
	['mobile', 375, 812],
	['desktop', 1280, 800]
]) {
	const context = await browser.newContext({ viewport: { width, height }, locale: 'zh-TW' });
	const page = await context.newPage();
	if (login) {
		const res = await context.request.post('http://localhost:8080/api/auth/login', {
			headers: HEADERS,
			data: { email: 'admin@example.com', password: 'admin12345' }
		});
		if (!res.ok()) throw new Error(`login ${res.status()}`);
	}
	if (cartSlug) {
		await page.goto(`http://localhost:5173/products/${cartSlug}`, { waitUntil: 'networkidle' });
		await page.getByRole('button', { name: '加入購物車' }).click();
		await page.getByText('已加入購物車').waitFor();
	}
	await page.goto(url, { waitUntil: 'networkidle' });
	const overflow = await page.evaluate(() => document.documentElement.scrollWidth > document.documentElement.clientWidth);
	await page.screenshot({ path: `${out}-${name}.png`, fullPage: true });
	console.log(`${name}: ${out}-${name}.png overflow=${overflow}`);
	await context.close();
}
await browser.close();
```

```bash
mkdir -p /Users/wilson08/.claude/jobs/82e5d2c3/tmp/shots
node /Users/wilson08/.claude/jobs/82e5d2c3/tmp/shot.mjs http://localhost:5173/ /Users/wilson08/.claude/jobs/82e5d2c3/tmp/shots/home
node /Users/wilson08/.claude/jobs/82e5d2c3/tmp/shot.mjs http://localhost:5173/products /Users/wilson08/.claude/jobs/82e5d2c3/tmp/shots/products-before
```
Expected: 四張 PNG，四行都 `overflow=false`。（`/products` 這時還是舊版，只是拿來對照。）用 Read 工具自己看一次 `home-mobile.png` 與 `home-desktop.png`：看板是橘色、店名深藍、分類白膠囊、商品格 2 欄／4 欄、頁尾深藍。有問題先修再回報。

- [ ] **Step 10: Commit（腳本與圖片在工作區外，不會被 add）**

```bash
git add web/src/lib/assets/favicon.svg web/src/routes/+layout.svelte web/src/routes/+page.svelte web/src/lib/components/ProductCard.svelte
git commit -m "feat(web): 新外框（吊牌 Logo、頁首頁尾）、首頁橘色看板、商品卡、favicon" -m "Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>"
```

回報：commit、四張截圖的路徑、overflow 結果、伺服器還開著（pid）。**控制者在這裡把 `home-mobile.png`、`home-desktop.png` 給使用者看，使用者說可以才派 Task 3。**

---

### Task 3: 商品列表、分頁

**Files:**
- Modify: `web/src/routes/products/+page.svelte`（整檔）
- Modify: `web/src/lib/components/Pagination.svelte`（整檔）

**Interfaces:**
- Consumes: Task 1 的 `PageHeader`、`Button`、`Icon`、`EmptyState`；Task 2 的 `ProductCard`、`shot.mjs`。
- Produces: 無新介面。

- [ ] **Step 1: 商品列表（整檔覆蓋 `web/src/routes/products/+page.svelte`）**

```svelte
<script lang="ts">
	import Pagination from '$lib/components/Pagination.svelte';
	import ProductCard from '$lib/components/ProductCard.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import EmptyState from '$lib/components/ui/EmptyState.svelte';
	import Icon from '$lib/components/ui/Icon.svelte';
	import PageHeader from '$lib/components/ui/PageHeader.svelte';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	const currentCategory = $derived(data.categories.find((c) => c.slug === data.category));
	const title = $derived(currentCategory ? currentCategory.name : '全部商品');
</script>

<svelte:head><title>{title}</title></svelte:head>

<PageHeader {title} subtitle={`共 ${data.result.total} 件`} />

<!-- 原本的 GET 表單照舊；手機：搜尋一列、分類＋排序一列、按鈕一列；桌機：一列排完 -->
<form method="GET" class="mt-5 grid grid-cols-2 gap-2 md:grid-cols-[1fr_10rem_10rem_auto]">
	<label class="relative col-span-2 block md:col-span-1">
		<span class="sr-only">搜尋商品</span>
		<Icon name="search" class="pointer-events-none absolute top-1/2 left-3 -translate-y-1/2 text-ink-soft" />
		<input name="q" value={data.q} placeholder="搜尋商品" class="input pl-10" />
	</label>
	<select name="category" value={data.category} class="input">
		<option value="">全部分類</option>
		{#each data.categories as c (c.id)}
			<option value={c.slug}>{c.name}</option>
		{/each}
	</select>
	<select name="sort" value={data.sort} class="input">
		<option value="newest">最新</option>
		<option value="price_asc">價格低到高</option>
		<option value="price_desc">價格高到低</option>
	</select>
	<Button type="submit" class="col-span-2 md:col-span-1">搜尋</Button>
</form>

{#if data.result.items.length === 0}
	<div class="mt-6"><EmptyState message="沒有符合的商品" /></div>
{:else}
	<div class="mt-6 grid grid-cols-2 gap-4 md:grid-cols-3 lg:grid-cols-4">
		{#each data.result.items as item (item.slug)}
			<ProductCard {item} />
		{/each}
	</div>
	<Pagination page={data.result.page} perPage={data.result.per_page} total={data.result.total} />
{/if}
```

- [ ] **Step 2: 分頁（整檔覆蓋 `web/src/lib/components/Pagination.svelte`）**

```svelte
<script lang="ts">
	import { page as currentPage } from '$app/state';
	import Button from '$lib/components/ui/Button.svelte';
	import Icon from '$lib/components/ui/Icon.svelte';

	let { page, perPage, total }: { page: number; perPage: number; total: number } = $props();

	const pages = $derived(Math.max(1, Math.ceil(total / perPage)));

	function hrefFor(target: number): string {
		const url = new URL(currentPage.url);
		url.searchParams.set('page', String(target));
		return url.pathname + url.search;
	}
</script>

{#if pages > 1}
	<nav class="mt-8 flex items-center justify-center gap-3 text-sm" aria-label="分頁">
		{#if page > 1}
			<Button variant="secondary" size="sm" href={hrefFor(page - 1)}><Icon name="chevron-left" size={16} />上一頁</Button>
		{/if}
		<span class="text-ink-soft tabular-nums">第 {page} / {pages} 頁</span>
		{#if page < pages}
			<Button variant="secondary" size="sm" href={hrefFor(page + 1)}>下一頁<Icon name="chevron-right" size={16} /></Button>
		{/if}
	</nav>
{/if}
```

- [ ] **Step 3: gate 與截圖**

```bash
pnpm -C web check
pnpm -C web test
pnpm -C web build
node /Users/wilson08/.claude/jobs/82e5d2c3/tmp/shot.mjs "http://localhost:5173/products" /Users/wilson08/.claude/jobs/82e5d2c3/tmp/shots/products
node /Users/wilson08/.claude/jobs/82e5d2c3/tmp/shot.mjs "http://localhost:5173/products?q=zzzz-no-such" /Users/wilson08/.claude/jobs/82e5d2c3/tmp/shots/products-empty
```
Expected: 0/0、46 全過、build 成功、四張圖 `overflow=false`；`products-mobile.png` 裡搜尋框佔整列、下面分類與排序並排、按鈕整列、商品 2 欄；`products-empty-*.png` 有淡灰藍空狀態區塊「沒有符合的商品」。（開發 DB 商品超過 20 件才會出現分頁；沒有也沒關係，Task 9 的 e2e 不測分頁。）

- [ ] **Step 4: Commit**

```bash
git add web/src/routes/products/+page.svelte web/src/lib/components/Pagination.svelte
git commit -m "feat(web): 商品列表與分頁套新樣式" -m "Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>"
```

---

### Task 4: 商品頁（ProductView）

**Files:**
- Modify: `web/src/lib/components/ProductView.svelte`（只換 `</script>` 之後的 markup，`<script>` 只加 import）

**Interfaces:**
- Consumes: Task 1 的 `Button`、`Icon`、`QtyStepper`、`.chip*`；`shot.mjs`。
- Produces: 無新介面。**整頁只有一顆「加入購物車」按鈕**（Playwright strict mode）。

- [ ] **Step 1: `<script>` 加三個 import（放在 `import { cart } …` 前面）**

```ts
	import Button from '$lib/components/ui/Button.svelte';
	import Icon from '$lib/components/ui/Icon.svelte';
	import QtyStepper from '$lib/components/ui/QtyStepper.svelte';
```
其餘 `<script>` 內容（`sel1`、`sel2`、`qty`、`activeImage`、`opt1Values`、`opt2Values`、`selected`、`prices`、`minPrice`、`maxPrice`、`shownImage`、`isAvailable`、`addToCart`）一字不改。

- [ ] **Step 2: `</script>` 之後整段換成**

```svelte
<nav class="flex items-center gap-1 text-sm text-ink-soft" aria-label="麵包屑">
	<a href="/products" class="hover:text-ink hover:underline">全部商品</a>
	{#if p.category}
		<Icon name="chevron-right" size={14} />
		<a href={`/products?category=${p.category.slug}`} class="hover:text-ink hover:underline">{p.category.name}</a>
	{/if}
</nav>

<div class="mt-4 grid gap-8 md:grid-cols-2 md:gap-10">
	<div>
		<div class="aspect-square overflow-hidden rounded-control bg-surface">
			{#if shownImage}
				<img src={shownImage} alt={p.name} class="h-full w-full object-cover" />
			{:else}
				<div class="flex h-full w-full items-center justify-center text-ink-soft"><Icon name="image" size={40} /></div>
			{/if}
		</div>
		{#if p.images.length > 1}
			<div class="mt-3 flex gap-2 overflow-x-auto pb-1">
				{#each p.images as img, i (img.path)}
					<button
						type="button"
						class="h-16 w-16 shrink-0 overflow-hidden rounded-lg border-2 {i === activeImage ? 'border-brand' : 'border-line'}"
						onclick={() => (activeImage = i)}
						aria-label={`第 ${i + 1} 張圖`}
					>
						<img src={img.thumb_path} alt={img.alt} class="h-full w-full object-cover" />
					</button>
				{/each}
			</div>
		{/if}
	</div>

	<div class="flex flex-col">
		<h1 class="text-2xl font-extrabold tracking-tight md:text-[28px]">{p.name}</h1>
		<div class="mt-3 text-[28px] font-bold tabular-nums">
			{#if selected}
				{twd(selected.price)}
				{#if selected.compare_at_price && selected.compare_at_price > selected.price}
					<span class="ml-2 text-base font-normal text-ink-soft line-through">{twd(selected.compare_at_price)}</span>
				{/if}
			{:else}
				{priceRange(minPrice, maxPrice)}
			{/if}
		</div>

		{#if p.option1_name}
			<div class="mt-5">
				<div class="text-sm font-semibold">{p.option1_name}</div>
				<div class="mt-2 flex flex-wrap gap-2">
					{#each opt1Values as value (value)}
						<button
							type="button"
							class="chip {sel1 === value ? 'chip-selected' : 'chip-idle'} {isAvailable(value, null) ? '' : 'chip-off'}"
							onclick={() => {
								sel1 = value;
								sel2 = null;
							}}
						>
							{value}
						</button>
					{/each}
				</div>
			</div>
		{/if}
		{#if p.option2_name && sel1 !== null}
			<div class="mt-4">
				<div class="text-sm font-semibold">{p.option2_name}</div>
				<div class="mt-2 flex flex-wrap gap-2">
					{#each opt2Values as value (value)}
						<button
							type="button"
							class="chip {sel2 === value ? 'chip-selected' : 'chip-idle'} {isAvailable(sel1 ?? '', value) ? '' : 'chip-off'}"
							onclick={() => (sel2 = value)}
						>
							{value}
						</button>
					{/each}
				</div>
			</div>
		{/if}

		<div class="mt-5 text-sm text-ink-soft">
			{#if selected}
				{selected.stock > 0 ? `庫存 ${selected.stock}` : '已售完'}
			{:else if p.option1_name}
				請選擇規格
			{/if}
		</div>

		<!-- 手機：黏在螢幕底部的動作列（規格 §5.5）；md 以上回到資訊欄裡。整頁只有這一顆「加入購物車」 -->
		<div
			class="sticky bottom-0 z-30 -mx-4 mt-4 flex items-center gap-3 border-t border-line bg-ground/95 px-4 py-3 shadow-[0_-8px_24px_rgba(22,39,74,.08)] backdrop-blur md:static md:mx-0 md:border-0 md:bg-transparent md:p-0 md:shadow-none md:backdrop-blur-none"
		>
			<QtyStepper bind:value={qty} max={selected?.stock ?? 99} />
			<Button size="lg" class="flex-1 md:flex-none" onclick={addToCart} disabled={!selected || selected.stock <= 0}>
				<Icon name="cart" />加入購物車
			</Button>
		</div>

		{#if p.description}
			<div class="mt-8">
				<h2 class="text-lg font-semibold">商品說明</h2>
				<p class="mt-2 max-w-[65ch] text-base leading-7 whitespace-pre-line text-ink">{p.description}</p>
			</div>
		{/if}
	</div>
</div>
```

- [ ] **Step 3: gate 與截圖**

```bash
pnpm -C web check
pnpm -C web test
pnpm -C web build
node /Users/wilson08/.claude/jobs/82e5d2c3/tmp/shot.mjs http://localhost:5173/products/demo-ui-2 /Users/wilson08/.claude/jobs/82e5d2c3/tmp/shots/product
node /Users/wilson08/.claude/jobs/82e5d2c3/tmp/shot.mjs http://localhost:5173/products/demo-ui-7 /Users/wilson08/.claude/jobs/82e5d2c3/tmp/shots/product-soldout
```
Expected: 0/0、46、build 成功；`product-mobile.png` 底部有數量膠囊＋橘色「加入購物車」動作列，規格「米白／深藍」是膠囊；`product-soldout-*.png` 顯示「已售完」且按鈕半透明。都 `overflow=false`。

- [ ] **Step 4: Commit**

```bash
git add web/src/lib/components/ProductView.svelte
git commit -m "feat(web): 商品頁套新樣式（膠囊規格、數量 stepper、手機底部動作列）" -m "Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>"
```

---
### Task 5: 購物車

**Files:**
- Modify: `web/src/routes/cart/+page.svelte`（`<script>` 只加 import；markup 整段換）

**Interfaces:**
- Consumes: Task 1 的 `PageHeader`、`Card`、`Alert`、`Button`、`EmptyState`、`QtyStepper`；`cart.setQty`（`qty <= 0` 會移除該列，所以 stepper 要傳 `min={0}`，減到 0 的行為才跟原本一樣）。
- Produces: 無。「前往結帳」永遠只有一個元素：可結帳時是 `<a>`，不可時是 `<span aria-disabled>`。

- [ ] **Step 1: `<script>` 加 import（放在 `import { api } …` 後面）**

```ts
	import Button from '$lib/components/ui/Button.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import Alert from '$lib/components/ui/Alert.svelte';
	import EmptyState from '$lib/components/ui/EmptyState.svelte';
	import PageHeader from '$lib/components/ui/PageHeader.svelte';
	import QtyStepper from '$lib/components/ui/QtyStepper.svelte';
```
其餘（`checked`、`checking`、`checkedOnce`、`validate`、`$effect`、`problems`、`problemOf`、`removeUnavailable`、`canCheckout`）一字不改。

- [ ] **Step 2: `</script>` 之後整段換成**

```svelte
<svelte:head><title>購物車</title></svelte:head>

<PageHeader title="購物車" />

{#if !cart.loaded}
	<p class="mt-4 text-ink-soft">載入中…</p>
{:else if cart.lines.length === 0}
	<div class="mt-4">
		<EmptyState message="購物車是空的。"><Button variant="secondary" href="/products">去逛逛</Button></EmptyState>
	</div>
{:else}
	{#if problems.length > 0}
		<Alert tone="warning" class="mt-4">
			<div class="flex flex-wrap items-center gap-3">
				<span>有 {problems.length} 項商品目前無法購買，請移除後再結帳。</span>
				<Button size="sm" onclick={removeUnavailable}>移除無法購買的商品</Button>
			</div>
		</Alert>
	{/if}
	<div class="mt-4 grid gap-6 lg:grid-cols-[1fr_20rem]">
		<ul class="space-y-3">
			{#each cart.lines as line (line.variant_id)}
				{@const p = problemOf(line.variant_id)}
				<li>
					<Card>
						<div class="flex flex-wrap items-center gap-4">
							<a href={`/products/${line.product_slug}`} class="h-20 w-20 shrink-0 overflow-hidden rounded-control bg-surface">
								{#if line.image_thumb}<img src={line.image_thumb} alt="" class="h-full w-full object-cover" />{/if}
							</a>
							<div class="min-w-0 flex-1">
								<a href={`/products/${line.product_slug}`} class="font-semibold hover:underline">{line.product_name}</a>
								<div class="text-sm text-ink-soft">{line.variant_label}</div>
								<div class="text-sm tabular-nums">{twd(line.price)}</div>
								{#if p?.reason === 'unavailable'}
									<div class="text-sm font-medium text-danger">已下架</div>
								{:else if p?.reason === 'sold_out'}
									<div class="text-sm font-medium text-danger">已售完</div>
								{:else if p?.reason === 'qty_reduced'}
									<div class="text-sm font-medium text-warning">庫存只剩 {p.stock} 件，數量已調整</div>
								{/if}
							</div>
							<QtyStepper value={line.qty} min={0} max={p?.available ? p.stock : 99} onchange={(v) => cart.setQty(line.variant_id, v)} />
							<div class="w-24 text-right font-bold tabular-nums">{twd(line.price * line.qty)}</div>
							<Button variant="ghost" size="sm" onclick={() => cart.remove(line.variant_id)}>移除</Button>
						</div>
					</Card>
				</li>
			{/each}
		</ul>
		<!-- 摘要：桌機黏在右欄上方；手機在清單下面、黏在螢幕底部（規格 §5.6） -->
		<aside class="sticky bottom-0 z-30 h-fit lg:top-20 lg:bottom-auto">
			<Card class="shadow-[0_-8px_24px_rgba(22,39,74,.08)] lg:shadow-none">
				<div class="flex items-baseline justify-between">
					<span class="text-lg">小計</span>
					<span class="text-[22px] font-bold tabular-nums">{twd(cart.subtotal)}</span>
				</div>
				<div class="mt-3">
					<Button href="/checkout" size="lg" class="w-full" disabled={!canCheckout}>前往結帳</Button>
				</div>
				<div class="mt-3 flex justify-center">
					<Button variant="ghost" size="sm" onclick={validate} disabled={checking}>{checking ? '確認中…' : '重新確認庫存'}</Button>
				</div>
				{#if cart.subtotal > CVS_SUBTOTAL_LIMIT}
					<p class="mt-2 text-sm text-warning">商品小計超過 {twd(CVS_SUBTOTAL_LIMIT)}，只能選擇宅配。</p>
				{/if}
				<p class="mt-2 text-xs text-ink-soft">價格與庫存會在結帳時再次確認。</p>
			</Card>
		</aside>
	</div>
{/if}
```

- [ ] **Step 3: gate 與截圖（`--cart` 先加一件）**

```bash
pnpm -C web check
pnpm -C web test
pnpm -C web build
node /Users/wilson08/.claude/jobs/82e5d2c3/tmp/shot.mjs http://localhost:5173/cart /Users/wilson08/.claude/jobs/82e5d2c3/tmp/shots/cart --cart demo-ui-1
node /Users/wilson08/.claude/jobs/82e5d2c3/tmp/shot.mjs http://localhost:5173/cart /Users/wilson08/.claude/jobs/82e5d2c3/tmp/shots/cart-empty
```
Expected: 0/0、46、build 成功；`cart-mobile.png` 有一張商品卡、底部黏著小計＋橘色「前往結帳」；`cart-desktop.png` 右欄有摘要卡；`cart-empty-*.png` 空狀態＋「去逛逛」。都 `overflow=false`。

- [ ] **Step 4: Commit**

```bash
git add web/src/routes/cart/+page.svelte
git commit -m "feat(web): 購物車套新樣式（卡片列、stepper、黏底摘要）" -m "Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>"
```

---

### Task 6: 結帳（含 AddressFields、InvoiceFields）

**Files:**
- Modify: `web/src/lib/components/AddressFields.svelte`（markup 整段換；`<script>` 加 import）
- Modify: `web/src/lib/components/checkout/InvoiceFields.svelte`（markup 整段換；`<script>` 加 import、刪 `const input`）
- Modify: `web/src/routes/checkout/+page.svelte`（markup 整段換；`<script>` 加 import、刪第 196 行的 `const input = 'mt-1 w-full rounded border border-gray-300 px-3 py-2';`）

**Interfaces:**
- Consumes: Task 1 的 `PageHeader`、`Card`（`step`）、`Field`、`Alert`、`Button`、`EmptyState`、`.option-card`。
- Produces: 無。Playwright 契約：label `Email（訂單通知寄到這裡）`、`收件人`、`手機`、`縣市`、`鄉鎮市區`、`郵遞區號`、`地址`；radio `宅配（…）`／`超商取貨（…）`／`7-ELEVEN`／`全家`／`萊爾富`；按鈕 `選擇門市`、`送出訂單`；文字 `總計`。

- [ ] **Step 1: AddressFields**

`<script>` 在 `import type { HomeAddressInput } …` 後加：
```ts
	import Field from '$lib/components/ui/Field.svelte';
```
`</script>` 之後整段換成：
```svelte
<div class="grid grid-cols-3 gap-3">
	<Field label="縣市" error={err('city')}>
		<select bind:value={address.city} onchange={onCity} class="input mt-1">
			<option value="">請選擇</option>
			{#each cityOptions as c (c)}<option value={c}>{c}</option>{/each}
		</select>
	</Field>
	<Field label="鄉鎮市區" error={err('district')}>
		<select bind:value={address.district} onchange={onDistrict} class="input mt-1">
			<option value="">請選擇</option>
			{#each districtOptions as d (d)}<option value={d}>{d}</option>{/each}
		</select>
	</Field>
	<Field label="郵遞區號" type="text" bind:value={address.postal_code} inputmode="numeric" error={err('postal_code')} />
</div>
<Field class="mt-3" label="地址" type="text" bind:value={address.street} autocomplete="street-address" error={err('street')} />
```

- [ ] **Step 2: InvoiceFields**

`<script>`：在 `import type { InvoiceForm } …` 後加 `import Field from '$lib/components/ui/Field.svelte';`；刪掉 `const input = 'mt-1 w-full rounded border border-gray-300 px-3 py-2';`。其餘（`invoice`、`errors`、`types`、`carriers`）不動。
`</script>` 之後整段換成（`legend` 改成 `sr-only`：結帳頁的卡片標題會顯示「發票」，避免出現兩次）：
```svelte
<fieldset class="space-y-3">
	<legend class="sr-only">發票</legend>
	<div class="grid gap-2 sm:grid-cols-3">
		{#each types as t (t)}
			<label class="option-card"><input type="radio" bind:group={invoice.type} value={t} /> {INVOICE_LABELS[t]}</label>
		{/each}
	</div>
	{#if invoice.type === 'personal'}
		<Field label="載具">
			<select bind:value={invoice.carrier_type} class="input mt-1">
				{#each carriers as c (c.value)}<option value={c.value}>{c.label}</option>{/each}
			</select>
		</Field>
		{#if invoice.carrier_type !== '1'}
			<Field
				label={invoice.carrier_type === '2' ? '自然人憑證條碼' : '手機條碼'}
				type="text"
				bind:value={invoice.carrier_num}
				placeholder={invoice.carrier_type === '2' ? 'AB12345678901234' : '/ABC+123'}
				error={errors['invoice.carrier_num']}
			/>
		{/if}
	{:else if invoice.type === 'company'}
		<Field label="統一編號" type="text" bind:value={invoice.tax_id} inputmode="numeric" error={errors['invoice.tax_id']} />
		<Field label="發票抬頭" type="text" bind:value={invoice.title} error={errors['invoice.title']} />
		<Field label="發票地址" type="text" bind:value={invoice.address} error={errors['invoice.address']} />
	{:else}
		<Field label="愛心碼" type="text" bind:value={invoice.love_code} inputmode="numeric" placeholder="例如 168" error={errors['invoice.love_code']} />
	{/if}
</fieldset>
```

- [ ] **Step 3: 結帳頁 `<script>`**

在 `import AddressFields …` 後加：
```ts
	import Alert from '$lib/components/ui/Alert.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import EmptyState from '$lib/components/ui/EmptyState.svelte';
	import Field from '$lib/components/ui/Field.svelte';
	import PageHeader from '$lib/components/ui/PageHeader.svelte';
```
刪掉 `const input = 'mt-1 w-full rounded border border-gray-300 px-3 py-2';`（第 196 行附近）。其餘一字不改。

- [ ] **Step 4: 結帳頁 `</script>` 之後整段換成**

```svelte
<PageHeader title="結帳" />

{#if !cart.loaded}
	<p class="mt-4 text-ink-soft">載入中…</p>
{:else if cart.lines.length === 0}
	<div class="mt-4">
		<EmptyState message="購物車是空的。"><Button variant="secondary" href="/products">去逛逛</Button></EmptyState>
	</div>
{:else}
	<form onsubmit={submit} class="mt-6 grid gap-8 lg:grid-cols-[1fr_20rem]" novalidate>
		<div class="space-y-6">
			<!-- 1 聯絡與收件人 -->
			<Card title="聯絡資料" step={1}>
				<div class="space-y-3">
					{#if !data.user}
						<p class="text-sm text-ink-soft">
							訪客結帳；<a href="/login?redirect=/checkout" class="link">登入</a>可以用常用地址、在會員中心看訂單。
						</p>
					{/if}
					<Field label="Email（訂單通知寄到這裡）" type="email" bind:value={form.email} autocomplete="email" error={errors.email} />
					{#if data.addresses.length > 0}
						<Field label="常用地址">
							<div class="mt-1 flex gap-2">
								<select bind:value={selectedAddressId} class="input">
									<option value="">選一個帶入</option>
									{#each data.addresses as a (a.id)}
										<option value={a.id}>{a.recipient_name}｜{a.city}{a.district}{a.street}</option>
									{/each}
								</select>
								<Button variant="secondary" onclick={useAddress}>帶入</Button>
							</div>
						</Field>
					{/if}
					<div class="grid grid-cols-2 gap-3">
						<Field label="收件人" type="text" bind:value={form.recipient_name} autocomplete="name" error={errors.recipient_name} />
						<Field label="手機" type="tel" bind:value={form.recipient_phone} autocomplete="tel" placeholder="09xxxxxxxx" error={errors.recipient_phone} />
					</div>
				</div>
			</Card>
			<!-- 2 取貨方式 -->
			<Card title="取貨方式" step={2}>
				<div class="space-y-3">
					<div class="grid gap-2 sm:grid-cols-2">
						<label class="option-card"><input type="radio" bind:group={form.shipping_method} value="home" /> 宅配（{twd(shipping.home_fee)}）</label>
						<label class="option-card">
							<input type="radio" bind:group={form.shipping_method} value="cvs" disabled={cvsBlocked} /> 超商取貨（{twd(shipping.cvs_fee)}）
						</label>
					</div>
					{#if shipping.free_threshold > 0}
						<p class="text-sm text-ink-soft">商品小計滿 {twd(shipping.free_threshold)} 免運。</p>
					{/if}
					{#if cvsBlocked}<p class="text-sm text-warning">商品小計超過 {twd(CVS_SUBTOTAL_LIMIT)}，只能宅配。</p>{/if}
					{#if errors.shipping_method}<p class="field-error">{errors.shipping_method}</p>{/if}
					{#if form.shipping_method === 'home'}
						<AddressFields bind:address={form.address} {errors} prefix="address." />
					{:else}
						<div class="grid gap-2 sm:grid-cols-3">
							{#each cvsTypes as t (t)}
								<label class="option-card"><input type="radio" bind:group={form.cvs_sub_type} value={t} onchange={onSubTypeChange} /> {CVS_LABELS[t]}</label>
							{/each}
						</div>
						{#if store}
							<Alert tone="info">
								<p class="font-semibold">{CVS_LABELS[store.sub_type]} {store.store_name}（{store.store_id}）</p>
								<p class="text-ink-soft">{store.store_address}</p>
							</Alert>
						{/if}
						<Button variant="secondary" onclick={pickStore} disabled={pickingStore}>
							{pickingStore ? '前往綠界地圖…' : store ? '重新選擇門市' : '選擇門市'}
						</Button>
						{#if errors.cvs_store}<p class="field-error">{errors.cvs_store}</p>{/if}
						<p class="text-xs text-ink-soft">超商取貨收件人請填 2～5 個中文字的本名，取貨時要核對證件。</p>
					{/if}
				</div>
			</Card>
			<!-- 3 發票 -->
			<Card title="發票" step={3}>
				<InvoiceFields bind:invoice={form.invoice} {errors} />
			</Card>
			<!-- 4 付款方式 -->
			<Card title="付款方式" step={4}>
				<div class="space-y-3">
					<div class="grid gap-2 sm:grid-cols-3">
						{#each enabledPayments as m (m)}
							<label class="option-card"><input type="radio" bind:group={form.payment_method} value={m} /> {PAYMENT_LABELS[m]}</label>
						{/each}
					</div>
					{#if errors.payment_method}<p class="field-error">{errors.payment_method}</p>{/if}
					<Field label="備註（選填，最多 200 字）" error={errors.note}>
						<textarea bind:value={form.note} rows="2" class="input mt-1"></textarea>
					</Field>
				</div>
			</Card>
		</div>
		<!-- 摘要（規格 §5.7）：桌機黏右欄；手機在最下面、不黏 -->
		<aside class="h-fit lg:sticky lg:top-20">
			<Card title="訂單摘要">
				<div class="space-y-3 text-sm">
					{#if checking}<p class="text-ink-soft">確認庫存中…</p>{/if}
					<ul class="divide-y divide-line">
						{#each lines as l (l.variant_id)}
							<li class="flex justify-between gap-2 py-2">
								<span class="min-w-0 truncate">{l.product_name}<span class="text-ink-soft">（{l.variant_label}）× {l.qty}</span></span>
								<span class="shrink-0 tabular-nums">{twd(l.price * l.qty)}</span>
							</li>
						{/each}
					</ul>
					{#if checked && checked.items.some((i) => !i.available)}
						<p class="text-danger">有商品無法購買，請回<a href="/cart" class="link">購物車</a>處理。</p>
					{/if}
					{#each reduced as r, i (i)}
						<p class="text-warning">{r.name} 庫存不足，數量已調整為 {r.qty}</p>
					{/each}
					{#if checkFailed}
						<p class="text-danger">無法確認庫存，請重新確認</p>
						<Button variant="secondary" class="w-full" onclick={validateCart} disabled={checking}>重新確認庫存</Button>
					{/if}
					{#if checked}
						<div class="flex justify-between"><span>商品小計</span><span class="tabular-nums">{twd(subtotal)}</span></div>
						<div class="flex justify-between"><span>運費</span><span class="tabular-nums">{fee === 0 ? '免運' : twd(fee)}</span></div>
						<div class="flex justify-between text-lg font-bold"><span>總計</span><span class="tabular-nums">{twd(total)}</span></div>
					{/if}
					<Button type="submit" size="lg" class="w-full" disabled={submitting || checking || lines.length === 0}>
						{submitting ? '送出中…' : '送出訂單'}
					</Button>
					<p class="text-xs text-ink-soft">送出後會建立訂單並前往綠界付款頁；付款完成會回到訂單頁。</p>
				</div>
			</Card>
		</aside>
	</form>
{/if}
```

- [ ] **Step 5: gate 與截圖**

```bash
pnpm -C web check
pnpm -C web test
pnpm -C web build
node /Users/wilson08/.claude/jobs/82e5d2c3/tmp/shot.mjs http://localhost:5173/checkout /Users/wilson08/.claude/jobs/82e5d2c3/tmp/shots/checkout --cart demo-ui-1
```
Expected: 0/0、46、build 成功；`checkout-desktop.png` 左邊四張卡各有橘色 1～4 圓圈、右邊摘要卡有「送出訂單」；`checkout-mobile.png` 宅配／超商是兩張可點的卡片。`overflow=false`。

- [ ] **Step 6: 用 Playwright 跑一次結帳 e2e 的宅配那條（確認 label／radio 契約沒斷）**

```bash
pnpm -C web test:e2e -- --grep "宅配"
```
Expected: `1 passed`（api 與 web 都要在跑；綠界的 POST 會被測試攔截，不會連出去）。失敗就回去對 §Global Constraints 第 4 條逐一檢查 label 文字。

- [ ] **Step 7: Commit**

```bash
git add web/src/lib/components/AddressFields.svelte web/src/lib/components/checkout/InvoiceFields.svelte web/src/routes/checkout/+page.svelte
git commit -m "feat(web): 結帳頁套新樣式（四步驟卡片、Field、可點的取貨／付款卡）" -m "Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>"
```

---

### Task 7: 訂單頁、會員中心、登入／註冊／忘記／重設、錯誤頁、Toast

**Files:**
- Modify: `web/src/routes/orders/[id]/+page.svelte`（markup 整段換；`<script>` 加 import）
- Modify: `web/src/routes/account/+layout.svelte`、`account/+page.svelte`、`account/orders/+page.svelte`、`account/addresses/+page.svelte`
- Modify: `web/src/routes/login/+page.svelte`、`register/+page.svelte`、`forgot-password/+page.svelte`、`reset/[token]/+page.svelte`
- Modify: `web/src/routes/+error.svelte`、`web/src/lib/components/Toasts.svelte`
- Create（工作區外）：`/Users/wilson08/.claude/jobs/82e5d2c3/tmp/shot-order.mjs`

**Interfaces:**
- Consumes: Task 1 全部元件；Task 6 的 `AddressFields`（常用地址表單用）。
- Produces: 無。Playwright 契約：訂單頁 `h1` 含訂單編號、文字 `待付款`／`已取消`、按鈕 `取消訂單` → `確定取消這筆訂單`。

- [ ] **Step 1: 訂單頁 `<script>`**

在 `import { api, ApiError } …` 後加：
```ts
	import Alert from '$lib/components/ui/Alert.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import PageHeader from '$lib/components/ui/PageHeader.svelte';
	import StatusBadge from '$lib/components/ui/StatusBadge.svelte';
```
其餘（polling、`cancel`、`repay`、狀態）一字不改。

- [ ] **Step 2: 訂單頁 `</script>` 之後整段換成**

```svelte
<svelte:head><title>訂單 {o.order_no}</title></svelte:head>

<PageHeader title={`訂單 ${o.order_no}`} subtitle={`成立時間 ${formatDate(o.created_at)}`}>
	<StatusBadge status={o.status} />
</PageHeader>

{#if o.status === 'pending_payment'}
	<Alert tone="warning" class="mt-5" title={`付款方式：${PAYMENT_LABELS[o.payment?.method ?? 'credit']}`}>
		{#if o.payment && hasPaymentInfo(o)}
			{#if o.payment.atm_vaccount}
				<p class="mt-2">請在期限內轉帳到下面的帳號：</p>
				<p class="mt-1 text-[22px] font-bold tabular-nums">銀行代碼 {o.payment.atm_bank_code}　帳號 {o.payment.atm_vaccount}</p>
			{:else}
				<p class="mt-2">請到超商多媒體機台輸入繳費代碼：</p>
				<p class="mt-1 text-[22px] font-bold tabular-nums">{o.payment.cvs_payment_no}</p>
			{/if}
			<p class="mt-1">
				金額 {twd(o.payment.amount)}{#if o.payment.expire_at}　繳費期限 {formatDate(o.payment.expire_at)}{/if}
			</p>
			{#if paymentExpired(o)}
				<p class="mt-2 font-medium text-danger">繳費期限已過，請重新付款。</p>
			{:else}
				<p class="mt-2">繳費後幾分鐘內會收到付款成功的 Email；重新整理這一頁也會更新。</p>
			{/if}
		{:else if pollTimedOut}
			<p class="mt-1">還沒收到付款結果。請重新整理這一頁；若已付款卻沒更新，請聯絡我們。</p>
		{:else}
			<p class="mt-1">等候綠界付款結果中…（每 3 秒自動更新）</p>
		{/if}
		<div class="mt-3 flex flex-wrap items-center gap-2">
			<label for="repay-method">重新付款：</label>
			<select id="repay-method" bind:value={repayMethod} class="input w-auto">
				{#each enabledPayments as m (m)}
					<option value={m}>{PAYMENT_LABELS[m]}</option>
				{/each}
			</select>
			<Button size="sm" onclick={repay} disabled={repaying}>{repaying ? '前往付款…' : '前往付款'}</Button>
		</div>
	</Alert>
{:else if o.status === 'paid' || o.status === 'shipped' || o.status === 'completed'}
	<Alert tone="success" class="mt-5">
		<p class="font-semibold">已付款{#if o.paid_at}（{formatDate(o.paid_at)}）{/if}，目前狀態：{ORDER_STATUS_LABELS[o.status]}</p>
	</Alert>
{:else if o.status === 'refunded'}
	<Alert tone="info" class="mt-5">這筆訂單已退款。</Alert>
{:else if o.status === 'cancelled'}
	<Alert tone="info" class="mt-5">這筆訂單已取消{#if o.cancelled_at}（{formatDate(o.cancelled_at)}）{/if}。</Alert>
{/if}

{#if data.token && !data.user}
	<p class="mt-4 text-sm text-ink-soft">請把這個網頁的網址存起來，之後用它查看訂單。</p>
{/if}

<Card class="mt-6 p-0 md:p-0">
	<ul class="divide-y divide-line">
		{#each o.items as item, i (i)}
			<li class="flex items-center gap-4 p-4">
				<div class="h-14 w-14 shrink-0 overflow-hidden rounded-lg bg-surface">
					{#if item.image_path}<img src={item.image_path} alt="" class="h-full w-full object-cover" />{/if}
				</div>
				<div class="min-w-0 flex-1">
					<div class="font-semibold">{item.product_name}</div>
					<div class="text-sm text-ink-soft">{item.variant_label} × {item.quantity}</div>
				</div>
				<div class="text-right">
					<div class="font-semibold tabular-nums">{twd(item.line_total)}</div>
					<div class="text-xs text-ink-soft">單價 {twd(item.unit_price)}</div>
				</div>
			</li>
		{/each}
	</ul>
	<div class="space-y-1 border-t border-line p-4 text-sm">
		<div class="flex justify-between"><span>商品小計</span><span class="tabular-nums">{twd(o.subtotal)}</span></div>
		<div class="flex justify-between"><span>運費</span><span class="tabular-nums">{o.shipping_fee === 0 ? '免運' : twd(o.shipping_fee)}</span></div>
		<div class="flex justify-between text-base font-bold"><span>總計</span><span class="tabular-nums">{twd(o.total)}</span></div>
	</div>
</Card>

<div class="mt-6 grid gap-4 md:grid-cols-2">
	<Card title="取貨">
		<div class="text-sm">
			<p>{o.recipient_name}　{o.recipient_phone}</p>
			{#if o.shipment?.method === 'cvs'}
				<p class="mt-1">超商取貨：{o.shipment.cvs_sub_type ? CVS_LABELS[o.shipment.cvs_sub_type] : ''} {o.shipment.cvs_store_name}</p>
				<p class="text-ink-soft">{o.shipment.cvs_store_address}</p>
			{:else if o.shipment}
				<p class="mt-1">宅配：{o.shipment.home_postal_code} {o.shipment.home_city}{o.shipment.home_district}{o.shipment.home_street}</p>
				{#if o.shipment.tracking_no}<p class="text-ink-soft">{o.shipment.carrier} {o.shipment.tracking_no}</p>{/if}
			{/if}
			{#if o.note}<p class="mt-2 text-ink-soft">備註：{o.note}</p>{/if}
		</div>
	</Card>
	<Card title="發票">
		<div class="text-sm">
			<p>{INVOICE_LABELS[o.invoice_type]}</p>
			{#if o.invoice_type === 'company'}
				<p class="text-ink-soft">統編 {o.invoice_tax_id}｜{o.invoice_title}</p>
				<p class="text-ink-soft">{o.invoice_address}</p>
			{:else if o.invoice_type === 'donation'}
				<p class="text-ink-soft">愛心碼 {o.invoice_love_code}</p>
			{:else if o.invoice_carrier_num}
				<p class="text-ink-soft">載具 {o.invoice_carrier_num}</p>
			{:else}
				<p class="text-ink-soft">發票會寄到 {o.email}</p>
			{/if}
			{#if o.invoice?.status === 'issued'}
				<p class="mt-2">
					發票號碼 {o.invoice.invoice_no}　隨機碼 {o.invoice.random_number}{#if o.invoice.invoice_date}　{formatDate(o.invoice.invoice_date)}{/if}
				</p>
			{:else if o.status === 'paid' || o.status === 'shipped' || o.status === 'completed'}
				<p class="mt-2 text-ink-soft">發票開立中，開好會通知您。</p>
			{/if}
		</div>
	</Card>
</div>

{#if o.status === 'pending_payment'}
	<div class="mt-6 flex items-center gap-3">
		{#if confirming}
			<Button variant="danger" onclick={cancel} disabled={cancelling}>{cancelling ? '取消中…' : '確定取消這筆訂單'}</Button>
			<Button variant="secondary" onclick={() => (confirming = false)}>保留</Button>
		{:else}
			<Button variant="ghost" size="sm" onclick={() => (confirming = true)}>取消訂單</Button>
		{/if}
	</div>
{/if}

{#if data.user}
	<p class="mt-8 text-sm"><a href="/account/orders" class="link">回我的訂單</a></p>
{/if}
```
（`Card class="mt-6 p-0 md:p-0"`：商品清單自己有內距，所以把卡片的內距歸零；utilities 層會蓋過 `.card` 的 `p-4 md:p-5`。）

- [ ] **Step 3: 會員中心 layout（整檔覆蓋 `web/src/routes/account/+layout.svelte`）**

```svelte
<script lang="ts">
	import { page } from '$app/state';
	import type { LayoutProps } from './$types';

	let { children }: LayoutProps = $props();

	const links = [
		{ href: '/account', label: '個人資料' },
		{ href: '/account/orders', label: '我的訂單' },
		{ href: '/account/addresses', label: '常用地址' }
	];

	function isActive(href: string): boolean {
		return href === '/account' ? page.url.pathname === '/account' : page.url.pathname.startsWith(href);
	}
</script>

<div class="flex flex-col gap-6 md:flex-row md:gap-8">
	<aside class="md:w-44 md:shrink-0">
		<!-- 手機：水平可捲的膠囊列；md 以上直排（規格 §5.10） -->
		<nav class="-mx-4 flex gap-1 overflow-x-auto px-4 md:mx-0 md:flex-col md:px-0" aria-label="會員中心">
			{#each links as link (link.href)}
				<a
					href={link.href}
					class="shrink-0 rounded-full px-4 py-2 text-sm transition-colors duration-150 {isActive(link.href) ? 'bg-brand-soft font-bold text-ink' : 'font-medium text-ink-soft hover:bg-surface'}"
				>
					{link.label}
				</a>
			{/each}
		</nav>
	</aside>
	<section class="min-w-0 flex-1">
		{@render children()}
	</section>
</div>
```

- [ ] **Step 4: 個人資料（`web/src/routes/account/+page.svelte`）**

`<script>` 在 `import { api, ApiError } …` 後加：
```ts
	import Button from '$lib/components/ui/Button.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import Field from '$lib/components/ui/Field.svelte';
	import PageHeader from '$lib/components/ui/PageHeader.svelte';
```
`</script>` 之後整段換成：
```svelte
<svelte:head><title>個人資料</title></svelte:head>

<PageHeader title="個人資料" subtitle={data.user?.email} />

<form onsubmit={save} class="mt-6 max-w-md space-y-4" novalidate>
	<Card>
		<div class="space-y-4">
			<Field label="姓名" type="text" bind:value={name} error={errors.name} />
			<Field label="手機" type="tel" bind:value={phone} placeholder="09xxxxxxxx" error={errors.phone} />
		</div>
	</Card>
	<Card title="更改密碼（不改就留空）">
		<div class="space-y-4">
			<Field label="目前密碼" type="password" bind:value={currentPassword} autocomplete="current-password" error={errors.current_password} />
			<Field label="新密碼（至少 8 碼）" type="password" bind:value={newPassword} autocomplete="new-password" error={errors.new_password} />
		</div>
	</Card>
	<Button type="submit" disabled={saving}>{saving ? '儲存中…' : '儲存'}</Button>
</form>
```

- [ ] **Step 5: 我的訂單（`web/src/routes/account/orders/+page.svelte`）**

`<script>` 加：
```ts
	import Button from '$lib/components/ui/Button.svelte';
	import EmptyState from '$lib/components/ui/EmptyState.svelte';
	import PageHeader from '$lib/components/ui/PageHeader.svelte';
	import StatusBadge from '$lib/components/ui/StatusBadge.svelte';
```
（`ORDER_STATUS_LABELS` 的 import 若因此沒人用了就一起刪掉。）`</script>` 之後整段換成：
```svelte
<svelte:head><title>我的訂單</title></svelte:head>

<PageHeader title="我的訂單" />

{#if data.result.items.length === 0}
	<div class="mt-4">
		<EmptyState message="還沒有訂單。"><Button variant="secondary" href="/products">去逛逛</Button></EmptyState>
	</div>
{:else}
	<div class="card mt-4 overflow-x-auto p-0 md:p-0">
		<table class="table">
			<thead>
				<tr><th>訂單編號</th><th>日期</th><th>狀態</th><th>件數</th><th class="text-right">金額</th></tr>
			</thead>
			<tbody>
				{#each data.result.items as o (o.id)}
					<tr>
						<td><a href={`/orders/${o.id}`} class="link">{o.order_no}</a></td>
						<td class="text-ink-soft">{formatDate(o.created_at)}</td>
						<td><StatusBadge status={o.status} /></td>
						<td class="tabular-nums">{o.item_count}</td>
						<td class="text-right tabular-nums">{twd(o.total)}</td>
					</tr>
				{/each}
			</tbody>
		</table>
	</div>
	<Pagination page={data.result.page} perPage={data.result.per_page} total={data.result.total} />
{/if}
```

- [ ] **Step 6: 常用地址（`web/src/routes/account/addresses/+page.svelte`）**

`<script>` 加：
```ts
	import Badge from '$lib/components/ui/Badge.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import Field from '$lib/components/ui/Field.svelte';
	import PageHeader from '$lib/components/ui/PageHeader.svelte';
```
`</script>` 之後整段換成：
```svelte
<svelte:head><title>常用地址</title></svelte:head>

<PageHeader title="常用地址">
	{#if editing === null && data.addresses.length < 10}
		<Button onclick={startNew}>新增地址</Button>
	{/if}
</PageHeader>

{#if editing !== null}
	<form onsubmit={save} class="mt-4 max-w-lg" novalidate>
		<Card>
			<div class="space-y-3">
				<div class="grid grid-cols-2 gap-3">
					<Field label="收件人" type="text" bind:value={form.recipient_name} error={errors.recipient_name} />
					<Field label="手機" type="tel" bind:value={form.phone} placeholder="09xxxxxxxx" error={errors.phone} />
				</div>
				<AddressFields bind:address={addressPart} {errors} />
				<label class="flex items-center gap-2 text-sm">
					<input type="checkbox" bind:checked={form.is_default} class="accent-brand" /> 設為預設地址
				</label>
				<div class="flex gap-2">
					<Button type="submit" disabled={saving}>{saving ? '儲存中…' : '儲存'}</Button>
					<Button variant="secondary" onclick={cancel}>取消</Button>
				</div>
			</div>
		</Card>
	</form>
{/if}

{#if data.addresses.length === 0 && editing === null}
	<p class="mt-4 text-ink-soft">還沒有常用地址。</p>
{:else}
	<ul class="mt-4 space-y-3">
		{#each data.addresses as a (a.id)}
			<li>
				<Card>
					<div class="flex flex-wrap items-center gap-3">
						<div class="min-w-0 flex-1 text-sm">
							<div class="flex items-center gap-2 font-semibold">
								{a.recipient_name}
								{#if a.is_default}<Badge tone="brand">預設</Badge>{/if}
							</div>
							<div class="text-ink-soft">{a.phone}</div>
							<div class="text-ink-soft">{a.postal_code} {a.city}{a.district}{a.street}</div>
						</div>
						<Button variant="ghost" size="sm" onclick={() => startEdit(a)}>編輯</Button>
						{#if confirmDeleteId === a.id}
							<Button variant="danger" size="sm" onclick={() => remove(a.id)}>確定刪除？</Button>
						{:else}
							<Button variant="ghost" size="sm" onclick={() => (confirmDeleteId = a.id)}>刪除</Button>
						{/if}
					</div>
				</Card>
			</li>
		{/each}
	</ul>
{/if}
```

- [ ] **Step 7: 登入（`web/src/routes/login/+page.svelte`）**

`<script>` 在 `import { api, ApiError } …` 後加：
```ts
	import Alert from '$lib/components/ui/Alert.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import Field from '$lib/components/ui/Field.svelte';
	import Icon from '$lib/components/ui/Icon.svelte';
```
`</script>` 之後整段換成：
```svelte
<svelte:head><title>登入</title></svelte:head>

<div class="mx-auto mt-4 max-w-sm">
	<Card class="p-6 md:p-8">
		<div class="mb-6 flex flex-col items-center gap-3 text-center">
			<span class="flex h-14 w-14 items-center justify-center rounded-full bg-brand-soft text-brand"><Icon name="tag" size={28} /></span>
			<h1 class="text-2xl font-extrabold tracking-tight">登入</h1>
		</div>
		<form onsubmit={submit} class="space-y-4">
			<Field label="Email" type="email" bind:value={email} required autocomplete="email" />
			<Field label="密碼" type="password" bind:value={password} required autocomplete="current-password" />
			{#if error}<Alert tone="danger">{error}</Alert>{/if}
			<Button type="submit" size="lg" class="w-full" disabled={submitting}>{submitting ? '登入中…' : '登入'}</Button>
		</form>
		<div class="mt-5 flex justify-between text-sm">
			<a href="/register" class="link">還沒有帳號？註冊</a>
			<a href="/forgot-password" class="link">忘記密碼？</a>
		</div>
	</Card>
</div>
```

- [ ] **Step 8: 註冊（`web/src/routes/register/+page.svelte`）**

`<script>` 加同登入頁的五個 import。`</script>` 之後整段換成：
```svelte
<svelte:head><title>註冊</title></svelte:head>

<div class="mx-auto mt-4 max-w-sm">
	<Card class="p-6 md:p-8">
		<div class="mb-6 flex flex-col items-center gap-3 text-center">
			<span class="flex h-14 w-14 items-center justify-center rounded-full bg-brand-soft text-brand"><Icon name="tag" size={28} /></span>
			<h1 class="text-2xl font-extrabold tracking-tight">註冊</h1>
		</div>
		<form onsubmit={submit} class="space-y-4" novalidate>
			<Field label="Email" type="email" bind:value={email} required autocomplete="email" error={errors.email} />
			<Field label="密碼（至少 8 碼）" type="password" bind:value={password} required autocomplete="new-password" error={errors.password} />
			<Field label="姓名" type="text" bind:value={name} required autocomplete="name" error={errors.name} />
			<Field label="手機（選填）" type="tel" bind:value={phone} autocomplete="tel" placeholder="09xxxxxxxx" error={errors.phone} />
			{#if message}<Alert tone="danger">{message}</Alert>{/if}
			<Button type="submit" size="lg" class="w-full" disabled={submitting}>{submitting ? '註冊中…' : '建立帳號'}</Button>
		</form>
		<p class="mt-5 text-center text-sm">已經有帳號？<a href="/login" class="link">登入</a></p>
	</Card>
</div>
```

- [ ] **Step 9: 忘記密碼（`web/src/routes/forgot-password/+page.svelte`）**

`<script>` 加同登入頁的五個 import。`</script>` 之後整段換成：
```svelte
<svelte:head><title>忘記密碼</title></svelte:head>

<div class="mx-auto mt-4 max-w-sm">
	<Card class="p-6 md:p-8">
		<div class="mb-6 flex flex-col items-center gap-3 text-center">
			<span class="flex h-14 w-14 items-center justify-center rounded-full bg-brand-soft text-brand"><Icon name="tag" size={28} /></span>
			<h1 class="text-2xl font-extrabold tracking-tight">忘記密碼</h1>
		</div>
		{#if sent}
			<Alert tone="success">如果這個 Email 有註冊過，我們會寄出重設密碼的連結（1 小時內有效），請到信箱查看。</Alert>
		{:else}
			<form onsubmit={submit} class="space-y-4" novalidate>
				<Field label="註冊時用的 Email" type="email" bind:value={email} required autocomplete="email" />
				{#if error}<Alert tone="danger">{error}</Alert>{/if}
				<Button type="submit" size="lg" class="w-full" disabled={submitting}>{submitting ? '送出中…' : '寄送重設連結'}</Button>
			</form>
		{/if}
		<p class="mt-5 text-center text-sm"><a href="/login" class="link">回登入</a></p>
	</Card>
</div>
```

- [ ] **Step 10: 重設密碼（`web/src/routes/reset/[token]/+page.svelte`）**

`<script>` 加同登入頁的五個 import。`</script>` 之後整段換成：
```svelte
<svelte:head><title>重設密碼</title></svelte:head>

<div class="mx-auto mt-4 max-w-sm">
	<Card class="p-6 md:p-8">
		<div class="mb-6 flex flex-col items-center gap-3 text-center">
			<span class="flex h-14 w-14 items-center justify-center rounded-full bg-brand-soft text-brand"><Icon name="tag" size={28} /></span>
			<h1 class="text-2xl font-extrabold tracking-tight">重設密碼</h1>
		</div>
		<form onsubmit={submit} class="space-y-4" novalidate>
			<Field label="新密碼（至少 8 碼）" type="password" bind:value={password} required autocomplete="new-password" />
			<Field label="再輸入一次" type="password" bind:value={confirm} required autocomplete="new-password" />
			{#if error}<Alert tone="danger">{error}</Alert>{/if}
			<Button type="submit" size="lg" class="w-full" disabled={submitting}>{submitting ? '更新中…' : '更新密碼'}</Button>
		</form>
	</Card>
</div>
```

- [ ] **Step 11: 錯誤頁（整檔覆蓋 `web/src/routes/+error.svelte`）**

```svelte
<script lang="ts">
	import { page } from '$app/state';
	import Button from '$lib/components/ui/Button.svelte';
</script>

<div class="mx-auto max-w-md py-20 text-center">
	<h1 class="text-6xl font-extrabold tracking-tight tabular-nums">{page.status}</h1>
	<p class="mt-3 text-ink-soft">{page.error?.message ?? '發生錯誤'}</p>
	<div class="mt-6"><Button href="/">回首頁</Button></div>
</div>
```

- [ ] **Step 12: Toast（整檔覆蓋 `web/src/lib/components/Toasts.svelte`）**

```svelte
<script lang="ts">
	import { toast } from '$lib/toast.svelte';
</script>

<div class="pointer-events-none fixed inset-x-0 bottom-6 z-50 flex flex-col items-center gap-2 px-4">
	{#each toast.items as item (item.id)}
		<div class="rounded-full bg-ink px-4 py-2 text-sm font-medium text-ground shadow-[0_8px_24px_rgba(22,39,74,.18)]">{item.message}</div>
	{/each}
</div>
```

- [ ] **Step 13: gate**

```bash
pnpm -C web check
pnpm -C web test
pnpm -C web build
```
Expected: 0/0、46、build 成功。

- [ ] **Step 14: 訂單頁截圖腳本（走一次真的結帳，綠界的 POST 攔下來，拿 ClientBackURL）**

用 Write 工具寫 `/Users/wilson08/.claude/jobs/82e5d2c3/tmp/shot-order.mjs`：
```js
// 建一筆待付款訂單（宅配、信用卡）並截訂單頁；綠界 POST 被攔截，不會連出去
import { chromium } from '/Users/wilson08/IdeaProjects/dog_shop/.claude/worktrees/ui-redesign/web/node_modules/@playwright/test/index.mjs';

const OUT = '/Users/wilson08/.claude/jobs/82e5d2c3/tmp/shots/order';
const browser = await chromium.launch();
const context = await browser.newContext({ viewport: { width: 1280, height: 800 }, locale: 'zh-TW' });
const page = await context.newPage();
await page.goto('http://localhost:5173/products/demo-ui-1', { waitUntil: 'networkidle' });
await page.getByRole('button', { name: '加入購物車' }).click();
await page.getByText('已加入購物車').waitFor();
await page.goto('http://localhost:5173/checkout', { waitUntil: 'networkidle' });
await page.getByLabel(/^Email/).fill('shot@test.local');
await page.getByLabel('收件人').fill('王小明');
await page.getByLabel('手機', { exact: true }).fill('0912345678');
await page.getByLabel('縣市').selectOption('臺北市');
await page.getByLabel('鄉鎮市區').selectOption('中正區');
await page.getByLabel('地址').fill('重慶南路一段 122 號');
await page.route('https://payment-stage.ecpay.com.tw/**', (route) =>
	route.fulfill({ status: 200, contentType: 'text/html; charset=utf-8', body: '<!doctype html><title>stub</title><p>stub</p>' })
);
const ecpay = page.waitForRequest((r) => r.url().includes('/Cashier/AioCheckOut/V5') && r.method() === 'POST');
await page.getByRole('button', { name: '送出訂單' }).click();
const backUrl = new URLSearchParams((await ecpay).postData() ?? '').get('ClientBackURL');
if (!backUrl) throw new Error('沒有 ClientBackURL');
for (const [name, width, height] of [
	['mobile', 375, 812],
	['desktop', 1280, 800]
]) {
	await page.setViewportSize({ width, height });
	await page.goto(backUrl, { waitUntil: 'networkidle' });
	const overflow = await page.evaluate(() => document.documentElement.scrollWidth > document.documentElement.clientWidth);
	await page.screenshot({ path: `${OUT}-${name}.png`, fullPage: true });
	console.log(`${name}: ${OUT}-${name}.png overflow=${overflow}`);
}
// 順便確認取消流程的兩顆按鈕文字沒斷（不真的取消）
await page.getByRole('button', { name: '取消訂單' }).click();
await page.getByRole('button', { name: '確定取消這筆訂單' }).waitFor();
console.log('取消流程按鈕 OK');
await browser.close();
```

```bash
node /Users/wilson08/.claude/jobs/82e5d2c3/tmp/shot-order.mjs
node /Users/wilson08/.claude/jobs/82e5d2c3/tmp/shot.mjs http://localhost:5173/login /Users/wilson08/.claude/jobs/82e5d2c3/tmp/shots/login
node /Users/wilson08/.claude/jobs/82e5d2c3/tmp/shot.mjs http://localhost:5173/account/addresses /Users/wilson08/.claude/jobs/82e5d2c3/tmp/shots/addresses --login
node /Users/wilson08/.claude/jobs/82e5d2c3/tmp/shot.mjs http://localhost:5173/no-such-page /Users/wilson08/.claude/jobs/82e5d2c3/tmp/shots/error
```
Expected: `order-*.png` 標題右邊有黃色「待付款」膠囊、黃色付款區塊；`login-*.png` 置中卡片＋橘色吊牌；`addresses-*.png`（admin 帳號也能看會員中心的頁面；若 `/account` 對 admin 導向別處，改截 `/admin` 也可以，Task 8 再截）；`error-*.png` 大數字 404。全部 `overflow=false`。（這次會在開發 DB 留一筆 `shot@test.local` 的待付款訂單，屬於開發雜物，回報時提一下。）

- [ ] **Step 15: Commit**

```bash
git add web/src/routes/orders web/src/routes/account web/src/routes/login web/src/routes/register web/src/routes/forgot-password web/src/routes/reset web/src/routes/+error.svelte web/src/lib/components/Toasts.svelte
git commit -m "feat(web): 訂單頁、會員中心、登入註冊忘記重設、錯誤頁、toast 套新樣式" -m "Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>"
```

---
### Task 8: 後台（版面不動，套元件）

**Files:**
- Modify: `web/src/routes/admin/+layout.svelte`（整檔）
- Modify: `web/src/routes/admin/+page.svelte`（markup 整段換；`<script>` 的 tiles 加 `alert` 旗標）
- Modify: `web/src/routes/admin/orders/+page.svelte`、`admin/orders/[id]/+page.svelte`（後者刪 `btn/primary/secondary/danger` 四個常數）
- Modify: `web/src/routes/admin/products/+page.svelte`、`admin/products/new/+page.svelte`、`admin/products/[id]/+page.svelte`
- Modify: `web/src/routes/admin/categories/+page.svelte`
- Modify: `web/src/routes/admin/import/+page.svelte`（刪 `btn/primary/secondary` 三個常數）
- Modify: `web/src/routes/admin/settings/+page.svelte`（刪 `const input`）
- Modify: `web/src/lib/components/admin/ProductForm.svelte`（427 行；只換 `</script>` 之後的 markup，`<script>` 只加 import）

**Interfaces:**
- Consumes: Task 1 全部元件、`productStatusTone`、`.table`；Task 3 的 `Pagination`。
- Produces: 無。

**風險提醒**：`ProductForm.svelte` 與 `admin/orders/[id]` 是全案最複雜的兩個檔。規則：任何 `bind:value`、`bind:checked`、`bind:group`、`onclick`、`onchange`、`ondragstart/over/drop/end`、`disabled` 條件、`{#if}` 條件都原樣搬過去，只改包在外面的元素與 class。表格裡的欄位（規格列、分類列）用原生 `<input class="input …">`，不包 `Field`（表格欄位沒有自己的 label，用 `Field` 反而多一層）。

- [ ] **Step 1: 後台 layout（整檔覆蓋 `web/src/routes/admin/+layout.svelte`）**

```svelte
<script lang="ts">
	import { page } from '$app/state';
	import type { LayoutProps } from './$types';

	let { children }: LayoutProps = $props();

	const links = [
		{ href: '/admin', label: '儀表板' },
		{ href: '/admin/orders', label: '訂單' },
		{ href: '/admin/products', label: '商品' },
		{ href: '/admin/import', label: '匯入' },
		{ href: '/admin/categories', label: '分類' },
		{ href: '/admin/settings', label: '設定' }
	];

	function isActive(href: string): boolean {
		return href === '/admin' ? page.url.pathname === '/admin' : page.url.pathname.startsWith(href);
	}
</script>

<div class="flex flex-col gap-6 md:flex-row md:gap-8">
	<aside class="md:w-44 md:shrink-0">
		<nav class="-mx-4 flex gap-1 overflow-x-auto px-4 md:mx-0 md:flex-col md:px-0" aria-label="後台">
			{#each links as link (link.href)}
				<a
					href={link.href}
					class="shrink-0 rounded-full px-4 py-2 text-sm transition-colors duration-150 {isActive(link.href) ? 'bg-brand-soft font-bold text-ink' : 'font-medium text-ink-soft hover:bg-surface'}"
				>
					{link.label}
				</a>
			{/each}
		</nav>
	</aside>
	<section class="min-w-0 flex-1">
		{@render children()}
	</section>
</div>
```

- [ ] **Step 2: 儀表板（`web/src/routes/admin/+page.svelte`）**

`<script>`：加 `import Card from '$lib/components/ui/Card.svelte';`、`import PageHeader from '$lib/components/ui/PageHeader.svelte';`、`import StatusBadge from '$lib/components/ui/StatusBadge.svelte';`（`ORDER_STATUS_LABELS` 若沒人用就刪 import）。`tiles` 陣列是本計畫唯一允許的 `<script>` 資料改動：後三個 tile 各加 `alert: true`，前兩個不加：
```ts
	const tiles = $derived([
		{ label: '今日訂單', value: String(d.today_orders), note: `已付款 ${twd(d.today_paid_total)}`, href: '/admin/orders', alert: false },
		{ label: '待出貨', value: String(d.pending_shipment), note: '已付款、還沒出貨', href: '/admin/orders?status=paid', alert: false },
		{ label: '發票開立失敗', value: String(d.invoice_failed), note: '到訂單頁重開', href: '/admin/orders?flag=invoice_failed', alert: true },
		{ label: '需退款', value: String(d.needs_refund), note: '遲到或金額不符的付款', href: '/admin/orders?flag=needs_refund', alert: true },
		{ label: '超商退回', value: String(d.cvs_returned), note: '買家未取件', href: '/admin/orders?flag=cvs_returned', alert: true }
	]);
```
`</script>` 之後整段換成：
```svelte
<svelte:head><title>儀表板</title></svelte:head>

<PageHeader title="儀表板" />

<div class="mt-5 grid grid-cols-2 gap-3 md:grid-cols-5">
	{#each tiles as t (t.label)}
		<a href={t.href} class="card block transition-colors duration-150 hover:border-brand">
			<div class="text-sm text-ink-soft">{t.label}</div>
			<div class="mt-1 text-[28px] font-extrabold tabular-nums {t.alert && t.value !== '0' ? 'text-danger' : ''}">{t.value}</div>
			<div class="text-xs text-ink-soft">{t.note}</div>
		</a>
	{/each}
</div>

{#snippet list(title: string, items: AdminOrderListItem[], href: string)}
	<Card {title}>
		{#snippet actions()}<a {href} class="link text-sm">全部</a>{/snippet}
		{#if items.length === 0}
			<p class="text-sm text-ink-soft">沒有</p>
		{:else}
			<ul class="divide-y divide-line text-sm">
				{#each items as o (o.id)}
					<li class="flex items-center justify-between gap-2 py-2">
						<a href={`/admin/orders/${o.id}`} class="font-semibold hover:underline">{o.order_no}</a>
						<span class="min-w-0 flex-1 truncate text-ink-soft">{o.recipient_name}｜{o.item_count} 件｜{twd(o.total)}</span>
						<StatusBadge status={o.status} />
						<span class="text-xs text-ink-soft">{formatDate(o.created_at)}</span>
					</li>
				{/each}
			</ul>
		{/if}
	</Card>
{/snippet}

<div class="mt-6 grid gap-4 lg:grid-cols-2">
	{@render list('待出貨', d.pending_shipment_items, '/admin/orders?status=paid')}
	{@render list('需退款', d.needs_refund_items, '/admin/orders?flag=needs_refund')}
	{@render list('超商退回', d.cvs_returned_items, '/admin/orders?flag=cvs_returned')}
	{@render list('發票開立失敗', d.invoice_failed_items, '/admin/orders?flag=invoice_failed')}
</div>
```

- [ ] **Step 3: 訂單列表（`web/src/routes/admin/orders/+page.svelte`）**

`<script>` 加 `Button`、`PageHeader`、`StatusBadge` 三個 import（`ORDER_STATUS_LABELS` 仍被 select 用到，留著）。`</script>` 之後整段換成：
```svelte
<svelte:head><title>訂單管理</title></svelte:head>

<PageHeader title="訂單" />

<form method="GET" class="mt-5 grid grid-cols-2 gap-2 md:flex md:flex-wrap">
	<input name="q" value={data.q} placeholder="訂單編號、Email、收件人" class="input col-span-2 md:w-72" />
	<select name="status" value={data.status} class="input md:w-40">
		<option value="">全部狀態</option>
		{#each statuses as s (s)}
			<option value={s}>{ORDER_STATUS_LABELS[s]}</option>
		{/each}
	</select>
	<select name="flag" value={data.flag} class="input md:w-40">
		<option value="">全部</option>
		{#each flags as f (f)}
			<option value={f}>{ADMIN_FLAG_LABELS[f]}</option>
		{/each}
	</select>
	<Button type="submit" variant="secondary" class="col-span-2 md:col-span-1">篩選</Button>
</form>

<div class="card mt-4 overflow-x-auto p-0 md:p-0">
	<table class="table">
		<thead>
			<tr>
				<th>訂單</th>
				<th>時間</th>
				<th>買家</th>
				<th>取貨</th>
				<th>金額</th>
				<th>狀態</th>
				<th>提醒</th>
			</tr>
		</thead>
		<tbody>
			{#each data.result.items as o (o.id)}
				{@const tags = attention(o)}
				<tr class={tags.length > 0 ? 'bg-danger-soft/60' : ''}>
					<td><a href={`/admin/orders/${o.id}`} class="font-semibold hover:underline">{o.order_no}</a></td>
					<td class="text-ink-soft">{formatDate(o.created_at)}</td>
					<td>
						<div>{o.recipient_name}</div>
						<div class="text-xs text-ink-soft">{o.email}</div>
					</td>
					<td>
						<div>{o.shipping_method === 'cvs' ? '超商取貨' : '宅配'}</div>
						{#if o.shipment_status}<div class="text-xs text-ink-soft">{SHIPMENT_STATUS_LABELS[o.shipment_status]}</div>{/if}
					</td>
					<td class="tabular-nums">{twd(o.total)}<div class="text-xs text-ink-soft">{o.item_count} 件</div></td>
					<td>
						<StatusBadge status={o.status} />
						{#if o.invoice_status}<div class="mt-1 text-xs text-ink-soft">發票{INVOICE_STATUS_LABELS[o.invoice_status]}</div>{/if}
					</td>
					<td class="font-medium text-danger">{tags.join('、')}</td>
				</tr>
			{:else}
				<tr><td colspan="7" class="p-6 text-center text-ink-soft">沒有訂單</td></tr>
			{/each}
		</tbody>
	</table>
</div>
<Pagination page={data.result.page} perPage={data.result.per_page} total={data.result.total} />
```

- [ ] **Step 4: 訂單明細（`web/src/routes/admin/orders/[id]/+page.svelte`）**

`<script>`：加 `Alert`、`Button`、`Card`、`Field`、`Icon`、`PageHeader`、`StatusBadge` 的 import；刪掉 `const btn = …`、`const primary = …`、`const secondary = …`、`const danger = …` 四行（第 76～79 行附近；先 `grep -n "const btn\|const primary\|const secondary\|const danger" web/src/routes/admin/orders/\[id\]/+page.svelte` 確認）。其餘（`notes`、`actions`、`confirmActions`、`CONFIRM`、`act`、`printLabel`、`carrier`、`trackingNo`、`busy`、`confirming`、`errors`、`paymentLabel`）一字不改。
`</script>` 之後整段換成：
```svelte
<svelte:head><title>訂單 {o.order_no}</title></svelte:head>

<a href="/admin/orders" class="link inline-flex items-center gap-1 text-sm"><Icon name="chevron-left" size={16} />訂單列表</a>
<div class="mt-2">
	<PageHeader
		title={`訂單 ${o.order_no}`}
		subtitle={`成立 ${formatDate(o.created_at)}${o.paid_at ? `｜付款 ${formatDate(o.paid_at)}` : ''}${o.shipped_at ? `｜出貨 ${formatDate(o.shipped_at)}` : ''}${o.completed_at ? `｜完成 ${formatDate(o.completed_at)}` : ''}${o.cancelled_at ? `｜結束 ${formatDate(o.cancelled_at)}` : ''}`}
	>
		<StatusBadge status={o.status} />
	</PageHeader>
</div>

{#if notes.length > 0}
	<Alert tone="danger" class="mt-4">
		<ul class="list-disc pl-5">
			{#each notes as n, i (i)}<li>{n}</li>{/each}
		</ul>
	</Alert>
{/if}

{#if actions.length > 0}
	<Card title="動作" class="mt-4">
		<div class="flex flex-wrap items-start gap-3">
			{#if actions.includes('ship_cvs')}
				<Button disabled={busy} onclick={() => act('ship-cvs', {}, '物流單已建立，訂單已出貨')}>
					建立物流單（{o.shipment?.cvs_sub_type ? CVS_LABELS[o.shipment.cvs_sub_type] : ''} {o.shipment?.cvs_store_name ?? ''}）
				</Button>
			{/if}
			{#if actions.includes('ship_home')}
				<form
					class="flex flex-wrap items-end gap-2"
					onsubmit={(e) => {
						e.preventDefault();
						void act('ship-home', { carrier, tracking_no: trackingNo }, '已出貨');
					}}
				>
					<Field label="貨運公司" type="text" bind:value={carrier} placeholder="例如 黑貓" error={errors.carrier} />
					<Field label="單號" type="text" bind:value={trackingNo} error={errors.tracking_no} />
					<Button type="submit" disabled={busy}>宅配出貨</Button>
				</form>
			{/if}
			{#if actions.includes('print_label')}
				<Button variant="secondary" disabled={busy} onclick={printLabel}>列印託運單</Button>
			{/if}
			{#if actions.includes('retry_invoice')}
				<Button variant="secondary" disabled={busy} onclick={() => act('retry-invoice', {}, '已重新排入開立')}>重開發票</Button>
			{/if}
			{#each confirmActions as a (a)}
				{#if actions.includes(a)}
					{@const c = CONFIRM[a]}
					{#if confirming === a}
						<span class="flex items-center gap-2">
							<Button variant={c.danger ? 'danger' : 'primary'} disabled={busy} onclick={() => act(c.path, {}, c.done)}>{c.confirm}</Button>
							<Button variant="secondary" onclick={() => (confirming = null)}>返回</Button>
						</span>
					{:else}
						<Button variant="secondary" disabled={busy} onclick={() => (confirming = a)}>{c.label}</Button>
					{/if}
				{/if}
			{/each}
		</div>
		{#if actions.includes('mark_refunded')}
			<p class="mt-2 text-xs text-ink-soft">退款要先在綠界廠商後台操作；這裡只記錄狀態{o.status === 'paid' ? '並把庫存加回去' : ''}。</p>
		{/if}
	</Card>
{/if}

<div class="card mt-4 overflow-x-auto p-0 md:p-0">
	<table class="table">
		<tbody>
			{#each o.items as item, i (i)}
				<tr>
					<td class="w-16">
						{#if item.image_path}<img src={item.image_path} alt="" class="h-12 w-12 rounded-lg object-cover" />{/if}
					</td>
					<td>{item.product_name}<div class="text-xs text-ink-soft">{item.variant_label}</div></td>
					<td class="text-right tabular-nums">{twd(item.unit_price)} × {item.quantity}</td>
					<td class="text-right tabular-nums">{twd(item.line_total)}</td>
				</tr>
			{/each}
		</tbody>
	</table>
	<div class="space-y-1 border-t border-line p-3 text-sm">
		<div class="flex justify-between"><span>商品小計</span><span class="tabular-nums">{twd(o.subtotal)}</span></div>
		<div class="flex justify-between"><span>運費</span><span class="tabular-nums">{o.shipping_fee === 0 ? '免運' : twd(o.shipping_fee)}</span></div>
		<div class="flex justify-between font-bold"><span>總計</span><span class="tabular-nums">{twd(o.total)}</span></div>
	</div>
</div>

<div class="mt-4 grid gap-4 md:grid-cols-2">
	<Card title="取貨">
		<div class="text-sm">
			<p>{o.recipient_name}　{o.recipient_phone}　<span class="text-ink-soft">{o.email}</span></p>
			{#if o.shipment}
				{@const s = o.shipment}
				{#if s.method === 'cvs'}
					<p class="mt-1">超商取貨：{s.cvs_sub_type ? CVS_LABELS[s.cvs_sub_type] : ''} {s.cvs_store_name}（{s.cvs_store_id}）</p>
					<p class="text-ink-soft">{s.cvs_store_address}</p>
				{:else}
					<p class="mt-1">宅配：{s.home_postal_code} {s.home_city}{s.home_district}{s.home_street}</p>
					{#if s.tracking_no}<p class="text-ink-soft">{s.carrier} {s.tracking_no}</p>{/if}
				{/if}
				<p class="mt-2">出貨狀態：<span class="font-semibold">{SHIPMENT_STATUS_LABELS[s.status]}</span></p>
				{#if s.ecpay_logistics_id}
					<p class="text-ink-soft">綠界物流單 {s.ecpay_logistics_id}｜廠商單號 {s.ecpay_merchant_trade_no}{#if s.cvs_payment_no}｜寄貨編號 {s.cvs_payment_no}{/if}{#if s.cvs_validation_no}｜驗證碼 {s.cvs_validation_no}{/if}</p>
				{/if}
				{#if s.last_status_code}
					<p class="text-ink-soft">最近通知：{s.last_status_code} {s.last_status_msg ?? ''}（{formatDate(s.updated_at)}）</p>
				{/if}
			{/if}
			{#if o.note}<p class="mt-2 text-ink-soft">買家備註：{o.note}</p>{/if}
		</div>
	</Card>
	<Card title="發票">
		<div class="text-sm">
			<p>{INVOICE_LABELS[o.invoice_type]}</p>
			{#if o.invoice_type === 'company'}
				<p class="text-ink-soft">統編 {o.invoice_tax_id}｜{o.invoice_title}｜{o.invoice_address}</p>
			{:else if o.invoice_type === 'donation'}
				<p class="text-ink-soft">愛心碼 {o.invoice_love_code}</p>
			{:else if o.invoice_carrier_num}
				<p class="text-ink-soft">載具 {o.invoice_carrier_num}</p>
			{/if}
			{#if o.invoice}
				<p class="mt-2">狀態：<span class="font-semibold">{INVOICE_STATUS_LABELS[o.invoice.status]}</span></p>
				{#if o.invoice.invoice_no}
					<p class="text-ink-soft">發票號碼 {o.invoice.invoice_no}　隨機碼 {o.invoice.random_number}{#if o.invoice.invoice_date}　{formatDate(o.invoice.invoice_date)}{/if}</p>
				{/if}
				{#if o.invoice.error}<p class="font-medium text-danger">{o.invoice.error}</p>{/if}
			{/if}
		</div>
	</Card>
</div>

<Card title="付款嘗試" class="mt-4">
	<div class="-mx-4 overflow-x-auto md:-mx-5">
		<table class="table">
			<thead>
				<tr>
					<th>綠界交易編號</th>
					<th>方式</th>
					<th>狀態</th>
					<th>金額</th>
					<th>綠界單號</th>
					<th>繳費資訊</th>
					<th>時間</th>
				</tr>
			</thead>
			<tbody>
				{#each o.payments as p (p.id)}
					<tr>
						<td class="tabular-nums">{p.merchant_trade_no}</td>
						<td>{paymentLabel(p.method)}</td>
						<td>{PAYMENT_STATUS_LABELS[p.status]}</td>
						<td class="tabular-nums">{twd(p.amount)}</td>
						<td class="tabular-nums">{p.ecpay_trade_no ?? '—'}</td>
						<td class="text-ink-soft">
							{#if p.atm_vaccount}銀行 {p.atm_bank_code} 帳號 {p.atm_vaccount}{:else if p.cvs_payment_no}代碼 {p.cvs_payment_no}{:else}—{/if}
							{#if p.expire_at}<div class="text-xs">期限 {formatDate(p.expire_at)}</div>{/if}
						</td>
						<td class="text-ink-soft">{formatDate(p.payment_date ?? p.created_at)}</td>
					</tr>
				{:else}
					<tr><td colspan="7" class="p-4 text-center text-ink-soft">沒有付款紀錄</td></tr>
				{/each}
			</tbody>
		</table>
	</div>
</Card>
```

- [ ] **Step 5: 商品列表（`web/src/routes/admin/products/+page.svelte`）**

`<script>` 加 `Badge`、`Button`、`PageHeader` 的 import 與 `import { productStatusTone } from '$lib/components/ui/status';`。`</script>` 之後整段換成：
```svelte
<svelte:head><title>商品管理</title></svelte:head>

<PageHeader title="商品">
	<Button href="/admin/products/new">新增商品</Button>
</PageHeader>

<form method="GET" class="mt-5 grid grid-cols-2 gap-2 md:flex md:flex-wrap">
	<input name="q" value={data.q} placeholder="搜尋名稱" class="input col-span-2 md:w-72" />
	<select name="status" value={data.status} class="input md:w-48">
		<option value="">全部（不含已下架）</option>
		<option value="draft">草稿</option>
		<option value="active">上架</option>
		<option value="archived">已下架</option>
	</select>
	<Button type="submit" variant="secondary">篩選</Button>
</form>

<div class="card mt-4 overflow-x-auto p-0 md:p-0">
	<table class="table">
		<thead>
			<tr>
				<th>圖</th>
				<th>名稱</th>
				<th>狀態</th>
				<th>價格</th>
				<th>庫存</th>
				<th>更新</th>
			</tr>
		</thead>
		<tbody>
			{#each data.result.items as item (item.id)}
				<tr>
					<td class="w-16">
						{#if item.image_thumb}<img src={item.image_thumb} alt="" class="h-12 w-12 rounded-lg object-cover" />{/if}
					</td>
					<td>
						<a href={`/admin/products/${item.id}`} class="font-semibold hover:underline">{item.name}</a>
						<div class="text-xs text-ink-soft">{item.category_name ?? '未分類'} · /products/{item.slug}</div>
					</td>
					<td><Badge tone={productStatusTone(item.status)}>{statusLabel[item.status] ?? item.status}</Badge></td>
					<td class="tabular-nums">
						{item.price_min === null || item.price_max === null ? '—' : priceRange(item.price_min, item.price_max)}
					</td>
					<td class="tabular-nums">{item.stock_total}</td>
					<td class="text-ink-soft">{formatDate(item.updated_at)}</td>
				</tr>
			{:else}
				<tr><td colspan="6" class="p-6 text-center text-ink-soft">沒有商品</td></tr>
			{/each}
		</tbody>
	</table>
</div>
<Pagination page={data.result.page} perPage={data.result.per_page} total={data.result.total} />
```

- [ ] **Step 6: 新增／編輯商品頁**

`web/src/routes/admin/products/new/+page.svelte` 整檔：
```svelte
<script lang="ts">
	import ProductForm from '$lib/components/admin/ProductForm.svelte';
	import PageHeader from '$lib/components/ui/PageHeader.svelte';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
</script>

<svelte:head><title>新增商品</title></svelte:head>

<PageHeader title="新增商品" />
<div class="mt-5">
	<ProductForm categories={data.categories} />
</div>
```

`web/src/routes/admin/products/[id]/+page.svelte` 整檔：
```svelte
<script lang="ts">
	import ProductForm from '$lib/components/admin/ProductForm.svelte';
	import PageHeader from '$lib/components/ui/PageHeader.svelte';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
</script>

<svelte:head><title>編輯商品</title></svelte:head>

<PageHeader title="編輯商品" />
<p class="mt-2 text-sm text-ink-soft">
	前台網址：<a href={`/products/${data.product.slug}`} class="link" target="_blank" rel="noreferrer">/products/{data.product.slug}</a>（要上架才看得到）
</p>
<div class="mt-5">
	<!-- 存檔後 updated_at 會變，用 key 讓表單重新讀取伺服器回來的資料（新規格才會有 id） -->
	{#key data.product.updated_at}
		<ProductForm product={data.product} categories={data.categories} />
	{/key}
</div>
```

- [ ] **Step 7: 分類（`web/src/routes/admin/categories/+page.svelte`）**

`<script>` 加 `Button`、`Card`、`Field`、`PageHeader` 的 import。`</script>` 之後整段換成：
```svelte
<svelte:head><title>分類管理</title></svelte:head>

<PageHeader title="分類" />

<form onsubmit={create} class="mt-5">
	<Card>
		<div class="flex flex-wrap items-end gap-2">
			<Field label="名稱" bind:value={newName} required class="w-48" />
			<Field label="網址代稱（可空白，會自動產生）" bind:value={newSlug} placeholder="例如 food" class="w-64" />
			<Button type="submit">新增</Button>
		</div>
	</Card>
</form>

<div class="card mt-6 overflow-x-auto p-0 md:p-0">
	<table class="table">
		<thead>
			<tr>
				<th>排序</th>
				<th>名稱</th>
				<th>網址代稱</th>
				<th></th>
			</tr>
		</thead>
		<tbody>
			{#each rows as row (row.id)}
				<tr>
					<td><input type="number" bind:value={row.sort_order} class="input w-20 py-1.5" /></td>
					<td><input bind:value={row.name} class="input py-1.5" /></td>
					<td><input bind:value={row.slug} class="input py-1.5" /></td>
					<td class="whitespace-nowrap">
						<Button variant="secondary" size="sm" onclick={() => save(row)}>儲存</Button>
						<Button variant={confirmDeleteId === row.id ? 'danger' : 'secondary'} size="sm" class="ml-2" onclick={() => remove(row.id)}>
							{confirmDeleteId === row.id ? '確定刪除？' : '刪除'}
						</Button>
					</td>
				</tr>
			{:else}
				<tr><td colspan="4" class="p-4 text-center text-ink-soft">還沒有分類</td></tr>
			{/each}
		</tbody>
	</table>
</div>
```

- [ ] **Step 8: 匯入（`web/src/routes/admin/import/+page.svelte`）**

`<script>`：加 `Alert`、`Button`、`Card`、`PageHeader` 的 import；刪掉 `const btn = …`、`const primary = …`、`const secondary = …`（第 74～76 行附近，先 grep 確認）。`</script>` 之後整段換成：
```svelte
<svelte:head><title>匯入商品</title></svelte:head>

<PageHeader title="匯入商品" />
<p class="mt-2 max-w-[65ch] text-sm text-ink-soft">
	上傳 xlsx（依範本或蝦皮匯出檔）。第一列是標題：商品編號、商品名稱、商品描述、分類、規格名稱1、規格選項1、規格名稱2、規格選項2、價格、庫存、SKU、圖片網址（逗號分隔，最多 9 個）。同一個商品編號的多列會合併成多規格；已存在的商品編號會更新既有商品。匯入的新商品是「草稿」，檢查後再上架。<strong class="text-ink">工作表沒有列到的規格會被刪除（有訂單引用的會改成停用），所以要改價也必須把該商品全部的規格都列出來，不能只列要改的那幾個。</strong>
</p>
<div class="mt-4 flex flex-wrap items-center gap-3">
	<input
		type="file"
		accept=".xlsx,application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
		onchange={onFile}
		disabled={busy !== null}
		class="text-sm file:mr-3 file:rounded-full file:border file:border-line file:bg-ground file:px-4 file:py-2 file:text-sm file:font-semibold file:text-ink"
	/>
	<Button variant="secondary" onclick={doPreview} disabled={!file || busy !== null}>
		{busy === 'preview' ? '解析中…' : '預覽'}
	</Button>
</div>
{#if error}
	<Alert tone="danger" class="mt-3">{error}</Alert>
{/if}

{#if preview && summary}
	<Card class="mt-6">
		<p class="font-semibold">{summary.line}</p>
		<p class="text-sm text-ink-soft">工作表「{preview.parsed.sheet}」，標題在第 {preview.parsed.header_row} 列，共 {preview.parsed.row_count} 列資料。</p>
		{#if preview.parsed.unmatched_columns.length > 0}
			<p class="mt-2 text-sm text-warning">對不上的欄位（會被忽略）：{preview.parsed.unmatched_columns.join('、')}</p>
		{/if}
		{#if preview.parsed.errors.length > 0}
			<h3 class="mt-4 font-semibold text-danger">{preview.parsed.errors.length} 個錯誤，修正後重新上傳才能匯入</h3>
			<div class="mt-2 overflow-x-auto">
				<table class="table">
					<thead>
						<tr>
							<th>列</th>
							<th>欄位</th>
							<th>問題</th>
						</tr>
					</thead>
					<tbody>
						{#each preview.parsed.errors as e (e.row + (e.column ?? '') + e.message)}
							<tr class="text-danger">
								<td class="tabular-nums">{e.row}</td>
								<td>{e.column ?? '—'}</td>
								<td>{e.message}</td>
							</tr>
						{/each}
					</tbody>
				</table>
			</div>
		{/if}
		<h3 class="mt-4 font-semibold">商品（前 50 筆）</h3>
		<div class="mt-2 overflow-x-auto">
			<table class="table">
				<thead>
					<tr>
						<th>商品編號</th>
						<th>名稱</th>
						<th>分類</th>
						<th class="text-right">規格</th>
						<th class="text-right">圖片</th>
					</tr>
				</thead>
				<tbody>
					{#each shown as p (p.external_ref)}
						<tr>
							<td>{p.external_ref}</td>
							<td>{p.name}</td>
							<td>{p.category ?? '—'}</td>
							<td class="text-right tabular-nums">{p.variants.length}</td>
							<td class="text-right tabular-nums">{p.image_urls.length}</td>
						</tr>
					{:else}
						<tr><td colspan="5" class="p-6 text-center text-ink-soft">沒有商品</td></tr>
					{/each}
				</tbody>
			</table>
		</div>
		<div class="mt-4">
			{#if confirming}
				<span class="flex flex-wrap items-center gap-2">
					<Button onclick={doCommit} disabled={busy !== null}>
						{busy === 'commit' ? '匯入中，請不要關閉頁面…' : `再按一次確認匯入 ${preview.product_count} 個商品`}
					</Button>
					<Button variant="secondary" onclick={() => (confirming = false)} disabled={busy !== null}>返回</Button>
				</span>
			{:else}
				<Button onclick={() => (confirming = true)} disabled={!canCommit(preview) || busy !== null}>確認匯入</Button>
			{/if}
		</div>
	</Card>
{/if}

{#if result}
	<Card class="mt-6" title={`匯入完成：新增 ${result.result.created}、更新 ${result.result.updated}`}>
		{#if result.result.warnings.length > 0}
			<h3 class="text-warning">{result.result.warnings.length} 個警告（商品已匯入，只是圖片沒抓到）</h3>
			<ul class="mt-1 list-disc pl-5 text-sm">
				{#each result.result.warnings as w (w.external_ref + w.row + w.message)}
					<li>{w.external_ref}（第 {w.row} 列）：{w.message}</li>
				{/each}
			</ul>
		{/if}
		<ul class="mt-3 text-sm">
			{#each result.result.products as p (p.id)}
				<li><a class="link" href={`/admin/products/${p.id}`}>{p.name}</a>（{p.created ? '新增' : '更新'}，{p.images} 張圖）</li>
			{/each}
		</ul>
	</Card>
{/if}
```

- [ ] **Step 9: 設定（`web/src/routes/admin/settings/+page.svelte`）**

`<script>`：加 `Button`、`Card`、`Field`、`PageHeader` 的 import；刪掉 `const input = 'mt-1 w-full rounded border border-gray-300 px-3 py-2';`（第 18 行）。`</script>` 之後整段換成：
```svelte
<svelte:head><title>商店設定</title></svelte:head>

<PageHeader title="商店設定" />

<form onsubmit={save} class="mt-6 max-w-2xl space-y-6" novalidate>
	<Card title="商店資訊">
		<div class="space-y-3">
			<Field label="商店名稱" type="text" bind:value={form.shop.name} error={errors['shop.name']} />
			<Field label="簡介（選填）" error={errors['shop.description']}>
				<textarea bind:value={form.shop.description} rows="3" class="input mt-1"></textarea>
			</Field>
			<div class="grid grid-cols-2 gap-3">
				<Field label="聯絡 Email" type="email" bind:value={form.shop.contact_email} error={errors['shop.contact_email']} />
				<Field label="聯絡電話" type="text" bind:value={form.shop.contact_phone} error={errors['shop.contact_phone']} />
			</div>
		</div>
	</Card>
	<Card title="運費">
		<div class="grid grid-cols-3 gap-3">
			<Field label="超商取貨（元）" type="number" min="0" bind:value={form.shipping.cvs_fee} error={errors['shipping.cvs_fee']} />
			<Field label="宅配（元）" type="number" min="0" bind:value={form.shipping.home_fee} error={errors['shipping.home_fee']} />
			<Field label="免運門檻（元，0 = 不免運）" type="number" min="0" bind:value={form.shipping.free_threshold} error={errors['shipping.free_threshold']} />
		</div>
		<p class="mt-3 text-xs text-ink-soft">免運以商品小計比較；超商取貨商品小計上限 20,000 元（綠界規定）。</p>
	</Card>
	<Card title="付款方式">
		<div class="flex flex-wrap gap-4 text-sm">
			<label class="flex items-center gap-2"><input type="checkbox" bind:checked={form.payment_methods.credit} class="accent-brand" /> 信用卡</label>
			<label class="flex items-center gap-2"><input type="checkbox" bind:checked={form.payment_methods.atm} class="accent-brand" /> ATM 轉帳</label>
			<label class="flex items-center gap-2"><input type="checkbox" bind:checked={form.payment_methods.cvs_code} class="accent-brand" /> 超商代碼繳費</label>
		</div>
		{#if errors.payment_methods}<p class="field-error">{errors.payment_methods}</p>{/if}
	</Card>
	<Card title="寄件人（超商物流單用）">
		<div class="grid grid-cols-2 gap-3">
			<Field label="姓名" type="text" bind:value={form.sender.name} error={errors['sender.name']} />
			<Field label="手機" type="tel" bind:value={form.sender.phone} placeholder="09xxxxxxxx" error={errors['sender.phone']} />
		</div>
	</Card>
	<Card title="退貨門市（買家未取件時退回這裡）">
		<div class="grid grid-cols-3 gap-3">
			<Field label="超商" error={errors['return_store.sub_type']}>
				<select bind:value={form.return_store.sub_type} class="input mt-1">
					<option value="">未設定</option>
					{#each cvsTypes as t (t)}<option value={t}>{CVS_LABELS[t]}</option>{/each}
				</select>
			</Field>
			<Field label="門市代號" type="text" bind:value={form.return_store.store_id} error={errors['return_store.store_id']} />
			<Field label="門市名稱" type="text" bind:value={form.return_store.store_name} error={errors['return_store.store_name']} />
		</div>
	</Card>
	<Button type="submit" disabled={saving}>{saving ? '儲存中…' : '儲存設定'}</Button>
</form>
```

- [ ] **Step 10: 商品表單（`web/src/lib/components/admin/ProductForm.svelte`）**

`<script>` 在 `import { toast } …` 後加：
```ts
	import Button from '$lib/components/ui/Button.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import Field from '$lib/components/ui/Field.svelte';
```
其餘 236 行 `<script>` 一字不改。`</script>` 之後（第 238 行起）整段換成：
```svelte
<form onsubmit={save} class="space-y-6">
	<Card title="基本資料">
		<div class="grid gap-4 md:grid-cols-2">
			<Field class="md:col-span-2" label="商品名稱" bind:value={name} required error={errors.name} />
			<Field label="網址代稱（空白會自動產生）" bind:value={slug} placeholder="例如 chicken-food" error={errors.slug} />
			<Field label="分類" error={errors.category_id}>
				<select bind:value={category_id} class="input mt-1">
					<option value="">未分類</option>
					{#each categories as c (c.id)}
						<option value={c.id}>{c.name}</option>
					{/each}
				</select>
			</Field>
			<Field label="狀態">
				<select bind:value={status} class="input mt-1">
					<option value="draft">草稿（買家看不到）</option>
					<option value="active">上架</option>
					<option value="archived">已下架</option>
				</select>
			</Field>
			<Field label="排序（數字小的在前）" type="number" bind:value={sort_order} />
			<Field class="md:col-span-2" label="商品描述（純文字，會保留換行）">
				<textarea bind:value={description} rows="6" class="input mt-1"></textarea>
			</Field>
		</div>
	</Card>

	<Card title="圖片（最多 9 張；第一張是主圖；拖曳可以換順序）">
		{#if errors.images}<p class="field-error mb-2">{errors.images}</p>{/if}
		<ul class="flex flex-wrap gap-3">
			{#each images as img, i (img.path)}
				<li
					class="w-32 rounded-control border border-line p-1 {dragIndex === i ? 'opacity-50' : ''}"
					draggable="true"
					ondragstart={() => (dragIndex = i)}
					ondragover={(e) => e.preventDefault()}
					ondrop={(e) => {
						e.preventDefault();
						if (dragIndex !== null) moveImage(dragIndex, i);
						dragIndex = null;
					}}
					ondragend={() => (dragIndex = null)}
				>
					<img src={img.thumb_path} alt={img.alt} class="aspect-square w-full rounded-lg object-cover" />
					<input bind:value={img.alt} placeholder="圖片說明" class="input mt-1 px-2 py-1 text-xs" />
					<div class="mt-1 flex justify-between text-xs">
						<span class="text-ink-soft">{i === 0 ? '主圖' : `第 ${i + 1} 張`}</span>
						<button type="button" class="font-medium text-danger" onclick={() => removeImage(i)}>移除</button>
					</div>
				</li>
			{/each}
			{#if images.length < 9}
				<li>
					<label class="flex aspect-square w-32 cursor-pointer items-center justify-center rounded-control border-2 border-dashed border-line text-sm text-ink-soft hover:border-brand hover:bg-brand-soft/40">
						{uploading ? '上傳中…' : '＋ 加圖片'}
						<input
							type="file"
							accept="image/jpeg,image/png,image/webp,image/gif"
							multiple
							onchange={onFiles}
							disabled={uploading}
							class="hidden"
						/>
					</label>
				</li>
			{/if}
		</ul>
	</Card>

	<Card title="規格">
		<p class="text-sm text-ink-soft">
			沒有規格就留空，只會有一列預設規格。有規格就填名稱（例如「口味」「尺寸」），用逗號列出選項，按「依選項產生規格」。
		</p>
		{#if errors.variants}<p class="field-error">{errors.variants}</p>{/if}
		{#if errors.option2_name}<p class="field-error">{errors.option2_name}</p>{/if}
		<div class="mt-3 grid gap-4 md:grid-cols-2">
			<div class="flex gap-2">
				<Field class="w-1/3" label="規格 1 名稱" bind:value={option1_name} placeholder="口味" />
				<Field class="flex-1" label="選項（逗號分隔）" bind:value={opt1Input} placeholder="雞肉, 牛肉" />
			</div>
			<div class="flex gap-2">
				<Field class="w-1/3" label="規格 2 名稱" bind:value={option2_name} disabled={!hasOpt1} placeholder="尺寸" />
				<Field class="flex-1" label="選項（逗號分隔）" bind:value={opt2Input} disabled={!hasOpt2} placeholder="S, M, L" />
			</div>
		</div>
		{#if hasOpt1}
			<div class="mt-3 flex gap-2">
				<Button variant="secondary" size="sm" onclick={generateVariants}>依選項產生規格</Button>
				<Button variant="secondary" size="sm" onclick={addVariant}>手動加一列</Button>
			</div>
		{/if}

		<div class="-mx-4 mt-3 overflow-x-auto md:-mx-5">
			<table class="table">
				<thead>
					<tr>
						{#if hasOpt1}<th>{option1_name}</th>{/if}
						{#if hasOpt2}<th>{option2_name}</th>{/if}
						<th>售價</th>
						<th>原價（可空）</th>
						<th>庫存</th>
						<th>SKU</th>
						<th>圖</th>
						<th>啟用</th>
						<th></th>
					</tr>
				</thead>
				<tbody>
					{#each variants as v, i}
						<tr class={v.is_active ? '' : 'opacity-60'}>
							{#if hasOpt1}<td><input bind:value={v.option1_value} class="input w-24 py-1.5" /></td>{/if}
							{#if hasOpt2}<td><input bind:value={v.option2_value} class="input w-20 py-1.5" /></td>{/if}
							<td><input type="number" min="0" bind:value={v.price} class="input w-24 py-1.5" /></td>
							<td><input type="number" min="0" bind:value={v.compare_at_price} class="input w-24 py-1.5" /></td>
							<td><input type="number" min="0" bind:value={v.stock} class="input w-20 py-1.5" /></td>
							<td><input bind:value={v.sku} class="input w-28 py-1.5" /></td>
							<td>
								<select bind:value={v.image_path} class="input w-auto py-1.5">
									<option value={null}>—</option>
									{#each images as img, j (img.path)}
										<option value={img.path}>第 {j + 1} 張</option>
									{/each}
								</select>
							</td>
							<td class="text-center"><input type="checkbox" bind:checked={v.is_active} class="accent-brand" /></td>
							<td>
								<button type="button" class="font-medium text-danger disabled:opacity-30" onclick={() => removeVariant(i)} disabled={variants.length === 1}>刪</button>
							</td>
						</tr>
						{#if variantError(i)}
							<tr><td colspan="9" class="text-danger">{variantError(i)}</td></tr>
						{/if}
					{/each}
				</tbody>
			</table>
		</div>
	</Card>

	<div class="flex items-center gap-3">
		<Button type="submit" size="lg" disabled={saving || uploading}>
			{saving ? '儲存中…' : product ? '儲存' : '建立商品'}
		</Button>
		<a href="/admin/products" class="link text-sm">回列表</a>
		{#if product && product.status !== 'archived'}
			<Button variant={confirmArchive ? 'danger' : 'secondary'} class="ml-auto" onclick={archive}>
				{confirmArchive ? '確定下架封存？' : '下架封存'}
			</Button>
		{/if}
	</div>
</form>
```

- [ ] **Step 11: gate 與截圖（後台要登入）**

```bash
pnpm -C web check
pnpm -C web test
pnpm -C web build
node /Users/wilson08/.claude/jobs/82e5d2c3/tmp/shot.mjs http://localhost:5173/admin /Users/wilson08/.claude/jobs/82e5d2c3/tmp/shots/admin --login
node /Users/wilson08/.claude/jobs/82e5d2c3/tmp/shot.mjs http://localhost:5173/admin/orders /Users/wilson08/.claude/jobs/82e5d2c3/tmp/shots/admin-orders --login
node /Users/wilson08/.claude/jobs/82e5d2c3/tmp/shot.mjs http://localhost:5173/admin/products /Users/wilson08/.claude/jobs/82e5d2c3/tmp/shots/admin-products --login
node /Users/wilson08/.claude/jobs/82e5d2c3/tmp/shot.mjs http://localhost:5173/admin/settings /Users/wilson08/.claude/jobs/82e5d2c3/tmp/shots/admin-settings --login
```
再用 `curl -s http://localhost:8080/api/products/demo-ui-2` 拿到 `id`，截 `http://localhost:5173/admin/products/<id>`（`--login`）到 `shots/admin-product-edit`。
Expected: 0/0、46、build 成功；每張 `overflow=false`；側欄膠囊、表格有淡灰藍表頭與條紋、狀態是膠囊、按鈕是橘色／白色膠囊。

- [ ] **Step 12: 用 Playwright 點一次商品編輯表單（確認 bind 沒斷）**

用 Write 工具寫 `/Users/wilson08/.claude/jobs/82e5d2c3/tmp/check-form.mjs`：
```js
// 登入後台 → 打開 demo-ui-2 的編輯頁 → 把「排序」改成 99 存檔 → 用 API 確認 → 改回 2 存檔
import { chromium } from '/Users/wilson08/IdeaProjects/dog_shop/.claude/worktrees/ui-redesign/web/node_modules/@playwright/test/index.mjs';

const HEADERS = { 'x-requested-with': 'fetch', origin: 'http://localhost:5173' };
const browser = await chromium.launch();
const context = await browser.newContext({ viewport: { width: 1280, height: 900 }, locale: 'zh-TW' });
const login = await context.request.post('http://localhost:8080/api/auth/login', {
	headers: HEADERS,
	data: { email: 'admin@example.com', password: 'admin12345' }
});
if (!login.ok()) throw new Error(`login ${login.status()}`);
const list = await (await context.request.get('http://localhost:8080/api/admin/products?q=帆布托特包')).json();
const product = list.items.find((p) => p.slug === 'demo-ui-2');
if (!product) throw new Error('找不到 demo-ui-2（先跑 seed-demo.mjs）');

const page = await context.newPage();
async function setSortOrder(value) {
	await page.goto(`http://localhost:5173/admin/products/${product.id}`, { waitUntil: 'networkidle' });
	await page.getByLabel('排序（數字小的在前）').fill(String(value));
	await page.getByRole('button', { name: '儲存', exact: true }).click();
	await page.getByText('已儲存').waitFor();
	const fresh = await (await context.request.get('http://localhost:8080/api/products/demo-ui-2')).json();
	console.log(`存 ${value} → API 回 sort_order=${fresh.sort_order}`);
	if (fresh.sort_order !== value) throw new Error('sort_order 沒存進去：表單的 bind 斷了');
}
await setSortOrder(99);
await setSortOrder(2);
await browser.close();
```
Run: `node /Users/wilson08/.claude/jobs/82e5d2c3/tmp/check-form.mjs`
Expected: 印出 `存 99 → API 回 sort_order=99` 與 `存 2 → API 回 sort_order=2`。若公開的 `/api/products/{slug}` 回應沒有 `sort_order` 欄位，改用 `GET /api/admin/products/{id}` 讀（回應有 `sort_order`）。

- [ ] **Step 13: Commit**

```bash
git add web/src/routes/admin web/src/lib/components/admin/ProductForm.svelte
git commit -m "feat(web): 後台套同一套元件（版面不動）" -m "Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>"
```

---

### Task 9: 總驗收（三個 gate、全站截圖、Playwright e2e）

**Files:** 不改任何檔（只跑驗證；若有小修，單獨 commit）。

**Interfaces:** 無。

- [ ] **Step 1: 確認沒有殘留的舊色與舊 class**

```bash
grep -rn -E "gray-[0-9]|red-[0-9]|green-[0-9]|yellow-[0-9]|amber-[0-9]|blue-[0-9]|font-mono" web/src --include=*.svelte --include=*.html --include=*.css
```
Expected: 沒有輸出。有的話逐一換成 token（`ink-soft`、`danger`、`success`、`warning`、`surface`、`line`）再 commit 一次 `style(web): 清掉殘留的舊色`。

- [ ] **Step 2: 三個 gate**

```bash
pnpm -C web check
pnpm -C web test
pnpm -C web build
```
Expected: 0/0、46 全過、build 成功。

- [ ] **Step 3: 確認沒有新依賴**

```bash
git diff main -- web/package.json web/pnpm-lock.yaml
```
Expected: 沒有輸出。

- [ ] **Step 4: Playwright e2e（api 與 web 都要在跑）**

```bash
pnpm -C web test:e2e
```
Expected: `2 passed`（宅配 + 超商取貨）。綠界的兩個 POST 都被測試攔截。若失敗，看 `web/test-results/` 的 trace，對照 Global Constraints 第 4 條。

- [ ] **Step 5: 全站截圖總表（給控制者看最後一眼）**

跑一遍 Task 2～8 的每個 `shot.mjs` 指令（首頁、列表、商品頁、購物車、結帳、訂單、登入、會員中心、錯誤頁、後台四頁），全部 `overflow=false`。把所有 PNG 路徑列成清單回報。

- [ ] **Step 6: 停伺服器、回報**

```bash
lsof -ti :5173 -sTCP:LISTEN
lsof -ti :8080 -sTCP:LISTEN
```
拿到 pid 後各 `kill <pid>`。回報：`git log --oneline main..HEAD` 的 commit 清單、gate 結果、e2e 結果、截圖清單、開發 DB 新增的雜物（8 個 `demo-ui-*` 商品、2 個分類、`shot@test.local` 的待付款訂單、e2e 留下的 `E2E 狗糧` 商品與取消訂單）。**不 push、不開 PR。**

---

## 控制者自留備註（不派給子代理）

- Task 2 做完是看圖關卡：把 `home-mobile.png`、`home-desktop.png` 用 Read 看過，再交給使用者決定；使用者要改顏色或看板，改 `app.css` 的 token 與 `+page.svelte` 即可，不用重做元件。
- Task 9 通過後，依使用者的固定規則跑 `/codex-review-fix`（`-s read-only`，nohup + wait-pid），修完再跑一次 Task 9 的 Step 2 與 Step 4。
- 結束時更新記憶檔：開發 DB 雜物清單、`worktree-ui-redesign` 分支狀態（未 push）、示意資料的識別方式（slug `demo-ui-*`）。
