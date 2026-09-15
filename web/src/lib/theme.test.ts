/// <reference types="vite/client" />
import { describe, expect, it } from 'vitest';

// 規格 §3.1 的對比規則（WCAG AA）：文字 4.5:1、焦點框等非文字 3:1。
// 直接讀 app.css 的 @theme token 來算，token 改色時這裡會先擋下來（codex 第二意見 P2：hover 對比、焦點框在橘看板上消失）。
// vitest 預設把 .css 匯入（含 ?raw）換成空字串，所以用 fs 讀；專案沒裝 @types/node，用動態 import 避開型別。
const { readFileSync } = (await import(/* @vite-ignore */ 'node:' + 'fs')) as {
	readFileSync: (path: URL, encoding: 'utf8') => string;
};
const css = readFileSync(new URL('../app.css', import.meta.url), 'utf8');
const tokens: Record<string, string> = Object.fromEntries(
	[...css.matchAll(/--color-([\w-]+):\s*(#[0-9a-fA-F]{6})/g)].map((m) => [m[1], m[2]])
);

function luminance(hex: string): number {
	const channel = (c: number) => {
		const s = c / 255;
		return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
	};
	const n = parseInt(hex.slice(1), 16);
	return 0.2126 * channel(n >> 16) + 0.7152 * channel((n >> 8) & 255) + 0.0722 * channel(n & 255);
}

function contrast(a: string, b: string): number {
	const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
	return (hi + 0.05) / (lo + 0.05);
}

describe('app.css 色彩 token 對比（規格 §3.1 WCAG AA）', () => {
	it('有讀到 token', () => {
		expect(tokens.ink).toBeDefined();
		expect(tokens.brand).toBeDefined();
		expect(tokens['brand-deep']).toBeDefined();
	});
	it('主按鈕：深藍字在 brand 底與 hover 的 brand-deep 底都 ≥ 4.5:1（按鈕字 14–16px，不算大字）', () => {
		expect(contrast(tokens.ink, tokens.brand)).toBeGreaterThanOrEqual(4.5);
		expect(contrast(tokens.ink, tokens['brand-deep'])).toBeGreaterThanOrEqual(4.5);
	});
	it('全域焦點框用 ink：在白底、surface、淡橘、首頁看板的橘底上都 ≥ 3:1（橘框在橘看板上會消失）', () => {
		expect(css).toMatch(/:focus-visible\s*\{[^}]*outline:\s*2px solid var\(--color-ink\)/);
		for (const bg of ['ground', 'surface', 'brand-soft', 'brand']) {
			expect(contrast(tokens.ink, tokens[bg]), `ink on ${bg}`).toBeGreaterThanOrEqual(3);
		}
	});
	it('.input 的 focus 邊框用 ink（橘色在白底只有 2.36:1）', () => {
		expect(css).toMatch(/\.input\s*\{[^}]*focus:border-ink/);
	});
});
