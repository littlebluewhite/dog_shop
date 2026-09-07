-- 電子發票（規格 §3）。下單時建 pending 一列；付款成功後由 issue_invoice job 開立
CREATE TABLE invoices (
    id            uuid PRIMARY KEY,
    order_id      uuid NOT NULL UNIQUE REFERENCES orders(id) ON DELETE CASCADE,
    status        text NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'issued', 'failed')),
    relate_number text NOT NULL UNIQUE,
    invoice_no    text,
    invoice_date  timestamptz,
    random_number text,
    request       jsonb,
    response      jsonb,
    error         text,
    created_at    timestamptz NOT NULL DEFAULT now(),
    updated_at    timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX invoices_status_idx ON invoices (status);

-- 過期掃描只看待付款訂單（規格 §5）
CREATE INDEX orders_pending_created_idx ON orders (created_at) WHERE status = 'pending_payment';
