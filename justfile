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

swift-tests *args:
    cargo build --release -q
    python3 bench/swift_tests.py --baseline bench/swift_tests_passing.txt {{args}}

swift-tests-selftest:
    python3 bench/test_swift_tests.py

verify:
    cargo fmt --all --check
    cargo clippy --all-targets --all-features -- -D warnings
    just swift-tests-selftest
    just test
    just coverage
