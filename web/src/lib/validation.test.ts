import { describe, expect, it } from 'vitest';
import {
	isCitizenCert,
	isCvsRecipientName,
	isLoveCode,
	isMobileBarcode,
	isPostalCode,
	isTaxId,
	isTwMobile,
	shippingFee
} from './validation';

describe('validation', () => {
	it('mobile / postal code', () => {
		expect(isTwMobile('0912345678')).toBe(true);
		expect(isTwMobile('091234567')).toBe(false);
		expect(isTwMobile('0212345678')).toBe(false);
		expect(isPostalCode('100')).toBe(true);
		expect(isPostalCode('10058')).toBe(true);
		expect(isPostalCode('10')).toBe(false);
		expect(isPostalCode('100a')).toBe(false);
	});

	it('names and carriers', () => {
		expect(isCvsRecipientName('王小明')).toBe(true);
		expect(isCvsRecipientName('王')).toBe(false);
		expect(isCvsRecipientName('John')).toBe(false);
		expect(isMobileBarcode('/ABC+123')).toBe(true);
		expect(isMobileBarcode('/abc+123')).toBe(false);
		expect(isCitizenCert('AB12345678901234')).toBe(true);
		expect(isCitizenCert('A123456789012345')).toBe(false);
		expect(isLoveCode('168')).toBe(true);
		expect(isLoveCode('12')).toBe(false);
	});

	it('tax id checksum matches the backend vectors', () => {
		expect(isTaxId('04595257')).toBe(true);
		expect(isTaxId('10000004')).toBe(true);
		expect(isTaxId('12345675')).toBe(true);
		expect(isTaxId('12345678')).toBe(false);
		expect(isTaxId('12345674')).toBe(false);
		expect(isTaxId('1234567')).toBe(false);
		expect(isTaxId('00000000')).toBe(false);
	});

	it('shipping fee with free threshold', () => {
		const s = { cvs_fee: 60, home_fee: 100, free_threshold: 0 };
		expect(shippingFee(s, 'cvs', 99999)).toBe(60);
		expect(shippingFee(s, 'home', 1)).toBe(100);
		const free = { ...s, free_threshold: 1000 };
		expect(shippingFee(free, 'home', 999)).toBe(100);
		expect(shippingFee(free, 'home', 1000)).toBe(0);
	});
});
