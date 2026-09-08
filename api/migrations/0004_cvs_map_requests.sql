-- 按「選擇門市」時先登記一筆；綠界地圖回傳時核對 token 存在且未過期，核對成功就刪掉（單次使用）
-- （規格 §11、與規格不同之處 34）。過期的由每日 purge 清
CREATE TABLE cvs_map_requests (
    token      text PRIMARY KEY,
    sub_type   text NOT NULL CHECK (sub_type IN ('UNIMARTC2C', 'FAMIC2C', 'HILIFEC2C')),
    created_at timestamptz NOT NULL DEFAULT now(),
    expires_at timestamptz NOT NULL
);
