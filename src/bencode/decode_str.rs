use crate::bencode::BencodeParseError;

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

#[cfg(test)]
mod tests {
    use super::*;
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
}