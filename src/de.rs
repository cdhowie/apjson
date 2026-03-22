use std::borrow::Cow;

use pyo3::{
    exceptions::PyValueError,
    prelude::*,
    types::{PyBool, PyDict, PyFloat, PyFunction, PyInt, PyList, PyNone, PyString},
};

#[must_use]
enum TrampolineResult<'json, D: Deserialization> {
    Ok(D::Any),
    Err(ParseError<D::Error>),

    Incomplete(Trampoline<'json, D::List, D::Map>),
}

enum Trampoline<'json, TList, TDict> {
    ParsingList(TList),

    ParsingMap { dict: TDict, key: Cow<'json, str> },
}

macro_rules! trampoline_try {
    ( $e:expr ) => {
        match $e {
            ::std::result::Result::Ok(v) => v,
            ::std::result::Result::Err(e) => return TrampolineResult::Err(e.into()),
        }
    };
}

pub enum ParseError<E> {
    Eof,
    ExpectedEof,
    Expected(&'static str),
    ExpectedListItem,
    ExpectedMapItem,
    ExpectedDigit,
    ExpectedAny,
    InvalidUtf8,
    InvalidStringEscape,
    InvalidNumber,
    UnescapedControlCharacter,
    Custom(E),
}

impl From<PyErr> for ParseError<PyErr> {
    fn from(value: PyErr) -> Self {
        Self::Custom(value)
    }
}

impl From<ParseError<PyErr>> for PyErr {
    fn from(value: ParseError<PyErr>) -> Self {
        PyErr::new::<PyValueError, _>(match value {
            ParseError::Eof => "unexpected EOF",
            ParseError::ExpectedEof => "expected EOF",
            ParseError::ExpectedListItem => "expected list item",
            ParseError::ExpectedMapItem => "expected map item",
            ParseError::ExpectedDigit => "expected a decimal digit",
            ParseError::ExpectedAny => "expected a JSON value",
            ParseError::InvalidUtf8 => "invalid UTF-8 encoding",
            ParseError::InvalidStringEscape => "invalid string escape sequence",
            ParseError::InvalidNumber => "invalid number",
            ParseError::UnescapedControlCharacter => "unescaped control character in string",

            ParseError::Expected(s) => {
                return PyErr::new::<PyValueError, _>(format!("expected {s:?}"));
            }

            ParseError::Custom(e) => return e,
        })
    }
}

impl ParseError<PyErr> {
    fn into_pyerr_with_location<'py>(self, python: Python<'py>, location: usize) -> PyErr {
        let e: PyErr = self.into();

        // Disregard any error from add_note; we have no feasible way to
        // handle it, and it shouldn't happen anyway.
        e.add_note(python, format!("at input location {location}"))
            .ok();

        e
    }
}

/// Defines what kind of values are deserialized and how.
///
/// This can be used later to support deserializing to other kinds of types, or
/// even to support validation without deserialization by e.g. deserializing
/// everything to ().
trait Deserialization {
    type Any;

    type Null: Into<Self::Any>;
    type Bool: Into<Self::Any>;
    type String: Into<Self::Any>;
    type Number: Into<Self::Any>;
    type Map: Into<Self::Any>;
    type List: Into<Self::Any>;

    type Error;

    fn create_null(&self) -> Self::Null;

    fn create_bool(&self, value: bool) -> Self::Bool;

    fn create_string(&self, value: &str) -> Self::String;

    fn create_number(&self, value: &str, is_float: bool) -> Result<Self::Number, Self::Error>;

    fn create_map(&self) -> Self::Map;

    fn extend_map(
        &self,
        map: &mut Self::Map,
        key: Cow<'_, str>,
        value: Self::Any,
    ) -> Result<(), Self::Error>;

    fn finish_map(&self, map: Self::Map) -> Result<Self::Any, Self::Error>;

    fn create_list(&self) -> Self::List;

    fn extend_list(&self, list: &mut Self::List, value: Self::Any) -> Result<(), Self::Error>;
}

#[repr(transparent)]
struct BoundAny<'py>(Bound<'py, PyAny>);

impl<'py, T> From<Bound<'py, T>> for BoundAny<'py> {
    fn from(value: Bound<'py, T>) -> Self {
        Self(value.into_any())
    }
}

struct PyDeserialization<'py> {
    python: Python<'py>,
    object_hook: Option<&'py Bound<'py, PyFunction>>,
}

impl<'py> Deserialization for PyDeserialization<'py> {
    type Any = BoundAny<'py>;

    type Null = Bound<'py, PyNone>;
    type Bool = Bound<'py, PyBool>;
    type String = Bound<'py, PyString>;
    // Can be float or int, so must be any.
    type Number = BoundAny<'py>;
    type Map = Bound<'py, PyDict>;
    type List = Bound<'py, PyList>;

    type Error = PyErr;

    fn create_null(&self) -> Self::Null {
        PyNone::get(self.python).to_owned()
    }

    fn create_bool(&self, value: bool) -> Self::Bool {
        PyBool::new(self.python, value).to_owned()
    }

    fn create_string(&self, value: &str) -> Self::String {
        PyString::new(self.python, value)
    }

    fn create_number(&self, value: &str, is_float: bool) -> Result<Self::Number, Self::Error> {
        match is_float {
            false => {
                // Try parsing as a 64-bit number first; this is significantly
                // faster than using the Python int type constructor.
                //
                // We will use that constructor if parsing fails here in order
                // to support numbers that don't fit in 64 bits.

                if value.starts_with('-') {
                    if let Ok(parsed) = value.parse::<i64>() {
                        return Ok(PyInt::new(self.python, parsed).into());
                    }
                } else if let Ok(parsed) = value.parse::<u64>() {
                    return Ok(PyInt::new(self.python, parsed).into());
                }

                Ok(self.python.get_type::<PyInt>().call1((value,))?.into())
            }

            true => {
                let parsed: f64 = value.parse().map_err(|_| ParseError::InvalidNumber)?;

                Ok(PyFloat::new(self.python, parsed).into())
            }
        }
    }

    fn create_map(&self) -> Self::Map {
        PyDict::new(self.python)
    }

    fn extend_map(
        &self,
        map: &mut Self::Map,
        key: Cow<'_, str>,
        value: Self::Any,
    ) -> Result<(), Self::Error> {
        map.set_item(key, value.0)
    }

    fn finish_map(&self, map: Self::Map) -> Result<Self::Any, Self::Error> {
        match &self.object_hook {
            None => Ok(map.into()),
            Some(hook) => hook.call1((map,)).map(|r| r.into()),
        }
    }

    fn create_list(&self) -> Self::List {
        PyList::empty(self.python)
    }

    fn extend_list(&self, list: &mut Self::List, value: Self::Any) -> Result<(), Self::Error> {
        list.append(value.0)
    }
}

trait Cursor {
    fn peek<T>(&self) -> Result<u8, ParseError<T>>;

    fn skip(&mut self);

    fn skip_n(&mut self, n: usize);

    fn read<T>(&mut self) -> Result<u8, ParseError<T>>;

    fn consume_whitespace(&mut self);
}

impl Cursor for &[u8] {
    fn peek<T>(&self) -> Result<u8, ParseError<T>> {
        self.first().copied().ok_or(ParseError::Eof)
    }

    fn skip(&mut self) {
        self.skip_n(1);
    }

    fn skip_n(&mut self, n: usize) {
        *self = &self[n..];
    }

    fn read<T>(&mut self) -> Result<u8, ParseError<T>> {
        match self.first().copied() {
            Some(v) => {
                self.skip();
                Ok(v)
            }

            None => Err(ParseError::Eof),
        }
    }

    fn consume_whitespace(&mut self) {
        for i in 0..self.len() {
            if !matches!(self[i], b' ' | b'\n' | b'\r' | b'\t') {
                *self = &self[i..];
                return;
            }
        }

        *self = &[];
    }
}

trait Expect {
    fn expect(self, b: &mut &[u8]) -> bool;
}

impl<const N: usize> Expect for &[u8; N] {
    fn expect(self, b: &mut &[u8]) -> bool {
        if b.starts_with(self) {
            b.skip_n(N);
            true
        } else {
            false
        }
    }
}

impl Expect for u8 {
    fn expect(self, b: &mut &[u8]) -> bool {
        b.read::<()>().is_ok_and(|v| v == self)
    }
}

fn expect<T>(
    b: &mut &[u8],
    expected: impl Expect,
    err: impl FnOnce() -> ParseError<T>,
) -> Result<(), ParseError<T>> {
    match expected.expect(b) {
        true => Ok(()),
        false => Err(err()),
    }
}

fn parse_any<'json, D: Deserialization>(
    deserialization: &D,
    b: &mut &'json [u8],
) -> TrampolineResult<'json, D> {
    TrampolineResult::<D>::Ok(match b.peek() {
        Err(e) => return TrampolineResult::<D>::Err(e),

        Ok(b'n') => {
            b.skip();
            trampoline_try!(expect(b, b"ull", || ParseError::Expected("null")));
            deserialization.create_null().into()
        }

        Ok(b'f') => {
            b.skip();
            trampoline_try!(expect(b, b"alse", || ParseError::Expected("false")));
            deserialization.create_bool(false).into()
        }

        Ok(b't') => {
            b.skip();
            trampoline_try!(expect(b, b"rue", || ParseError::Expected("true")));
            deserialization.create_bool(true).into()
        }

        Ok(b'"') => {
            b.skip();
            deserialization
                .create_string(&trampoline_try!(parse_str(b)))
                .into()
        }

        Ok(b'[') => {
            b.skip();
            return parse_list(deserialization, b);
        }

        Ok(b'{') => {
            b.skip();
            return parse_map(deserialization, b);
        }

        Ok(b'-' | b'0'..=b'9') => trampoline_try!(parse_number(deserialization, b)).into(),

        Ok(_) => return TrampolineResult::<D>::Err(ParseError::ExpectedAny),
    })
}

#[allow(clippy::manual_is_ascii_check)]
fn parse_number<D: Deserialization>(
    deserialization: &D,
    b: &mut &[u8],
) -> Result<D::Number, ParseError<D::Error>> {
    let start = *b;
    let mut is_float = false;

    let c = match b.read()? {
        b'-' => b.read()?,

        c => c,
    };

    match c {
        b'1'..=b'9' => {
            while matches!(b.first(), Some(b'0'..=b'9')) {
                b.skip();
            }
        }

        b'0' => {}

        _ => return Err(ParseError::ExpectedDigit),
    }

    if matches!(b.peek::<()>(), Ok(b'.')) {
        b.skip();
        is_float = true;

        if !matches!(b.read()?, b'0'..=b'9') {
            return Err(ParseError::ExpectedDigit);
        }

        while matches!(b.first(), Some(b'0'..=b'9')) {
            b.skip();
        }
    }

    if matches!(b.first(), Some(b'E' | b'e')) {
        b.skip();
        is_float = true;

        if matches!(b.first(), Some(b'-' | b'+')) {
            b.skip();
        }

        if !matches!(b.read()?, b'0'..=b'9') {
            return Err(ParseError::ExpectedDigit);
        }

        while matches!(b.first(), Some(b'0'..=b'9')) {
            b.skip();
        }
    }

    let bytes = start.len() - b.len();

    let number = &start[0..bytes];

    // SAFETY: The byte slice only contains ASCII characters, otherwise we would
    // have already returned Err somewhere above.
    let number = unsafe { str::from_utf8_unchecked(number) };

    deserialization
        .create_number(number, is_float)
        .map_err(ParseError::Custom)
}

fn read_utf8_char<T>(b: &mut &[u8]) -> Result<char, ParseError<T>> {
    let first = b.read()?;

    let (mut val, additional) = if first & 0b10000000 == 0 {
        return Ok(first as char);
    } else if first & 0b11100000 == 0b11000000 {
        (u32::from(first & 0b00011111), 1)
    } else if first & 0b11110000 == 0b11100000 {
        (u32::from(first & 0b00001111), 2)
    } else if first & 0b11111000 == 0b11110000 {
        (u32::from(first & 0b00000111), 3)
    } else {
        return Err(ParseError::InvalidUtf8);
    };

    for _ in 0..additional {
        let c = b.read::<()>().map_err(|_| ParseError::InvalidUtf8)?;

        if c & 0b11000000 != 0b10000000 {
            return Err(ParseError::InvalidUtf8);
        }

        val = (val << 6) | u32::from(c & 0b00111111);
    }

    char::from_u32(val).ok_or(ParseError::InvalidUtf8)
}

fn parse_str<'json, T>(b: &mut &'json [u8]) -> Result<Cow<'json, str>, ParseError<T>> {
    let start = *b;

    // Start under the assumption that we can borrow the encoded string.  The
    // only thing that can make this impossible is escape sequences.
    let mut buf = loop {
        match b.peek()? {
            b'"' => {
                let bytes = start.len() - b.len();
                b.skip();

                return Ok(Cow::Borrowed(
                    str::from_utf8(&start[0..bytes]).map_err(|_| ParseError::InvalidUtf8)?,
                ));
            }

            b'\\' => {
                let bytes = start.len() - b.len();

                // Convert what we've already read into an owned string and fall
                // down to the next loop, which handles building an owned
                // string.
                break str::from_utf8(&start[0..bytes])
                    .map_err(|_| ParseError::InvalidUtf8)?
                    .to_owned();
            }

            c if c < b' ' => return Err(ParseError::UnescapedControlCharacter),

            _ => {
                b.skip();
            }
        }
    };

    loop {
        let c = read_utf8_char(b)?;

        match c {
            '\\' => match read_utf8_char(b)? {
                'b' => buf.push('\x08'),
                'f' => buf.push('\x0C'),
                'n' => buf.push('\n'),
                'r' => buf.push('\r'),
                't' => buf.push('\t'),
                'u' => {
                    let c1 = parse_unicode_escape(b)?;

                    buf.push(match char::from_u32(c1.into()) {
                        Some(c) => c,

                        None => match c1 {
                            // Leading surrogate.
                            0xD800..=0xDBFF => {
                                expect(b, b"\\u", || ParseError::InvalidStringEscape)?;

                                let c2 = parse_unicode_escape(b)?;

                                if !matches!(c2, 0xDC00..=0xDFFF) {
                                    return Err(ParseError::InvalidStringEscape);
                                }

                                char::from_u32(
                                    (u32::from(c1 - 0xD800) << 10) + u32::from(c2 - 0xDC00),
                                )
                                .ok_or(ParseError::InvalidStringEscape)?
                            }

                            // Trailing surrogate without leading surrogate.
                            0xDC00..=0xDFFF => return Err(ParseError::InvalidStringEscape),

                            // from_u32 should have returned Some in this case.
                            _ => unreachable!(),
                        },
                    });
                }

                c @ ('\\' | '/' | '"') => buf.push(c),

                _ => return Err(ParseError::InvalidStringEscape),
            },

            '"' => return Ok(Cow::Owned(buf)),

            c if c < ' ' => return Err(ParseError::UnescapedControlCharacter),

            c => buf.push(c),
        };
    }
}

fn parse_unicode_escape<T>(b: &mut &[u8]) -> Result<u16, ParseError<T>> {
    if b.len() < 4 {
        return Err(ParseError::Eof);
    };

    let hex = str::from_utf8(&b[0..4]).map_err(|_| ParseError::InvalidUtf8)?;

    b.skip_n(4);

    if hex.len() != 4 {
        return Err(ParseError::InvalidStringEscape);
    }

    u16::from_str_radix(hex, 16).map_err(|_| ParseError::InvalidStringEscape)
}

fn parse_list<'json, D: Deserialization>(
    deserialization: &D,
    b: &mut &'json [u8],
) -> TrampolineResult<'json, D> {
    let list = deserialization.create_list();

    b.consume_whitespace();

    if trampoline_try!(b.peek()) == b']' {
        b.skip();
        return TrampolineResult::<D>::Ok(list.into());
    }

    TrampolineResult::Incomplete(Trampoline::ParsingList(list))
}

fn continue_parse_list<'json, D: Deserialization>(
    deserialization: &D,
    mut list: D::List,
    value: D::Any,
    b: &mut &'json [u8],
) -> TrampolineResult<'json, D> {
    trampoline_try!(
        deserialization
            .extend_list(&mut list, value)
            .map_err(ParseError::Custom)
    );

    b.consume_whitespace();

    match trampoline_try!(b.read()) {
        b']' => TrampolineResult::Ok(list.into()),

        b',' => {
            b.consume_whitespace();
            TrampolineResult::Incomplete(Trampoline::ParsingList(list))
        }

        _ => TrampolineResult::Err(ParseError::ExpectedListItem),
    }
}

fn parse_map<'json, D: Deserialization>(
    deserialization: &D,
    b: &mut &'json [u8],
) -> TrampolineResult<'json, D> {
    let dict = deserialization.create_map();

    b.consume_whitespace();

    let key = match trampoline_try!(b.read()) {
        b'}' => {
            return TrampolineResult::<D>::Ok(trampoline_try!(
                deserialization.finish_map(dict).map_err(ParseError::Custom)
            ));
        }

        b'"' => trampoline_try!(parse_str(b)),

        _ => return TrampolineResult::<D>::Err(ParseError::ExpectedMapItem),
    };

    b.consume_whitespace();
    trampoline_try!(expect(b, b':', || ParseError::ExpectedMapItem));
    b.consume_whitespace();

    TrampolineResult::Incomplete(Trampoline::ParsingMap { dict, key })
}

fn continue_parse_map<'json, D: Deserialization>(
    deserialization: &D,
    mut dict: D::Map,
    key: Cow<'json, str>,
    value: D::Any,
    b: &mut &'json [u8],
) -> TrampolineResult<'json, D> {
    trampoline_try!(
        deserialization
            .extend_map(&mut dict, key, value)
            .map_err(ParseError::Custom)
    );

    b.consume_whitespace();

    match trampoline_try!(b.read()) {
        b'}' => TrampolineResult::Ok(trampoline_try!(
            deserialization.finish_map(dict).map_err(ParseError::Custom)
        )),

        b',' => {
            b.consume_whitespace();
            trampoline_try!(expect(b, b'"', || ParseError::ExpectedMapItem));

            let key = trampoline_try!(parse_str(b));

            b.consume_whitespace();
            trampoline_try!(expect(b, b':', || ParseError::ExpectedMapItem));
            b.consume_whitespace();

            TrampolineResult::Incomplete(Trampoline::ParsingMap { dict, key })
        }

        _ => TrampolineResult::Err(ParseError::ExpectedMapItem),
    }
}

pub fn parse_json<'py>(
    python: Python<'py>,
    mut json: &[u8],
    object_hook: Option<&'py Bound<'py, PyFunction>>,
) -> Result<Bound<'py, PyAny>, PyErr> {
    let len = json.len();

    json.consume_whitespace();

    let deserialization = PyDeserialization {
        python,
        object_hook,
    };

    let mut stack = vec![];

    let mut last_any = parse_any(&deserialization, &mut json);

    let result = loop {
        last_any = match last_any {
            TrampolineResult::Err(e) => {
                return Err(e.into_pyerr_with_location(python, len - json.len()));
            }

            TrampolineResult::Incomplete(op) => {
                stack.push(op);

                parse_any(&deserialization, &mut json)
            }

            TrampolineResult::Ok(value) => match stack.pop() {
                Some(op) => match op {
                    Trampoline::ParsingList(list) => {
                        continue_parse_list(&deserialization, list, value, &mut json)
                    }

                    Trampoline::ParsingMap { dict, key } => {
                        continue_parse_map(&deserialization, dict, key, value, &mut json)
                    }
                },

                None => break value.0,
            },
        };
    };

    json.consume_whitespace();

    match json.is_empty() {
        true => Ok(result),
        false => Err(ParseError::ExpectedEof.into_pyerr_with_location(python, len - json.len())),
    }
}
