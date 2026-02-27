use crate::parser::diagnostic::{Diagnostic, Position, Span};

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    Identifier(String),
    Number(String),
    StringLiteral(String),
    Equal,
    Plus,
    Minus,
    Star,
    Slash,
    Caret,
    LParen,
    RParen,
    Comma,
    Newline,
    Eof,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

pub fn lex(input: &str) -> Result<Vec<Token>, Vec<Diagnostic>> {
    let mut lexer = Lexer::new(input);
    lexer.lex_all();
    if lexer.diagnostics.is_empty() {
        Ok(lexer.tokens)
    } else {
        Err(lexer.diagnostics)
    }
}

struct Lexer<'a> {
    chars: std::iter::Peekable<std::str::CharIndices<'a>>,
    line: usize,
    column: usize,
    offset: usize,
    tokens: Vec<Token>,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> Lexer<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            chars: input.char_indices().peekable(),
            line: 1,
            column: 1,
            offset: 0,
            tokens: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    fn lex_all(&mut self) {
        while let Some((_, ch)) = self.peek().copied() {
            match ch {
                ' ' | '\t' | '\r' => {
                    self.bump();
                }
                '\n' => {
                    let start = self.current_position();
                    self.bump();
                    let end = self.current_position();
                    self.push_token(TokenKind::Newline, Span { start, end });
                }
                '/' => {
                    if self.peek_nth_char(1) == Some('/') {
                        self.skip_line_comment();
                    } else {
                        let span = self.single_char_span();
                        self.push_token(TokenKind::Slash, span);
                    }
                }
                '{' => {
                    self.skip_block_comment();
                }
                '=' => {
                    let span = self.single_char_span();
                    self.push_token(TokenKind::Equal, span);
                }
                '+' => {
                    let span = self.single_char_span();
                    self.push_token(TokenKind::Plus, span);
                }
                '-' => {
                    let span = self.single_char_span();
                    self.push_token(TokenKind::Minus, span);
                }
                '*' => {
                    let span = self.single_char_span();
                    self.push_token(TokenKind::Star, span);
                }
                '^' => {
                    let span = self.single_char_span();
                    self.push_token(TokenKind::Caret, span);
                }
                '(' => {
                    let span = self.single_char_span();
                    self.push_token(TokenKind::LParen, span);
                }
                ')' => {
                    let span = self.single_char_span();
                    self.push_token(TokenKind::RParen, span);
                }
                ',' => {
                    let span = self.single_char_span();
                    self.push_token(TokenKind::Comma, span);
                }
                '"' => self.lex_string(),
                c if c.is_ascii_digit() => self.lex_number(),
                c if is_identifier_start(c) => self.lex_identifier(),
                _ => {
                    let span = self.single_char_span();
                    self.diagnostics.push(Diagnostic::new(
                        format!("Unexpected character '{ch}'"),
                        span,
                    ));
                }
            }
        }

        let pos = self.current_position();
        self.push_token(
            TokenKind::Eof,
            Span {
                start: pos,
                end: pos,
            },
        );
    }

    fn skip_line_comment(&mut self) {
        self.bump();
        self.bump();
        while let Some((_, ch)) = self.peek().copied() {
            if ch == '\n' {
                break;
            }
            self.bump();
        }
    }

    fn skip_block_comment(&mut self) {
        let start = self.current_position();
        self.bump();
        while let Some((_, ch)) = self.peek().copied() {
            if ch == '}' {
                self.bump();
                return;
            }
            self.bump();
        }
        let end = self.current_position();
        self.diagnostics.push(Diagnostic::new(
            "Unterminated block comment",
            Span { start, end },
        ));
    }

    fn lex_identifier(&mut self) {
        let start = self.current_position();
        let mut value = String::new();
        while let Some((_, ch)) = self.peek().copied() {
            if is_identifier_continue(ch) {
                value.push(ch);
                self.bump();
            } else {
                break;
            }
        }
        let end = self.current_position();
        self.push_token(TokenKind::Identifier(value), Span { start, end });
    }

    fn lex_number(&mut self) {
        let start = self.current_position();
        let mut value = String::new();

        self.consume_digits(&mut value);

        if self.peek_char() == Some('.')
            && self.peek_nth_char(1).is_some_and(|c| c.is_ascii_digit())
        {
            value.push('.');
            self.bump();
            self.consume_digits(&mut value);
        }

        if self.peek_char().is_some_and(|c| c == 'e' || c == 'E') {
            let exp_mark = self.peek_char().unwrap_or_default();
            let next = self.peek_nth_char(1);
            let next2 = self.peek_nth_char(2);
            let exp_has_digits = next.is_some_and(|c| c.is_ascii_digit())
                || (next.is_some_and(|c| c == '+' || c == '-')
                    && next2.is_some_and(|c| c.is_ascii_digit()));

            if exp_has_digits {
                value.push(exp_mark);
                self.bump();
                if self.peek_char().is_some_and(|c| c == '+' || c == '-') {
                    value.push(self.peek_char().unwrap_or_default());
                    self.bump();
                }
                self.consume_digits(&mut value);
            }
        }

        let end = self.current_position();
        self.push_token(TokenKind::Number(value), Span { start, end });
    }

    fn lex_string(&mut self) {
        let start = self.current_position();
        self.bump();
        let mut value = String::new();
        let mut terminated = false;

        while let Some((_, ch)) = self.peek().copied() {
            match ch {
                '"' => {
                    self.bump();
                    terminated = true;
                    break;
                }
                '\n' => {
                    break;
                }
                _ => {
                    value.push(ch);
                    self.bump();
                }
            }
        }

        let end = self.current_position();
        if terminated {
            self.push_token(TokenKind::StringLiteral(value), Span { start, end });
        } else {
            self.diagnostics.push(Diagnostic::new(
                "Unterminated string literal",
                Span { start, end },
            ));
        }
    }

    fn consume_digits(&mut self, value: &mut String) {
        while let Some((_, ch)) = self.peek().copied() {
            if ch.is_ascii_digit() {
                value.push(ch);
                self.bump();
            } else {
                break;
            }
        }
    }

    fn peek(&mut self) -> Option<&(usize, char)> {
        self.chars.peek()
    }

    fn peek_char(&mut self) -> Option<char> {
        self.peek().map(|(_, ch)| *ch)
    }

    fn peek_nth_char(&mut self, n: usize) -> Option<char> {
        let mut cloned = self.chars.clone();
        cloned.nth(n).map(|(_, ch)| ch)
    }

    fn bump(&mut self) -> Option<char> {
        let (_, ch) = self.chars.next()?;
        self.offset += ch.len_utf8();
        if ch == '\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
        Some(ch)
    }

    fn current_position(&self) -> Position {
        Position {
            line: self.line,
            column: self.column,
            offset: self.offset,
        }
    }

    fn single_char_span(&mut self) -> Span {
        let start = self.current_position();
        self.bump();
        let end = self.current_position();
        Span { start, end }
    }

    fn push_token(&mut self, kind: TokenKind, span: Span) {
        self.tokens.push(Token { kind, span });
    }
}

fn is_identifier_start(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '_'
}

fn is_identifier_continue(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}
