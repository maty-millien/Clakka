#!/usr/bin/env python3
"""Build and benchmark the C++ engine (from a git ref) and the Rust engine (working tree).

    python3 bench/run.py                 # both, C++ taken from `main`
    python3 bench/run.py --only cpp      # baseline only
    python3 bench/run.py --cpp-ref <ref> --tap-n 2000 --runs 20

Results go to bench/out/results-<impl>.json; a comparison table is printed when
both result files exist. Run from anywhere; everything runs with cwd = repo root
so that assets/ resolves.
"""
import argparse
import json
import os
import re
import shutil
import signal
import statistics
import subprocess
import sys
import tarfile
import io
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BENCH = os.path.join(ROOT, "bench")
OUT = os.path.join(BENCH, "out")
FRAMEWORKS = ["CoreGraphics", "AudioToolbox", "CoreFoundation", "CoreAudio", "Accelerate"]
FW_FLAGS = [x for f in FRAMEWORKS for x in ("-framework", f)]
LINE = re.compile(r"^(\S+) n=(\d+)(?: p50=(\S+) p99=(\S+) max=(\S+) mean=(\S+))?")


def sh(cmd, **kw):
    print("+", " ".join(cmd), file=sys.stderr)
    subprocess.run(cmd, check=True, cwd=kw.pop("cwd", ROOT), **kw)


def cargo():
    return shutil.which("cargo") or os.path.expanduser("~/.cargo/bin/cargo")


def build_cpp(ref):
    src = os.path.join(OUT, "cpp-src")
    if os.path.exists(src):
        subprocess.run(["trash", src], check=True)
    os.makedirs(src)
    data = subprocess.run(["git", "archive", ref, "src", "lib", "assets", "CMakeLists.txt"],
                          cwd=ROOT, check=True, capture_output=True).stdout
    tarfile.open(fileobj=io.BytesIO(data)).extractall(src, filter="data")
    # Shipped binary, built exactly like `make` on main did.
    build = os.path.join(OUT, "cpp-build")
    sh(["cmake", "-S", src, "-B", build, "-DCMAKE_BUILD_TYPE=Release"], stdout=subprocess.DEVNULL)
    sh(["cmake", "--build", build, "-j"], stdout=subprocess.DEVNULL)
    # Bench driver: same flags as CMakeLists.txt (-O3 -march=native -funroll-loops, IPO, NDEBUG).
    flags = ["-O3", "-march=native", "-funroll-loops", "-flto", "-DNDEBUG"]
    objs = []
    for f in ["src/audio/Audio.cpp", "src/theme/Theme.cpp", "src/input/Input.cpp", "SHIM"]:
        path = os.path.join(BENCH, "shim_cpp.cpp") if f == "SHIM" else os.path.join(src, f)
        obj = os.path.join(OUT, "cpp-" + os.path.basename(path) + ".o")
        sh(["clang++", "-std=c++17", *flags, "-I", os.path.join(src, "src"), "-I", os.path.join(src, "lib"),
            "-I", BENCH, "-c", path, "-o", obj])
        objs.append(obj)
    drv = os.path.join(OUT, "bench.o")
    sh(["clang", "-std=c11", "-O3", "-march=native", "-c", os.path.join(BENCH, "bench.c"), "-o", drv])
    exe = os.path.join(OUT, "bench-cpp")
    sh(["clang++", "-flto", "-O3", drv, *objs, *FW_FLAGS, "-o", exe])
    return exe, os.path.join(build, "kream_engine")


def build_rust():
    sh([cargo(), "build", "--release"])
    env = dict(os.environ, CARGO_TARGET_DIR=os.path.join(OUT, "shim-target"))
    # cwd = repo root so .cargo/config.toml (target-cpu=native) applies to the shim too.
    sh([cargo(), "build", "--release", "--manifest-path", os.path.join(BENCH, "shim-rs", "Cargo.toml")], env=env)
    lib = os.path.join(OUT, "shim-target", "release", "libkream_bench_shim.a")
    drv = os.path.join(OUT, "bench.o")
    sh(["clang", "-std=c11", "-O3", "-march=native", "-c", os.path.join(BENCH, "bench.c"), "-o", drv])
    exe = os.path.join(OUT, "bench-rust")
    sh(["clang", "-O3", drv, lib, *FW_FLAGS, "-o", exe])
    return exe, os.path.join(ROOT, "target", "release", "kream_engine")


def run_driver(exe, *args):
    p = subprocess.run([exe, *args], cwd=ROOT, capture_output=True, text=True)
    res = {}
    for line in p.stdout.splitlines():
        m = LINE.match(line)
        if m:
            name, n, *vals = m.groups()
            res[name] = {"n": int(n)}
            if vals[0] is not None:
                res[name].update(zip(["p50", "p99", "max", "mean"], map(float, vals)))
        elif line.strip():
            res.setdefault("_notes", []).append(line.strip())
    if p.returncode != 0:
        res.setdefault("_notes", []).append(f"exit {p.returncode}: {p.stderr.strip()[-300:]}")
    return res


def summarize(name, values):
    v = sorted(values)
    return {name: {"n": len(v), "p50": v[len(v) // 2], "p99": v[max(0, int(len(v) * 0.99 + 0.999999) - 1)],
                   "max": v[-1], "mean": statistics.fmean(v)}}


def run_binary(binary, runs):
    """Startup to the 'started' line, RSS while idle, max RSS, plus stdout/stderr of the first run."""
    ready, rss, maxrss, first = [], [], [], None
    for i in range(runs):
        t0 = time.perf_counter()
        p = subprocess.Popen([binary], cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        out_lines = []
        started = False
        for line in p.stdout:
            out_lines.append(line)
            if "Press keys" in line:
                ready.append((time.perf_counter() - t0) * 1e9)
                started = True
                break
        if started:
            time.sleep(1.0)
            ps = subprocess.run(["ps", "-o", "rss=", "-p", str(p.pid)], capture_output=True, text=True)
            rss.append(int(ps.stdout.strip()) * 1024)
            p.send_signal(signal.SIGINT)
        _, status, ru = os.wait4(p.pid, 0)
        p.returncode = os.waitstatus_to_exitcode(status)
        maxrss.append(ru.ru_maxrss)
        rest_out = p.stdout.read()
        err = p.stderr.read()
        if first is None:
            first = {"stdout": "".join(out_lines) + rest_out, "stderr": err, "exit": p.returncode}
    res = {}
    if ready:
        res.update(summarize("process_exec_to_ready", ready))
    if rss:
        res.update(summarize("rss_idle_bytes", rss))
    res.update(summarize("max_rss_bytes", maxrss))
    res["_first_run"] = first
    return res


def stripped_size(binary, tag):
    dst = os.path.join(OUT, f"stripped-{tag}")
    shutil.copyfile(binary, dst)
    subprocess.run(["strip", dst], check=True)
    return os.path.getsize(dst)


def bench(impl, exe, binary, args):
    res = {}
    startup = []
    for _ in range(args.runs):
        r = run_driver(exe, "startup")
        startup.append(r["startup_load_themes"]["p50"])
    res.update(summarize("startup_load_themes", startup))
    res.update(run_driver(exe, "play"))
    res.update(run_driver(exe, "callback", os.path.join(OUT, f"callback-{impl}.log")))
    res.update(run_driver(exe, "render"))
    run_driver(exe, "golden", os.path.join(OUT, f"golden-{impl}.f32"))
    if args.tap_n > 0:
        res.update(run_driver(exe, "tap", os.path.join(OUT, f"tap-{impl}.log"), str(args.tap_n)))
    res.update(run_binary(binary, args.runs))
    res["binary_bytes"] = os.path.getsize(binary)
    res["binary_stripped_bytes"] = stripped_size(binary, impl)
    with open(os.path.join(OUT, f"results-{impl}.json"), "w") as f:
        json.dump(res, f, indent=1)
    return res


def fmt_ns(x):
    return f"{x / 1e6:.2f} ms" if x >= 1e6 else f"{x / 1e3:.2f} us" if x >= 1e4 else f"{x:.0f} ns"


def table(a, b):
    rows = []
    keys = [k for k in a if not k.startswith("_") and isinstance(a[k], dict) and "p50" in a[k]]
    print("| metric | C++ p50 / p99 / max | Rust p50 / p99 / max |\n|---|---|---|")
    for k in keys:
        f = (lambda x: f"{x / 2**20:.1f} MiB") if "bytes" in k else fmt_ns
        cell = lambda r: " / ".join(f(r[k][s]) for s in ("p50", "p99", "max")) if k in r and "p50" in r[k] else "n/a"
        print(f"| {k} | {cell(a)} | {cell(b)} |")
    for k in ("binary_bytes", "binary_stripped_bytes"):
        print(f"| {k} | {a.get(k, 0) / 1024:.0f} KiB | {b.get(k, 0) / 1024:.0f} KiB |")


def compare_golden():
    import array
    out = {}
    for impl in ("cpp", "rust"):
        a = array.array("f")
        with open(os.path.join(OUT, f"golden-{impl}.f32"), "rb") as f:
            a.frombytes(f.read())
        out[impl] = a
    a, b = out["cpp"], out["rust"]
    if len(a) != len(b):
        print(f"golden: length differs {len(a)} vs {len(b)}")
        return
    diff = max((abs(x - y) for x, y in zip(a, b)), default=0.0)
    same = sum(1 for x, y in zip(a, b) if x == y)
    print(f"golden: {len(a)} samples, {same} bit-identical, max abs diff {diff:.3g}")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--only", choices=["cpp", "rust"])
    ap.add_argument("--cpp-ref", default="main")
    ap.add_argument("--tap-n", type=int, default=2000, help="synthetic events for the tap test, 0 to skip")
    ap.add_argument("--runs", type=int, default=20, help="process launches for startup/RSS")
    args = ap.parse_args()
    os.makedirs(OUT, exist_ok=True)
    if args.only != "rust":
        print(json.dumps(bench("cpp", *build_cpp(args.cpp_ref), args), indent=1))
    if args.only != "cpp":
        print(json.dumps(bench("rust", *build_rust(), args), indent=1))
    paths = [os.path.join(OUT, f"results-{i}.json") for i in ("cpp", "rust")]
    if all(map(os.path.exists, paths)):
        a, b = (json.load(open(p)) for p in paths)
        table(a, b)
        compare_golden()


if __name__ == "__main__":
    main()
