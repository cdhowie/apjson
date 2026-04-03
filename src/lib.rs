use pyo3::prelude::*;

mod de;
#[cfg(all(feature = "pymem-alloc", not(windows), not(test)))]
mod pymem;
mod ser;
mod simd;
mod thunk;

#[pymodule]
mod queson {
    use std::num::NonZeroUsize;

    use pyo3::{
        exceptions::PyTypeError,
        prelude::*,
        types::{PyBytes, PyFunction, PyString},
    };

    /// Deserialize a JSON-encoded value.
    ///
    /// This function accepts either `bytes` or `str`.  A `bytes` will be more
    /// efficient, as a `str` will be UTF-8 encoded first.
    ///
    /// `object_hook` can be set to a function.  Whenever a JSON object has been
    /// fully deserialized, this function will be called with the resulting
    /// `dict` as its only parameter.  The return value of the function (which
    /// may be the `dict`) will be substituted for the `dict` in the
    /// deserialized object graph.  This can be used to handle deserializing
    /// custom types.
    ///
    /// `depth_limit` specifies how deep the deserialized structure can be.  If
    /// provided and the given structure exceeds the depth limit, an error will
    /// be immediately raised.  Note that queson uses a heap-based stack when
    /// deserializing, which allows arbitrarily-deep structures to be
    /// deserialized without a stack overflow.  However, very deep structures
    /// will still cause a large number of Python objects to be allocated and
    /// can therefore take an unreasonable amount of time to fully deserialize,
    /// which may be a denial-of-service attack vector if the input is
    /// untrusted.
    #[pyfunction]
    #[pyo3(signature = (json, *, object_hook = None, depth_limit = None))]
    fn loads<'py>(
        json: &Bound<'py, PyAny>,
        object_hook: Option<&'py Bound<'py, PyFunction>>,
        depth_limit: Option<NonZeroUsize>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let bytes = if let Ok(s) = json.cast::<PyString>() {
            s.to_str()?.as_bytes()
        } else if let Ok(b) = json.cast::<PyBytes>() {
            b.as_bytes()
        } else {
            return Err(PyErr::new::<PyTypeError, _>("expected a str or bytes"));
        };

        crate::de::parse_json(json.py(), bytes, object_hook, depth_limit)
    }

    /// Deserialize a JSON-encoded value.
    ///
    /// This function accepts either `bytes` or `str`.  A `bytes` will be more
    /// efficient, as a `str` will be UTF-8 encoded first.
    ///
    /// `object_hook` can be set to a function.  Whenever a JSON object has been
    /// fully deserialized, this function will be called with the resulting
    /// `dict` as its only parameter.  The return value of the function (which
    /// may be the `dict`) will be substituted for the `dict` in the
    /// deserialized object graph.  This can be used to handle deserializing
    /// custom types.
    ///
    /// `depth_limit` specifies how deep the deserialized structure can be.  If
    /// provided and the given structure exceeds the depth limit, an error will
    /// be immediately raised.  Note that queson uses a heap-based stack when
    /// deserializing, which allows arbitrarily-deep structures to be
    /// deserialized without a stack overflow.  However, very deep structures
    /// will still cause a large number of Python objects to be allocated and
    /// can therefore take an unreasonable amount of time to fully deserialize,
    /// which may be a denial-of-service attack vector if the input is
    /// untrusted.
    #[pyfunction]
    #[pyo3(signature = (json, *, object_hook = None, depth_limit = None))]
    fn loadb<'py>(
        json: &Bound<'py, PyAny>,
        object_hook: Option<&'py Bound<'py, PyFunction>>,
        depth_limit: Option<NonZeroUsize>,
    ) -> PyResult<Bound<'py, PyAny>> {
        loads(json, object_hook, depth_limit)
    }

    /// Serialize a value into a JSON `str`.
    ///
    /// The following Python types are supported:
    ///
    /// * `None` becomes `null`.
    /// * `bool` becomes `true` or `false`.
    /// * `str` becomes a string.
    /// * `int` becomes a number.  Arbitrarily-large values are supported and
    ///   will be serialized with no loss of precision.
    /// * `float` becomes a number.  Non-finite values (`NaN`, `inf`, `-inf`)
    ///   will cause serialization to fail with a `ValueError`.
    /// * `list` and `tuple` become arrays.
    /// * `dict` becomes an object.  All supported key types are converted to
    ///   strings as required by the JSON spec.  Supported key types are:
    ///     * `str`
    ///     * `bool`
    ///     * `int`
    ///     * `None` (becomes `"null"`)
    /// * `queson.Fragment` dumps its contents directly.
    ///
    /// All other types may cause a `ValueError` to be raised; a provided object
    /// hook gets the opportunity to convert unsupported values to supported
    /// ones.
    ///
    /// `object_hook` can be set to a function.  If provided, it will be called
    /// any time a value of an unsupported type is encountered, and will be
    /// passed that value.  If the value returned by the function is of a
    /// supported type then serialization will proceed using that value instead;
    /// otherwise, serialization will fail with a `ValueError`.
    ///
    /// If `check_circular` is `True` (the default), then a cycle in the object
    /// graph to be serialized will result in a `ValueError`.  If this is
    /// disabled, cycles will not be detected and will cause this function to
    /// never return.  Note that a heap-based stack is used, which allows
    /// arbitrarily-deep structures to be serialized.  If a recursive structure
    /// is passed with `check_circular` disabled, the heap-based stack will
    /// continue to grow until something steps in to kill the Python process.
    /// This can cause "swap death" or other failure conditions.  Furthermore,
    /// in benchmarks comparing performance with `check_circular` enabled and
    /// disabled, no significant performance difference was detected.
    #[pyfunction]
    #[pyo3(signature = (value, *, object_hook = None, check_circular = true))]
    fn dumps<'py>(
        value: Bound<'py, PyAny>,
        object_hook: Option<&'py Bound<'py, PyFunction>>,
        check_circular: bool,
    ) -> PyResult<Bound<'py, PyString>> {
        PyString::from_bytes(
            value.py(),
            &crate::ser::into_json(value, object_hook, check_circular)?,
        )
    }

    /// Serialize a value into a UTF-8 encoded JSON `bytes`.
    ///
    /// The following Python types are supported:
    ///
    /// * `None` becomes `null`.
    /// * `bool` becomes `true` or `false`.
    /// * `str` becomes a string.
    /// * `int` becomes a number.  Arbitrarily-large values are supported and
    ///   will be serialized with no loss of precision.
    /// * `float` becomes a number.  Non-finite values (`NaN`, `inf`, `-inf`)
    ///   will cause serialization to fail with a `ValueError`.
    /// * `list` and `tuple` become arrays.
    /// * `dict` becomes an object.  All supported key types are converted to
    ///   strings as required by the JSON spec.  Supported key types are:
    ///     * `str`
    ///     * `bool`
    ///     * `int`
    ///     * `None` (becomes `"null"`)
    /// * `queson.Fragment` dumps its contents directly.
    ///
    /// All other types may cause a `ValueError` to be raised; a provided object
    /// hook gets the opportunity to convert unsupported values to supported
    /// ones.
    ///
    /// `object_hook` can be set to a function.  If provided, it will be called
    /// any time a value of an unsupported type is encountered, and will be
    /// passed that value.  If the value returned by the function is of a
    /// supported type then serialization will proceed using that value instead;
    /// otherwise, serialization will fail with a `ValueError`.
    ///
    /// If `check_circular` is `True` (the default), then a cycle in the object
    /// graph to be serialized will result in a `ValueError`.  If this is
    /// disabled, cycles will not be detected and will cause this function to
    /// never return.  Note that a heap-based stack is used, which allows
    /// arbitrarily-deep structures to be serialized.  If a recursive structure
    /// is passed with `check_circular` disabled, the heap-based stack will
    /// continue to grow until something steps in to kill the Python process.
    /// This can cause "swap death" or other failure conditions.  Furthermore,
    /// in benchmarks comparing performance with `check_circular` enabled and
    /// disabled, no significant performance difference was detected.
    #[pyfunction]
    #[pyo3(signature = (value, *, object_hook = None, check_circular = true))]
    fn dumpb<'py>(
        value: Bound<'py, PyAny>,
        object_hook: Option<&'py Bound<'py, PyFunction>>,
        check_circular: bool,
    ) -> PyResult<Bound<'py, PyBytes>> {
        Ok(PyBytes::new(
            value.py(),
            &crate::ser::into_json(value, object_hook, check_circular)?,
        ))
    }

    #[pymodule_export]
    use crate::ser::Fragment;
}
