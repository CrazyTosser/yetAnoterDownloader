pub mod errors;
pub mod tests;

use std::collections::BTreeMap;
use std::mem;
pub use crate::errors::BencodeParseError;

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