import benchtemplate

import orjson

benchtemplate.run_bench(
    orjson.loads,
    orjson.dumps,
    ['oops-all-bigints.json'], # orjson doesn't handle these correctly so the benchmark is meaningless
)
