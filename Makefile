.PHONY: dev build check test migrate docker deploy

dev:
	cargo run --bin aivory-mail-api

check:
	cargo check

build:
	cargo build --release

test:
	cargo test

migrate:
	sqlx migrate run

docker:
	docker compose build

up:
	docker compose up -d --build

down:
	docker compose down

logs:
	docker compose logs -f avry-mail

deploy-vps:
	./scripts/deploy-vps.sh

fmt:
	cargo fmt
