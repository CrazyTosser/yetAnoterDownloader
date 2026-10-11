use crate::bencode::BencodeParseError;

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

#[cfg(test)]
mod tests {
    use super::*;
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
