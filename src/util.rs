use anyhow::Result;
use once_cell::sync::Lazy;
use std::collections::HashMap;

const VALID_OPENERS: [char; 3] = ['[', '(', '{'];
static BRACKET_CLOSERS: Lazy<HashMap<char, char>> = Lazy::new(|| {
    let mut m = HashMap::new();
    m.insert('[', ']');
    m.insert('(', ')');
    m.insert('{', '}');
    m
});

// inclusive substring extraction
// for the time being, we assume the javascript that comes with the page will be well-formed
// so we don't bother balancing all brackets encountered
pub fn matching_bracket_substring<'a>(input: &'a str, opener: char) -> anyhow::Result<&'a str> {
    if !VALID_OPENERS.contains(&opener) {
        return Err(anyhow::anyhow!(
            "char [{}] is not a supported opener",
            &opener
        ));
    }

    let closer = BRACKET_CLOSERS[&opener];
    let mut counter = 0;
    let mut substr_started = false;
    let mut start_idx = 0;
    let mut end_idx = 0;

    for (i, c) in input.chars().enumerate() {
        match c {
            _ if c == opener => {
                if !substr_started {
                    substr_started = true;
                    start_idx = i;
                }

                counter += 1;
            }
            _ if c == closer => {
                if !substr_started {
                    anyhow::bail!("found closer before opener in matching bracket search");
                }
                counter -= 1;

                if counter == 0 {
                    // found matching bracket
                    end_idx = i;
                    break;
                }
            }
            _ => {}
        }
    }

    if !substr_started {
        anyhow::bail!("no brackets found");
    }

    if counter != 0 {
        anyhow::bail!("mismatched brackets");
    }

    Ok(&input[start_idx..=end_idx])
}

use std::num::ParseIntError;
pub fn decode_hex(s: &str) -> Result<Vec<u8>> {
    let mut start_idx = 0;

    if s.to_lowercase().starts_with("0x") {
        start_idx = 2;
    }

    let result: Result<Vec<u8>, ParseIntError> = (start_idx..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16))
        .collect();

    Ok(result?)
}

#[cfg(test)]
mod hex_conversion_tests {
    use super::*;

    #[test]
    fn decode_hex_happy_path() {
        let manifest_hex = "0xb87f84a4ced179cfc020624ade3d7f71";
        let true_hex = "b87f84a4ced179cfc020624ade3d7f71";

        assert!(decode_hex(&manifest_hex).is_ok());
        assert!(decode_hex(&true_hex).is_ok());
    }
}

#[cfg(test)]
mod matching_bracket_tests {
    use super::*;

    #[test]
    fn unsupported_character_is_rejected() {
        let result = matching_bracket_substring("lmao", 'o');
        assert!(result.is_err())
    }

    #[test]
    fn supported_character_is_accepted() {
        let result = matching_bracket_substring("[]", '[');
        assert!(result.is_ok());
    }

    #[test]
    fn closer_first_will_fail() {
        let result = matching_bracket_substring("][", '[');
        assert!(result.is_err())
    }

    #[test]
    fn will_parse_nested_brackets() {
        let result = matching_bracket_substring("[[[[]]]]", '[');
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "[[[[]]]]");
    }

    #[test]
    fn will_parse_nested_mixed_brackets() {
        let result = matching_bracket_substring("([(<[{}]>)] junk;", '[');
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "[(<[{}]>)]");
    }

    #[test]
    fn will_fail_on_unbalanced_brackets() {
        let result = matching_bracket_substring("[[[[[[]]]]", '[');
        assert!(result.is_err());
    }
}
