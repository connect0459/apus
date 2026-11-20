import os
import resource
import shutil
import subprocess
import sys
import tempfile
import time

SIZES = [int(n) for n in (sys.argv[1:] or ["1000", "5000"])]
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
APUS = os.path.join(ROOT, "target", "release", "apus")
GEN = os.path.join(ROOT, "bench", "gen.py")
APUS_RUNS = 5
SWIFTC_RUNS = 3
CHECK_TWEAK = 99


def timed(cmd, env=None):
    before = resource.getrusage(resource.RUSAGE_CHILDREN)
    start = time.perf_counter()
    result = subprocess.run(cmd, capture_output=True, text=True, env=env)
    wall = (time.perf_counter() - start) * 1000
    after = resource.getrusage(resource.RUSAGE_CHILDREN)
    cpu = ((after.ru_utime - before.ru_utime) + (after.ru_stime - before.ru_stime)) * 1000
    if result.returncode != 0:
        sys.exit(f"{cmd} failed: {result.stderr[:300]}")
    return wall, cpu


def best(samples):
    return min(s[0] for s in samples), min(s[1] for s in samples)


def generate(n, path, tweak=0):
    subprocess.run([sys.executable, GEN, str(n), path, str(tweak)], check=True)


def output_of(path):
    return subprocess.run([path], capture_output=True, text=True).stdout.strip()


def write_with_main_call(source_path, destination):
    with open(source_path) as src, open(destination, "w") as dst:
        dst.write(src.read() + "main()\n")


class Apus:
    def __init__(self, tmp, name):
        self.out = os.path.join(tmp, name)
        self.cache = os.path.join(tmp, name + ".cache")
        self.env = dict(os.environ, APUS_CACHE_DIR=self.cache)

    def build(self, source):
        return timed([APUS, "build", source, "-o", self.out], self.env)

    def reset(self):
        shutil.rmtree(self.cache, ignore_errors=True)


def measure_apus(tmp, n):
    base = os.path.join(tmp, f"g{n}.swift")
    generate(n, base)
    apus = Apus(tmp, f"a{n}")
    cold = []
    for _ in range(APUS_RUNS):
        apus.reset()
        cold.append(apus.build(base))
    noop = [apus.build(base) for _ in range(APUS_RUNS)]
    comment = []
    for i in range(APUS_RUNS):
        path = os.path.join(tmp, f"c{n}_{i}.swift")
        with open(base) as src, open(path, "w") as dst:
            dst.write(f"// edit {i}\n" + src.read())
        comment.append(apus.build(path))
    edit = []
    for i in range(APUS_RUNS):
        path = os.path.join(tmp, f"e{n}_{i}.swift")
        generate(n, path, tweak=i + 1)
        edit.append(apus.build(path))
    return best(cold), best(noop), best(comment), best(edit), apus


def measure_swiftc(tmp, n):
    base = os.path.join(tmp, f"g{n}.swift")
    directory = os.path.join(tmp, f"sw{n}")
    os.makedirs(directory, exist_ok=True)
    main = os.path.join(directory, "main.swift")
    out = os.path.join(tmp, f"w{n}")
    cold = []
    edit = []
    for _ in range(SWIFTC_RUNS):
        write_with_main_call(base, main)
        cold.append(timed(["swiftc", "-Onone", main, "-o", out]))
    for i in range(SWIFTC_RUNS):
        edited = os.path.join(tmp, f"se{n}_{i}.swift")
        generate(n, edited, tweak=i + 1)
        write_with_main_call(edited, main)
        edit.append(timed(["swiftc", "-Onone", main, "-o", out]))
    return best(cold), best(edit)


def outputs_match(tmp, n, apus):
    source = os.path.join(tmp, f"check{n}.swift")
    generate(n, source, tweak=CHECK_TWEAK)
    apus.build(source)
    main = os.path.join(tmp, f"check_sw{n}.swift")
    write_with_main_call(source, main)
    expected_out = os.path.join(tmp, f"check_w{n}")
    timed(["swiftc", "-Onone", main, "-o", expected_out])
    return output_of(apus.out) == output_of(expected_out)


def line(label, apus, swift=None):
    wall, cpu = apus
    cells = f"  {label:30}{wall:9.1f}{cpu:9.1f}"
    if swift is not None:
        cells += f"{swift[0]:12.1f}{swift[1]:9.1f}{swift[0] / wall:9.1f}x"
    print(cells)


def warn_if_busy():
    load = os.getloadavg()[0]
    cores = os.cpu_count() or 1
    if load > cores / 2:
        print(
            f"warning: load average is {load:.1f} on {cores} cores; "
            "timings below are not reliable. Close other work and rerun.",
            file=sys.stderr,
        )


def main():
    warn_if_busy()
    subprocess.run(["cargo", "build", "--release", "-q"], cwd=ROOT, check=True)
    with tempfile.TemporaryDirectory() as tmp:
        for n in SIZES:
            cold, noop, comment, edit, apus = measure_apus(tmp, n)
            swift_cold, swift_edit = measure_swiftc(tmp, n)
            print(f"gen{n}: best of several runs, wall ms / cpu ms")
            print(f"  {'':30}{'apus':>9}{'':>9}{'swiftc':>12}{'':>9}{'wall':>10}")
            line("cold build", cold, swift_cold)
            line("rebuild after editing 1 fn", edit, swift_edit)
            line("rebuild after a comment", comment)
            line("rebuild with no change", noop)
            print(f"  outputs match swiftc's: {outputs_match(tmp, n, apus)}")


main()
