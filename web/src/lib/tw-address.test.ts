import { describe, expect, it } from 'vitest';
import { cities, districts, postalCode } from './tw-address';

describe('tw-address', () => {
	it('has all 22 cities and counties', () => {
		expect(cities()).toHaveLength(22);
		expect(cities()[0]).toBe('臺北市');
	});
	it('looks up districts and postal codes', () => {
		expect(districts('臺北市')).toContain('中正區');
		expect(postalCode('臺北市', '中正區')).toBe('100');
		expect(postalCode('高雄市', '鳳山區')).toBe('830');
		expect(postalCode('連江縣', '南竿鄉')).toBe('209');
		expect(postalCode('火星', '陨石區')).toBe('');
		expect(districts('火星')).toEqual([]);
	});
	it('every postal code is 3 digits', () => {
		for (const city of cities()) {
			for (const d of districts(city)) expect(postalCode(city, d)).toMatch(/^\d{3}$/);
		}
	});
});
