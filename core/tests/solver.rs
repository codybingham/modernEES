use std::collections::HashMap;

use modern_ees_core::props::MockPropsProvider;
use modern_ees_core::solve_program;
use modern_ees_core::{parse_program, solve_program_with_options, ConvergenceStatus, SolveOptions};

fn parse(source: &str) -> modern_ees_core::ast::Program {
    parse_program(source).expect("program should parse")
}

#[test]
fn solves_linear_two_by_two_system() {
    let provider = MockPropsProvider::new();
    let program = parse(
        "x + y = 5
x - y = 1
",
    );

    let result = solve_program(&program, &provider).expect("solve should succeed");

    let x = result.solution.get("x").copied().expect("x should exist");
    let y = result.solution.get("y").copied().expect("y should exist");
    assert!((x - 3.0).abs() < 1e-9);
    assert!((y - 2.0).abs() < 1e-9);
    assert_eq!(result.report.status, ConvergenceStatus::Converged);
}

#[test]
fn solves_nonlinear_equation() {
    let provider = MockPropsProvider::new();
    let program = parse("x^2 = 2\n");

    let result = solve_program(&program, &provider).expect("solve should succeed");
    let x = result.solution.get("x").copied().expect("x should exist");

    assert!((x - 2.0_f64.sqrt()).abs() < 1e-7);
}

#[test]
fn solves_system_with_property_call() {
    let provider = MockPropsProvider::new().with_fallback_formula(true);
    let program = parse(
        "p = 100
target = 40
h(\"Water\", t, p) = target
",
    );

    let result = solve_program(&program, &provider).expect("solve should succeed");
    let t = result.solution.get("t").copied().expect("t should exist");
    assert!((t - 40.0).abs() < 1e-7);
}

#[test]
fn converges_from_bad_initial_guess_with_damping() {
    let provider = MockPropsProvider::new();
    let program = parse("x^2 = 2\n");

    let mut initial_guess = HashMap::new();
    initial_guess.insert("x".to_string(), 0.1);
    let options = SolveOptions {
        initial_guess,
        ..SolveOptions::default()
    };

    let result = solve_program_with_options(&program, &provider, &options)
        .expect("damped Newton should converge from poor initial guess");

    let x = result.solution.get("x").copied().expect("x should exist");
    assert!((x - 2.0_f64.sqrt()).abs() < 1e-7);
}

#[test]
fn returns_clean_error_for_invalid_systems() {
    let provider = MockPropsProvider::new();

    let underdetermined = parse("x + y = 1\n");
    let err = solve_program(&underdetermined, &provider).expect_err("should fail");
    assert_eq!(err.report.status, ConvergenceStatus::InvalidSystem);
    assert!(err.message.contains("not square"));

    let inconsistent = parse(
        "x = 1
x = 2
",
    );
    let err = solve_program(&inconsistent, &provider).expect_err("should fail");
    assert_eq!(err.report.status, ConvergenceStatus::InvalidSystem);
    assert!(
        err.message.contains("non-zero residuals")
            || err.message.contains("Conflicting constant assignments")
    );
}

use modern_ees_core::props::{Prop, PropsQuery, StateVar};

#[test]
fn solves_with_enthalpy_keyword_arguments_unordered() {
    let provider = MockPropsProvider::new();
    provider.insert(
        PropsQuery::new(
            "Water",
            Prop::H,
            (StateVar::T, 300.0),
            (StateVar::P, 101_325.0),
        ),
        1234.5,
    );

    let program = parse("target = 1234.5\nEnthalpy(\"Water\", P=101325, T=300) = target\n");
    let result = solve_program(&program, &provider).expect("solve should succeed");
    assert_eq!(result.report.status, ConvergenceStatus::Converged);
}

#[test]
fn ees_keyword_arguments_report_duplicate_missing_and_unknown() {
    let provider = MockPropsProvider::new();

    let duplicate = parse("x = Enthalpy(\"Water\", T=300, T=310)\n");
    let err = solve_program(&duplicate, &provider).expect_err("duplicate should fail");
    assert!(err.message.contains("duplicate state key"));

    let missing = parse("x = Entropy(\"Water\", T=300)\n");
    let err = solve_program(&missing, &provider).expect_err("missing should fail");
    assert!(
        err.message
            .contains("expects fluid plus two state keyword arguments")
            || err.message.contains("exactly two state keywords")
    );

    let unknown = parse("x = Density(\"Water\", X=1, P=101325)\n");
    let err = solve_program(&unknown, &provider).expect_err("unknown key should fail");
    assert!(err.message.contains("unknown state key"));
}

#[test]
fn ees_keyword_argument_quantity_literal_converts_to_si() {
    let provider = MockPropsProvider::new();
    provider.insert(
        PropsQuery::new(
            "Water",
            Prop::D,
            (StateVar::T, 300.0),
            (StateVar::P, 101_325.0),
        ),
        997.0,
    );

    let program = parse(
        "rho_target = 997\nDensity(\"Water\", T=300 [K], P=101325 [kg/m/s^2]) = rho_target\n",
    );
    let result = solve_program(&program, &provider).expect("solve should succeed");
    assert_eq!(result.report.status, ConvergenceStatus::Converged);
}
