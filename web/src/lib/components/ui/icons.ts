export type IconName =
	| 'tag'
	| 'cart'
	| 'user'
	| 'search'
	| 'check'
	| 'x'
	| 'minus'
	| 'plus'
	| 'chevron-left'
	| 'chevron-right'
	| 'image';

/** 手寫的 24×24、2px 線、currentColor 圖示（規格 §3.4）；每個是一條 path 的 d（可含多段） */
export const ICON_PATHS: Record<IconName, string> = {
	tag: 'M3 3h8.6a2 2 0 0 1 1.4.6l7.4 7.4a2 2 0 0 1 0 2.8l-6.6 6.6a2 2 0 0 1-2.8 0L3.6 13A2 2 0 0 1 3 11.6V3zM7.5 6.5a1 1 0 1 0 0 2 1 1 0 1 0 0-2',
	cart: 'M3 4h2l2.4 11.2a2 2 0 0 0 2 1.6h7.8a2 2 0 0 0 2-1.5L21 8H7M9 21a1 1 0 1 0 0-2 1 1 0 0 0 0 2M18 21a1 1 0 1 0 0-2 1 1 0 0 0 0 2',
	user: 'M20 21a8 8 0 0 0-16 0M12 13a4 4 0 1 0 0-8 4 4 0 0 0 0 8',
	search: 'm21 21-4.3-4.3M11 18a7 7 0 1 0 0-14 7 7 0 0 0 0 14',
	check: 'M20 6 9 17l-5-5',
	x: 'M18 6 6 18M6 6l12 12',
	minus: 'M5 12h14',
	plus: 'M12 5v14M5 12h14',
	'chevron-left': 'm15 18-6-6 6-6',
	'chevron-right': 'm9 18 6-6-6-6',
	image: 'M5 3h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2M9 9a1 1 0 1 0 0-2 1 1 0 0 0 0 2m12 8-5-5L5 21'
};

export const ICON_NAMES = Object.keys(ICON_PATHS) as IconName[];
