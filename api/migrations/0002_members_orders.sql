-- 忘記密碼：DB 只存 token 的 SHA-256（hex），1 小時有效，用過作廢（規格 §11）
CREATE TABLE password_resets (
    token_hash text PRIMARY KEY,
    user_id    uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    expires_at timestamptz NOT NULL,
    used_at    timestamptz,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX password_resets_user_idx ON password_resets (user_id);

-- 會員常用地址
CREATE TABLE addresses (
    id             uuid PRIMARY KEY,
    user_id        uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    recipient_name text NOT NULL,
    phone          text NOT NULL,
    postal_code    text NOT NULL,
    city           text NOT NULL,
    district       text NOT NULL,
    street         text NOT NULL,
    is_default     boolean NOT NULL DEFAULT false,
    created_at     timestamptz NOT NULL DEFAULT now(),
    updated_at     timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX addresses_user_idx ON addresses (user_id, created_at);

-- 訂單主檔（規格 §3）。訪客訂單 user_id 為 NULL，用 guest_token 看訂單頁
CREATE TABLE orders (
    id                   uuid PRIMARY KEY,
    order_no             text NOT NULL UNIQUE,
    user_id              uuid REFERENCES users(id) ON DELETE SET NULL,
    guest_token          text NOT NULL,
    status               text NOT NULL DEFAULT 'pending_payment'
                         CHECK (status IN ('pending_payment', 'paid', 'shipped', 'completed', 'cancelled', 'refunded')),
    email                text NOT NULL,
    recipient_name       text NOT NULL,
    recipient_phone      text NOT NULL,
    shipping_method      text NOT NULL CHECK (shipping_method IN ('cvs', 'home')),
    subtotal             integer NOT NULL CHECK (subtotal >= 0),
    shipping_fee         integer NOT NULL CHECK (shipping_fee >= 0),
    total                integer NOT NULL CHECK (total >= 0),
    note                 text NOT NULL DEFAULT '',
    invoice_type         text NOT NULL CHECK (invoice_type IN ('personal', 'company', 'donation')),
    invoice_carrier_type text,
    invoice_carrier_num  text,
    invoice_tax_id       text,
    invoice_title        text,
    invoice_address      text,
    invoice_love_code    text,
    needs_refund         boolean NOT NULL DEFAULT false,
    created_at           timestamptz NOT NULL DEFAULT now(),
    paid_at              timestamptz,
    shipped_at           timestamptz,
    completed_at         timestamptz,
    cancelled_at         timestamptz,
    cancel_reason        text
);
CREATE INDEX orders_user_created_idx ON orders (user_id, created_at DESC);
CREATE INDEX orders_status_idx ON orders (status);
CREATE INDEX orders_created_idx ON orders (created_at DESC);

-- 下單當時的快照。variant_id 用 RESTRICT：有訂單的規格不能真刪（規格 §10）
CREATE TABLE order_items (
    id            uuid PRIMARY KEY,
    order_id      uuid NOT NULL REFERENCES orders(id) ON DELETE CASCADE,
    variant_id    uuid NOT NULL REFERENCES product_variants(id) ON DELETE RESTRICT,
    product_name  text NOT NULL,
    variant_label text NOT NULL,
    unit_price    integer NOT NULL CHECK (unit_price >= 0),
    quantity      integer NOT NULL CHECK (quantity > 0),
    line_total    integer NOT NULL CHECK (line_total >= 0),
    image_path    text,
    sort_order    integer NOT NULL DEFAULT 0
);
CREATE INDEX order_items_order_idx ON order_items (order_id, sort_order);
CREATE INDEX order_items_variant_idx ON order_items (variant_id);

-- 一次付款嘗試一列（規格 §3）。計畫 3 才會填綠界欄位；cod 預留給貨到付款（規格 §1.2）
CREATE TABLE payments (
    id                uuid PRIMARY KEY,
    order_id          uuid NOT NULL REFERENCES orders(id) ON DELETE CASCADE,
    merchant_trade_no text NOT NULL UNIQUE,
    method            text NOT NULL CHECK (method IN ('credit', 'atm', 'cvs_code', 'cod')),
    status            text NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'paid', 'failed', 'expired')),
    amount            integer NOT NULL CHECK (amount >= 0),
    ecpay_trade_no    text,
    payment_type      text,
    payment_date      timestamptz,
    atm_bank_code     text,
    atm_vaccount      text,
    cvs_payment_no    text,
    expire_at         timestamptz,
    raw               jsonb,
    created_at        timestamptz NOT NULL DEFAULT now(),
    updated_at        timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX payments_order_idx ON payments (order_id, created_at DESC);

-- 一筆訂單一筆出貨（規格 §3）。本計畫只填地址／門市與 pending；物流欄位計畫 4 填
CREATE TABLE shipments (
    id                      uuid PRIMARY KEY,
    order_id                uuid NOT NULL UNIQUE REFERENCES orders(id) ON DELETE CASCADE,
    method                  text NOT NULL CHECK (method IN ('cvs', 'home')),
    cvs_sub_type            text CHECK (cvs_sub_type IN ('UNIMARTC2C', 'FAMIC2C', 'HILIFEC2C')),
    cvs_store_id            text,
    cvs_store_name          text,
    cvs_store_address       text,
    cvs_store_phone         text,
    home_postal_code        text,
    home_city               text,
    home_district           text,
    home_street             text,
    status                  text NOT NULL DEFAULT 'pending'
                            CHECK (status IN ('pending', 'created', 'in_transit', 'arrived', 'picked_up', 'returned', 'shipped')),
    ecpay_logistics_id      text,
    ecpay_merchant_trade_no text,
    cvs_payment_no          text,
    cvs_validation_no       text,
    carrier                 text,
    tracking_no             text,
    last_status_code        text,
    last_status_msg         text,
    raw                     jsonb,
    created_at              timestamptz NOT NULL DEFAULT now(),
    updated_at              timestamptz NOT NULL DEFAULT now()
);

-- 背景工作 outbox（規格 §9）。本計畫只寫入；worker 在計畫 3
CREATE TABLE jobs (
    id           bigserial PRIMARY KEY,
    kind         text NOT NULL,
    payload      jsonb NOT NULL DEFAULT '{}'::jsonb,
    dedupe_key   text UNIQUE,
    status       text NOT NULL DEFAULT 'queued' CHECK (status IN ('queued', 'running', 'done', 'failed')),
    attempts     integer NOT NULL DEFAULT 0,
    max_attempts integer NOT NULL DEFAULT 5,
    run_at       timestamptz NOT NULL DEFAULT now(),
    last_error   text,
    created_at   timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX jobs_status_run_at_idx ON jobs (status, run_at);

-- 綠界地圖選完門市的暫存，1 小時過期（規格 §3）。本計畫只讀（下單）與測試寫入；計畫 4 的 map-reply 才會寫
CREATE TABLE cvs_store_selections (
    token         text PRIMARY KEY,
    sub_type      text NOT NULL CHECK (sub_type IN ('UNIMARTC2C', 'FAMIC2C', 'HILIFEC2C')),
    store_id      text NOT NULL,
    store_name    text NOT NULL,
    store_address text NOT NULL,
    store_phone   text NOT NULL DEFAULT '',
    created_at    timestamptz NOT NULL DEFAULT now(),
    expires_at    timestamptz NOT NULL
);

-- 商店設定的新 key（與 shop 分開，不塞進 shop）
INSERT INTO settings (key, value) VALUES
    ('shipping', '{"cvs_fee": 60, "home_fee": 100, "free_threshold": 0}'),
    ('payment_methods', '{"credit": true, "atm": true, "cvs_code": true}'),
    ('sender', '{"name": "", "phone": ""}'),
    ('return_store', '{"sub_type": "", "store_id": "", "store_name": ""}')
ON CONFLICT (key) DO NOTHING;
