import data from './data/tw-districts.json';

const cityMap = data as Record<string, Record<string, string>>;

/** 縣市列表（JSON 的順序：北到南、再離島） */
export function cities(): string[] {
	return Object.keys(cityMap);
}

export function districts(city: string): string[] {
	return Object.keys(cityMap[city] ?? {});
}

/** 找不到回空字串（表單會讓使用者自己填） */
export function postalCode(city: string, district: string): string {
	return cityMap[city]?.[district] ?? '';
}
