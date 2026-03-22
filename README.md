# apjson

An experimental JSON encoder/decoder Python module written in Rust.

Goals:

* Faster and lower RAM usage than Python's built-in `json` module.
* Transparent support for arbitrary-precision integers, without encoding them as strings.
* During decoding, support for a custom "object hook" function that possibly maps decoded objects to other values.
* During encoding, support for "fragment" values, which represent already-JSON-encoded strings that should be dumped verbatim in the output.

It is explicitly _not_ a goal to _exactly_ match the behavior of Python's `json` module.
