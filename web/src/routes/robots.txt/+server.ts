import { env } from '$env/dynamic/public';
import type { RequestHandler } from './$types';

export const GET: RequestHandler = ({ url }) => {
	const base = (env.PUBLIC_BASE_URL ?? url.origin).replace(/\/$/, '');
	const lines = [
		'User-agent: *',
		'Allow: /',
		'Disallow: /admin',
		'Disallow: /account',
		'Disallow: /cart',
		'Disallow: /checkout',
		'Disallow: /login',
		`Sitemap: ${base}/sitemap.xml`,
		''
	];
	return new Response(lines.join('\n'), { headers: { 'content-type': 'text/plain; charset=utf-8' } });
};
