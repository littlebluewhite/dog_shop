export type User = {
	id: string;
	email: string;
	name: string;
	phone: string | null;
	role: 'customer' | 'admin';
};

export type ShopSettings = {
	name: string;
	description: string;
	contact_email: string;
	contact_phone: string;
};

export type ShippingSettings = { cvs_fee: number; home_fee: number; free_threshold: number };
export type PaymentMethods = { credit: boolean; atm: boolean; cvs_code: boolean };
export type SenderSettings = { name: string; phone: string };
export type ReturnStore = { sub_type: '' | 'UNIMARTC2C' | 'FAMIC2C' | 'HILIFEC2C'; store_id: string; store_name: string };
export type PublicSettings = { shop: ShopSettings; shipping: ShippingSettings; payment_methods: PaymentMethods };
export type AllSettings = PublicSettings & { sender: SenderSettings; return_store: ReturnStore };

export type Category = { id: string; slug: string; name: string; sort_order: number };

export type Page<T> = { items: T[]; total: number; page: number; per_page: number };

export type ProductStatus = 'draft' | 'active' | 'archived';

export type ProductImage = {
	id: string;
	product_id: string;
	path: string;
	thumb_path: string;
	alt: string;
	sort_order: number;
};

export type AdminVariant = {
	id: string;
	product_id: string;
	option1_value: string | null;
	option2_value: string | null;
	sku: string | null;
	price: number;
	compare_at_price: number | null;
	stock: number;
	is_active: boolean;
	image_id: string | null;
	sort_order: number;
};

export type AdminProduct = {
	id: string;
	slug: string;
	name: string;
	description: string;
	category_id: string | null;
	status: ProductStatus;
	option1_name: string | null;
	option2_name: string | null;
	external_ref: string | null;
	sort_order: number;
	created_at: string;
	updated_at: string;
	variants: AdminVariant[];
	images: ProductImage[];
};

export type AdminProductListItem = {
	id: string;
	slug: string;
	name: string;
	status: ProductStatus;
	category_name: string | null;
	price_min: number | null;
	price_max: number | null;
	stock_total: number;
	image_thumb: string | null;
	updated_at: string;
};

export type ProductListItem = {
	slug: string;
	name: string;
	price_min: number;
	price_max: number;
	image_thumb: string | null;
	in_stock: boolean;
};

export type PublicVariant = {
	id: string;
	option1_value: string | null;
	option2_value: string | null;
	price: number;
	compare_at_price: number | null;
	stock: number;
	image_path: string | null;
};

export type ProductDetail = {
	id: string;
	slug: string;
	name: string;
	description: string;
	category: { slug: string; name: string } | null;
	option1_name: string | null;
	option2_name: string | null;
	images: { path: string; thumb_path: string; alt: string }[];
	variants: PublicVariant[];
};
