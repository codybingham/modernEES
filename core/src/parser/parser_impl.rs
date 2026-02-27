use crate::parser::ast::{BinaryOp, Expr, ExprKind, Program, Statement, StatementKind, UnaryOp};
use crate::parser::diagnostic::{Diagnostic, Span};
use crate::parser::lexer::{lex, Token, TokenKind};

pub fn parse_program(input: &str) -> Result<Program, Vec<Diagnostic>> {
    let tokens = lex(input)?;
    let mut parser = Parser::new(tokens);
    let program = parser.parse_program();

    if parser.diagnostics.is_empty() {
        Ok(program)
    } else {
        Err(parser.diagnostics)
    }
}

pub fn parse_expression(input: &str) -> Result<Expr, Vec<Diagnostic>> {
    let tokens = lex(input)?;
    let mut parser = Parser::new(tokens);

    while parser.matches(|k| matches!(k, TokenKind::Newline)) {}

    let expr = match parser.parse_expression() {
        Some(expr) => expr,
        None => return Err(parser.diagnostics),
    };

    while parser.matches(|k| matches!(k, TokenKind::Newline)) {}

    if !parser.check(|k| matches!(k, TokenKind::Eof)) {
        let span = parser.peek().span;
        parser
            .diagnostics
            .push(Diagnostic::new("Expected end of input", span));
    }

    if parser.diagnostics.is_empty() {
        Ok(expr)
    } else {
        Err(parser.diagnostics)
    }
}

struct Parser {
    tokens: Vec<Token>,
    current: usize,
    diagnostics: Vec<Diagnostic>,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            current: 0,
            diagnostics: Vec::new(),
        }
    }

    fn parse_program(&mut self) -> Program {
        let mut statements = Vec::new();

        while !self.is_at_end() {
            while self.matches(|k| matches!(k, TokenKind::Newline)) {}
            if self.is_at_end() {
                break;
            }

            match self.parse_statement() {
                Some(stmt) => statements.push(stmt),
                None => self.synchronize_line(),
            }
        }

        Program { statements }
    }

    fn parse_statement(&mut self) -> Option<Statement> {
        let lhs = self.parse_expression()?;

        if !self.matches(|k| matches!(k, TokenKind::Equal)) {
            let span = self.peek().span;
            if self.check(|k| matches!(k, TokenKind::UnitAnnotation(_))) {
                self.diagnostics.push(Diagnostic::new(
                    "Unit annotations are only allowed on numeric literals",
                    span,
                ));
            } else {
                self.diagnostics
                    .push(Diagnostic::new("Expected '=' in assignment", span));
            }
            return None;
        }

        let rhs = match self.parse_expression() {
            Some(expr) => expr,
            None => {
                let span = self.peek().span;
                self.diagnostics
                    .push(Diagnostic::new("Expected right-hand side expression", span));
                return None;
            }
        };

        if !self.check(|k| matches!(k, TokenKind::Newline | TokenKind::Eof)) {
            let span = self.peek().span;
            self.diagnostics.push(Diagnostic::new(
                "Expected end of line after assignment",
                span,
            ));
            return None;
        }

        while self.matches(|k| matches!(k, TokenKind::Newline)) {}

        let span = Span {
            start: lhs.span.start,
            end: rhs.span.end,
        };

        Some(Statement {
            kind: StatementKind::Assignment { lhs, rhs },
            span,
        })
    }

    fn parse_expression(&mut self) -> Option<Expr> {
        self.parse_add_sub()
    }

    fn parse_add_sub(&mut self) -> Option<Expr> {
        let mut expr = self.parse_mul_div()?;

        while self.matches(|k| matches!(k, TokenKind::Plus | TokenKind::Minus)) {
            let op_token = self.previous().clone();
            let op = match op_token.kind {
                TokenKind::Plus => BinaryOp::Add,
                TokenKind::Minus => BinaryOp::Subtract,
                _ => unreachable!(),
            };
            let right = self.parse_mul_div()?;
            let span = Span {
                start: expr.span.start,
                end: right.span.end,
            };
            expr = Expr {
                kind: ExprKind::Binary {
                    op,
                    left: Box::new(expr),
                    right: Box::new(right),
                },
                span,
            };
        }

        Some(expr)
    }

    fn parse_mul_div(&mut self) -> Option<Expr> {
        let mut expr = self.parse_power()?;

        while self.matches(|k| matches!(k, TokenKind::Star | TokenKind::Slash)) {
            let op_token = self.previous().clone();
            let op = match op_token.kind {
                TokenKind::Star => BinaryOp::Multiply,
                TokenKind::Slash => BinaryOp::Divide,
                _ => unreachable!(),
            };
            let right = self.parse_power()?;
            let span = Span {
                start: expr.span.start,
                end: right.span.end,
            };
            expr = Expr {
                kind: ExprKind::Binary {
                    op,
                    left: Box::new(expr),
                    right: Box::new(right),
                },
                span,
            };
        }

        Some(expr)
    }

    fn parse_power(&mut self) -> Option<Expr> {
        let left = self.parse_unary()?;
        if self.matches(|k| matches!(k, TokenKind::Caret)) {
            let right = self.parse_power()?;
            let span = Span {
                start: left.span.start,
                end: right.span.end,
            };
            Some(Expr {
                kind: ExprKind::Binary {
                    op: BinaryOp::Power,
                    left: Box::new(left),
                    right: Box::new(right),
                },
                span,
            })
        } else {
            Some(left)
        }
    }

    fn parse_unary(&mut self) -> Option<Expr> {
        if self.matches(|k| matches!(k, TokenKind::Plus | TokenKind::Minus)) {
            let op_token = self.previous().clone();
            let op = match op_token.kind {
                TokenKind::Plus => UnaryOp::Plus,
                TokenKind::Minus => UnaryOp::Minus,
                _ => unreachable!(),
            };
            let expr = self.parse_unary()?;
            let span = Span {
                start: op_token.span.start,
                end: expr.span.end,
            };
            Some(Expr {
                kind: ExprKind::Unary {
                    op,
                    expr: Box::new(expr),
                },
                span,
            })
        } else {
            self.parse_primary()
        }
    }

    fn parse_primary(&mut self) -> Option<Expr> {
        let token = self.peek().clone();
        match token.kind {
            TokenKind::Number(ref n) => {
                self.advance();
                let mut expr = Expr {
                    kind: ExprKind::Number(n.clone()),
                    span: token.span,
                };

                if self.matches(|k| matches!(k, TokenKind::UnitAnnotation(_))) {
                    let unit_token = self.previous().clone();
                    let TokenKind::UnitAnnotation(unit) = unit_token.kind else {
                        unreachable!();
                    };

                    let value = match n.parse::<f64>() {
                        Ok(value) => value,
                        Err(_) => {
                            self.diagnostics.push(Diagnostic::new(
                                "Invalid numeric literal for quantity",
                                token.span,
                            ));
                            return None;
                        }
                    };

                    let span = Span {
                        start: token.span.start,
                        end: unit_token.span.end,
                    };
                    expr = Expr {
                        kind: ExprKind::QuantityLiteral { value, unit, span },
                        span,
                    };
                }

                Some(expr)
            }
            TokenKind::Identifier(ref name) => {
                self.advance();
                if self.matches(|k| matches!(k, TokenKind::LParen)) {
                    let mut args = Vec::new();
                    if !self.check(|k| matches!(k, TokenKind::RParen)) {
                        loop {
                            args.push(self.parse_expression()?);
                            if !self.matches(|k| matches!(k, TokenKind::Comma)) {
                                break;
                            }
                        }
                    }

                    if !self.matches(|k| matches!(k, TokenKind::RParen)) {
                        let span = self.peek().span;
                        self.diagnostics.push(Diagnostic::new(
                            "Expected ')' after function arguments",
                            span,
                        ));
                        return None;
                    }

                    let end = self.previous().span.end;
                    Some(Expr {
                        kind: ExprKind::Call {
                            callee: name.clone(),
                            args,
                        },
                        span: Span {
                            start: token.span.start,
                            end,
                        },
                    })
                } else {
                    Some(Expr {
                        kind: ExprKind::Identifier(name.clone()),
                        span: token.span,
                    })
                }
            }
            TokenKind::StringLiteral(ref s) => {
                self.advance();
                Some(Expr {
                    kind: ExprKind::StringLiteral(s.clone()),
                    span: token.span,
                })
            }
            TokenKind::LParen => {
                let open_span = token.span;
                self.advance();
                let inner = self.parse_expression()?;
                if !self.matches(|k| matches!(k, TokenKind::RParen)) {
                    let span = self.peek().span;
                    self.diagnostics
                        .push(Diagnostic::new("Expected ')' after expression", span));
                    return None;
                }
                let close_span = self.previous().span;
                Some(Expr {
                    kind: ExprKind::Group(Box::new(inner)),
                    span: Span {
                        start: open_span.start,
                        end: close_span.end,
                    },
                })
            }
            _ => {
                self.diagnostics.push(Diagnostic::new(
                    format!(
                        "Expected expression, found {}",
                        token_description(&token.kind)
                    ),
                    token.span,
                ));
                None
            }
        }
    }

    fn synchronize_line(&mut self) {
        while !self.is_at_end() {
            if self.matches(|k| matches!(k, TokenKind::Newline)) {
                return;
            }
            self.advance();
        }
    }

    fn matches(&mut self, predicate: impl Fn(&TokenKind) -> bool) -> bool {
        if self.check(&predicate) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn check(&self, predicate: impl Fn(&TokenKind) -> bool) -> bool {
        predicate(&self.peek().kind)
    }

    fn advance(&mut self) {
        if !self.is_at_end() {
            self.current += 1;
        }
    }

    fn is_at_end(&self) -> bool {
        matches!(self.peek().kind, TokenKind::Eof)
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.current]
    }

    fn previous(&self) -> &Token {
        &self.tokens[self.current.saturating_sub(1)]
    }
}

fn token_description(kind: &TokenKind) -> &'static str {
    match kind {
        TokenKind::Identifier(_) => "identifier",
        TokenKind::Number(_) => "number",
        TokenKind::StringLiteral(_) => "string",
        TokenKind::Equal => "'='",
        TokenKind::Plus => "'+'",
        TokenKind::Minus => "'-'",
        TokenKind::Star => "'*'",
        TokenKind::Slash => "'/'",
        TokenKind::Caret => "'^'",
        TokenKind::LParen => "'('",
        TokenKind::RParen => "')'",
        TokenKind::Comma => "','",
        TokenKind::UnitAnnotation(_) => "unit annotation",
        TokenKind::Newline => "end of line",
        TokenKind::Eof => "end of input",
    }
}
