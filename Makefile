# 開發用一鍵啟動。要在 repo 根目錄下 make。
# 需要：Docker、pnpm、Rust（cargo）。第一次會自動裝前端套件。
#
#   make run    資料庫 + 後端(:8080) + 前端(:5173)，Ctrl+C 全部停掉
#   make db     只開開發資料庫
#   make api    只跑後端
#   make web    只跑前端
#   make build  只編譯後端

CARGO := $(HOME)/.cargo/bin/cargo
COMPOSE_DEV := docker compose -f deploy/docker-compose.dev.yml

.PHONY: run db api web build check-env

# 一鍵啟動：資料庫等到健康、後端先編好、前端套件裝好，才同時把兩個 server 拉起來。
# 先編譯是為了避免「網頁開得起來但後端還沒好」的空窗期。
# 兩個 server 是同一個 process group，Ctrl+C 會一起收掉。
run: check-env db build web/node_modules
	@echo "前台 http://localhost:5173  後台 http://localhost:5173/admin"
	@echo "（用 localhost，不要用 127.0.0.1：前端只聽 IPv6）"
	@$(MAKE) -j2 api web

# PostgreSQL 17，port 5435，compose 專案 dog_shop。--wait 會等到 healthy 才回來。
db:
	$(COMPOSE_DEV) up -d --wait db

build:
	$(CARGO) build --manifest-path api/Cargo.toml

api: check-env
	$(CARGO) run --manifest-path api/Cargo.toml

web: web/node_modules
	pnpm -C web dev

# 只有 lock 檔比 node_modules 新才重裝。
web/node_modules: web/pnpm-lock.yaml
	pnpm -C web install
	@touch web/node_modules

# .env 是唯一一份、不進 git，所以這裡只檢查、絕不自動覆蓋。
check-env:
	@test -f .env || { echo "找不到 .env：先 cp .env.example .env，再改裡面的值"; exit 1; }
