#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use crate::{decode, encode, BencodeValue, decode_string, decode_int};
    use crate::errors::BencodeParseError;

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
        encode(info2, &mut enc2);

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
