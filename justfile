setup:
    pre-commit install
    cargo build

test:
    cargo test --all-targets

coverage:
    cargo llvm-cov --fail-under-lines 80 --fail-under-functions 90

bench-front n="1000":
    python3 bench/gen.py {{n}} tmp/gen{{n}}.swift
    cargo run --release -q --example front_bench tmp/gen{{n}}.swift

verify:
    cargo fmt --all --check
    cargo clippy --all-targets --all-features -- -D warnings
    just test
    just coverage
