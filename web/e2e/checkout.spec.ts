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

test('瀏覽 → 加入購物車 → 結帳（宅配）→ 送往綠界的表單 → 訂單頁', async ({ page, request }) => {
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

	// 攔截送往綠界的頂層表單 POST：不真的去綠界，回一頁假的；欄位用 postData 檢查（規格 §15）
	await page.route('https://payment-stage.ecpay.com.tw/**', (route) =>
		route.fulfill({
			status: 200,
			contentType: 'text/html; charset=utf-8',
			body: '<!doctype html><title>ECPay stub</title><p>ECPay stub</p>'
		})
	);
	const ecpayRequest = page.waitForRequest(
		(r) => r.url().includes('/Cashier/AioCheckOut/V5') && r.method() === 'POST'
	);
	await page.getByRole('button', { name: '送出訂單' }).click();
	const fields = new URLSearchParams((await ecpayRequest).postData() ?? '');
	expect(fields.get('MerchantID')).toBe('3002607');
	expect(fields.get('ChoosePayment')).toBe('Credit');
	expect(fields.get('PaymentType')).toBe('aio');
	expect(fields.get('EncryptType')).toBe('1');
	expect(fields.get('MerchantTradeNo')).toMatch(/^DS\d{6}[A-Z0-9]{4}01$/);
	expect(fields.get('TotalAmount')).toMatch(/^\d+$/);
	expect(fields.get('CheckMacValue')).toMatch(/^[0-9A-F]{64}$/);
	expect(fields.get('ReturnURL')).toMatch(/\/api\/ecpay\/payment\/return$/);
	expect(fields.get('PaymentInfoURL')).toMatch(/\/api\/ecpay\/payment\/info$/);
	const backUrl = fields.get('ClientBackURL') ?? '';
	expect(backUrl).toMatch(/\/orders\/[0-9a-f-]{36}\?t=[0-9a-f]{64}$/);
	await expect(page.getByText('ECPay stub')).toBeVisible();

	// 買家從綠界回來（ClientBackURL）
	await page.goto(backUrl);
	const orderNo = (fields.get('MerchantTradeNo') ?? '').slice(0, -2);
	// 純文字比對會同時吃到 SvelteKit 導覽時寫入的 #svelte-announcer（也含訂單編號），改比對標題本身
	await expect(page.getByRole('heading', { name: orderNo })).toBeVisible();
	await expect(page.getByText('待付款')).toBeVisible();
	await expect(page.getByText(product.name)).toBeVisible();
	await expect(page.getByText('重慶南路一段 122 號')).toBeVisible();

	// 取消 → 狀態變已取消。這裡也是整頁導覽（ClientBackURL）過來，「取消訂單」按鈕 SSR 就存在，
	// click 可能搶在 hydration 掛上 onclick 之前（冷的 vite dev 每次都會發生）；重試直到確認按鈕出現
	await expect(async () => {
		await page.getByRole('button', { name: '取消訂單' }).click();
		await expect(page.getByRole('button', { name: '確定取消這筆訂單' })).toBeVisible({ timeout: 2_000 });
	}).toPass({ timeout: 15_000 });
	await page.getByRole('button', { name: '確定取消這筆訂單' }).click();
	await expect(page.getByText('已取消', { exact: false }).first()).toBeVisible();
});

test('超商取貨：選擇門市 → 綠界地圖表單 → 門市回傳 → 結帳頁顯示門市 → 送出訂單', async ({ page, request }) => {
	const product = await seedProduct(request);

	await page.goto(`/products/${product.slug}`);
	await expect(async () => {
		await page.getByRole('button', { name: '加入購物車' }).click();
		await expect(page.getByText('已加入購物車')).toBeVisible({ timeout: 2_000 });
	}).toPass({ timeout: 15_000 });

	await page.goto('/checkout');
	await page.getByLabel(/^Email/).fill('e2e-cvs@test.local');
	await page.getByLabel('收件人').fill('王小明');
	await page.getByLabel('手機', { exact: true }).fill('0912345678');
	await page.getByRole('radio', { name: /超商取貨/ }).check();
	await page.getByRole('radio', { name: '全家' }).check();

	// 攔截送往綠界電子地圖的頂層表單 POST（規格 §8.3：測試環境本來就不顯示地圖）
	await page.route('https://logistics-stage.ecpay.com.tw/**', (route) =>
		route.fulfill({
			status: 200,
			contentType: 'text/html; charset=utf-8',
			body: '<!doctype html><title>ECPay map stub</title><p>ECPay map stub</p>'
		})
	);
	const mapRequest = page.waitForRequest((r) => r.url().includes('/Express/map') && r.method() === 'POST');
	await page.getByRole('button', { name: '選擇門市', exact: true }).click();
	const mapFields = new URLSearchParams((await mapRequest).postData() ?? '');
	expect(mapFields.get('MerchantID')).toBe('2000933');
	expect(mapFields.get('LogisticsType')).toBe('CVS');
	expect(mapFields.get('LogisticsSubType')).toBe('FAMIC2C');
	expect(mapFields.get('IsCollection')).toBe('N');
	expect(mapFields.get('ServerReplyURL')).toMatch(/\/api\/ecpay\/logistics\/map-reply$/);
	expect(mapFields.has('CheckMacValue')).toBe(false);
	const token = mapFields.get('ExtraData') ?? '';
	expect(token).toMatch(/^[A-Za-z0-9]{20}$/);
	expect(mapFields.get('MerchantTradeNo')).toBe(token);
	await expect(page.getByText('ECPay map stub')).toBeVisible();

	// 模擬綠界把門市 POST 回 map-reply（form-urlencoded、沒有我們的 CSRF header）；303 不要自動跟
	const reply = await request.post(`${API}/api/ecpay/logistics/map-reply`, {
		maxRedirects: 0,
		form: {
			MerchantID: '2000933',
			MerchantTradeNo: token,
			LogisticsSubType: 'FAMIC2C',
			CVSStoreID: '006598',
			CVSStoreName: '全家測試店',
			CVSAddress: '台北市中正區重慶南路一段 122 號',
			CVSTelephone: '0223456789',
			CVSOutSide: '0',
			ExtraData: token
		}
	});
	expect(reply.status()).toBe(303);
	const location = reply.headers()['location'] ?? '';
	expect(location).toMatch(new RegExp(`/checkout\\?store=${token}$`));

	// 回到結帳頁：門市顯示出來、sessionStorage 的草稿還原（同一個分頁、同一個 origin）
	await page.goto(location);
	await expect(page.getByText('全家測試店')).toBeVisible();
	await expect(page.getByLabel(/^Email/)).toHaveValue('e2e-cvs@test.local');
	await expect(page.getByRole('radio', { name: '全家' })).toBeChecked();
	await expect(page.getByText('總計')).toBeVisible();

	// 送出訂單 → 攔到送往綠界金流的表單；訂單頁顯示門市
	await page.route('https://payment-stage.ecpay.com.tw/**', (route) =>
		route.fulfill({ status: 200, contentType: 'text/html; charset=utf-8', body: '<!doctype html><title>ECPay stub</title><p>ECPay stub</p>' })
	);
	const aioRequest = page.waitForRequest((r) => r.url().includes('/Cashier/AioCheckOut/V5') && r.method() === 'POST');
	await page.getByRole('button', { name: '送出訂單' }).click();
	const aio = new URLSearchParams((await aioRequest).postData() ?? '');
	expect(aio.get('MerchantTradeNo')).toMatch(/^DS\d{6}[A-Z0-9]{4}01$/);
	await page.goto(aio.get('ClientBackURL') ?? '');
	await expect(page.getByText('全家測試店')).toBeVisible();
	await expect(page.getByText('待付款')).toBeVisible();
});
