use anyhow::anyhow;
use bytes::{Buf, Bytes};
use std::convert::TryInto;
use std::io::Cursor;
use std::num::TryFromIntError;
use std::string::FromUtf8Error;
use std::{fmt, str, vec};

#[derive(Clone, Debug)]
pub enum Frame {
    Simple(String),
    Error(String),
    Integer(u64),
    Bulk(Bytes),
    Null,
    Array(Vec<Frame>),
}

#[derive(Debug)]
pub enum Error {
    Incomplete,

    Other(anyhow::Error),
}

#[derive(Debug)]
pub struct Parse {
    parts: vec::IntoIter<Frame>,
}

#[derive(Debug)]
pub enum ParseError {
    EndOfStream,

    Other(anyhow::Error),
}

impl Frame {
    pub(crate) fn array() -> Frame {
        Frame::Array(vec![])
    }

    pub(crate) fn push_frame(&mut self, frame: Frame) {
        match self {
            Frame::Array(vec) => {
                vec.push(frame);
            }
            _ => panic!("not an array frame"),
        }
    }

    pub(crate) fn push_bulk(&mut self, bytes: Bytes) {
        match self {
            Frame::Array(vec) => {
                vec.push(Frame::Bulk(bytes));
            }
            _ => panic!("not an array frame"),
        }
    }

    pub(crate) fn push_int(&mut self, value: u64) {
        match self {
            Frame::Array(vec) => {
                vec.push(Frame::Integer(value));
            }
            _ => panic!("not an array frame"),
        }
    }

    pub(crate) fn push_req_id(self, req_id: u64) -> Frame {
        match self {
            Frame::Array(mut vec) => {
                vec.push(Frame::Integer(req_id));
                Frame::Array(vec)
            }
            _ => panic!("not an array frame"),
        }
    }

    /// Checks if an entire message can be decoded from `src`
    pub fn check(src: &mut Cursor<&[u8]>) -> anyhow::Result<(), Error> {
        match get_u8(src)? {
            b'+' => {
                get_line(src)?;
                Ok(())
            }
            b'-' => {
                get_line(src)?;
                Ok(())
            }
            b':' => {
                let _ = get_decimal(src)?;
                Ok(())
            }
            b'$' => {
                if b'-' == peek_u8(src)? {
                    // Skip '-1\r\n'
                    skip(src, 4)
                } else {
                    // Read the bulk string
                    let len: usize = get_decimal(src)?.try_into()?;

                    // skip that number of bytes + 2 (\r\n).
                    skip(src, len + 2)
                }
            }
            b'*' => {
                let len = get_decimal(src)?;

                for _ in 0..len {
                    Frame::check(src)?;
                }

                Ok(())
            }
            actual => Err(format!("protocol error; invalid frame type byte `{}`", actual).into()),
        }
    }

    pub fn parse(src: &mut Cursor<&[u8]>) -> anyhow::Result<Frame, Error> {
        match get_u8(src)? {
            b'+' => {
                // Read the line and convert it to `Vec<u8>`
                let line = get_line(src)?.to_vec();

                // Convert the line to a String
                let string = String::from_utf8(line)?;

                Ok(Frame::Simple(string))
            }
            b'-' => {
                // Read the line and convert it to `Vec<u8>`
                let line = get_line(src)?.to_vec();

                // Convert the line to a String
                let string = String::from_utf8(line)?;

                Ok(Frame::Error(string))
            }
            b':' => {
                let len = get_decimal(src)?;
                Ok(Frame::Integer(len))
            }
            b'$' => {
                if b'-' == peek_u8(src)? {
                    let line = get_line(src)?;

                    if line != b"-1" {
                        return Err("protocol error; invalid frame format".into());
                    }

                    Ok(Frame::Null)
                } else {
                    // Read the bulk string
                    let len = get_decimal(src)?.try_into()?;
                    let n = len + 2;

                    if src.remaining() < n {
                        return Err(Error::Incomplete);
                    }

                    let data = Bytes::copy_from_slice(&src.chunk()[..len]);

                    // skip that number of bytes + 2 (\r\n).
                    skip(src, n)?;

                    Ok(Frame::Bulk(data))
                }
            }
            b'*' => {
                let len = get_decimal(src)?.try_into()?;
                let mut out = Vec::with_capacity(len);

                for _ in 0..len {
                    out.push(Frame::parse(src)?);
                }

                Ok(Frame::Array(out))
            }
            _ => unimplemented!(),
        }
    }
}

impl Parse {
    pub(crate) fn new(frame: Frame) -> anyhow::Result<Parse, ParseError> {
        let array = match frame {
            Frame::Array(array) => array,
            frame => return Err(format!("protocol error; expected array, got {:?}", frame).into()),
        };

        Ok(Parse {
            parts: array.into_iter(),
        })
    }

    pub fn next_part(&mut self) -> anyhow::Result<Frame, ParseError> {
        self.parts.next().ok_or(ParseError::EndOfStream)
    }

    pub(crate) fn next_string(&mut self) -> anyhow::Result<String, ParseError> {
        match self.next_part()? {
            Frame::Simple(s) => Ok(s),
            Frame::Bulk(data) => str::from_utf8(&data[..])
                .map(|s| s.to_string())
                .map_err(|_| "protocol error; invalid string".into()),
            frame => Err(format!(
                "protocol error; expected simple frame or bulk frame, got {:?}",
                frame
            )
            .into()),
        }
    }

    pub(crate) fn next_bytes(&mut self) -> anyhow::Result<Bytes, ParseError> {
        match self.next_part()? {
            Frame::Simple(s) => Ok(Bytes::from(s.into_bytes())),
            Frame::Bulk(data) => Ok(data),
            frame => Err(format!(
                "protocol error; expected simple frame or bulk frame, got {:?}",
                frame
            )
            .into()),
        }
    }

    pub(crate) fn next_int(&mut self) -> anyhow::Result<u64, ParseError> {
        use atoi::atoi;

        const MSG: &str = "protocol error; invalid number";

        match self.next_part()? {
            Frame::Integer(v) => Ok(v),
            Frame::Simple(data) => atoi::<u64>(data.as_bytes()).ok_or_else(|| MSG.into()),
            Frame::Bulk(data) => atoi::<u64>(&data).ok_or_else(|| MSG.into()),
            frame => Err(format!("protocol error; expected int frame but got {:?}", frame).into()),
        }
    }

    pub(crate) fn finish(&mut self) -> anyhow::Result<(), ParseError> {
        if self.parts.next().is_none() {
            Ok(())
        } else {
            Err("protocol error; expected end of frame, but there was more".into())
        }
    }
}

impl PartialEq<&str> for Frame {
    fn eq(&self, other: &&str) -> bool {
        match self {
            Frame::Simple(s) => s.eq(other),
            Frame::Bulk(s) => s.eq(other),
            _ => false,
        }
    }
}

impl fmt::Display for Frame {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Frame::Simple(response) => response.fmt(fmt),
            Frame::Error(msg) => write!(fmt, "error: {}", msg),
            Frame::Integer(num) => num.fmt(fmt),
            Frame::Bulk(msg) => {
                if msg.len() <= 128 {
                    match str::from_utf8(msg) {
                        Ok(string) => string.fmt(fmt),
                        Err(_) => write!(fmt, "{:?}", msg),
                    }
                } else {
                    msg.len().fmt(fmt)
                }
            }
            Frame::Null => "(nil)".fmt(fmt),
            Frame::Array(parts) => {
                write!(fmt, "[")?;
                for (i, part) in parts.iter().enumerate() {
                    if i != 0 {
                        write!(fmt, ", ")?;
                    }
                    part.fmt(fmt)?;
                }
                write!(fmt, "]")?;

                Ok(())
            }
        }
    }
}

fn peek_u8(src: &mut Cursor<&[u8]>) -> anyhow::Result<u8, Error> {
    if !src.has_remaining() {
        return Err(Error::Incomplete);
    }

    Ok(src.chunk()[0])
}

fn get_u8(src: &mut Cursor<&[u8]>) -> anyhow::Result<u8, Error> {
    if !src.has_remaining() {
        return Err(Error::Incomplete);
    }

    Ok(src.get_u8())
}

fn skip(src: &mut Cursor<&[u8]>, n: usize) -> anyhow::Result<(), Error> {
    if src.remaining() < n {
        return Err(Error::Incomplete);
    }

    src.advance(n);
    Ok(())
}

/// Read a new-line terminated decimal
fn get_decimal(src: &mut Cursor<&[u8]>) -> anyhow::Result<u64, Error> {
    use atoi::atoi;

    let line = get_line(src)?;

    atoi::<u64>(line).ok_or_else(|| "protocol error; invalid frame format".into())
}

/// Find a line
fn get_line<'a>(src: &mut Cursor<&'a [u8]>) -> anyhow::Result<&'a [u8], Error> {
    // Scan the bytes directly
    let start = src.position() as usize;
    // Scan to the second to last byte
    let end = src.get_ref().len() - 1;

    for i in start..end {
        if src.get_ref()[i] == b'\r' && src.get_ref()[i + 1] == b'\n' {
            // We found a line, update the position to be *after* the \n
            src.set_position((i + 2) as u64);

            // Return the line
            return Ok(&src.get_ref()[start..i]);
        }
    }

    Err(Error::Incomplete)
}

impl From<String> for Error {
    fn from(src: String) -> Error {
        Error::Other(anyhow!(src))
    }
}

impl From<&str> for Error {
    fn from(src: &str) -> Error {
        src.to_string().into()
    }
}

impl From<FromUtf8Error> for Error {
    fn from(_src: FromUtf8Error) -> Error {
        "protocol error; invalid frame format".into()
    }
}

impl From<TryFromIntError> for Error {
    fn from(_src: TryFromIntError) -> Error {
        "protocol error; invalid frame format".into()
    }
}

impl std::error::Error for Error {}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::Incomplete => "stream ended early".fmt(fmt),
            Error::Other(err) => err.fmt(fmt),
        }
    }
}

impl From<String> for ParseError {
    fn from(src: String) -> ParseError {
        ParseError::Other(anyhow!(src))
    }
}

impl From<&str> for ParseError {
    fn from(src: &str) -> ParseError {
        src.to_string().into()
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::EndOfStream => "protocol error; unexpected end of stream".fmt(f),
            ParseError::Other(err) => err.fmt(f),
        }
    }
}

impl std::error::Error for ParseError {}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    #[test]
    fn test_frame_simple_parse() {
        let data = b"+hello\r\n";
        let mut cursor = Cursor::new(data.as_slice());
        let frame = Frame::parse(&mut cursor).unwrap();
        assert!(frame == "hello");
    }

    #[test]
    fn test_frame_error_parse() {
        let data = b"-error message\r\n";
        let mut cursor = Cursor::new(data.as_slice());
        let frame = Frame::parse(&mut cursor).unwrap();
        match frame {
            Frame::Error(msg) => assert_eq!(msg, "error message"),
            _ => panic!("expected error frame"),
        }
    }

    #[test]
    fn test_frame_integer_parse() {
        let data = b":12345\r\n";
        let mut cursor = Cursor::new(data.as_slice());
        let frame = Frame::parse(&mut cursor).unwrap();
        match frame {
            Frame::Integer(val) => assert_eq!(val, 12345),
            _ => panic!("expected integer frame"),
        }
    }

    #[test]
    fn test_frame_bulk_parse() {
        let data = b"$5\r\nhello\r\n";
        let mut cursor = Cursor::new(data.as_slice());
        let frame = Frame::parse(&mut cursor).unwrap();
        match frame {
            Frame::Bulk(bytes) => assert_eq!(&bytes[..], b"hello"),
            _ => panic!("expected bulk frame"),
        }
    }

    #[test]
    fn test_frame_bulk_null_parse() {
        let data = b"$-1\r\n";
        let mut cursor = Cursor::new(data.as_slice());
        let frame = Frame::parse(&mut cursor).unwrap();
        match frame {
            Frame::Null => {}
            _ => panic!("expected null frame"),
        }
    }

    #[test]
    fn test_frame_array_parse() {
        let data = b"*2\r\n+hello\r\n:12345\r\n";
        let mut cursor = Cursor::new(data.as_slice());
        let frame = Frame::parse(&mut cursor).unwrap();
        match frame {
            Frame::Array(arr) => {
                assert_eq!(arr.len(), 2);
                assert!(arr[0] == "hello");
                match &arr[1] {
                    Frame::Integer(val) => assert_eq!(*val, 12345),
                    _ => panic!("expected integer frame"),
                }
            }
            _ => panic!("expected array"),
        }
    }

    #[test]
    fn test_frame_check_simple() {
        let data = b"+hello\r\n";
        let mut cursor = Cursor::new(data.as_slice());
        assert!(Frame::check(&mut cursor).is_ok());
    }

    #[test]
    fn test_frame_check_incomplete() {
        let data = b"+hello";
        let mut cursor = Cursor::new(data.as_slice());
        assert!(matches!(Frame::check(&mut cursor), Err(Error::Incomplete)));
    }

    #[test]
    fn test_frame_check_invalid_type() {
        let data = b"#invalid\r\n";
        let mut cursor = Cursor::new(data.as_slice());
        let result = Frame::check(&mut cursor);
        assert!(result.is_err());
    }

    #[test]
    fn test_frame_check_bulk_incomplete() {
        let data = b"$5\r\nhello";
        let mut cursor = Cursor::new(data.as_slice());
        assert!(matches!(Frame::check(&mut cursor), Err(Error::Incomplete)));
    }

    #[test]
    fn test_frame_check_array() {
        let data = b"*2\r\n+hello\r\n:12345\r\n";
        let mut cursor = Cursor::new(data.as_slice());
        assert!(Frame::check(&mut cursor).is_ok());
    }

    #[test]
    fn test_parse_new_from_array_frame() {
        let frame = Frame::Array(vec![
            Frame::Bulk(Bytes::from("ping")),
            Frame::Bulk(Bytes::from("message")),
        ]);
        let mut parse = Parse::new(frame).unwrap();
        assert!(parse.next_part().is_ok());
    }

    #[test]
    fn test_parse_new_from_non_array_error() {
        let frame = Frame::Simple("hello".to_string());
        let result = Parse::new(frame);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_next_string_from_simple() {
        let mut parse = Parse::new(Frame::Array(vec![Frame::Simple("test".to_string())])).unwrap();
        let result = parse.next_string().unwrap();
        assert_eq!(result, "test");
    }

    #[test]
    fn test_parse_next_string_from_bulk() {
        let mut parse = Parse::new(Frame::Array(vec![Frame::Bulk(Bytes::from("bulk"))])).unwrap();
        let result = parse.next_string().unwrap();
        assert_eq!(result, "bulk");
    }

    #[test]
    fn test_parse_next_string_error() {
        let mut parse = Parse::new(Frame::Array(vec![Frame::Integer(123)])).unwrap();
        let result = parse.next_string();
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_next_int_from_integer() {
        let mut parse = Parse::new(Frame::Array(vec![Frame::Integer(42)])).unwrap();
        let result = parse.next_int().unwrap();
        assert_eq!(result, 42);
    }

    #[test]
    fn test_parse_next_int_from_simple() {
        let mut parse = Parse::new(Frame::Array(vec![Frame::Simple("123".to_string())])).unwrap();
        let result = parse.next_int().unwrap();
        assert_eq!(result, 123);
    }

    #[test]
    fn test_parse_next_int_invalid() {
        let mut parse = Parse::new(Frame::Array(vec![Frame::Simple("abc".to_string())])).unwrap();
        let result = parse.next_int();
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_next_bytes() {
        let mut parse = Parse::new(Frame::Array(vec![Frame::Bulk(Bytes::from("data"))])).unwrap();
        let result = parse.next_bytes().unwrap();
        assert_eq!(result, Bytes::from("data"));
    }

    #[test]
    fn test_parse_finish_success() {
        let mut parse = Parse::new(Frame::Array(vec![])).unwrap();
        assert!(parse.finish().is_ok());
    }

    #[test]
    fn test_parse_finish_with_extra_data() {
        let mut parse = Parse::new(Frame::Array(vec![
            Frame::Simple("first".to_string()),
            Frame::Simple("second".to_string()),
        ]))
        .unwrap();
        let _ = parse.next_part().unwrap();
        let result = parse.finish();
        assert!(result.is_err());
    }

    #[test]
    fn test_frame_partial_eq_simple() {
        assert_eq!(Frame::Simple("hello".to_string()), "hello");
    }

    #[test]
    fn test_frame_partial_eq_bulk() {
        assert_eq!(Frame::Bulk(Bytes::from("hello")), "hello");
    }

    #[test]
    fn test_frame_partial_eq_not_equal() {
        assert_ne!(Frame::Simple("hello".to_string()), "world");
    }

    #[test]
    fn test_frame_display_simple() {
        let frame = Frame::Simple("hello".to_string());
        assert_eq!(format!("{}", frame), "hello");
    }

    #[test]
    fn test_frame_display_error() {
        let frame = Frame::Error("error msg".to_string());
        assert_eq!(format!("{}", frame), "error: error msg");
    }

    #[test]
    fn test_frame_display_integer() {
        let frame = Frame::Integer(42);
        assert_eq!(format!("{}", frame), "42");
    }

    #[test]
    fn test_frame_display_null() {
        let frame = Frame::Null;
        assert_eq!(format!("{}", frame), "(nil)");
    }

    #[test]
    fn test_frame_display_array() {
        let frame = Frame::Array(vec![Frame::Simple("a".to_string()), Frame::Integer(1)]);
        assert_eq!(format!("{}", frame), "[a, 1]");
    }

    #[test]
    fn test_frame_array_creation() {
        let mut frame = Frame::array();
        frame.push_frame(Frame::Simple("test".to_string()));
        match frame {
            Frame::Array(vec) => assert_eq!(vec.len(), 1),
            _ => panic!("expected array"),
        }
    }

    #[test]
    fn test_frame_push_bulk() {
        let mut frame = Frame::array();
        frame.push_bulk(Bytes::from("bulk"));
        match frame {
            Frame::Array(vec) => {
                assert_eq!(vec.len(), 1);
                match &vec[0] {
                    Frame::Bulk(bytes) => assert_eq!(&bytes[..], b"bulk"),
                    _ => panic!("expected bulk frame"),
                }
            }
            _ => panic!("expected array"),
        }
    }

    #[test]
    fn test_frame_push_int() {
        let mut frame = Frame::array();
        frame.push_int(100);
        match frame {
            Frame::Array(vec) => {
                assert_eq!(vec.len(), 1);
                match &vec[0] {
                    Frame::Integer(val) => assert_eq!(*val, 100),
                    _ => panic!("expected integer frame"),
                }
            }
            _ => panic!("expected array"),
        }
    }

    #[test]
    fn test_frame_push_req_id() {
        let mut frame = Frame::array();
        frame.push_bulk(Bytes::from("test"));
        let result = frame.push_req_id(42);
        match result {
            Frame::Array(vec) => {
                assert_eq!(vec.len(), 2);
                assert!(vec[0] == "test");
                match &vec[1] {
                    Frame::Integer(val) => assert_eq!(*val, 42),
                    _ => panic!("expected integer frame"),
                }
            }
            _ => panic!("expected array"),
        }
    }
}
