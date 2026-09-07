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

export type Address = {
	id: string;
	recipient_name: string;
	phone: string;
	postal_code: string;
	city: string;
	district: string;
	street: string;
	is_default: boolean;
};
export type AddressInput = Omit<Address, 'id' | 'is_default'> & { is_default?: boolean };

export type ShippingMethod = 'cvs' | 'home';
export type PaymentMethod = 'credit' | 'atm' | 'cvs_code';
export type InvoiceType = 'personal' | 'company' | 'donation';
export type CvsSubType = 'UNIMARTC2C' | 'FAMIC2C' | 'HILIFEC2C';

export type CvsStore = {
	token: string;
	sub_type: CvsSubType;
	store_id: string;
	store_name: string;
	store_address: string;
	store_phone: string;
};

export type CartCheckedLine = {
	variant_id: string;
	product_slug: string;
	product_name: string;
	variant_label: string;
	price: number;
	image_thumb: string | null;
	stock: number;
	qty: number;
	available: boolean;
	reason: null | 'unavailable' | 'sold_out' | 'qty_reduced';
};
export type CartValidateResponse = {
	items: CartCheckedLine[];
	subtotal: number;
	shipping: ShippingSettings;
	cvs_limit_exceeded: boolean;
};

export type OrderStatus = 'pending_payment' | 'paid' | 'shipped' | 'completed' | 'cancelled' | 'refunded';

export type HomeAddressInput = { postal_code: string; city: string; district: string; street: string };
export type InvoiceInput = {
	type: InvoiceType;
	carrier_type?: '1' | '2' | '3';
	carrier_num?: string;
	tax_id?: string;
	title?: string;
	address?: string;
	love_code?: string;
};
export type OrderInput = {
	items: { variant_id: string; qty: number }[];
	email: string;
	recipient_name: string;
	recipient_phone: string;
	shipping_method: ShippingMethod;
	cvs_store_token?: string | null;
	address?: HomeAddressInput | null;
	invoice: InvoiceInput;
	payment_method: PaymentMethod;
	note?: string;
};
/** 送往綠界的隱藏表單：action 是網址，fields 是欄位（含 CheckMacValue） */
export type EcpayForm = { action: string; fields: Record<string, string> };
/** ecpay 只有在訂單已成立、但事後組表單失敗時才會是 null；這時要把買家導去訂單頁 */
export type OrderCreated = { order_id: string; order_no: string; guest_token: string; ecpay: EcpayForm | null };

export type OrderItem = {
	product_name: string;
	variant_label: string;
	unit_price: number;
	quantity: number;
	line_total: number;
	image_path: string | null;
};
export type OrderShipment = {
	method: ShippingMethod;
	cvs_sub_type: CvsSubType | null;
	cvs_store_id: string | null;
	cvs_store_name: string | null;
	cvs_store_address: string | null;
	home_postal_code: string | null;
	home_city: string | null;
	home_district: string | null;
	home_street: string | null;
	status: string;
	carrier: string | null;
	tracking_no: string | null;
};
export type OrderPayment = {
	method: PaymentMethod;
	status: 'pending' | 'paid' | 'failed' | 'expired';
	amount: number;
	atm_bank_code: string | null;
	atm_vaccount: string | null;
	cvs_payment_no: string | null;
	expire_at: string | null;
};
export type OrderDetail = {
	id: string;
	order_no: string;
	status: OrderStatus;
	email: string;
	recipient_name: string;
	recipient_phone: string;
	shipping_method: ShippingMethod;
	subtotal: number;
	shipping_fee: number;
	total: number;
	note: string;
	invoice_type: InvoiceType;
	invoice_carrier_type: string | null;
	invoice_carrier_num: string | null;
	invoice_tax_id: string | null;
	invoice_title: string | null;
	invoice_address: string | null;
	invoice_love_code: string | null;
	created_at: string;
	paid_at: string | null;
	shipped_at: string | null;
	completed_at: string | null;
	cancelled_at: string | null;
	cancel_reason: string | null;
	items: OrderItem[];
	shipment: OrderShipment | null;
	payment: OrderPayment | null;
};
export type OrderSummary = {
	id: string;
	order_no: string;
	status: OrderStatus;
	total: number;
	item_count: number;
	created_at: string;
};
