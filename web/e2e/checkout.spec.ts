import { expect, test, type APIRequestContext } from '@playwright/test';

const API = process.env.E2E_API_URL ?? 'http://localhost:8080';
const ORIGIN = process.env.E2E_BASE_URL ?? 'http://localhost:5173';
const ADMIN = {
	email: process.env.E2E_ADMIN_EMAIL ?? 'admin@example.com',
	password: process.env.E2E_ADMIN_PASSWORD ?? 'admin12345'
};
// api 的 CSRF 檢查：變更請求要帶這兩個 header
const HEADERS = { 'x-requested-with': 'fetch', origin: ORIGIN };

/** 用後台 API 建一個上架商品，回 slug 與名稱 */
async function seedProduct(request: APIRequestContext): Promise<{ slug: string; name: string }> {
	const login = await request.post(`${API}/api/auth/login`, { headers: HEADERS, data: ADMIN });
	expect(login.ok(), `admin 登入失敗：${login.status()} ${await login.text()}`).toBeTruthy();
	const name = `E2E 狗糧 ${Date.now()}`;
	const created = await request.post(`${API}/api/admin/products`, {
		headers: HEADERS,
		data: { name, status: 'active', variants: [{ price: 300, stock: 5 }], images: [] }
	});
	expect(created.status(), await created.text()).toBe(201);
	const body = await created.json();
	return { slug: body.slug, name };
}

test('瀏覽 → 加入購物車 → 結帳（宅配）→ 訂單頁', async ({ page, request }) => {
	const product = await seedProduct(request);

	await page.goto(`/products/${product.slug}`);
	await expect(page.getByRole('heading', { name: product.name })).toBeVisible();
	// 商品頁的按鈕在 SSR 就存在（不像購物車／結帳頁要等 client-only 的 cart.loaded），
	// 第一次從整頁導覽過來時 click 可能搶在 hydration 掛上事件監聽器之前發生；重試直到成功
	await expect(async () => {
		await page.getByRole('button', { name: '加入購物車' }).click();
		await expect(page.getByText('已加入購物車')).toBeVisible({ timeout: 2_000 });
	}).toPass({ timeout: 15_000 });

	await page.goto('/cart');
	await expect(page.getByText(product.name)).toBeVisible();
	await expect(page.getByText('小計')).toBeVisible();
	await page.getByRole('link', { name: '前往結帳' }).click();
	await expect(page).toHaveURL(/\/checkout$/);

	// 'Email'、'手機' 用寬鬆比對會同時吃到發票「載具」select 的選項文字（綠界會員載具…Email、…手機條碼），改用更精準的比對
	await page.getByLabel(/^Email/).fill('e2e@test.local');
	await page.getByLabel('收件人').fill('王小明');
	await page.getByLabel('手機', { exact: true }).fill('0912345678');
	await page.getByLabel('縣市').selectOption('臺北市');
	await page.getByLabel('鄉鎮市區').selectOption('中正區');
	await expect(page.getByLabel('郵遞區號')).toHaveValue('100');
	await page.getByLabel('地址').fill('重慶南路一段 122 號');
	await expect(page.getByText('總計')).toBeVisible();
	await page.getByRole('button', { name: '送出訂單' }).click();

	await expect(page).toHaveURL(/\/orders\/[0-9a-f-]{36}\?t=[0-9a-f]{64}$/);
	// 純文字比對會同時吃到 SvelteKit 導覽時寫入的 #svelte-announcer（也含訂單編號），改比對標題本身
	await expect(page.getByRole('heading', { name: /DS\d{6}[A-Z0-9]{4}/ })).toBeVisible();
	await expect(page.getByText('待付款')).toBeVisible();
	await expect(page.getByText(product.name)).toBeVisible();
	await expect(page.getByText('重慶南路一段 122 號')).toBeVisible();

	// 取消 → 狀態變已取消
	await page.getByRole('button', { name: '取消訂單' }).click();
	await page.getByRole('button', { name: '確定取消這筆訂單' }).click();
	await expect(page.getByText('已取消', { exact: false }).first()).toBeVisible();
});
