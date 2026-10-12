use crate::bencode::BencodeValue;

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
    use crate::bencode::decode;
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
}