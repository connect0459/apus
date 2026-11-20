setup:
    pre-commit install
    cargo build

test:
    cargo test --all-targets

coverage:
    cargo llvm-cov --fail-under-lines 80 --fail-under-functions 90

coverage-gaps:
    cargo llvm-cov --show-missing-lines

coverage-report:
    cargo llvm-cov report --show-missing-lines

coverage-html:
    cargo llvm-cov --html --open

bench-front n="1000":
    mkdir -p tmp
    python3 bench/gen.py {{n}} tmp/gen{{n}}.swift
    cargo run --release -q --example front_bench tmp/gen{{n}}.swift

bench-vs-swiftc:
    python3 bench/vs_swiftc.py

verify:
    cargo fmt --all --check
    cargo clippy --all-targets --all-features -- -D warnings
    just test
    just coverage
