# vim: ft=make

set dotenv-load := false

setup:
	echo "just commit" >> ./.git/hooks/pre-commit
	chmod +x ./.git/hooks/pre-commit

commit:
	cargo fmt
	cargo clippy --no-default-features --features dev
	cargo clippy --no-default-features --features prod
	cargo test --no-default-features --features dev
	cargo test --no-default-features --features prod
