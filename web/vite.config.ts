/// <reference types="vitest/config" />
import { sveltekit } from '@sveltejs/kit/vite';
import tailwindcss from '@tailwindcss/vite';
import { defineConfig } from 'vite';

// 專案沒裝 @types/node；用最小 ambient 宣告存取 Node 的 process.env，避免 svelte-check 報錯
declare const process: { env: Record<string, string | undefined> };

// 開發時瀏覽器打同源 /api、/uploads，由 Vite 轉到後端；位址沿用 SSR 用的 API_INTERNAL_URL（shell 環境變數），預設 :8080
const apiTarget = process.env.API_INTERNAL_URL ?? 'http://localhost:8080';

export default defineConfig({
	plugins: [tailwindcss(), sveltekit()],
	// .env 放在 repo 根目錄，api 與 web 共用
	envDir: '..',
	server: {
		proxy: {
			'/api': apiTarget,
			'/uploads': apiTarget
		}
	},
	test: {
		include: ['src/**/*.test.ts'],
		environment: 'node'
	}
});
