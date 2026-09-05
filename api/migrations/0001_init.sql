-- 使用者（老闆也是 user，role = admin）
CREATE TABLE users (
    id            uuid PRIMARY KEY,
    email         text NOT NULL,
    password_hash text NOT NULL,
    name          text NOT NULL DEFAULT '',
    phone         text,
    role          text NOT NULL DEFAULT 'customer' CHECK (role IN ('customer', 'admin')),
    created_at    timestamptz NOT NULL DEFAULT now(),
    updated_at    timestamptz NOT NULL DEFAULT now()
);
-- email 不分大小寫唯一（程式存入前一律轉小寫）
CREATE UNIQUE INDEX users_email_lower_idx ON users (lower(email));

-- 登入 session；cookie sid 存這裡的 id
CREATE TABLE sessions (
    id         uuid PRIMARY KEY,
    user_id    uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    expires_at timestamptz NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX sessions_expires_at_idx ON sessions (expires_at);
CREATE INDEX sessions_user_id_idx ON sessions (user_id);

-- 單層分類
CREATE TABLE categories (
    id         uuid PRIMARY KEY,
    slug       text NOT NULL UNIQUE,
    name       text NOT NULL,
    sort_order integer NOT NULL DEFAULT 0
);

-- 商品主檔（刪除 = status archived）
CREATE TABLE products (
    id           uuid PRIMARY KEY,
    slug         text NOT NULL UNIQUE,
    name         text NOT NULL,
    description  text NOT NULL DEFAULT '',
    category_id  uuid REFERENCES categories(id) ON DELETE SET NULL,
    status       text NOT NULL DEFAULT 'draft' CHECK (status IN ('draft', 'active', 'archived')),
    option1_name text,
    option2_name text,
    external_ref text UNIQUE,
    sort_order   integer NOT NULL DEFAULT 0,
    created_at   timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX products_category_status_idx ON products (category_id, status);
CREATE INDEX products_status_created_idx ON products (status, created_at DESC);

-- 商品圖片（最多 9 張由程式限制）
CREATE TABLE product_images (
    id         uuid PRIMARY KEY,
    product_id uuid NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    path       text NOT NULL,
    thumb_path text NOT NULL,
    alt        text NOT NULL DEFAULT '',
    sort_order integer NOT NULL DEFAULT 0
);
CREATE INDEX product_images_product_idx ON product_images (product_id, sort_order);

-- 規格：每個規格一列；沒規格的商品也有一列「預設」（option 值為 NULL）
CREATE TABLE product_variants (
    id               uuid PRIMARY KEY,
    product_id       uuid NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    option1_value    text,
    option2_value    text,
    sku              text,
    price            integer NOT NULL CHECK (price >= 0),
    compare_at_price integer CHECK (compare_at_price >= 0),
    stock            integer NOT NULL DEFAULT 0 CHECK (stock >= 0),
    is_active        boolean NOT NULL DEFAULT true,
    image_id         uuid REFERENCES product_images(id) ON DELETE SET NULL,
    sort_order       integer NOT NULL DEFAULT 0
);
CREATE INDEX product_variants_product_idx ON product_variants (product_id);

-- 商店設定（key/value）。計畫 2 會加運費等 key
CREATE TABLE settings (
    key        text PRIMARY KEY,
    value      jsonb NOT NULL,
    updated_at timestamptz NOT NULL DEFAULT now()
);
INSERT INTO settings (key, value) VALUES (
    'shop',
    '{"name": "dog_shop", "description": "", "contact_email": "", "contact_phone": ""}'
);
