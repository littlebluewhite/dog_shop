# 前端改版最終審查：生活雜貨小物店 UI／UX

日期：2026-09-14
範圍：`74f13e6..40dc8cb`（分支 `worktree-ui-redesign`，14 個 commit：規格與計畫文件、Task 1–8 程式碼、一個 dev proxy chore；Task 9 只驗證、沒有 commit）。審查後的修正：修正波 `617cf06`（附錄 B，範圍複審乾淨，gate 與 Playwright e2e 重跑全綠）；codex 第二意見見附錄 C；本審查文件的 commit 在程式碼最終 HEAD 之上。
審查者：Senior Code Reviewer（opus，整支分支、只讀）；範圍複審：sonnet
規格權威：`docs/superpowers/specs/2026-09-09-ui-redesign-design.md`；計畫：`docs/superpowers/plans/2026-09-09-ui-redesign.md`；控制者裁決：附錄 A
分支狀態：**未 push、未開 PR**（使用者決定）。

---


### Strengths

Reviewed in four passes — tokens/`app.css`/`ui/` components + tests; storefront routes; admin
routes + `ProductForm`; config/assets/spec coverage — with four checks run mechanically over all
40 changed `.svelte` files rather than by eye, because those constraints only break branch-wide:
a CJK-token-set diff (copy), a `<script>`-block diff (logic), a `bind:`/handler/control-flow diff
(runtime), and an unused-import scan.

**The hard constraints hold, and they hold verifiably — not by assertion.**

- `git diff 74f13e6..40dc8cb -- web/package.json web/pnpm-lock.yaml` and `-- web/e2e` are both
  empty. The root layout's `<title>` (`web/src/routes/+layout.svelte:32`) and the SSR-settle
  comment above it (`:30-31`) are byte-identical.
- Zero forbidden palette tokens anywhere in `web/src` — I grepped the full Tailwind default ramp
  (`gray|slate|zinc|neutral|stone|red|green|yellow|amber|blue|indigo|purple|pink|emerald|teal|
  cyan|sky|violet|fuchsia|rose|lime|orange`-`[0-9]`) across every prefix
  (`bg|text|border|ring|from|to|via|decoration|divide|outline|fill|stroke|accent|placeholder|
  shadow`), not just the five the plan names. Nothing. No raw hex in any `.svelte`. No
  `bg-white`/`text-black` survivors.
- Orange is never a text colour. The only `text-brand*` uses are an SVG `currentColor` fill on
  the hero pattern and an `aria-hidden` icon stroke — see Minor #9 for the one caveat.
- No dev-only leakage into shipped code: no `localhost`, port, or seed data anywhere in
  `web/src` (the two hits are a pre-existing SSR fallback in `lib/server/api.ts` and a comment).
  Demo/seed and screenshot scripts correctly stayed out of git.

**The `<script>` discipline is the most impressive part of this branch.** Diffing every
`<script>` block old-vs-new across all 40 changed files, the complete set of changes is: added
`ui/` imports; deleted dead style-string constants (`const input`, `btn`, `primary`,
`secondary`, `danger` in `InvoiceFields`, `admin/import`, `admin/orders/[id]`, `admin/settings`,
`checkout`); three `ORDER_STATUS_LABELS` imports dropped where `StatusBadge` replaced them; and
the ruled dashboard `alert` flag. Zero logic drift across 14 commits and ~2,600 changed lines.
That does not happen by accident.

**Nothing was retargeted at the markup level either.** A separate diff of every `bind:` target,
`on*=` handler and `{#if}`/`{#each}` expression old-vs-new comes back fully explainable: the
removed `{#if errors.X}` blocks are the ones that became `Field error={…}` props; the removed
`{#if canCheckout}` became `<Button disabled={!canCheckout}>`; the removed
`onclick={() => cart.setQty(line.variant_id, line.qty ± 1)}` pair became `QtyStepper`'s internal
buttons with the old bound preserved as `max={p?.available ? p.stock : 99}`; the `{#if o.paid_at}`
family folded into the `PageHeader` subtitle template. No `bind:` target changed anywhere on the
branch. The one genuine behaviour delta is an improvement the plan intended: typing an
out-of-range quantity in the cart now reaches `cart.setQty` already clamped
(`cart/+page.svelte:112`) instead of raw.

**Copy is preserved, including the parts that look like changes.** The CJK token-set diff shows
only relocations: `數量`/`增加`/`減少` moved from inline markup into `QtyStepper`'s `aria-label`s
(same accessible names, same strings), and `發票` moved from `InvoiceFields`' visible `<legend>`
to the checkout `Card title` while the legend became `sr-only` — visible occurrence count 1
before, 1 after. Everything else flagged was an added HTML comment or a purely additive
`aria-label` (`主選單`, `頁尾選單`, `麵包屑`, `搜尋商品`, `會員中心`, `後台`). One genuine
removal, and it is plan-authored (Minor #10).

**`Field` is the best design call in the branch, and it does more than restyle.** Moving `hint`
and `error` *outside* the `<label>` fixes a real pre-existing defect: the old markup put the
error `<span>` inside the label, so a field in error announced as "縣市 縣市為必填" and
`getByLabel('縣市')` would have broken the moment validation fired. The new structure makes the
Playwright contract *more* robust than it was, not merely preserved. The comment at
`Field.svelte:28` says exactly why, which is why the next person won't undo it.

**Tailwind 4 layering is used correctly, which is the part people usually get wrong.** `.input`
lives in `@layer components` (`app.css:56-57`), so the variant table's
`class="input w-24 py-1.5"` (`ProductForm.svelte:351-355`) overrides cleanly from the utilities
layer without a single `!important`. The `prefers-reduced-motion` block sits in `@layer base`
with `!important`, which correctly beats non-important utilities regardless of layer order.
`:focus-visible` is global and `.input` deliberately swaps it for a focus ring at higher
specificity. The `@theme` token set matches Global Constraint 6 exactly and the four radius tiers
match Constraint 9.

**`Button`'s three-element render is well-reasoned.** `HTMLAttributes<HTMLElement>` rather than
`HTMLButtonAttributes` is the right call for a component that spreads one rest onto `<a>`,
`<span>` and `<button>`, and the comment at `Button.svelte:8-9` says so. Quietly load-bearing:
the `type = 'button'` default is what stops `帶入`, `選擇門市` and `重新確認庫存` from submitting
the checkout form. No call site passes both `href` and `onclick`.

**The e2e contract holds structurally, not just empirically.** I verified each locator against
HEAD markup rather than trusting the green run: `前往結帳` is `<Button href>` → real `<a>` when
enabled (`cart:128`); `加入購物車` appears exactly once in the DOM — the mobile sticky bar and the
desktop panel are the *same* element repositioned (`ProductView:151-158`), not a `md:hidden`
duplicate, which is the trap here; `選擇門市` resolves to exactly `選擇門市` under `exact:true`
because `Icon` is `aria-hidden` and adds no name; `取消訂單` / `確定取消這筆訂單` sit in an
`{#if}/{:else}` so they never coexist, and neither is a substring of the other; `小計` and `總計`
each resolve to one element on the relevant page.

**Backward compatibility and assets are clean.** Every caller of the five pre-existing shared
components is inside the diff and every prop signature is unchanged old-vs-new: `ProductCard`
`{item}` (2 callers), `Pagination` `{page, perPage, total}` (4), `AddressFields`
`{address, errors, prefix}` (2), `InvoiceFields` `{invoice, errors}` (1), `ProductForm`
`{product, categories}` (2), `Toasts` (no props). No consumer was left behind. The favicon is a
300-byte, 3-shape SVG at the same `$lib/assets/favicon.svg` path, so the existing import and
`<link rel="icon">` at `+layout.svelte:7,29` needed no change; `app.html` lost only its two body
classes; there is no `web/static` directory and no new asset of any kind.

Also worth naming: unit tests assert real decisions rather than shape (the ruled `clampQty`
min-over-max sold-out case is a named test, `qty.test.ts:16-19`); `icons.test.ts:11` guards a
*spec intent* ("this is not a pet shop"), which is unusual and good; heading structure is
consistent across all 22 route pages (exactly one `PageHeader`/`h1`, `Card` supplies `h2`, no
skips); no nested `<form>` anywhere; the z-index stack is coherent (toasts 50 > header 40 >
sticky bars 30); and Constraint 15 verifiably holds — both `admin/+layout.svelte` and
`account/+layout.svelte` diffs are class-only with an identical DOM skeleton.

### Issues

#### Critical (Must Fix)

None. No runtime break, no data-loss path, no security issue, no broken e2e locator.

#### Important (Should Fix)

**1. `Field`'s children mode silently drops `aria-invalid` and `aria-describedby` — this is spec
non-compliance, not just a nicety.**

`web/src/lib/components/ui/Field.svelte:32-33` renders `{@render children()}` with no arguments,
so a caller-supplied `<select>`/`<textarea>` can never receive the error wiring. Spec §6 states
the requirement outright: 「所有表單控制項維持真正的 `<label>`…錯誤訊息用 `aria-describedby`
連到欄位」. Six call sites pass an `error` that is rendered but never associated:

- `web/src/lib/components/AddressFields.svelte:28` (縣市) and `:34` (鄉鎮市區)
- `web/src/lib/components/admin/ProductForm.svelte:246` (分類)
- `web/src/routes/admin/settings/+page.svelte:59` (簡介) and `:92` (超商)
- `web/src/routes/checkout/+page.svelte:291` (備註)

Why it matters beyond a11y: `.input` carries `aria-[invalid=true]:border-danger`
(`app.css:57`), so the red-border affordance is missing too. In the checkout address row the
result is visible and inconsistent — a `郵遞區號` error turns its input border red while the
`縣市` select beside it in the same 3-column grid shows red text with a normal border. This is on
the primary conversion path.

Fix (markup-only, no `<script>` logic touched, no new dependency — give the snippet parameters):

```svelte
// Field.svelte
children?: Snippet<[{ errorId: string | undefined; invalid: 'true' | undefined }]>;
...
{#if children}
    {@render children({ errorId, invalid: error ? 'true' : undefined })}
{:else}
```

```svelte
// each of the six error-bearing call sites, e.g. AddressFields.svelte:28
<Field label="縣市" error={err('city')}>
    {#snippet children({ errorId, invalid })}
        <select bind:value={address.city} onchange={onCity} class="input mt-1"
                aria-invalid={invalid} aria-describedby={errorId}>
        ...
    {/snippet}
</Field>
```

**5 files, 6 call sites.** The four *implicit*-children sites that pass no `error`
(載具, 狀態, 常用地址, 商品描述) need no edit — a zero-argument snippet is assignable to
`Snippet<[X]>`. Confirm with `pnpm -C web check`. (This is deferred item 2; elevated from Minor
because the spec asks for it explicitly and the missing red border is user-visible.)

**2. The global header nav breaks mid-word at 375px whenever a user is logged in.**

`web/src/routes/+layout.svelte:24-25` — the `navLink` class string has no `whitespace-nowrap`,
and once the 登出 button plus two icon links are present the row overflows and wraps inside the
words: 「全部商品」 renders as 全部商／品 and 「登出」 as 登／出. Confirmed in
`shots/addresses-mobile.png` and `shots/admin-products-mobile.png`. `overflow=false` passed
because wrapping is exactly how the page avoids a horizontal scrollbar — the gate cannot catch
this, which is why it survived nine task reviews.

This is the site-wide header on every logged-in page at the primary mobile width, so it is the
most-seen defect on the branch. **Fix it as a pair, not with `whitespace-nowrap` alone.** At
375px logged in as admin the row already budgets ≈ 32 (page padding) + 108 (Logo) + 12 (gap) +
226 (nav: 80 + 44 + 44 + 52 + 6) ≈ 378px, so `nowrap` on its own converts a wrap defect into a
real horizontal scrollbar — which Constraint 11 forbids and the gate *will* catch. Apply
`whitespace-nowrap` **and** narrow the link padding to `px-2 md:px-3` in the same string
(−32px → ≈ 346px, fits), then re-shoot `addresses-mobile`.

**3. Cart rows collapse the product name to ~70px at 375px.**

`web/src/routes/cart/+page.svelte:96` — `flex flex-wrap items-center gap-4` with a fixed 80px
thumb, a `shrink-0` 128px `QtyStepper`, a `w-24` line total and a 移除 button. At 375px the
card's inner width is ≈ 309px, so after the thumb + gaps + stepper the `min-w-0 flex-1` info
column gets ≈ 71px: the product name wraps (「木質收納／盒」 in `shots/cart-mobile.png`) and the
price and 移除 fall to a second row, which reads as a layout accident rather than a design.

**The notes' suggested fix (`basis-[calc(100%-6rem)] md:basis-auto` on the info block) does not
survive the arithmetic** — it fixes row 1 but leaves row 2 at 128 + 16 + 96 + 16 + 56 = 312px
against 309px available, so 移除 drops alone to a third row. Use the grouped form instead: keep
the info block full-width on mobile *and* wrap the three trailing controls
(`cart:112-114`) in one row container:

```svelte
<div class="flex basis-full items-center justify-between gap-3 md:basis-auto md:gap-4">
    <QtyStepper … /> <div class="w-24 text-right …">…</div> <Button …>移除</Button>
</div>
```

Row 2 becomes 128 + 12 + 96 + 12 + 56 = 304px ≤ 309px. `md:` and up is unchanged.

#### Minor (Nice to Have)

**4. Dead `hint` prop and dead `.field-hint` class.** `Field.svelte:10,38` and `app.css:62`. No
call site anywhere passes `hint=`. Both come from the spec verbatim. Harmless, but it's the only
fully-unreachable branch in the component library. Either keep it deliberately as API surface or
drop both together — don't drop one.

**5. `Logo`'s `size` prop is never exercised.** `Logo.svelte:2,4,15` — both call sites
(`+layout.svelte:37` header, `:69` footer) use the default `md`, so the `'lg'` path (40px mark,
`text-2xl`) is dead. Speculative flexibility; low cost to leave, but note it never rendered.

**6. `check` and `x` icons are declared and unused.** `icons.ts:20-21`. Spec §3.4 names all
eleven and `icons.test.ts:6` asserts `toHaveLength(11)`, so the test now pins two dead entries in
place. Leave them (spec-mandated), but be aware the test will block their removal.

**7. Dead `eslint-disable` comment.** `Field.svelte:16`. The project has no eslint; carried in
from the plan text. One-line deletion — the `// value 刻意用 any` comment above it already
explains the intent for a human reader.

**8. The products-list selects are the only unlabelled controls left, and the same commit
labelled their neighbour.** `web/src/routes/products/+page.svelte:27` (category) and `:33` (sort)
have no label and no `aria-label`, while the search input in the same form got a new
`<span class="sr-only">搜尋商品</span>` at `:23`. The old code had none of the three labelled, so
the selects aren't a regression — but fixing one of three and leaving two is a new
inconsistency. Two attributes close it: `aria-label="分類"` and `aria-label="排序"`.

**9. Auth-page icon badge sits at 2.0:1 contrast.** `login/+page.svelte:46`, `register:49`,
`forgot-password:34`, `reset/[token]:45` — `text-brand` (#ff8a00) on `bg-brand-soft` (#ffe9cf)
computes to 2.00:1 (`brand-deep` would be 2.96:1, still under the 3:1 WCAG 1.4.11 asks of
non-text graphics). It is `aria-hidden` decorative so it is not a conformance failure, and it is
plan-verbatim, so this is **not** a Constraint 7 violation (that rule is about text). It just
reads washed out. `text-ink` gives 12.5:1 and matches the brand-soft badge treatment used
elsewhere.

**10. The homepage 「分類」 heading was deleted.** `web/src/routes/+page.svelte` — the old
`<h2 class="text-lg font-bold">分類</h2>` has no counterpart in the new hero. The plan's Task 2
markup (plan L727ff) authored it this way, and the same section newly surfaces
`data.shop.description` plus a three-link footer nav and `contact_phone`. Global Constraint 2
says copy may move but not change; deleting a heading and surfacing two previously-unrendered
fields is a content change, not a markup change. **The implementation is faithful — this is a
plan-vs-constraint conflict, not an implementer error.** Flagging so the controller can confirm
it was intentional rather than discover it later.

**11. Hero pattern opacity is 0.40 where the spec says 0.35.** `+page.svelte:13` uses
`opacity-40`; spec §3.4 says 「透明度 0.35」. Tailwind has no `opacity-35` step, which is almost
certainly why. Fine as-is; noting it so spec and code agree on the record.

**12. `account/addresses` is the one page that skips `EmptyState`.**
`web/src/routes/account/addresses/+page.svelte:116` renders a bare
`<p class="mt-4 text-ink-soft">還沒有常用地址。</p>` while the homepage, products, cart and orders
pages all use `<EmptyState>`. Copy is unchanged either way — purely a consistency nit.

**13. `Button`'s rest type is narrower than the spec's.** `Button.svelte:27` uses
`Omit<HTMLAttributes<HTMLElement>, 'class'>`, which excludes button-only attributes; spec §4
lists 「`onclick`、`aria-*`、`form`…」, so `form=` would not type-check. No call site needs it and
the union-of-three-elements reasoning is sound, so leave it — but that's the line to widen if a
future caller needs `form`.

**14. A disabled `Button href` is not reachable by keyboard.** `Button.svelte:48` renders
`<span aria-disabled="true">`. A disabled `<button>` is still in the a11y tree as a disabled
button; a `<span>` is inert text. The only site is 前往結帳 when the cart can't check out
(`cart:128`), where a screen-reader user gets no announcement of *why* the action is
unavailable. Spec §4 prescribes exactly this markup, so it's plan-level. `role="link"` +
`tabindex="-1"` would be a cheap improvement if this ever matters.

#### Pre-existing, verified, and deliberately not counted against this branch

Checked because they look like branch defects; they are not, and the branch should not be asked
to fix them:

- **`getByLabel('地址')` on checkout is substring-matched and would collide.** Labels containing
  地址 on that page: `常用地址` (rendered only when a logged-in user has saved addresses) and
  `發票地址` (only for a company invoice). The e2e always runs as a guest with a personal invoice,
  so it resolves to one element. The structure is byte-for-byte pre-existing
  (`74f13e6:web/src/routes/checkout/+page.svelte:222-233`). Worth knowing if the e2e is ever
  extended to a logged-in flow.
- **`<label>常用地址` wraps a `<select>` *and* the 帶入 `<button>`** (`checkout:224-234`), which
  pollutes the select's accessible name to 「常用地址 帶入」. Identical in the old code. Clicking
  the button is safe — the HTML spec suppresses label activation for interactive descendants.
- **The cart thumbnail `<a>` has no accessible name** — `alt=""` means it has none even *with* an
  image, not only when the thumbnail is missing as deferred item 9 describes (`cart:97-99`).
  Byte-identical to `74f13e6`.
- **`<td colspan="9">` is hardcoded** in the variant table (`ProductForm.svelte:371`) while the
  column count varies with `hasOpt1`/`hasOpt2`. Pre-existing.
- **Variant chips have no `aria-pressed`** — the old `ProductView` used the same
  `<button type="button">` toggle pattern (deferred item 6 below).

### Deferred minors — verdicts

1. **FIX** (trivial) — delete the dead `eslint-disable` at `Field.svelte:16`; the project has no
   eslint and the 刻意用 any comment above already documents the intent. Bundle it with the fix
   wave item 12 already forces.
2. **FIX** — elevated to Important #1. Spec §6 requires the `aria-describedby` link, and the
   missing `aria-invalid` also costs the red border on six controls. Concrete patch above:
   `children` becomes `Snippet<[{errorId, invalid}]>`, destructured at the six error-bearing call
   sites; the four implicit-children sites need no edit. Markup-only, constraint 3 intact.
3. **LEAVE** — no change. Svelte 5's `bind_value` decides number coercion at runtime from
   `input.type`, so a spread `type` is fine, and the controller round-tripped `sort_order`
   (99→99, 2→2) through `check-form.mjs`. Settled; don't relitigate.
4. **LEAVE** — `vite.config.ts:7`'s ambient `declare const process` is fine as-is. To be precise
   about the trade-off: `loadEnv()` would *also* read the repo-root `.env`, which would actually
   bring the proxy closer to SSR (SSR reads `.env` via `$env/dynamic/private` in dev), not
   further from it. The code comment's 「沿用 SSR 用的 API_INTERNAL_URL」 is therefore true only
   for the shell-variable path. Still LEAVE: this is a dev-only knob, the workflow is documented,
   and the controller ruled it (Ruling 3).
5. **FIX** (2 attributes) — see Minor #8. `aria-label="分類"` / `aria-label="排序"` on
   `products/+page.svelte:27,33`. Not a regression, but the same commit labelled the search input
   and left these two.
6. **LEAVE** (with recommendation) — verified against `74f13e6:ProductView.svelte:98-125`: the old
   code used the identical `<button type="button">` toggle pattern with no `aria-pressed`, so
   this is not a branch regression and the plan text didn't add it. If a fix wave happens,
   `aria-pressed={sel1 === value}` on `ProductView.svelte:111` and `aria-pressed={sel2 === value}`
   on `:130` is one attribute each and makes the chip group announce correctly.
7. **FIX** — elevated to Important #3, but **not with the suggested change**: I ran the widths and
   the notes' `basis-[calc(100%-6rem)]` leaves row 2 at 312px against 309px available, pushing
   移除 to a third row. Use the grouped-row form given above.
8. **LEAVE** — Ruling 4 stands. The cart page's toasts are validation-failure only and the
   container is `pointer-events-none`, so the overlap is transient and non-blocking. If it ever
   needs fixing, the clean form is a CSS variable for the sticky-bar height, not another
   hardcoded `bottom-*` step.
9. **LEAVE** (with recommendation) — pre-existing and actually broader than the item describes:
   `alt=""` leaves the link nameless even when a thumbnail exists. Since the adjacent
   product-name `<a>` on the same row already points to the same href, the correct fix is
   `aria-hidden="true" tabindex="-1"` on the thumbnail link (removing a duplicate tab stop), not
   an `aria-label`. Out of scope for this branch.
10. **FIX** — elevated to Important #2, and it must be the *two-part* fix (`whitespace-nowrap`
    **plus** `px-2 md:px-3`): `nowrap` alone trades the wrap for a ~378px row and a real
    horizontal scrollbar.
11. **LEAVE** — do not extract `AuthHeader`. It is three lines of markup with no logic, and
    CLAUDE.md §2 explicitly says not to abstract for flexibility that wasn't requested. Four
    copies of three lines is cheaper to read than a component. (The `text-brand` contrast inside
    that markup is a separate, real point — Minor #9.)
12. **FIX** — acknowledged, Ruling 7. `admin/orders/[id]/+page.svelte:14`: drop
    `ORDER_STATUS_LABELS` from the import list; the other six label maps on that line are all
    still used (I checked each). My unused-import scan over all of `web/src` found this as the
    **only** genuinely unused named import on the branch, so it's a one-line change.
13. **LEAVE** — acknowledged, Ruling 9. Accepted, not a finding.

### Recommendations

**Process — the gate has a known blind spot, and it just produced two findings.** `overflow=false`
at 375px proves no horizontal scrollbar, which is precisely what text wrapping *achieves*.
Findings 2 and 3 are both "the layout avoided overflow by degrading", and both survived nine
task-scoped reviews because the automated check reported green. If the screenshot script gains
one more assertion, make it a wrap check on the header nav and the cart row — e.g. compare each
nav link's `getBoundingClientRect().height` against its single-line height, and assert the cart
row's info column is wider than some floor. Cheap, and it closes the exact hole. Note that the
same script *will* catch a regression from fix #2 if the padding half is omitted, so re-shoot
`addresses-mobile` after that one specifically.

**Testing — do not ask for component render tests here.** `vite.config.ts` sets
`test: { environment: 'node' }` and there is no `jsdom`/`happy-dom` in `web/package.json`, so
rendering `Button`'s three branches or `Field`'s label-wrap is impossible without adding a
dependency, which Constraint 1 forbids. The 47 tests correctly cover exactly what is testable in
a node environment (the three pure modules), and Playwright is the only real guard on markup
structure. The honest gap, if the team ever relaxes Constraint 1, is a DOM test asserting that
`Field` keeps `hint`/`error` outside the `<label>` — that invariant is load-bearing for the
entire `getByLabel` contract and is currently protected by nothing but a code comment.

**Sequencing for the fix wave.** Items 12 (ruled), 1, 2, 5, 10 and the two layout fixes are
independent and all markup-or-import-only. Do them as one commit, re-run `check` / `test` /
`build` / e2e, and re-shoot only `cart-mobile`, `addresses-mobile`, `checkout-mobile` and
`admin-settings-mobile` — the four that actually change. Nothing in this list needs a plan
amendment except Minor #10, which needs a controller ruling rather than a code change.

**Spec/plan bookkeeping.** Two items belong in the spec rather than the code: Minor #10 (the plan
authored content changes that Constraint 2 forbids — either amend Constraint 2 to allow content
the spec introduces, or restore 分類) and Minor #11 (0.35 vs `opacity-40`). Also, spec §7 lists
`products/[slug]/+page.svelte` as modified; it correctly wasn't, because it only renders
`<ProductView>` and needed no change. Not a gap — just an over-broad file list.

### Assessment

**Ready to merge?** With fixes

**Reasoning:** There are no critical defects — the e2e contract holds structurally (each locator
verified against HEAD markup, not just the green run), `<script>` logic, `bind:` targets and copy
are provably untouched branch-wide, and the component library is genuinely well-built with
correct Tailwind 4 layering and one design decision (`Field` moving errors outside the `<label>`)
that fixes a pre-existing a11y bug rather than merely preserving it. The three Important items
are small and localized: one snippet-signature change plus six call-site destructures for the
`aria-describedby`/`aria-invalid` gap spec §6 requires, a two-part header fix, and a grouped row
on the cart line — all markup-only, no new dependency, no `<script>` change. Fold those in with
the already-ruled item 12 and this merges cleanly.


---

## 附錄 A：控制者裁決（Ruling 1–14）

每條格式：決定 — 依據 — 若錯的代價。

1. **Ruling 1（Task 1）** `clampQty` 先夾 max 再夾 min（min 優先）— 已售完時 max=0＜min=1，原順序會讓數量框顯示 0；規格 §5.5 已售完只要按鈕半透明 — 若錯：只影響已售完商品的數量框顯示（按鈕本來就 disabled）。
2. **Ruling 2（Task 1）** QtyStepper 焦點框：計畫原文 `focus:outline-none` 加容器 `overflow-hidden` 讓鍵盤焦點看不到，違反 Global Constraint 10「焦點框由 app.css 全域處理」— 規格優先：輸入框改用 `.input` 同款焦點、容器拿掉 `overflow-hidden`、減號鈕 `rounded-l-full`、加號鈕 `rounded-r-full`（修正 `e86dfdf`）— 若錯：只影響一個元件外觀。
3. **Ruling 3（Task 2）** 開發用 API 改跑 `:8081`，不停別人的容器（8080 被其他專案的 `mtb-mock-frontend` 占用）— 全部走環境變數（`LISTEN_ADDR`、`API_INTERNAL_URL`、`E2E_API_URL` 皆已支援），唯一改碼是 `web/vite.config.ts` 的 dev proxy 改讀 `process.env.API_INTERNAL_URL`（`523ffd5`）— 若錯：只影響本機 dev proxy，正式環境不走 Vite。
4. **Ruling 4（Task 4／7）** Toasts 容器 `bottom-24 md:bottom-6`，手機避開商品頁黏底動作列 — 若錯：只影響 toast 位置。
5. **Ruling 5（Task 5）** 購物車外層 `mt-4 grid …` 改成 `mt-4 flex flex-col gap-6 lg:grid lg:grid-cols-[1fr_20rem]`，手機黏底摘要卡的 containing block 才包含整個列表、sticky 才有位移空間（`6cf2bd3`，375×812 六件商品實測 sticky=true）— 若錯：一個 class 可還原。
6. **Ruling 6（流程）** pnpm 10 下 `pnpm -C web test:e2e -- --grep "X"` 不會過濾；要用 `pnpm -C web test:e2e --grep "X"`；Task 9 全跑不加 `--grep`。
7. **Ruling 7（Task 8）** 死 import `ORDER_STATUS_LABELS`（`admin/orders/[id]/+page.svelte:14`）留到最終修正波一併刪，不為單行 hygiene 開修正輪 — 已於 `617cf06` 刪除。
8. **Ruling 8（Task 8）** 後台訂單頁「← 訂單列表」改成 chevron 圖示＋「訂單列表」符合 Constraint 2「不改文案」— 箭頭是裝飾字元、字串不變、brief 明載 — 若錯：少一個箭頭字元。
9. **Ruling 9（Task 8）** 375px 後台 nav 橫滑條第 6 顆「設定」預設在畫面外 → 接受 — 計畫原文、與會員中心 layout 同款、頁面 overflow=false — 若錯：手機上「設定」較難發現。
10. **Ruling 10（流程）** Task 9 沒有 commit 時由控制者直接核對報告與截圖，不派空 diff 審查者。
11. **Ruling 11（流程）** codex 第二意見與 opus 最終審查同時對 `40dc8cb` 跑，兩邊 findings 合併進同一波修正。
12. **Ruling 12（流程）** codex 對 `40dc8cb` 撞到 OpenAI 用量上限（22:33 重置）、沒有產出 → 改在修正波之後對最終 HEAD 重跑；再撞上限則略過並請使用者自行跑 `/codex-review-fix`。
13. **Ruling 13（最終審查 Minor #10）** 首頁舊 `<h2>分類</h2>` 拿掉、看板露出 `data.shop.description`、頁尾露出 `contact_phone` → 接受 — 計畫 Task 2 原文如此，且使用者在看圖關卡對首頁截圖回「可以」；Constraint 2「不改文案」針對按鈕／標籤／提示／錯誤／空狀態字串 — 若錯：補回一行 h2、拿掉兩個欄位的顯示。
14. **Ruling 14（修正波）** `Field.svelte` 的 `Omit<HTMLInputAttributes, 'value' | 'class'>` 多排除 `'children'` — `svelte/elements` 的 `DOMAttributes` 自帶無參數 `children?: Snippet`，不排除會與帶參數的 `children` 交集成型別衝突，六個呼叫端都過不了 svelte-check；`Field` 從未把 children 展開到 `<input>` 上 — 若錯：型別而已，無執行期影響。

各 task 審查留下、最終審查判 LEAVE 的小項（不修、記錄在案）：`Field` 的 `hint` prop 與 `.field-hint` 沒人用（規格原文）；`Logo` 的 `size='lg'` 沒人用；`check`／`x` icon 沒人用（規格 11 顆、測試釘住）；登入／註冊／忘記／重設四頁 icon badge `text-brand` 於 `brand-soft` 上 2.0:1（裝飾、`aria-hidden`、計畫原文；可改 `text-ink`）；看板圖案 `opacity-40` vs 規格 0.35（Tailwind 無 35 階）；`account/addresses` 空狀態沒用 `EmptyState`；`Button` rest 型別不含 `form`；disabled `Button href` 是 `<span>`（規格原文）；規格膠囊沒有 `aria-pressed`（舊碼同款）；購物車縮圖 `<a>` 沒有可讀名稱（舊碼同款，建議 `aria-hidden="true" tabindex="-1"`）；`vite.config.ts` 的 `declare const process`。

## 附錄 B：修正波與範圍複審

**修正波 `617cf06`**（BASE `40dc8cb`；9 個檔，+44／−31；一個 commit）：

1. Important 1：`Field` 的 children snippet 改成 `Snippet<[{ errorId, invalid }]>`，`{@render children({ errorId, invalid: error ? 'true' : undefined })}`；六個傳 `error=` 的呼叫端（`AddressFields` 縣市／鄉鎮市區、`ProductForm` 分類、`admin/settings` 簡介／超商、`checkout` 備註）改成顯式 `{#snippet children({ errorId, invalid })}` 並在 `<select>`／`<textarea>` 接上 `aria-invalid`／`aria-describedby`；四個沒傳 `error=` 的隱式 children 呼叫端不動；`Omit` 多排除 `'children'`（Ruling 14）。
2. Important 2：根 layout `navLink` 加 `whitespace-nowrap` **且** `px-3` → `px-2 md:px-3`（只加 nowrap 會讓 375px 登入列約 378px、出現真的水平捲軸）。
3. Important 3：購物車列尾端的 QtyStepper、小計、移除包進 `flex basis-full items-center justify-between gap-3 md:basis-auto md:gap-4`，手機整列、md 以上不變（不用 `basis-[calc(100%-6rem)]`，第二列會塞不下）。
4. 刪 `Field.svelte` 死的 `eslint-disable` 註解（專案沒裝 eslint）。
5. 商品列表分類／排序 `<select>` 加 `aria-label="分類"`／`"排序"`。
6. 刪 `admin/orders/[id]/+page.svelte` 沒人用的 `ORDER_STATUS_LABELS` import（Ruling 7）。

實作者第一回合回報 NEEDS_CONTEXT（型別衝突，見 Ruling 14），裁決後同一實作者接續完成。

**實作者驗證**：`pnpm -C web check` 333 files 0 errors 0 warnings；`pnpm -C web test` 47 passed；`pnpm -C web build` 成功；`web/package.json`／`pnpm-lock.yaml` 對 main 無 diff；`E2E_API_URL=http://localhost:8081 pnpm -C web test:e2e` 2 passed；八組頁面（首頁、商品列表、購物車、結帳、常用地址、後台商品、後台商品編輯、後台設定）375×812 與 1280×800 皆 overflow=false；`fix-addresses-mobile.png`／`fix-admin-products-mobile.png` 目視「全部商品」「登出」單行；`check-cart-row.mjs` 在 375px 對減號鍵與「移除」鍵取 boundingBox → `CART_ROW_OK y1=251 y2=254`。

**範圍複審（sonnet，`40dc8cb..617cf06`）**：六條 finding 全部 ADDRESSED（逐條 file:line 佐證）；New Breakage: None；Out-of-Scope: None；確認四個隱式 children 呼叫端未動、`bind:value`／`onchange`／`onclick`／`max=` 表達式與文案一字未變、只動 9 個預期檔案、`Omit` 只多一個 token、commit 署名正確；獨立目視兩張頂欄 PNG。

**控制者對 `617cf06` 的整支 gate**：deps diff 0 行、舊色（`gray-*`／`red-*`／`green-*`／`yellow-*`／`amber-*`／`blue-*`／`font-mono`）0 筆、check 0/0、vitest 47/47、build 成功、e2e 2 passed（9.2s）、伺服器收乾淨（8081／5173 皆空）。

## 附錄 C：codex 第二意見

**未取得。** 依使用者規則用 `codex exec -s read-only` 對整支分支跑第二意見，兩次都在讀完檔案清單後被 OpenAI 用量上限擋下、沒有產出任何 finding：

- 第一次對 `74f13e6..40dc8cb`（與 opus 最終審查同時，Ruling 11）：38,886 tokens 後 `You've hit your usage limit … try again at 10:33 PM`。
- 第二次對 `74f13e6..617cf06`（22:34 額度重置後）：16,349 tokens 後再次撞上限，下次重置 `Sep 15th, 2026 3:30 AM`。

依 Ruling 12 略過。請在額度恢復後於分支 `worktree-ui-redesign` 自行執行 `/codex-review-fix`（codex 設定檔是 `danger-full-access`，記得加 `-s read-only`）；若有 P1／P2 再開一波修正並重跑 `pnpm -C web check`／`test`／`build` 與 Playwright e2e。

## 附錄 D：開發環境備忘（給下一個 session）

- 本機 API 用 `LISTEN_ADDR=0.0.0.0:8081`（8080 被其他專案的容器占用，Ruling 3）；web dev 與 e2e 用 `API_INTERNAL_URL=http://localhost:8081`／`E2E_API_URL=http://localhost:8081`；Vite 8 只綁 `::1`，一律用 `http://localhost:5173`。
- e2e 過濾要寫 `pnpm -C web test:e2e --grep "<名稱>"`；中間多一個 `--` 就不會過濾（Ruling 6）。
- 開發 DB（compose 專案 `dog_shop`，port 5435）目前的示意／測試雜物，只在開發 DB、正式環境沒有：`demo-ui-1..8` 商品（2 = 帆布托特包，規格米白／深藍；3 有原價；7 庫存 0）、分類「生活雜貨」「文具小物」、`shot@test.local` 的待付款訂單 ×2（Task 7、Task 9 各一）、e2e 留下的「E2E 狗糧 <時間戳>」商品與取消訂單 ×5 次（Task 6 兩次、Task 9、修正波、最終 gate 各一）。
- 截圖與實測腳本（`shot.mjs`、`shot-order.mjs`、`check-form.mjs`、`check-cart-row.mjs`）與 gate 腳本都在 job 暫存目錄、不在 repo；要重跑就照計畫 Task 9 的步驟。
- 本計畫的 SDD 工作區（`.superpowers/sdd/2026-09-09-ui-redesign/`，git 忽略）在結案時刪除；各 task 的 brief／report／審查包不再保留，裁決見附錄 A。
