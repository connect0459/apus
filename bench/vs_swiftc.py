import os
import statistics
import subprocess
import sys
import tempfile
import time

SIZES = [int(n) for n in (sys.argv[1:] or ["1000", "5000"])]
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
APUS = os.path.join(ROOT, "target", "release", "apus")
GEN = os.path.join(ROOT, "bench", "gen.py")


def best_of(cmd, runs):
    times = []
    for _ in range(runs):
        start = time.perf_counter()
        result = subprocess.run(cmd, capture_output=True, text=True)
        times.append(time.perf_counter() - start)
        if result.returncode != 0:
            sys.exit(f"{cmd} failed: {result.stderr[:300]}")
    return min(times) * 1000, statistics.median(times) * 1000


def output_of(path):
    return subprocess.run([path], capture_output=True, text=True).stdout.strip()


def main():
    subprocess.run(["cargo", "build", "--release", "-q"], cwd=ROOT, check=True)
    with tempfile.TemporaryDirectory() as tmp:
        for n in SIZES:
            source = os.path.join(tmp, f"g{n}.swift")
            swift_dir = os.path.join(tmp, f"sw{n}")
            os.makedirs(swift_dir)
            subprocess.run([sys.executable, GEN, str(n), source], check=True)
            with open(source) as f, open(os.path.join(swift_dir, "main.swift"), "w") as g:
                g.write(f.read() + "main()\n")
            apus_out = os.path.join(tmp, f"a{n}")
            swift_out = os.path.join(tmp, f"w{n}")
            apus_best, apus_median = best_of([APUS, "build", source, "-o", apus_out], 5)
            swift_best, swift_median = best_of(
                ["swiftc", "-Onone", os.path.join(swift_dir, "main.swift"), "-o", swift_out], 3
            )
            same = output_of(apus_out) == output_of(swift_out)
            print(
                f"gen{n}: apus {apus_best:.0f} ms (median {apus_median:.0f}), "
                f"swiftc -Onone {swift_best:.0f} ms (median {swift_median:.0f}), "
                f"{swift_best / apus_best:.1f}x faster, outputs match: {same}"
            )


main()
