import pathlib
import pyperf
import os

def run_bench(load, dump):
    runner = pyperf.Runner()

    runner.parse_args()

    if runner.args.inherit_environ is None:
        runner.args.inherit_environ = []

    runner.args.inherit_environ.extend(['BENCH_SKIP_LOAD', 'BENCH_SKIP_DUMP'])

    for entry in pathlib.Path('testfiles').iterdir():
        if entry.is_file():
            with open(entry, 'rb') as f:
                encoded = f.read()

            decoded = load(encoded)

            if 'BENCH_SKIP_LOAD' not in os.environ:
                runner.bench_func(f"{entry.name} load", load, encoded)
            if 'BENCH_SKIP_DUMP' not in os.environ:
                runner.bench_func(f"{entry.name} dump", dump, decoded)
