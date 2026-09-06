/// <reference types="vitest/config" />
import { sveltekit } from '@sveltejs/kit/vite';
import tailwindcss from '@tailwindcss/vite';
import { defineConfig } from 'vite';

export default defineConfig({
	plugins: [tailwindcss(), sveltekit()],
	// .env 放在 repo 根目錄，api 與 web 共用
	envDir: '..',
	server: {
		proxy: {
			'/api': 'http://localhost:8080',
			'/uploads': 'http://localhost:8080'
		}
	},
	test: {
		include: ['src/**/*.test.ts'],
		environment: 'node'
	}
});
