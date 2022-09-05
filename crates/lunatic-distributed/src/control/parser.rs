use anyhow::{anyhow, Result};
use dashmap::mapref::multiple::RefMulti;

use super::message::Registration;

// Small query language parser for node lookup based on tag metadata.
//
// Supports:
// * key=value
pub struct Parser {
    query: String,
}

pub trait Filter {
    fn apply(&self, e: &RefMulti<'_, u64, Registration>) -> bool;
}

pub struct EmptyFilter;

impl Filter for EmptyFilter {
    fn apply(&self, _: &RefMulti<'_, u64, Registration>) -> bool {
        true
    }
}

pub struct KeyValueFilter {
    key: String,
    value: String,
}

impl Filter for KeyValueFilter {
    fn apply(&self, e: &RefMulti<'_, u64, Registration>) -> bool {
        e.node_metadata
            .iter()
            .any(|(key, value)| key == &self.key && value == &self.value)
    }
}

pub struct AndFilter {
    key_value_filters: Vec<KeyValueFilter>,
}

impl Filter for AndFilter {
    fn apply(&self, e: &RefMulti<'_, u64, Registration>) -> bool {
        self.key_value_filters.iter().all(|f| f.apply(e))
    }
}

impl Parser {
    pub fn new(query: String) -> Self {
        Self { query }
    }

    pub fn parse(&self) -> Result<Box<dyn Filter>> {
        let mut tokens = Scanner::new(self.query.clone()).scan()?;
        tokens.truncate(tokens.len() - 1);
        if tokens.is_empty() {
            return Ok(Box::new(EmptyFilter));
        }
        let mut key_value_filters = vec![];
        for parts in tokens.split(|t| t.t == TokenType::And) {
            if parts.len() != 3 {
                return Err(anyhow!("invalid query"));
            }
            let key = &parts[0];
            let equal = &parts[1];
            let value = &parts[2];
            if !(key.t == TokenType::Literal
                && equal.t == TokenType::Equal
                && value.t == TokenType::Literal)
            {
                return Err(anyhow!("invalid query"));
            }

            if let (Some(key), Some(value)) = (key.literal.clone(), value.literal.clone()) {
                key_value_filters.push(KeyValueFilter { key, value })
            } else {
                return Err(anyhow!("invalid query"));
            }
        }
        Ok(Box::new(AndFilter { key_value_filters }))
    }
}

struct Scanner {
    query: String,
    start: usize,
    current: usize,
    tokens: Vec<Token>,
}

impl Scanner {
    fn new(query: String) -> Self {
        Self {
            query,
            start: 0,
            current: 0,
            tokens: Vec::new(),
        }
    }

    fn scan(mut self) -> Result<Vec<Token>> {
        while !self.is_at_end() {
            self.start = self.current;
            self.scan_token()?;
        }
        self.add_token(Token {
            t: TokenType::Eof,
            literal: None,
        });
        Ok(self.tokens)
    }

    fn scan_token(&mut self) -> Result<()> {
        let c = self.advance();
        match c {
            '=' => self.add_token(Token {
                t: TokenType::Equal,
                literal: None,
            }),
            '&' => self.add_token(Token {
                t: TokenType::And,
                literal: None,
            }),
            _ => {
                if c.is_alphabetic() {
                    self.literal();
                } else {
                    return Err(anyhow!("character {c} is not allowed"));
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
            literal: Some(literal),
        });
    }
}

#[derive(Debug)]
struct Token {
    t: TokenType,
    literal: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
enum TokenType {
    Literal,
    Equal,
    And,
    Eof,
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
                node_metadata: metadata.clone(),
                signing_request: "request01".to_string(),
            },
        );

        metadata.insert("name".to_string(), "test02".to_string());
        map.insert(
            2,
            Registration {
                node_address: "127.0.0.1:10001".parse().unwrap(),
                node_name: "test02".to_string(),
                node_metadata: metadata.clone(),
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
    fn scan_empty() {
        let scanner = Scanner::new("".to_string());
        let tokens = scanner.scan().unwrap();
        assert_eq!(1, tokens.len());
        assert_eq!(TokenType::Eof, tokens[0].t)
    }

    #[test]
    fn scan_simple_key_value() {
        let scanner = Scanner::new("name=value".to_string());
        let tokens = scanner.scan().unwrap();

        assert_eq!(4, tokens.len());
        assert_eq!(TokenType::Literal, tokens[0].t);
        assert_eq!(TokenType::Equal, tokens[1].t);
        assert_eq!(TokenType::Literal, tokens[2].t);
        assert_eq!(TokenType::Eof, tokens[3].t);
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

        assert_eq!(8, tokens.len());
        assert_eq!(TokenType::Literal, tokens[0].t);
        assert_eq!(TokenType::Equal, tokens[1].t);
        assert_eq!(TokenType::Literal, tokens[2].t);

        assert_eq!(TokenType::And, tokens[3].t);

        assert_eq!(TokenType::Literal, tokens[4].t);
        assert_eq!(TokenType::Equal, tokens[5].t);
        assert_eq!(TokenType::Literal, tokens[6].t);
        assert_eq!(TokenType::Eof, tokens[7].t);
    }

    #[test]
    fn scan_multiple_invalid() {
        let scanner = Scanner::new("k1=v1!k2=v2".to_string());
        assert!(scanner.scan().is_err());

        let scanner = Scanner::new("k1=1&k2=v2".to_string());
        assert!(scanner.scan().is_err());
    }
}
