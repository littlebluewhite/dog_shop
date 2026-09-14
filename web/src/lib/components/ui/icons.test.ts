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
