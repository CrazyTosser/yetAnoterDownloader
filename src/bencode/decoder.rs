use crate::bencode::{BencodeParseError, decode_int, decode_string};
use std::collections::BTreeMap;
use std::mem;

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
            },
            BencodeValue::List(items) => {
                writeln!(f, "{}[", prefix)?;
                for item in items {
                    item.fmt_with_indent(f, indent + 1)?;
                    writeln!(f)?;
                }
                write!(f, "{}]", prefix)
            },
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

#[cfg(test)]
mod tests {
    use super::*;
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
        let mut map = BTreeMap::new();
        map.insert(
            b"announce".to_vec(),
            BencodeValue::Bytes(b"http://tracker.example.com/announce".to_vec()),
        );
        let mut inner = BTreeMap::new();
        inner.insert(b"name".to_vec(), BencodeValue::Bytes(b"test".to_vec()));
        map.insert(b"info".to_vec(), BencodeValue::Dict(inner));
        assert_eq!(res.unwrap(), BencodeValue::Dict(map));
    }

    #[test]
    fn test_inner_structs() {
        let res = decode(b"d5:filesld6:lengthi1024e4:pathl4:dir14:fileeeee");
        assert!(res.is_ok());
        let mut root = BTreeMap::new();
        let mut inner = BTreeMap::new();
        inner.insert(b"length".to_vec(), BencodeValue::Integer(1024));
        inner.insert(
            b"path".to_vec(),
            BencodeValue::List(vec![
                BencodeValue::Bytes(b"dir1".to_vec()),
                BencodeValue::Bytes(b"file".to_vec()),
            ]),
        );
        root.insert(
            b"files".to_vec(),
            BencodeValue::List(vec![BencodeValue::Dict(inner)]),
        );
        assert_eq!(res.unwrap(), BencodeValue::Dict(root));
    }
}
