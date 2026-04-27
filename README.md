# queson

[![CI](https://github.com/cdhowie/queson/actions/workflows/CI.yml/badge.svg)](https://github.com/cdhowie/queson/actions/workflows/CI.yml)
[![PyPI - Version](https://img.shields.io/pypi/v/queson)](https://pypi.org/project/queson/)

An experimental JSON encoder/decoder Python module written in Rust.

Goals:

* [x] Faster than Python's built-in `json` module, with at least comparable RAM
  usage.
* [x] Only Rust code.
* [x] Transparent support for arbitrary-precision integers, without encoding
  them as strings.
* [x] During encoding and decoding, support for a custom "object hook" function.
    * When encoding, the object hook is called on any value that is unsupported
      by the encoder.  If the hook returns a value that is supported, encoding
      proceeds with that value instead.
    * When decoding, the object hook is called with each produced `dict` value.
      The value returned by the function takes the place of the `dict` in the
      decoded object graph.
* [x] During encoding, support for "fragment" values, which represent
  already-JSON-encoded strings that should be dumped verbatim in the output.

# Benchmark

The following results were collected using the `benchmarks` directory in this
repository.  The documents tested are real-world messages collected from the
[Archipelago](https://github.com/ArchipelagoMW/Archipelago) client.

Benchmark environment:

* Debian Trixie (Linux kernel 6.19.6)
* AMD Ryzen 9 3900X with 32GB RAM
* Python 3.11.11
    * orjson 3.9.5

| Benchmark                | json    | queson                | orjson                 |
|--------------------------|--------:|----------------------:|-----------------------:|
| jsonmsg-1.json load      | 176 us  | 81.8 us: 2.15x faster | 52.1 us: 3.37x faster  |
| jsonmsg-1.json dump      | 208 us  | 33.7 us: 6.19x faster | 20.7 us: 10.05x faster |
| jsonmsg-23.json load     | 2.29 ms | 1.52 ms: 1.51x faster | 765 us: 3.00x faster   |
| jsonmsg-23.json dump     | 2.67 ms | 434 us: 6.16x faster  | 232 us: 11.51x faster  |
| jsonmsg-5.json load      | 880 us  | 813 us: 1.08x faster  | 509 us: 1.73x faster   |
| jsonmsg-5.json dump      | 1.24 ms | 290 us: 4.28x faster  | 220 us: 5.65x faster   |
| jsonmsg-7.json load      | 659 us  | 507 us: 1.30x faster  | 292 us: 2.26x faster   |
| jsonmsg-7.json dump      | 854 us  | 142 us: 6.03x faster  | 71.8 us: 11.90x faster |
| oops-all-ints.json load  | 172 us  | 66.4 us: 2.60x faster | 39.6 us: 4.36x faster  |
| oops-all-ints.json dump  | 200 us  | 33.1 us: 6.06x faster | 21.3 us: 9.42x faster  |
| oops-all-uints.json load | 169 us  | 73.5 us: 2.29x faster | 44.0 us: 3.83x faster  |
| oops-all-uints.json dump | 202 us  | 31.8 us: 6.36x faster | 19.9 us: 10.19x faster |
| Geometric mean           | (ref)   | 3.17x faster          | 5.30x faster           |


Running the same benchmarks but monitoring memory usage concludes that `queson`
has a 2% higher RSS peak than `json`, and `orjson` has a 13% higher RSS peak
than `json`.

# Differences from Python's `json` module

This list may not be exhaustive.

* There is currently no proper streaming support (`load` and `dump` are shims
  that buffer the entire input and output in memory).  This may be changed in
  the future.
* Non-finite float values (`NaN`, `Infinity`, `-Infinity`) are rejected during
  encoding and decoding as they are not valid JSON.
* Dumping does not support `float` `dict` keys.  The JSON specification does not
  guarantee a particular method of formatting float values, nor does it
  guarantee any specific level of precision.  The lack of a canonical float
  representation means `float` keys are of dubious value.
* Loading does not support `bytearray` objects.  This is because they are
  mutable, and object hook support would allow Python code to mutate the
  contents while the decoder is running.  As this can invalidate the data
  pointer, it would be necessary to re-obtain the data pointer after every
  object hook invocation for soundness.

# Compliance

Passes the following test suites:

* https://github.com/nst/JSONTestSuite
