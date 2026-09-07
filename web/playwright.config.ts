import { defineConfig } from '@playwright/test';

/** 假設 api（:8080）與 web dev（:5173）已經在跑；不進 CI（與規格不同之處 17） */
export default defineConfig({
	testDir: 'e2e',
	timeout: 60_000,
	retries: 0,
	reporter: 'list',
	use: {
		baseURL: process.env.E2E_BASE_URL ?? 'http://localhost:5173',
		headless: true,
		locale: 'zh-TW'
	}
});
