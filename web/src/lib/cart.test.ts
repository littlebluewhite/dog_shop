import { describe, expect, it } from 'vitest';
import { Cart, MAX_QTY } from './cart.svelte';

const line = {
	variant_id: 'v1',
	product_slug: 'food',
	product_name: '狗糧',
	variant_label: '雞 / S',
	price: 300,
	image_thumb: null
};

describe('Cart', () => {
	it('starts empty; load() marks it loaded', () => {
		const c = new Cart();
		expect(c.loaded).toBe(false);
		c.load();
		expect(c.loaded).toBe(true);
		expect(c.lines).toEqual([]);
		expect(c.count).toBe(0);
		expect(c.subtotal).toBe(0);
	});

	it('adds and merges the same variant', () => {
		const c = new Cart();
		c.add(line, 1);
		c.add(line, 2);
		expect(c.lines).toHaveLength(1);
		expect(c.count).toBe(3);
		expect(c.subtotal).toBe(900);
	});

	it('caps quantity at MAX_QTY and floors fractions', () => {
		const c = new Cart();
		c.add(line, 500);
		expect(c.lines[0].qty).toBe(MAX_QTY);
		c.setQty('v1', 2.7);
		expect(c.lines[0].qty).toBe(2);
	});

	it('setQty updates; zero or less removes', () => {
		const c = new Cart();
		c.add(line);
		c.add({ ...line, variant_id: 'v2', price: 100 });
		c.setQty('v1', 5);
		expect(c.count).toBe(6);
		expect(c.subtotal).toBe(1600);
		c.setQty('v2', 0);
		expect(c.lines.map((l) => l.variant_id)).toEqual(['v1']);
		c.setQty('missing', 3);
		expect(c.count).toBe(5);
	});

	it('remove and clear', () => {
		const c = new Cart();
		c.add(line);
		c.add({ ...line, variant_id: 'v2' });
		c.remove('v1');
		expect(c.lines.map((l) => l.variant_id)).toEqual(['v2']);
		c.clear();
		expect(c.lines).toEqual([]);
	});
});
