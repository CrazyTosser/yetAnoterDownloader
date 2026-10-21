use std::collections::BTreeMap;
use std::mem;
use std::num::ParseIntError;
use std::str::Utf8Error;
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum BencodeParseError {
    #[error("empty integer found")]
    EmptyIntegerFound,
    #[error("missing terminator, expected '{expected}'")]
    MissingTerminator {
        expected: u8,
    },
    #[error("negative zero found")]
    NegativeZero,
    #[error("leading zero not approved")]
    LeadingZero,
    #[error(transparent)]
    InvalidInteger(#[from] Utf8Error),
    #[error(transparent)]
    Parse(#[from] ParseIntError),

    #[error("missing colon symbol")]
    ColonMissing,
    #[error("negative or zero size given: {given}")]
    NegativeOrZeroSize {
        given: i64,
    },
    #[error("unterminated string: expected len {expected}, got {got}")]
    InsufficientData {
        expected: usize,
        got: usize,
    },
    #[error("input string is empty")]
    NoDataFound,
    #[error("wrong bencode structure")]
    WrongStructure,
}

impl From<BencodeParseError> for std::io::Error {
    fn from(err: BencodeParseError) -> Self {
        std::io::Error::new(std::io::ErrorKind::InvalidData, err.to_string())
    }
}

pub fn decode_int(input: &[u8]) -> Result<(i128, &[u8]), BencodeParseError> {
    if !input.starts_with(b"i") {
        return Err(BencodeParseError::MissingTerminator { expected: b'i' });
    }
    let end = input
        .iter()
        .position(|&b| b == b'e')
        .ok_or(BencodeParseError::MissingTerminator { expected: b'e' })?;
    let number = &input[1..end];
    if number.is_empty() {
        return Err(BencodeParseError::EmptyIntegerFound);
    } else if number.len() >= 2 && number[0] == b'-' && number[1] == b'0' {
        return Err(BencodeParseError::NegativeZero);
    } else if number.len() >= 1 && number[0] == b'0' {
        return Err(BencodeParseError::LeadingZero);
    }
    let s = std::str::from_utf8(number).map_err(|e| BencodeParseError::InvalidInteger(e))?;
    let val = i128::from_str_radix(s, 10).map_err(|e| BencodeParseError::Parse(e))?;
    Ok((val, &input[end + 1..]))
}


pub fn decode_string(input: &[u8]) -> Result<(&[u8], &[u8]), BencodeParseError> {
    let delimeter = input.iter().position(|&b| b == b':')
        .ok_or(BencodeParseError::ColonMissing)?;
    let left = std::str::from_utf8(&input[..delimeter]).map_err(|e| BencodeParseError::InvalidInteger(e))?;
    let size = i64::from_str_radix(&left, 10).map_err(|e| BencodeParseError::Parse(e))?;
    if size < 0 {
        return Err(BencodeParseError::NegativeOrZeroSize { given: size });
    }
    if delimeter + 1 + size as usize > input.len() {
        return Err(BencodeParseError::InsufficientData { expected: size as usize, got: input.len() - delimeter - 1 });
    }
    Ok((&input[delimeter + 1..delimeter + 1 + (size as usize)], &input[delimeter + (size as usize) + 1..]))
}


#[derive(PartialEq, Debug)]
pub enum BencodeValue {
    Integer(i128),
    Bytes(Vec<u8>),
    List(Vec<BencodeValue>),
    Dict(BTreeMap<Vec<u8>, BencodeValue>),
}

impl std::fmt::Display for BencodeValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.fmt_with_indent(f, 0)
    }
}

#[warn(dead_code)]
impl BencodeValue {
    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            BencodeValue::Bytes(b) => Some(b),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        self.as_bytes().and_then(|b| std::str::from_utf8(b).ok())
    }

    pub fn as_integer(&self) -> Option<i128> {
        match self {
            BencodeValue::Integer(i) => Some(*i),
            _ => None,
        }
    }

    pub fn as_list(&self) -> Option<&Vec<BencodeValue>> {
        match self {
            BencodeValue::List(l) => Some(l),
            _ => None,
        }
    }

    pub fn as_dict(&self) -> Option<&BTreeMap<Vec<u8>, BencodeValue>> {
        match self {
            BencodeValue::Dict(d) => Some(d),
            _ => None,
        }
    }

    pub fn get(&self, key: &[u8]) -> Option<&BencodeValue> {
        self.as_dict().and_then(|d| d.get(key))
    }

    fn fmt_with_indent(&self, f: &mut std::fmt::Formatter<'_>, indent: usize) -> std::fmt::Result {
        let prefix = "  ".repeat(indent);

        match self {
            BencodeValue::Integer(i) => write!(f, "{}i{}", prefix, i),
            BencodeValue::Bytes(b) => {
                // Пробуем показать как строку, если это UTF-8, иначе как hex
                if let Ok(s) = std::str::from_utf8(b) {
                    write!(f, "{}\"{}\"", prefix, s)
                } else {
                    write!(f, "{}<bytes len={}>", prefix, b.len())
                }
            }
            BencodeValue::List(items) => {
                writeln!(f, "{}[", prefix)?;
                for item in items {
                    item.fmt_with_indent(f, indent + 1)?;
                    writeln!(f)?;
                }
                write!(f, "{}]", prefix)
            }
            BencodeValue::Dict(map) => {
                writeln!(f, "{}{{", prefix)?;
                for (key, value) in map {
                    let key_str = String::from_utf8_lossy(key);
                    write!(f, "{}  \"{}\": ", prefix, key_str)?;
                    value.fmt_with_indent(f, indent + 1)?;
                    writeln!(f)?;
                }
                write!(f, "{}}}", prefix)
            }
        }
    }
}

enum Frame {
    List(Vec<BencodeValue>),
    DictKey(BTreeMap<Vec<u8>, BencodeValue>),
    DictValue(BTreeMap<Vec<u8>, BencodeValue>, Vec<u8>),
}

fn apply_frame(stack: &mut Vec<Frame>, value: BencodeValue) -> Result<(), BencodeParseError> {
    if stack.is_empty() {
        return Err(BencodeParseError::WrongStructure);
    }
    let mut frame = stack.pop().ok_or(BencodeParseError::WrongStructure)?;
    match frame {
        Frame::List(ref mut items) => {
            items.push(value);
            stack.push(frame);
        }
        Frame::DictKey(ref mut map) => {
            if let BencodeValue::Bytes(bytes) = value {
                stack.push(Frame::DictValue(mem::take(map), bytes));
            } else {
                return Err(BencodeParseError::WrongStructure);
            }
        }
        Frame::DictValue(ref mut map, key) => {
            map.insert(key, value);
            stack.push(Frame::DictKey(mem::take(map)));
        }
    }
    Ok(())
}

fn unpack_frame(frame: Frame) -> Result<BencodeValue, BencodeParseError> {
    match frame {
        Frame::List(items) => Ok(BencodeValue::List(items)),
        Frame::DictKey(map) => Ok(BencodeValue::Dict(map)),
        Frame::DictValue(..) => Err(BencodeParseError::WrongStructure),
    }
}

pub fn decode(mut stream: &[u8]) -> Result<BencodeValue, BencodeParseError> {
    let mut stack: Vec<Frame> = Vec::new();
    loop {
        if stream.is_empty() {
            return Err(BencodeParseError::NoDataFound);
        }
        match stream[0] {
            b'i' => {
                let (val, next) = decode_int(stream)?;
                stream = &next;
                if stack.is_empty() {
                    return Ok(BencodeValue::Integer(val));
                }
                apply_frame(&mut stack, BencodeValue::Integer(val))?;
            }
            b'l' => {
                stream = &stream[1..];
                stack.push(Frame::List(Vec::new()));
            }
            b'd' => {
                stream = &stream[1..];
                stack.push(Frame::DictKey(BTreeMap::new()));
            }
            b'e' => {
                stream = &stream[1..];
                if stack.len() == 1 {
                    return Ok(unpack_frame(stack.pop().unwrap())?);
                }
                let value = unpack_frame(stack.pop().unwrap())?;
                apply_frame(&mut stack, value)?;
            }
            b'0'..=b'9' => {
                let (val, next) = decode_string(stream)?;
                let value = BencodeValue::Bytes(val.to_vec());
                stream = &next;
                if stack.is_empty() {
                    return Ok(value);
                }
                apply_frame(&mut stack, value)?;
            }
            _ => return Err(BencodeParseError::WrongStructure),
        }
    }
}


pub fn encode(value: &BencodeValue, buf: &mut Vec<u8>) {
    match value {
        BencodeValue::Integer(i) => {
            buf.push(b'i');
            buf.extend_from_slice(i.to_string().as_bytes());
            buf.push(b'e');
        }
        BencodeValue::Bytes(b) => {
            buf.extend_from_slice(b.len().to_string().as_bytes());
            buf.push(b':');
            buf.extend_from_slice(b);
        }
        BencodeValue::List(items) => {
            buf.push(b'l');
            for item in items {
                encode(item, buf);
            }
            buf.push(b'e');
        }
        BencodeValue::Dict(map) => {
            buf.push(b'd');
            for (key, value) in map {
                encode(&BencodeValue::Bytes(key.clone()), buf);
                encode(value, buf);
            }
            buf.push(b'e');
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use super::*;

    #[test]
    fn roundtrip_dict() {
        let input = b"d4:name4:test6:lengthi100ee";
        let parsed1 = decode(input).unwrap();

        let mut encoded = Vec::new();
        encode(&parsed1, &mut encoded);

        let parsed2 = decode(&encoded).unwrap();

        assert_eq!(parsed1, parsed2);
    }

    #[test]
    fn encode_integer_zero() {
        let mut buf = Vec::new();
        encode(&BencodeValue::Integer(0), &mut buf);
        assert_eq!(buf, b"i0e");
    }

    #[test]
    fn encode_negative_integer() {
        let mut buf = Vec::new();
        encode(&BencodeValue::Integer(-42), &mut buf);
        assert_eq!(buf, b"i-42e");
    }

    #[test]
    fn encode_empty_string() {
        let mut buf = Vec::new();
        encode(&BencodeValue::Bytes(vec![]), &mut buf);
        assert_eq!(buf, b"0:");
    }

    #[test]
    fn encode_empty_list() {
        let mut buf = Vec::new();
        encode(&BencodeValue::List(vec![]), &mut buf);
        assert_eq!(buf, b"le");
    }

    #[test]
    fn encode_empty_dict() {
        let mut buf = Vec::new();
        encode(&BencodeValue::Dict(BTreeMap::new()), &mut buf);
        assert_eq!(buf, b"de");
    }

    #[test]
    fn test_simple_list() {
        let res = decode(b"li123ei456ei789ee");
        assert!(res.is_ok());
        assert_eq!(
            res.unwrap(),
            BencodeValue::List(vec![
                BencodeValue::Integer(123),
                BencodeValue::Integer(456),
                BencodeValue::Integer(789)
            ])
        )
    }

    #[test]
    fn test_simple_dict() {
        let res = decode(b"d8:announce35:http://tracker.example.com/announce4:infod4:name4:testee");
        assert!(res.is_ok());

        assert_eq!(res.unwrap(), BencodeValue::Dict(BTreeMap::from([
            (b"announce".to_vec(), BencodeValue::Bytes(b"http://tracker.example.com/announce".to_vec())),
            (b"info".to_vec(), BencodeValue::Dict(BTreeMap::from([
                (b"name".to_vec(), BencodeValue::Bytes(b"test".to_vec()))
            ])))
        ])));
    }

    #[test]
    fn test_inner_structs() {
        let res = decode(b"d5:filesld6:lengthi1024e4:pathl4:dir14:fileeeee");
        assert!(res.is_ok());
        assert_eq!(
            res.unwrap(),
            BencodeValue::Dict(BTreeMap::from([(
                b"files".to_vec(),
                BencodeValue::List(vec![BencodeValue::Dict(BTreeMap::from([
                    (b"length".to_vec(), BencodeValue::Integer(1024)),
                    (b"path".to_vec(), BencodeValue::List(vec![
                        BencodeValue::Bytes(b"dir1".to_vec()),
                        BencodeValue::Bytes(b"file".to_vec()),
                    ])),
                ]))])
            ),]))
        );
    }

    #[test]
    fn test_realfile() {
        let ubuntu = include_bytes!("../tests/fixtures/ubuntu-26.04.1-desktop-amd64.iso.torrent");
        let p1 = decode(ubuntu).unwrap();
        let info1 = p1.get(b"info").unwrap();
        let mut enc1 = Vec::new();
        encode(info1, &mut enc1);

        let p2 = decode(ubuntu).unwrap();
        let info2 = p2.get(b"info").unwrap();
        let mut enc2 = Vec::new();
        encode(info1, &mut enc2);

        assert_eq!(enc1, enc2);
    }

    #[test]
    fn test_parse_string() {
        let input = b"4:spami123e";
        let res = decode_string(input);
        assert!(res.is_ok());
        let res = res.unwrap();
        assert_eq!(res.0, b"spam");
        assert_eq!(res.1, b"i123e");
    }

    #[test]
    fn test_parse_string_invalid() {
        let mut input: &[u8] = b"-10sssss";
        let mut res = decode_string(input);
        assert!(res.is_err());
        assert_eq!(res.unwrap_err(), BencodeParseError::ColonMissing);
        input = b"-10:sdsss";
        res = decode_string(input);
        assert!(res.is_err());
        assert_eq!(res.unwrap_err(), BencodeParseError::NegativeOrZeroSize { given: -10 });
        input = b"10:sdsss";
        res = decode_string(input);
        assert!(res.is_err());
        assert_eq!(res.unwrap_err(), BencodeParseError::InsufficientData { expected: 10, got: 5 });
    }

    #[test]
    fn test_parse_int_success() {
        let input = b"i-100ei200e";
        let res = decode_int(input);
        assert!(res.is_ok());
        assert_eq!(res.unwrap(), (-100i128, &input[6..]));
    }
    #[test]
    fn test_parse_int_fail() {
        let mut res = decode_int(b"ie");
        assert!(res.is_err());
        assert_eq!(res.err().unwrap(), BencodeParseError::EmptyIntegerFound);
        res = decode_int(b"555555e");
        assert!(res.is_err());
        assert_eq!(
            res.err().unwrap(),
            BencodeParseError::MissingTerminator { expected: b'i' }
        );
        res = decode_int(b"i55555");
        assert!(res.is_err());
        assert_eq!(
            res.err().unwrap(),
            BencodeParseError::MissingTerminator { expected: b'e' }
        );
        res = decode_int(b"i-0000e");
        assert!(res.is_err());
        assert_eq!(res.err().unwrap(), BencodeParseError::NegativeZero);
        res = decode_int(b"i012345e");
        assert!(res.is_err());
        assert_eq!(res.err().unwrap(), BencodeParseError::LeadingZero);
    }
}
