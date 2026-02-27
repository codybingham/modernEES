use crate::parser::ast::{BinaryOp, CallArg, ExprKind, StatementKind, UnaryOp};
use crate::parser::{parse_expression, parse_program};

#[test]
fn parses_simple_assignment() {
    let program = parse_program("x = 42\n").expect("should parse");
    assert_eq!(program.statements.len(), 1);
    let StatementKind::Assignment { lhs, rhs } = &program.statements[0].kind;
    assert!(matches!(lhs.kind, ExprKind::Identifier(ref n) if n == "x"));
    assert!(matches!(rhs.kind, ExprKind::Number(ref n) if n == "42"));
}

#[test]
fn parses_operator_precedence_and_parentheses() {
    let program = parse_program("y = (1 + 2) * 3\n").expect("should parse");
    let StatementKind::Assignment { rhs, .. } = &program.statements[0].kind;
    match &rhs.kind {
        ExprKind::Binary { op, .. } => assert_eq!(*op, BinaryOp::Multiply),
        _ => panic!("expected multiply expression"),
    }
}

#[test]
fn parses_right_associative_power() {
    let program = parse_program("z = 2 ^ 3 ^ 4\n").expect("should parse");
    let StatementKind::Assignment { rhs, .. } = &program.statements[0].kind;
    match &rhs.kind {
        ExprKind::Binary { op, right, .. } => {
            assert_eq!(*op, BinaryOp::Power);
            assert!(matches!(
                right.kind,
                ExprKind::Binary {
                    op: BinaryOp::Power,
                    ..
                }
            ));
        }
        _ => panic!("expected power expression"),
    }
}

#[test]
fn parses_unary_calls_and_strings() {
    let source = "rho = -density(\"Water\", 300, 101325)\n";
    let program = parse_program(source).expect("should parse");
    let StatementKind::Assignment { rhs, .. } = &program.statements[0].kind;
    match &rhs.kind {
        ExprKind::Unary { op, expr } => {
            assert_eq!(*op, UnaryOp::Minus);
            assert!(matches!(expr.kind, ExprKind::Call { .. }));
        }
        _ => panic!("expected unary call"),
    }
}

#[test]
fn parses_comments_and_multiple_lines() {
    let source = "// line comment\na = 1\n{block\ncomment}\nb = a + 2\n";
    let program = parse_program(source).expect("should parse");
    assert_eq!(program.statements.len(), 2);
}

#[test]
fn reports_missing_equals() {
    let diagnostics = parse_program("x 1\n").expect_err("should fail");
    assert!(diagnostics[0].message.contains("Expected '='"));
    assert_eq!(diagnostics[0].span.start.line, 1);
}

#[test]
fn reports_unterminated_string() {
    let diagnostics = parse_program("x = \"abc\n").expect_err("should fail");
    assert!(diagnostics[0]
        .message
        .contains("Unterminated string literal"));
    assert_eq!(diagnostics[0].span.start.line, 1);
}

#[test]
fn reports_unterminated_block_comment() {
    let diagnostics = parse_program("{ never ends\nx = 1\n").expect_err("should fail");
    assert!(diagnostics[0]
        .message
        .contains("Unterminated block comment"));
}

#[test]
fn reports_missing_closing_paren() {
    let diagnostics = parse_program("x = (1 + 2\n").expect_err("should fail");
    assert!(diagnostics
        .iter()
        .any(|d| d.message.contains("Expected ')' after expression")));
}

#[test]
fn parses_quantity_literal_expression() {
    let expr = parse_expression("10 [m]").expect("should parse");
    match expr.kind {
        ExprKind::QuantityLiteral { value, unit, .. } => {
            assert_eq!(value, 10.0);
            assert_eq!(unit, "m");
        }
        _ => panic!("expected quantity literal"),
    }
}

#[test]
fn parses_quantity_literal_in_assignment() {
    let program = parse_program(
        "rho = 62.4 [lbm/ft^3]
",
    )
    .expect("should parse");
    let StatementKind::Assignment { rhs, .. } = &program.statements[0].kind;
    match &rhs.kind {
        ExprKind::QuantityLiteral { value, unit, .. } => {
            assert_eq!(*value, 62.4);
            assert_eq!(unit, "lbm/ft^3");
        }
        _ => panic!("expected quantity literal"),
    }
}

#[test]
fn reports_unterminated_unit_annotation() {
    let diagnostics = parse_expression("10 [").expect_err("should fail");
    assert!(diagnostics[0]
        .message
        .contains("Unterminated unit annotation"));

    let diagnostics = parse_expression("10 [m").expect_err("should fail");
    assert!(diagnostics[0]
        .message
        .contains("Unterminated unit annotation"));
}

#[test]
fn rejects_unit_annotation_on_identifier() {
    let diagnostics = parse_expression("x [m]").expect_err("should fail");
    assert!(diagnostics.iter().any(|d| d
        .message
        .contains("Unit annotations are only allowed on numeric literals")
        || d.message.contains("Expected end of input")));
}

#[test]
fn parses_ees_keyword_arguments_with_trailing_comma() {
    let expr = parse_expression("Enthalpy(\"Water\", T=300, P=101325,)").expect("should parse");
    let ExprKind::Call { callee, args } = expr.kind else {
        panic!("expected call");
    };
    assert_eq!(callee, "Enthalpy");
    assert!(matches!(args[0], CallArg::Positional(_)));
    assert!(matches!(args[1], CallArg::Keyword { ref name, .. } if name == "T"));
    assert!(matches!(args[2], CallArg::Keyword { ref name, .. } if name == "P"));
}

#[test]
fn parses_mixed_positional_and_keyword_arguments() {
    let expr = parse_expression("Entropy(fluid, P=101325, T=300)").expect("should parse");
    let ExprKind::Call { args, .. } = expr.kind else {
        panic!("expected call");
    };
    assert!(matches!(args[0], CallArg::Positional(_)));
    assert!(matches!(args[1], CallArg::Keyword { ref name, .. } if name == "P"));
    assert!(matches!(args[2], CallArg::Keyword { ref name, .. } if name == "T"));
}

#[test]
fn reports_missing_equals_in_call_argument() {
    let diagnostics =
        parse_expression("Enthalpy(\"Water\", T 300, P=101325)").expect_err("should fail");
    assert!(diagnostics
        .iter()
        .any(|d| d.message.contains("did you mean to use '='")));
}

#[test]
fn parses_duplicate_keyword_arguments() {
    let expr = parse_expression("Enthalpy(\"Water\", T=300, T=310)").expect("should parse");
    let ExprKind::Call { args, .. } = expr.kind else {
        panic!("expected call");
    };
    assert!(matches!(args[1], CallArg::Keyword { ref name, .. } if name == "T"));
    assert!(matches!(args[2], CallArg::Keyword { ref name, .. } if name == "T"));
}
