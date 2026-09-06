import { env } from '$env/dynamic/public';
import type { RequestHandler } from './$types';
import { serverApi } from '$lib/server/api';
import type { Page, ProductListItem } from '$lib/types';

function escapeXml(s: string): string {
	return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
}

export const GET: RequestHandler = async (event) => {
	const base = (env.PUBLIC_BASE_URL ?? event.url.origin).replace(/\/$/, '');
	// 公開列表一頁最多 60 件，翻到沒有為止（上限 50 頁 = 3000 件，夠用）
	const slugs: string[] = [];
	for (let page = 1; page <= 50; page++) {
		const result = await serverApi<Page<ProductListItem>>(event, `/api/products?per_page=60&page=${page}`);
		slugs.push(...result.items.map((i) => i.slug));
		if (result.items.length < 60) break;
	}
	const paths = ['/', '/products', ...slugs.map((s) => `/products/${s}`)];
	const xml =
		'<?xml version="1.0" encoding="UTF-8"?>\n' +
		'<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n' +
		paths.map((p) => `  <url><loc>${escapeXml(base + p)}</loc></url>`).join('\n') +
		'\n</urlset>\n';
	return new Response(xml, { headers: { 'content-type': 'application/xml; charset=utf-8' } });
};
