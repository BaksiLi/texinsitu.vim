#!/usr/bin/env python3
"""Reproducible round-trip timings on generated, public formula source."""
import argparse
import json
import os
from pathlib import Path
import platform
from statistics import median
import subprocess
import tempfile
from time import perf_counter

ROOT = Path(__file__).resolve().parents[1]


def workload():
    lines = [r"\newcommand{\state}{\theta}"]
    lines += [fr"Term $a_{{{i}}}^2+\frac{{b}}{{c_{{{i}}}}}$ end." for i in range(600)]
    for i in range(100):
        lines += [r"\begin{equation}",
                  fr"\state_{{{i}}}=\frac{{x_{{{i}}}^2+y}}{{1+\sigma_{{{i}}}}}",
                  r"\end{equation}"]
    return lines


def request(lines, serial):
    return json.dumps({"id": serial, "lines": lines, "filetype": "tex"}) + "\n"


def check(output, serial):
    reply = json.loads(output)
    if reply["id"] != serial or len(reply["formulas"]) != 700:
        raise RuntimeError("expected the request id and all 700 formula layouts")


def measure(action):
    start = perf_counter()
    action()
    return (perf_counter() - start) * 1000


def screen(vim, runs):
    results = []
    with tempfile.TemporaryDirectory(prefix="texinsitu-bench-") as directory:
        directory = Path(directory)
        source, result = directory / "source.tex", directory / "result.json"
        source.write_text("\n".join(workload()), encoding="utf-8")
        env = dict(os.environ, TEXINSITU_BENCH_REPO=str(ROOT),
                   TEXINSITU_BENCH_SOURCE=str(source), TEXINSITU_BENCH_RESULT=str(result))
        for _ in range(runs):
            subprocess.run([vim, "-Nu", str(ROOT / "tests/benchmark.vim"),
                            "-i", "NONE", "-n", "-T", "xterm"],
                           env=env, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE,
                           check=True, timeout=30)
            reply = json.loads(result.read_text())
            if "error" in reply:
                raise RuntimeError(reply)
            results.append(reply)
    print("\nIsolated terminal Vim, 45 rows; movement and edits include redraw.")
    print("Resize cycles through 40, 75 and 100 columns. Line edits exclude reparsing.")
    print()
    print("| View | Open | Prose movement | Formula enter/leave | Resize | Same-line edit |")
    print("| --- | ---: | ---: | ---: | ---: | ---: |")
    for view in ("source", "rendered"):
        values = [median(run[view][key] for run in results) for key in
                  ("load_ms", "prose_move_ms", "formula_move_ms", "resize_ms", "line_edit_ms")]
        print(f"| {view.title()} | " + " | ".join(f"{value:.2f} ms" for value in values) + " |")


def memory(binary):
    context = "\n".join(fr"\newcommand{{\value{chr(97+i//26)}{chr(97+i%26)}}}{{x_{{{i}}}+y_{{{i}}}}}"
                        for i in range(200))
    with subprocess.Popen([binary], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True) as process:
        def exchange(lines):
            process.stdin.write(json.dumps({"id": 0, "lines": lines, "filetype": "tex",
                                           "context": context}) + "\n")
            process.stdin.flush()
            reply = json.loads(process.stdout.readline())
            if len(reply["formulas"]) != len(lines):
                raise RuntimeError("memory workload lost formulas")

        def report(label):
            kib = int(subprocess.check_output(["ps", "-o", "rss=", "-p", str(process.pid)], text=True))
            print(f"| {label} | {kib / 1024:.2f} MiB |")

        print("\nResident kernel RSS with 200 unchanged command definitions (OS/allocator dependent).")
        print()
        print("| Stage | RSS |")
        print("| --- | ---: |")
        exchange(["$x_0$"])
        report("One formula")
        for index in range(1, 2001):
            exchange([f"$x_{{{index}}}$"])
        report("After 2,000 formula edits")
        lines = [f"$z_{{{index}}}$" for index in range(10000)]
        exchange(lines)
        report("After 10,000 more unique formulas")
        exchange(lines)
        report("Repeated 10,000 formulas")
        process.stdin.close()
        process.wait(timeout=10)
        if process.returncode:
            raise RuntimeError(f"kernel exited with status {process.returncode}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--kernel", type=Path, default=ROOT / "kernel/target/release/texinsitu")
    parser.add_argument("--runs", type=int, default=21)
    parser.add_argument("--vim", metavar="EXECUTABLE", help="also measure an isolated terminal Vim")
    parser.add_argument("--memory", action="store_true", help="also measure resident RSS with ps (macOS/Linux)")
    args = parser.parse_args()
    if args.runs < 1:
        parser.error("--runs must be positive")
    binary = str(args.kernel.resolve())
    lines = workload()

    def cold():
        result = subprocess.run([binary], input=request(lines, 0), text=True,
                                capture_output=True, check=True, timeout=10)
        check(result.stdout, 0)

    opens = [measure(cold) for _ in range(args.runs)]
    with subprocess.Popen([binary], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                          text=True) as process:
        def exchange(serial):
            process.stdin.write(request(lines, serial))
            process.stdin.flush()
            check(process.stdout.readline(), serial)

        exchange(0)
        edits = []
        for serial in range(1, args.runs + 1):
            # Only this formula and its prose change; every other layout is reusable.
            lines[1] = fr"Edit {serial}: $a_0^2+\frac{{b+{serial}}}{{c_0}}$ end."
            edits.append(measure(lambda: exchange(serial)))
        process.stdin.close()
        process.wait(timeout=10)
        if process.returncode:
            raise RuntimeError(f"kernel exited with status {process.returncode}")

    print(f"{platform.system()} {platform.machine()}; median of {args.runs} runs.")
    print("Generated source: 600 unique inline formulas and 100 display equations.")
    print("Round trip includes JSON, scan, cache, layout and reply; open includes process startup.")
    print()
    print("| Workload | Open | After one formula edit |")
    print("| --- | ---: | ---: |")
    print(f"| 700 formulas | {median(opens):.2f} ms | {median(edits):.2f} ms |")
    if args.vim:
        screen(args.vim, args.runs)
    if args.memory:
        memory(binary)


if __name__ == "__main__":
    main()
