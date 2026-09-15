# dog_shop 前端視覺重新設計（UI/UX）設計文件

日期：2026-09-09
狀態：草稿，等使用者核可
上游：`docs/superpowers/specs/2026-09-06-dog-shop-mvp-design.md`（MVP 規格；本文件只改「長相」，不改 MVP 規格定義的資料、API、流程）

## 1. 目標與範圍

**目標**：把現在「能用但很素」的前端（灰色按鈕、灰色邊框、系統預設字、沒有品牌色、沒有 Logo、首頁沒有主視覺）換成有辨識度、好逛、手機優先的外觀，功能一個都不變。

**主題修正**：這家店賣的是**生活雜貨、小物**，不是寵物用品。專案名稱 `dog_shop` 與店名只是聽起來像寵物店。所有視覺元素不得出現狗、爪印、骨頭等寵物意象。

**範圍**：
- 前台（買家看的所有頁面）：全部重新設計。
- 後台（`/admin/**`）：**排版不動**，只換成同一套顏色、字級、按鈕、輸入框、卡片、表格樣式。
- 使用者已選定：風格「活潑」、顏色「橘 + 深藍」、沒有 Logo（用店名文字 + 吊牌圖示）、做法「Svelte 元件庫」。

**不做**：
- 不加任何新功能（沒有 Logo 上傳、沒有深色模式、沒有搜尋建議、沒有輪播）。
- 不改任何文案（按鈕字、標籤字、提示字、錯誤字全部照舊；Playwright 測試靠這些字找元素，見 §9）。
- 不改 `<script>` 裡的邏輯：狀態、衍生值、事件處理、API 呼叫全部照舊；只允許新增 import 與把原本的值接到新元件的 props。
- 不改後端、不改 API、不改資料庫。
- 不引入任何元件庫、icon 套件、網路字型（MVP 規格 §5：「自寫少量元件，不引入元件庫」）。

## 2. 設計方向

**主題**：台灣的生活雜貨小物網路商店，取代原本的蝦皮賣場。
**對象**：用手機逛的買家（原本在蝦皮買）；老闆用電腦管後台。
**主要任務**：看分類 → 找商品 → 選規格 → 加購物車 → 結帳（超商取貨或宅配）→ 到綠界付款。

**一句話**：白底、橘色主色、深藍墨字、膠囊按鈕、首頁一大塊橘色看板，其他地方安靜。

**最搶眼的一個東西只放一處**：首頁最上方的橘色看板（店名、簡介、散落的小圓點小方塊圖案代表「很多小東西」、分類膠囊）。商品卡、表單、表格都保持安靜：沒有陰影、沒有多餘邊框、沒有進場動畫。

**刻意避開的通用套路**（這些是「隨便一頁都長這樣」的預設，不是為這家店做的選擇）：
- 米白底 + 赭紅強調色。
- 每個東西都同一個圓角、每張卡都同一個灰陰影。
- 每張卡 hover 都會動、每個區塊都淡入。
- 標題旁邊加「→」、標籤全大寫、用等寬字當小標。
- 用 `#111` 這種「假黑」當文字色。

## 3. 設計 token

### 3.1 顏色（Tailwind 4 `@theme`，寫在 `web/src/app.css`）

| token | 值 | 用途 |
|---|---|---|
| `--color-ground` | `#FFFFFF` | 頁面底色、卡片底色 |
| `--color-ink` | `#16274A` | 主要文字、標題、價格、深藍頁尾底色、焦點框（深藍頁尾內的連結改白框） |
| `--color-ink-soft` | `#5B6B85` | 次要文字（規格名、時間、說明） |
| `--color-brand` | `#FF8A00` | 主按鈕底、選中的膠囊、購物車數字、看板底 |
| `--color-brand-deep` | `#E57200` | 主按鈕 hover/active 底（深藍字在上面 4.7:1；原 `#D96A00` 只有 4.2:1，不到 AA） |
| `--color-brand-soft` | `#FFE9CF` | 選中的分頁/導覽底、hover 底、看板內的白膠囊 hover |
| `--color-surface` | `#F1F5FA` | 圖片底、表格條紋、表頭底、次要區塊底 |
| `--color-line` | `#D9E1EC` | 邊框、分隔線 |
| `--color-success` / `--color-success-soft` | `#15734A` / `#E3F6EC` | 已付款、成功（深綠是為了 12px 字在淡綠底上也有 4.5:1） |
| `--color-warning` / `--color-warning-soft` | `#B45309` / `#FFF4DB` | 待付款、庫存調整、注意 |
| `--color-danger` / `--color-danger-soft` | `#B91C1C` / `#FDE8E8` | 錯誤、已售完、取消動作（同上，深紅才過 4.5:1） |
| `--color-brand-light` | `#FFA640` | 只給看板上的散落小形狀用 |

**對比規則**（WCAG AA）：
- 橘色 `#FF8A00` 在白底上對比只有約 2.4:1，**永遠不用來寫字**，只用來當底色與裝飾。
- 橘底一律配深藍字：`#16274A` 在 `#FF8A00` 上約 6.2:1，在 hover 的 `#E57200` 上約 4.7:1。按鈕字 14–16px 半粗不算 WCAG 的大字（門檻是 18.66px 粗體），所以 hover 也要過 4.5:1。
- 價格用 `ink` 粗體，不用橘色。
- 連結：`ink` 字 + 橘色底線（`text-decoration-color`），hover 底線變粗。

### 3.2 字

- 字型：系統字，順序 `"PingFang TC", "Noto Sans TC", "Microsoft JhengHei", system-ui, sans-serif`。理由：中文網路字型一套數 MB，手機第一次載會很慢；PingFang TC 本身圓潤、與「活潑」方向相合；零外部請求。
- 級距（px）：12 / 14 / 16（內文）/ 18 / 22 / 28 / 36；看板標題桌機 44。
- 粗細：內文 400、強調 600、標題與店名 800。標題 `letter-spacing: -0.01em`、行高 1.2；內文行高 1.6。
- 數字（價格、訂單編號、帳號、繳費代碼）：`font-variant-numeric: tabular-nums`，價格粗體 700。
- 商品說明、任何長文：最大寬 `65ch`。

### 3.3 形狀、間距、陰影、動態

- 圓角依層級不同：按鈕、膠囊、標籤 = 全圓（9999px）；輸入框、圖片格、小按鈕 = 12px；卡片 = 16px；看板 = 24px；縮圖 = 8px。
- 內容最大寬 72rem；左右內距手機 16px、桌機 24px。
- 陰影：預設沒有。只有兩處：手機底部黏住的動作列（`0 -8px 24px rgba(22,39,74,.08)`）與 toast。
- 動態：只有顏色變化 150ms（hover/active）與按下時 `scale(.98)`。沒有卡片 hover 位移、沒有進場淡入。`prefers-reduced-motion: reduce` 時關掉所有 transition。
- 焦點：所有可操作元素 `focus-visible` 顯示 2px 深藍（`ink`）外框、offset 2px；深藍頁尾裡的連結改白框。不用橘色：橘框在橘看板上會消失，在白底也只有 2.4:1，不到非文字的 3:1。

### 3.4 圖示與品牌記號

- **吊牌記號**：一個簡單的吊牌形狀（矩形一端收尖、上面一個圓孔），橘色實心、孔白色。放在店名左邊組成 Logo；也做成 favicon（橘色圓角方塊底 + 白色吊牌），取代現在的 Svelte 預設 favicon。
- **圖示**：全部是手寫 inline SVG（24 格、2px 線、`currentColor`），集中在一個 `Icon.svelte`，名稱：`tag`、`cart`、`user`、`search`、`check`、`x`、`minus`、`plus`、`chevron-left`、`chevron-right`、`image`（沒有圖片時的佔位）。不裝任何 icon 套件。
- **看板圖案**：看板右側散落 8～12 個小形狀（圓、圓角方、吊牌），用比看板底更淺的橘（`#FFA640`）、透明度 0.35，超出看板圓角的部分裁掉。手機寬度時只留右上角少量，不蓋到文字。

## 4. 元件庫（`web/src/lib/components/ui/`）

全部是 Svelte 5 runes 元件，`$props()` + `$bindable()` + snippet。**radio、checkbox、select 維持原生元素加 class**（`bind:group` 不能包成元件）；`select` 與 `textarea` 放在 `Field` 的 `children` snippet 裡。

| 元件 | props | 產出 |
|---|---|---|
| `Button.svelte` | `variant: 'primary' \| 'secondary' \| 'ghost' \| 'danger' = 'primary'`、`size: 'sm' \| 'md' \| 'lg' = 'md'`、`href?: string`、`disabled?: boolean`、`type = 'button'`、`class?: string`、`children`、其餘屬性（`onclick`、`aria-*`、`form`…）展開到元素上 | 有 `href` 且未 disabled → `<a>`；有 `href` 且 disabled → `<span aria-disabled="true">`；否則 `<button>`。膠囊；`primary` 橘底深藍字、`secondary` 白底 `line` 邊深藍字、`ghost` 無底無邊、`danger` 紅底白字。`disabled` 透明度 .5。全寬用 `class="w-full"` |
| `Field.svelte` | `label: string`、`value = $bindable()`、`error?: string`、`hint?: string`、`class?: string`、`children?: Snippet`、其餘屬性展開到 `<input>` | `<label class="block"><span class="field-label">{label}</span>` + （有 `children` 就 render 它，否則 `<input class="input" bind:value {...rest}>`）`</label>`，接著 `hint`（`<p class="field-hint">`）與 `error`（`<p class="field-error">`）**放在 `<label>` 外面**（這樣 label 的可讀名稱只有標籤字，Playwright `exact: true` 才會準）。用 `$props.id()` 產 id，`aria-describedby` 指向 error |
| `Card.svelte` | `title?: string`、`class?: string`、`children`、`actions?: Snippet` | `<section class="card">`：白底、`line` 1px 邊、16px 圓角、內距 16px（桌機 20px）；有 `title` 時上方一列 `h2`（18px 粗）＋右側 `actions` |
| `Badge.svelte` | `tone: 'brand' \| 'neutral' \| 'success' \| 'warning' \| 'danger' = 'neutral'`、`children` | 膠囊小標籤（12px 粗）；`brand` 橘底深藍字、`neutral` `surface` 底 `ink-soft` 字、其餘用對應的 soft 底 + 主色字 |
| `StatusBadge.svelte` | `status: OrderStatus` | 用 `ORDER_STATUS_LABELS` 顯示文字，tone 對應（`OrderStatus` 在 `web/src/lib/types.ts:158`，就這六個）：`pending_payment` → warning、`paid` → success、`shipped` → brand、`completed` → success、`cancelled` → neutral、`refunded` → neutral。後台商品狀態（`ProductStatus`：`draft` / `active` / `archived`）不用這個元件，直接用 `Badge`：`active` → success、`draft` → warning、`archived` → neutral，文字照後台現有寫法 |
| `Alert.svelte` | `tone: 'info' \| 'success' \| 'warning' \| 'danger' = 'info'`、`title?: string`、`children` | 12px 圓角的訊息區塊，soft 底 + 主色左邊 4px 粗線；`info` 用 `surface` 底 + `ink` 線 |
| `PageHeader.svelte` | `title: string`、`subtitle?: string`、`children?: Snippet`（右側動作） | `<h1>` 28px 800（手機 22px）+ 次要字 subtitle；`children` 靠右，手機時換行到下方 |
| `EmptyState.svelte` | `message: string`、`children?: Snippet` | 置中：`image`/`tag` 圖示（`ink-soft`）、訊息、下面放 `children`（通常是一個 `Button href`） |
| `QtyStepper.svelte` | `value = $bindable(1)`、`min = 1`、`max = 99`、`onchange?: (v: number) => void`、`label = '數量'` | `−` 按鈕（`aria-label="減少"`）、原生 `<input type="number" aria-label={label}>`、`+` 按鈕（`aria-label="增加"`），三個合成一個膠囊群。內部把值夾在 `[min, max]`，變動時更新 `value` 並呼叫 `onchange` |
| `Logo.svelte` | `name: string`、`size: 'md' \| 'lg' = 'md'`、`inverted = false` | 吊牌記號 + 店名（800）；`inverted` 給深藍頁尾用（白字、橘吊牌） |
| `Icon.svelte` | `name`（§3.4 的名稱）、`size = 20`、`class?: string` | inline SVG |

既有元件改樣式、位置不動：`components/Toasts.svelte`（深藍底白字膠囊、柔陰影）、`components/Pagination.svelte`（上一頁/下一頁改用 `Button variant="secondary" size="sm"` + chevron 圖示；「第 x / y 頁」照舊）、`components/ProductCard.svelte`、`components/ProductView.svelte`、`components/AddressFields.svelte`、`components/checkout/InvoiceFields.svelte`、`components/admin/ProductForm.svelte`。

全域 class（`app.css` 的 `@layer components`，給元件與原生元素共用）：`.input`（12px 圓角、`line` 邊、內距 12px×10px、focus 深藍框（橘框在白底只有 2.4:1）、`aria-invalid` 紅邊）、`.field-label`（14px、`ink`、600）、`.field-hint`（12px `ink-soft`）、`.field-error`（12px `danger`）、`.card`、`.table`（後台表格：表頭 `surface` 底 12px 600、列間 `line` 線、偶數列 `surface` 底、hover 淡橘）、`.option-card`（可點的 radio 卡：`label` 帶 `line` 邊 12px 圓角，`has-checked:` 時邊框變橘、底變淡橘）、`.link`（`ink` 字 + 橘底線）。

## 5. 版面與各頁設計

### 5.1 外框（`routes/+layout.svelte`、`app.html`、`app.css`）

- `app.html` 的 `<body>` 拿掉 `bg-gray-50 text-gray-900`；底色與字色改由 `app.css` 的 `@layer base` 用 token 設定。
- 頁首：白底、下方 `line` 線、`sticky top-0`。左邊 `Logo`；右邊導覽：「全部商品」（文字）、「購物車」（`cart` 圖示 + 文字，數量 > 0 時右上角橘色圓形數字，深藍字）、「會員中心」或「後台」（`user` 圖示 + 文字）、「登入」或「登出」。手機（< md）時「購物車」「會員中心」「後台」只留圖示，文字用 `sr-only`；「全部商品」「登入」「登出」保留文字。
- 頁尾：`ink` 深藍底、白字。左：`Logo inverted`；下面兩行：聯絡 Email、聯絡電話（有才顯示）；右：三個連結「全部商品」「購物車」「會員中心」。手機時堆疊置左。
- `<svelte:head>` 裡的 `<title>{page.data.title ?? data.shop.name}</title>` 與上面那段關於 SSR settle loop 的註解**原封不動保留**。

### 5.2 首頁（`routes/+page.svelte`）

```
手機 375                          桌機 1280
┌────────────────────┐           ┌──────────────────────────────────────────┐
│ 🏷 店名     商品 🛒 👤│           │ 🏷 店名              全部商品 🛒購物車 登入 │
├────────────────────┤           ├──────────────────────────────────────────┤
│╭──────────────────╮│           │╭────────────────────────────────────────╮│
││ 店名（36px）   ○ ▢ ││           ││ 店名（44px）                    ○  ▢   ○││
││ 簡介一行        ○  ││           ││ 簡介一行                      ▢   ○  🏷 ││
││ (全部)(分類)(分類) ││           ││ (全部)(分類)(分類)(分類)(分類)         ○ ││
│╰──────────────────╯│           │╰────────────────────────────────────────╯│
│ 最新商品      看全部 │           │ 最新商品                          看全部 │
│ ┌──────┐ ┌──────┐  │           │ ┌────┐ ┌────┐ ┌────┐ ┌────┐             │
│ │  圖  │ │  圖  │  │           │ │ 圖 │ │ 圖 │ │ 圖 │ │ 圖 │             │
│ │ 名稱 │ │ 名稱 │  │           │ │名稱│ │名稱│ │名稱│ │名稱│             │
│ │ $價  │ │ $價  │  │           │ │$價 │ │$價 │ │$價 │ │$價 │             │
│ └──────┘ └──────┘  │           │ └────┘ └────┘ └────┘ └────┘             │
├────────────────────┤           ├──────────────────────────────────────────┤
│ 深藍頁尾            │           │ 深藍頁尾                                 │
└────────────────────┘           └──────────────────────────────────────────┘
```

- 看板：橘底 24px 圓角，左對齊；店名 800；簡介用 `data.shop.description`（空就不顯示這行）；分類膠囊白底深藍字（hover 淡橘），第一顆固定「全部」→ `/products`，其餘 → `/products?category=<slug>`。右側散落小形狀（§3.4）。
- 「最新商品」：`h2` 22px 800 + 右側「看全部」連結（`.link`，沒有箭頭）。商品格：手機 2 欄、md 3 欄、lg 4 欄，間距 16px。
- 沒有商品：`EmptyState message="商品準備中，請稍後再來。"`（文案照舊）。

### 5.3 商品卡（`components/ProductCard.svelte`）

- 沒有邊框、沒有陰影。上：正方形圖片格（`surface` 底、12px 圓角、`object-cover`、`loading="lazy"` 照舊）；沒有圖片時置中一個 `image` 圖示（`ink-soft`）。
- `!in_stock` 時圖片右上角一顆 `Badge tone="danger"`「已售完」（文案照舊，改位置不改字），圖片加 `opacity-60`。
- 下：名稱 14px、兩行截斷、`ink`；價格 16px 700 tabular-nums。整張是一個 `<a>`，focus 時深藍框（§3.3）。

### 5.4 商品列表（`routes/products/+page.svelte`）

- `PageHeader title={title} subtitle={共 N 件（有結果時）}`。
- 篩選列（原本的 `<form method="GET">` 照舊）：搜尋框（`.input` + 左側 `search` 圖示、`aria-label="搜尋商品"`、placeholder 照舊）手機佔整列；下一列分類 `select`、排序 `select`（`.input`）、`Button type="submit"`「搜尋」。桌機一列排完。
- 格線同首頁；沒有結果：`EmptyState message="沒有符合的商品"`。`Pagination` 照舊。

### 5.5 商品頁（`components/ProductView.svelte`）

```
桌機                                       手機
┌───────────────┬──────────────────────┐   ┌────────────────────┐
│ 全部商品 › 分類 │                      │   │ 全部商品 › 分類      │
│ ┌───────────┐ │ 名稱（28px 800）      │   │ ┌────────────────┐ │
│ │           │ │ $價（28px 700）  劃掉價 │   │ │      大圖       │ │
│ │   大圖    │ │                      │   │ └────────────────┘ │
│ │           │ │ 規格名                │   │ [縮][縮][縮]        │
│ └───────────┘ │ (選項)(選項)(選項)     │   │ 名稱               │
│ [縮][縮][縮]   │ 庫存 N               │   │ $價                │
│               │ [− 1 +] [加入購物車]  │   │ (選項)(選項)        │
│               │ 商品說明              │   │ 庫存 N             │
│               │ ……                   │   │ 商品說明……          │
└───────────────┴──────────────────────┘   ├────────────────────┤
                                           │ [− 1 +][加入購物車] │ ← 黏在底部
                                           └────────────────────┘
```

- 麵包屑：`ink-soft` 14px，分隔用 `chevron-right` 圖示（把現在的「›」字換成圖示；連結文字「全部商品」與分類名照舊）。
- 大圖：12px 圓角、`surface` 底；縮圖列 8px 圓角，目前那張 2px 橘框，其餘 `line` 框；`aria-label` 照舊。
- 規格選項：膠囊；選中 = `ink` 底白字；沒庫存 = `line` 邊、`ink-soft` 字、虛線邊、透明度 .5（原本 `opacity-40` 改成這個，語意一樣）。
- 價格：`ink` 28px 700；劃掉價 `ink-soft` 16px 400 `line-through`。
- 數量 + 加入購物車那一列：桌機在資訊欄裡；手機（< md）`sticky bottom-0`、白底 95% + `backdrop-blur`、上方 `line` 線、§3.3 的陰影，左右撐滿。**整頁只有一顆「加入購物車」按鈕**（`Button size="lg"` + `Icon cart`，`disabled` 條件照舊）。
- 商品說明：`h2` 18px 600、內文 16px `ink` 行高 1.7、`whitespace-pre-line` 照舊、最大寬 65ch。

### 5.6 購物車（`routes/cart/+page.svelte`）

- `PageHeader title="購物車"`。
- 有無法購買的項目：`Alert tone="warning"` 內含原本的文字與 `Button size="sm"`「移除無法購買的商品」。
- 每一項一張 `Card`（無 title）：左 80px 圖（12px 圓角）、中 名稱（600）/ 規格（`ink-soft`）/ 單價 / 狀態文字（已下架、已售完、庫存只剩 N 件…文案與顏色語意照舊：紅 = danger、黃 = warning）、右 `QtyStepper`（`value={line.qty}`、`max` 照舊、`onchange` 呼叫 `cart.setQty`）與小計（700）、「移除」`Button variant="ghost" size="sm"`。
- 摘要：桌機（lg）右欄 `Card` 黏在 `top-20`；手機在清單下方且 `sticky bottom-0`。內容：「重新確認庫存」`Button variant="ghost" size="sm"`、「小計」+ 金額（22px 700）、「前往結帳」`Button href="/checkout" size="lg" class="w-full"`（可結帳時），不可結帳時同一顆 `disabled`（渲染成 `<span>`）；下面兩行提示文案照舊。
- 空的：`EmptyState message="購物車是空的。"` + `Button variant="secondary" href="/products"`「去逛逛」。

### 5.7 結帳（`routes/checkout/+page.svelte`、`AddressFields.svelte`、`checkout/InvoiceFields.svelte`）

- `PageHeader title="結帳"`。
- 四個區塊各一張 `Card`，`title` 前面一顆橘色圓圈數字 1～4（這裡的內容真的是順序，才用數字）：1 聯絡資料、2 取貨方式、3 發票、4 付款方式。標題文字照舊。
- 文字輸入全部改 `Field`（`label` 文字一字不改，例如「Email（訂單通知寄到這裡）」「收件人」「手機」「地址」）；`select`（縣市、鄉鎮市區、常用地址、載具）放在 `Field` 的 `children` 裡加 `.input`；錯誤字用 `Field` 的 `error`。
- 宅配 / 超商取貨、三家超商、付款方式：每個 radio 的 `<label>` 改成 `.option-card`；`<input type="radio">` 與 `bind:group`、`disabled`、`onchange` 照舊，只是加 class。
- 已選門市：`Alert tone="info"` 顯示原本的門市名稱與地址；「選擇門市」「重新選擇門市」`Button variant="secondary"`。
- 摘要欄：桌機右欄 `Card title="訂單摘要"` 黏在 `top-20`；商品列、小計、運費、總計（總計 18px 700）、`Button type="submit" size="lg" class="w-full"`「送出訂單」、提示文案照舊。手機時摘要在表單最下面（不黏，避免蓋住表單）。
- 所有警告／錯誤文字位置照舊、顏色改用 token（黃 → warning、紅 → danger）。

### 5.8 訂單頁（`routes/orders/[id]/+page.svelte`）

- `PageHeader title={訂單 {o.order_no}} subtitle={成立時間 …}`，`actions` 放 `StatusBadge`。
- 狀態區塊：待付款 → `Alert tone="warning"`（轉帳銀行代碼＋帳號、繳費代碼放大 22px 700 tabular-nums；其他句子照舊；「重新付款」`select` + `Button size="sm"`）；已付款/已出貨/已完成 → `Alert tone="success"`；已退款、已取消 → `Alert tone="info"`。
- 商品清單一張 `Card`，底部小計／運費／總計；「取貨」「發票」兩張 `Card title=…` 並排（md）。
- 取消流程：「取消訂單」`Button variant="ghost" size="sm"`；確認時「確定取消這筆訂單」`Button variant="danger"` + 「保留」`Button variant="secondary"`。文字照舊。

### 5.9 登入、註冊、忘記密碼、重設密碼

- 置中 `Card`（最大寬 24rem），上方置中一個大吊牌記號（橘）+ `h1` 標題；`Field` 表單；`Button size="lg" class="w-full"` 主按鈕；下方連結用 `.link`。文案照舊。
- 忘記密碼寄出後的那段綠色訊息改 `Alert tone="success"`；各頁的整體錯誤訊息（`error`）改 `Alert tone="danger"`，欄位錯誤走 `Field` 的 `error`。

### 5.10 會員中心（`routes/account/**`）

- 側欄改成分頁：手機水平可捲的膠囊列、md 以上直排；目前頁 = 淡橘底 + `ink` 700，其餘 `ink-soft`，hover `surface` 底。
- 「個人資料」：`PageHeader`（subtitle 放 Email）+ `Card` 包表單 + `Field`；「更改密碼」那個 `fieldset` 改成第二張 `Card title="更改密碼（不改就留空）"`。
- 「我的訂單」：現在就是表格，維持表格，加 `.table`；「狀態」欄改用 `StatusBadge`；訂單編號用 `.link`；沒有訂單 → `EmptyState message="還沒有訂單。"` + `Button variant="secondary" href="/products"`「去逛逛」。
- 「常用地址」：`PageHeader` 右側 `actions` 放「新增地址」`Button`（出現條件照舊）；編輯表單包在 `Card` 裡，欄位改 `Field`（`AddressFields` 已在 §5.7 改過）、「設為預設地址」checkbox 原生加 class；地址清單每筆一張 `Card`，「預設」用 `Badge tone="brand"`，「編輯」`Button variant="ghost" size="sm"`、「刪除」`Button variant="ghost" size="sm"`、「確定刪除？」`Button variant="danger" size="sm"`（三顆按鈕的文字與出現條件照舊）。

### 5.11 錯誤頁（`routes/+error.svelte`）

- 狀態碼 64px 800 `ink`、訊息 `ink-soft`、`Button href="/"`「回首頁」。

### 5.12 後台（`routes/admin/**`、`components/admin/ProductForm.svelte`）

- 側欄與會員中心同一套分頁樣式；版面（側欄 + 內容）不動。
- 每頁 `PageHeader`（右側動作如「新增商品」用 `Button`）；區塊改 `Card`；表單輸入改 `Field`（`select`／`textarea`／`checkbox` 原生加 class）；按鈕改 `Button`（主要動作 primary、次要 secondary、刪除 danger）；表格加 `.table`；商品狀態、訂單狀態用 `Badge`／`StatusBadge`；儀表板五格改 `Card`（數字 28px 800，「發票開立失敗」「需退款」「超商退回」的數字大於 0 時用 `danger` 字色）。
- `ProductForm.svelte`（427 行）與 `admin/orders/[id]`（254 行）是風險最高的兩個檔：只換 class 與元件外殼，**任何 `$state`、`$derived`、handler、`bind:` 目標都不能動**。

## 6. 無障礙、RWD、效能

- 所有表單控制項維持真正的 `<label>`（包住控制項或 `for`），錯誤訊息用 `aria-describedby` 連到欄位。
- 焦點框可見（§3.3）；顏色對比依 §3.1；`prefers-reduced-motion` 尊重。
- 手機 375px 到桌機 1280px 之間不得出現水平捲軸；寬內容（後台表格）自己 `overflow-x-auto`。
- 不載入任何外部資源；沒有新的 JS 依賴；`pnpm -C web build` 後 client bundle 不因本次而多出第三方套件。
- 圖片照舊 `loading="lazy"`；沒有新的 `{@html}`（商品頁既有的 JSON-LD 照舊）。

## 7. 檔案異動一覽

新增：
- `web/src/lib/components/ui/{Button,Field,Card,Badge,StatusBadge,Alert,PageHeader,EmptyState,QtyStepper,Logo,Icon}.svelte`
- `web/src/lib/assets/favicon.svg`（覆蓋現有的 Svelte 預設 favicon，檔名不變）
- `web/src/lib/components/ui/*.test.ts`：只測純邏輯（例如 `StatusBadge` 的 tone 對應、`QtyStepper` 的夾值函式若抽成 `lib/qty.ts`）；不做 DOM 快照測試

修改（只改 markup / class / import / 把值接到元件）：
- `web/src/app.css`、`web/src/app.html`、`web/src/routes/+layout.svelte`、`web/src/routes/+error.svelte`
- `web/src/routes/+page.svelte`、`products/+page.svelte`、`products/[slug]/+page.svelte`
- `web/src/lib/components/{ProductCard,ProductView,Pagination,Toasts,AddressFields}.svelte`、`checkout/InvoiceFields.svelte`
- `web/src/routes/cart/+page.svelte`、`checkout/+page.svelte`、`orders/[id]/+page.svelte`
- `web/src/routes/{login,register,forgot-password}/+page.svelte`、`reset/[token]/+page.svelte`
- `web/src/routes/account/{+layout,+page}.svelte`、`account/{orders,addresses}/+page.svelte`
- `web/src/routes/admin/{+layout,+page}.svelte`、`admin/{orders,products,categories,import,settings}/+page.svelte`、`admin/orders/[id]/+page.svelte`、`admin/products/{new,[id]}/+page.svelte`、`web/src/lib/components/admin/ProductForm.svelte`

不動：`web/src/lib/**/*.ts`（除非新增 `lib/qty.ts` 之類的純函式）、`web/src/routes/**/*.server.ts`、`web/src/hooks.server.ts`、`api/**`、`deploy/**`。

## 8. 測試與驗證

每個 task 結束都要過：
1. `pnpm -C web check` → 0 errors 0 warnings。
2. `pnpm -C web test` → 現有 38 個 vitest 全過（加新的純邏輯測試後數量只增不減）。
3. `pnpm -C web build` → 成功。
4. 截圖：用 Playwright 的 chromium 在 375×812 與 1280×800 各截一張該 task 負責的頁面（存到工作暫存目錄），控制者看圖檢查：沒有水平捲軸、文字沒被蓋住、按鈕看得到、對比正常。

整體結束前：
5. `pnpm -C web test:e2e`（要先 `make run` 把 DB + api + web 全開；只連 `localhost`，綠界一律被測試攔截，不得連到任何外部主機）→ 現有 2 個 e2e 全過。
6. 依使用者的固定規則跑 `/codex-review-fix`（`-s read-only`、nohup + wait-pid）。

**Task 1 之後有一個「看圖關卡」**：做完 token、元件庫、外框、favicon、首頁後，先截首頁與商品列表（手機 + 桌機）給使用者看，使用者說可以才做其他頁。方向錯在這裡改最便宜。

**示意用資料（只在開發 DB）**：開發 DB 現在的商品不是沒有圖片（`E2E 狗糧 …`）就是圖片 404（`PROBE5-*`）。為了看圖，Task 1 會用本機 ImageMagick 產 8 張純色（淡橘、淡灰藍、淡綠…）加大字的 PNG，透過開發用 api（`localhost:8080`）的後台圖片上傳與商品 API 建 8 個上架商品（名稱用自然的雜貨品名，方便看圖；辨識靠網址代稱前綴 `demo-ui-`、SKU 前綴 `DEMO-UI-`、描述第一行「示意商品」），分兩個分類「生活雜貨」「文具小物」。這些只存在開發 DB，永遠不匯入正式環境；記到記憶檔的「開發 DB 雜物」清單。

## 9. Playwright 定位契約（一字不能改）

`web/e2e/checkout.spec.ts` 用下面這些字找元素，重新設計後這些元素的**角色、可讀名稱、文字**都必須一樣：

- 按鈕（`getByRole('button')`）：`加入購物車`、`送出訂單`、`取消訂單`、`確定取消這筆訂單`、`選擇門市`（exact）。**每一頁同名按鈕只能有一顆**（strict mode）。
- 連結（`getByRole('link')`）：`前往結帳`（可結帳時必須是 `<a>`）。
- 標籤（`getByLabel`）：`/^Email/`、`收件人`、`手機`（exact）、`縣市`、`鄉鎮市區`、`郵遞區號`、`地址` → 都必須是真的 `<label>` 關聯，且 label 文字本身不含其他字（錯誤字放 label 外面）。
- radio（`getByRole('radio')`）：`/超商取貨/`、`全家` → radio 的可讀名稱來自它的 `<label>` 文字，`.option-card` 不能把文字拆到 label 外面。
- 文字（`getByText`）：`已加入購物車`（toast）、`小計`、`總計`、`待付款`、`已取消`。
- 標題（`getByRole('heading')`）：商品頁 `h1` = 商品名稱；訂單頁 `h1` 含訂單編號。

## 10. 與 MVP 規格的關係、已知舊問題

- MVP 規格 §5「自寫少量元件，不引入元件庫」：本次自寫的 11 個元件符合這條；沒有引入任何套件。
- MVP 規格 §17 第 5 點與 `docs/deploy.md`（第 13、45 行）說「Logo」在後台設定頁填，但 `ShopSettings` 沒有 logo 欄位、設定頁也沒有這個輸入框。**這是既有的文件與實作不一致，本次不修**（加 Logo 上傳要動 API 與資料庫，超出範圍）；本次用店名文字 + 吊牌記號當 Logo。
- 本次不改任何文案；如果之後要依新設計調整用字（例如「看全部」），另開需求，因為會牽動 e2e 定位。

## 11. 執行方式

- 分支 `worktree-ui-redesign`（工作區 `.claude/worktrees/ui-redesign`），每個 task 一個 commit，**不 push、不開 PR**，做完回報 commit 讓使用者決定。
- 依 `superpowers:writing-plans` 寫實作計畫（`docs/superpowers/plans/2026-09-09-ui-redesign.md`），再用 `superpowers:subagent-driven-development` 一個 task 一個子代理執行；task 順序：
  1. token、`app.css`、`app.html`、11 個 ui 元件（含純邏輯測試）
  2. favicon、外框（頁首頁尾）、首頁、`ProductCard`、示意資料、截圖腳本 → **看圖關卡**
  3. 商品列表、`Pagination`
  4. 商品頁 `ProductView`
  5. 購物車
  6. 結帳（含 `AddressFields`、`InvoiceFields`）
  7. 訂單頁、會員中心、登入/註冊/忘記/重設、錯誤頁、`Toasts`
  8. 後台（版面不動、套元件）
  9. 總驗收：全部 gates + e2e；之後控制者跑 codex review
