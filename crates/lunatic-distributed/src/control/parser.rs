use anyhow::{anyhow, Result};
use dashmap::mapref::multiple::RefMulti;

use super::message::Registration;

/// Query parser for node lookup based on tag attributes.
///
/// Syntax is like URL Query string, e.g. name=node01&group=workers, consisting of
/// literals and operators.
///
/// Literals are case sensitive alphanumeric values that start with an alphabetic character.
///
/// The syntax defines two operators:
///     * `=` - equality operator compares two literals, first the lookup key and second value, e.g. name=node01
///     * `&` - and operator which is used to chain multiple equality expressions, e.g. name=node01&group=workers
pub struct Parser {
    query: String,
}

pub trait Filter {
    fn apply(&self, e: &RefMulti<'_, u64, Registration>) -> bool;
}

struct EmptyFilter;

impl Filter for EmptyFilter {
    fn apply(&self, _: &RefMulti<'_, u64, Registration>) -> bool {
        true
    }
}

struct KeyValueFilter {
    key: String,
    value: String,
}

impl Filter for KeyValueFilter {
    fn apply(&self, e: &RefMulti<'_, u64, Registration>) -> bool {
        e.attributes
            .iter()
            .any(|(key, value)| key == &self.key && value == &self.value)
    }
}

struct AndFilter {
    key_value_filters: Vec<KeyValueFilter>,
}

impl Filter for AndFilter {
    fn apply(&self, e: &RefMulti<'_, u64, Registration>) -> bool {
        self.key_value_filters.iter().all(|f| f.apply(e))
    }
}

impl Parser {
    /// Creates a new `Parser` with input query `String`
    pub fn new(query: String) -> Self {
        Self { query }
    }

    /// Parses the query returning `Filter` if the query is valid
    pub fn parse(&self) -> Result<Box<dyn Filter>> {
        let tokens = Scanner::new(self.query.clone()).scan()?;
        if tokens.is_empty() {
            return Ok(Box::new(EmptyFilter));
        }
        let mut key_value_filters = vec![];
        for parts in tokens.split(|t| t.t == TokenType::And) {
            if parts.len() != 3 {
                let token_str = parts
                    .iter()
                    .map(|t| t.literal.as_str())
                    .collect::<Vec<&str>>()
                    .join("");
                return Err(anyhow!(
                    "Query syntax error at \"{token_str}\", expected \"key=value\""
                ));
            }
            let key = &parts[0];
            let equal = &parts[1];
            let value = &parts[2];
            if !(key.t == TokenType::Literal
                && equal.t == TokenType::Equal
                && value.t == TokenType::Literal)
            {
                let token_str = [
                    key.literal.as_str(),
                    equal.literal.as_str(),
                    value.literal.as_str(),
                ]
                .join("");
                return Err(anyhow!(
                    "Query syntax error at \"{token_str}\", expected \"key=value\""
                ));
            }

            key_value_filters.push(KeyValueFilter {
                key: key.literal.clone(),
                value: value.literal.clone(),
            })
        }
        Ok(Box::new(AndFilter { key_value_filters }))
    }
}

/// Scans and validates input query turning it into a list of `Token` values.
pub struct Scanner {
    query: String,
    start: usize,
    current: usize,
    tokens: Vec<Token>,
}

impl Scanner {
    pub fn new(query: String) -> Self {
        Self {
            query,
            start: 0,
            current: 0,
            tokens: Vec::new(),
        }
    }

    pub fn scan(mut self) -> Result<Vec<Token>> {
        while !self.is_at_end() {
            self.start = self.current;
            self.scan_token()?;
        }
        Ok(self.tokens)
    }

    fn scan_token(&mut self) -> Result<()> {
        let c = self.advance();
        match c {
            '=' => self.add_token(Token {
                t: TokenType::Equal,
                literal: '='.to_string(),
            }),
            '&' => self.add_token(Token {
                t: TokenType::And,
                literal: '&'.to_string(),
            }),
            _ => {
                if c.is_alphabetic() {
                    self.literal();
                } else {
                    let query_part = self.query.as_str()[self.start..].to_string();
                    return Err(anyhow!("Unexpected character {c} at {query_part}"));
                }
            }
        };
        Ok(())
    }

    fn is_at_end(&self) -> bool {
        self.current >= self.query.len()
    }

    fn advance(&mut self) -> char {
        let c = self.query.chars().nth(self.current).unwrap();
        self.current += 1;
        c
    }

    fn peek(&self) -> char {
        if self.is_at_end() {
            '\0'
        } else {
            self.query.chars().nth(self.current).unwrap()
        }
    }

    fn add_token(&mut self, token: Token) {
        self.tokens.push(token);
    }

    fn literal(&mut self) {
        while self.peek().is_alphanumeric() {
            self.advance();
        }
        let literal = self.query.as_str()[self.start..self.current].to_string();
        self.add_token(Token {
            t: TokenType::Literal,
            literal,
        });
    }
}

#[derive(Debug)]
pub struct Token {
    pub t: TokenType,
    pub literal: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum TokenType {
    Literal,
    Equal,
    And,
}

#[cfg(test)]
mod tests {

    use dashmap::DashMap;

    use super::*;
    use crate::control::message::Registration;
    use std::collections::HashMap;

    fn init_map() -> DashMap<u64, Registration> {
        let map = DashMap::new();
        let mut metadata = HashMap::new();

        metadata.insert("name".to_owned(), "test01".to_owned());
        metadata.insert("group".to_owned(), "testers".to_owned());
        map.insert(
            1,
            Registration {
                node_address: "127.0.0.1:10000".parse().unwrap(),
                node_name: "test01".to_string(),
                attributes: metadata.clone(),
                signing_request: "request01".to_string(),
            },
        );

        metadata.insert("name".to_string(), "test02".to_string());
        map.insert(
            2,
            Registration {
                node_address: "127.0.0.1:10001".parse().unwrap(),
                node_name: "test02".to_string(),
                attributes: metadata.clone(),
                signing_request: "request01".to_string(),
            },
        );

        map
    }

    #[test]
    fn parse_empty() {
        let map = init_map();
        let parser = Parser::new("".to_string());
        let filter = parser.parse().unwrap();
        let cnt = map.iter().filter(|e| filter.apply(e)).count();
        assert_eq!(cnt, 2);
    }

    #[test]
    fn parse_expression() {
        let map = init_map();

        let parser = Parser::new("name=test01".to_string());
        let filter = parser.parse().unwrap();
        let cnt = map.iter().filter(|e| filter.apply(e)).count();
        assert_eq!(cnt, 1);

        let parser = Parser::new("group=testers".to_string());
        let filter = parser.parse().unwrap();
        let cnt = map.iter().filter(|e| filter.apply(e)).count();
        assert_eq!(cnt, 2);

        let parser = Parser::new("random=string".to_string());
        let filter = parser.parse().unwrap();
        let cnt = map.iter().filter(|e| filter.apply(e)).count();
        assert_eq!(cnt, 0);
    }

    #[test]
    fn parse_invalid() {
        let parser = Parser::new("name=test01&".to_string());
        assert!(parser.parse().is_err());
        let parser = Parser::new("name==test01".to_string());
        assert!(parser.parse().is_err());
    }

    #[test]
    fn scan_empty() {
        let scanner = Scanner::new("".to_string());
        let tokens = scanner.scan().unwrap();
        assert_eq!(0, tokens.len());
    }

    #[test]
    fn scan_simple_key_value() {
        let scanner = Scanner::new("name=value".to_string());
        let tokens = scanner.scan().unwrap();

        assert_eq!(3, tokens.len());
        assert_eq!(TokenType::Literal, tokens[0].t);
        assert_eq!(TokenType::Equal, tokens[1].t);
        assert_eq!(TokenType::Literal, tokens[2].t);
    }

    #[test]
    fn scan_invalid() {
        let scanner = Scanner::new("name!value".to_string());
        assert!(scanner.scan().is_err());

        let scanner = Scanner::new("1241".to_string());
        assert!(scanner.scan().is_err());

        let scanner = Scanner::new("!asdad!sadsd".to_string());
        assert!(scanner.scan().is_err());

        let scanner = Scanner::new("key=1241".to_string());
        assert!(scanner.scan().is_err());
    }

    #[test]
    fn scan_multiple_key_value() {
        let scanner = Scanner::new("k1=v1&k2=v2".to_string());
        let tokens = scanner.scan().unwrap();

        assert_eq!(7, tokens.len());
        assert_eq!(TokenType::Literal, tokens[0].t);
        assert_eq!(TokenType::Equal, tokens[1].t);
        assert_eq!(TokenType::Literal, tokens[2].t);

        assert_eq!(TokenType::And, tokens[3].t);

        assert_eq!(TokenType::Literal, tokens[4].t);
        assert_eq!(TokenType::Equal, tokens[5].t);
        assert_eq!(TokenType::Literal, tokens[6].t);
    }

    #[test]
    fn scan_multiple_invalid() {
        let scanner = Scanner::new("k1=v1!k2=v2".to_string());
        assert!(scanner.scan().is_err());

        let scanner = Scanner::new("k1=1&k2=v2".to_string());
        assert!(scanner.scan().is_err());
    }
}
