use std::collections::{BTreeSet, HashMap};
use std::error::Error;
use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

use crate::parser::ast::{BinaryOp, CallArg, Expr, ExprKind, Program, StatementKind, UnaryOp};
use crate::parser::diagnostic::Span;
use crate::parser::parse_expression;
use crate::props::{Prop, PropsProvider, PropsQuery, StateVar};
use crate::units::UnitRegistry;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConvergenceStatus {
    Converged,
    MaxIterations,
    InvalidSystem,
    SingularJacobian,
    EvaluationError,
    LineSearchFailed,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EquationResidual {
    pub equation_index: usize,
    pub residual: f64,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConvergenceReport {
    pub iterations: usize,
    pub final_norm: f64,
    pub status: ConvergenceStatus,
    pub worst_residuals: Vec<EquationResidual>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SolveResult {
    pub solution: HashMap<String, f64>,
    pub report: ConvergenceReport,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SolveOptions {
    pub max_iters: usize,
    pub residual_tol: f64,
    pub step_tol: f64,
    pub fd_epsilon: f64,
    pub min_step_factor: f64,
    pub initial_guess: HashMap<String, f64>,
}

impl Default for SolveOptions {
    fn default() -> Self {
        Self {
            max_iters: 50,
            residual_tol: 1e-9,
            step_tol: 1e-9,
            fd_epsilon: 1e-6,
            min_step_factor: 1.0 / 1024.0,
            initial_guess: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SolveError {
    pub message: String,
    pub report: ConvergenceReport,
}

impl Display for SolveError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl Error for SolveError {}

#[derive(Debug, Clone)]
struct Equation {
    lhs: Expr,
    rhs: Expr,
    span: Span,
}

pub fn solve_program(
    program: &Program,
    provider: &dyn PropsProvider,
) -> Result<SolveResult, SolveError> {
    solve_program_with_options(program, provider, &SolveOptions::default())
}

pub fn solve_program_with_options(
    program: &Program,
    provider: &dyn PropsProvider,
    options: &SolveOptions,
) -> Result<SolveResult, SolveError> {
    solve_program_with_options_and_fixed(program, provider, options, &HashMap::new())
}

pub fn solve_program_with_options_and_fixed(
    program: &Program,
    provider: &dyn PropsProvider,
    options: &SolveOptions,
    fixed: &HashMap<String, f64>,
) -> Result<SolveResult, SolveError> {
    let equations = extract_equations(program);
    let mut known = extract_known_constants(program)?;
    let known_strings = extract_known_strings(program);
    for (name, value) in fixed {
        known.insert(name.clone(), *value);
    }
    let unknowns = discover_unknowns(&equations, &known);

    let mut env = known.clone();
    for name in &unknowns {
        let guess = options.initial_guess.get(name).copied().unwrap_or(1.0);
        env.insert(name.clone(), guess);
    }

    if unknowns.is_empty() {
        let residuals = evaluate_residuals(&equations, &env, &known_strings, &unknowns, provider)
            .map_err(|err| build_error(err, 0, &equations, &[]))?;
        let norm = l2_norm(&residuals);
        let report = build_report(
            0,
            norm,
            &equations,
            &residuals,
            ConvergenceStatus::InvalidSystem,
        );
        if norm <= options.residual_tol {
            return Ok(SolveResult {
                solution: env,
                report: ConvergenceReport {
                    status: ConvergenceStatus::Converged,
                    ..report
                },
            });
        }

        return Err(SolveError {
            message: "System has no unknowns and non-zero residuals".to_string(),
            report,
        });
    }

    if equations.len() != unknowns.len() {
        let residuals = evaluate_residuals(&equations, &env, &known_strings, &unknowns, provider)
            .unwrap_or_default();
        let norm = l2_norm(&residuals);
        return Err(SolveError {
            message: format!(
                "System is not square: {} equations for {} unknowns",
                equations.len(),
                unknowns.len()
            ),
            report: build_report(
                0,
                norm,
                &equations,
                &residuals,
                ConvergenceStatus::InvalidSystem,
            ),
        });
    }

    let mut residuals = evaluate_residuals(&equations, &env, &known_strings, &unknowns, provider)
        .map_err(|err| build_error(err, 0, &equations, &[]))?;
    let mut norm = l2_norm(&residuals);

    for iter in 0..options.max_iters {
        if norm <= options.residual_tol {
            return Ok(SolveResult {
                solution: env,
                report: build_report(
                    iter,
                    norm,
                    &equations,
                    &residuals,
                    ConvergenceStatus::Converged,
                ),
            });
        }

        let jac = build_jacobian(
            &equations,
            &env,
            &known_strings,
            &unknowns,
            &residuals,
            provider,
            options,
        )
        .map_err(|err| build_error(err, iter, &equations, &residuals))?;
        let rhs: Vec<f64> = residuals.iter().map(|v| -v).collect();
        let dx = solve_linear_system(jac, rhs).ok_or_else(|| SolveError {
            message: "Jacobian is singular; Newton step could not be computed".to_string(),
            report: build_report(
                iter,
                norm,
                &equations,
                &residuals,
                ConvergenceStatus::SingularJacobian,
            ),
        })?;

        let step_norm = scaled_step_norm(&dx, &env, &unknowns);
        let mut alpha = 1.0;
        let mut accepted = None;

        while alpha >= options.min_step_factor {
            let trial_env = apply_step(&env, &unknowns, &dx, alpha);
            let trial_residuals =
                evaluate_residuals(&equations, &trial_env, &known_strings, &unknowns, provider)
                    .map_err(|err| build_error(err, iter, &equations, &residuals))?;
            let trial_norm = l2_norm(&trial_residuals);

            if trial_norm < norm {
                accepted = Some((trial_env, trial_residuals, trial_norm));
                break;
            }

            alpha *= 0.5;
        }

        let Some((next_env, next_residuals, next_norm)) = accepted else {
            return Err(SolveError {
                message: "Line search failed to reduce residual norm".to_string(),
                report: build_report(
                    iter + 1,
                    norm,
                    &equations,
                    &residuals,
                    ConvergenceStatus::LineSearchFailed,
                ),
            });
        };

        env = next_env;
        residuals = next_residuals;
        norm = next_norm;

        if step_norm <= options.step_tol && norm <= options.residual_tol * 10.0 {
            return Ok(SolveResult {
                solution: env,
                report: build_report(
                    iter + 1,
                    norm,
                    &equations,
                    &residuals,
                    ConvergenceStatus::Converged,
                ),
            });
        }
    }

    Err(SolveError {
        message: format!(
            "Solver did not converge in {} iterations",
            options.max_iters
        ),
        report: build_report(
            options.max_iters,
            norm,
            &equations,
            &residuals,
            ConvergenceStatus::MaxIterations,
        ),
    })
}

pub fn evaluate_expression_string(
    source: &str,
    env: &HashMap<String, f64>,
    provider: &dyn PropsProvider,
) -> Result<f64, String> {
    let expr = parse_expression(source).map_err(|diagnostics| {
        diagnostics
            .into_iter()
            .map(|diag| diag.message)
            .collect::<Vec<_>>()
            .join("; ")
    })?;

    eval_expr(&expr, env, &HashMap::new(), &[], provider)
}

fn build_error(
    message: String,
    iterations: usize,
    equations: &[Equation],
    residuals: &[f64],
) -> SolveError {
    SolveError {
        message,
        report: build_report(
            iterations,
            l2_norm(residuals),
            equations,
            residuals,
            ConvergenceStatus::EvaluationError,
        ),
    }
}

fn extract_equations(program: &Program) -> Vec<Equation> {
    program
        .statements
        .iter()
        .filter_map(|statement| {
            let StatementKind::Assignment { lhs, rhs } = &statement.kind;
            if is_constant_assignment(lhs, rhs).is_some()
                || is_string_assignment(lhs, rhs).is_some()
            {
                None
            } else {
                Some(Equation {
                    lhs: lhs.clone(),
                    rhs: rhs.clone(),
                    span: statement.span,
                })
            }
        })
        .collect()
}

fn extract_known_constants(program: &Program) -> Result<HashMap<String, f64>, SolveError> {
    let mut known = HashMap::new();
    for statement in &program.statements {
        let StatementKind::Assignment { lhs, rhs } = &statement.kind;
        if let Some((name, value)) = is_constant_assignment(lhs, rhs) {
            if let Some(previous) = known.insert(name.to_string(), value) {
                if (previous - value).abs() > f64::EPSILON {
                    return Err(SolveError {
                        message: format!(
                            "Conflicting constant assignments for '{name}': {previous} vs {value}"
                        ),
                        report: ConvergenceReport {
                            iterations: 0,
                            final_norm: (previous - value).abs(),
                            status: ConvergenceStatus::InvalidSystem,
                            worst_residuals: Vec::new(),
                        },
                    });
                }
            }
        }
    }
    Ok(known)
}

fn extract_known_strings(program: &Program) -> HashMap<String, String> {
    let mut known = HashMap::new();
    for statement in &program.statements {
        let StatementKind::Assignment { lhs, rhs } = &statement.kind;
        if let Some((name, value)) = is_string_assignment(lhs, rhs) {
            known.insert(name.to_string(), value.to_string());
        }
    }
    known
}

fn is_constant_assignment<'a>(lhs: &'a Expr, rhs: &'a Expr) -> Option<(&'a str, f64)> {
    let ExprKind::Identifier(name) = &lhs.kind else {
        return None;
    };

    match &rhs.kind {
        ExprKind::Number(value) => parse_number(value).ok().map(|v| (name.as_str(), v)),
        ExprKind::QuantityLiteral { value, .. } => Some((name.as_str(), *value)),
        _ => None,
    }
}

fn is_string_assignment<'a>(lhs: &'a Expr, rhs: &'a Expr) -> Option<(&'a str, &'a str)> {
    let ExprKind::Identifier(name) = &lhs.kind else {
        return None;
    };

    match &rhs.kind {
        ExprKind::StringLiteral(value) => Some((name.as_str(), value.as_str())),
        _ => None,
    }
}

fn parse_number(raw: &str) -> Result<f64, SolveError> {
    raw.parse::<f64>().map_err(|_| SolveError {
        message: format!("Failed to parse numeric literal '{raw}'"),
        report: ConvergenceReport {
            iterations: 0,
            final_norm: f64::INFINITY,
            status: ConvergenceStatus::EvaluationError,
            worst_residuals: Vec::new(),
        },
    })
}

fn discover_unknowns(equations: &[Equation], known: &HashMap<String, f64>) -> Vec<String> {
    let mut vars = BTreeSet::new();
    for eq in equations {
        collect_identifiers(&eq.lhs, &mut vars);
        collect_identifiers(&eq.rhs, &mut vars);
    }

    vars.into_iter()
        .filter(|name| !known.contains_key(name))
        .collect()
}

fn collect_identifiers(expr: &Expr, out: &mut BTreeSet<String>) {
    match &expr.kind {
        ExprKind::Identifier(name) => {
            out.insert(name.clone());
        }
        ExprKind::Unary { expr, .. } | ExprKind::Group(expr) => collect_identifiers(expr, out),
        ExprKind::Binary { left, right, .. } => {
            collect_identifiers(left, out);
            collect_identifiers(right, out);
        }
        ExprKind::Call { args, .. } => {
            for arg in args {
                match arg {
                    CallArg::Positional(expr) => collect_identifiers(expr, out),
                    CallArg::Keyword { value, .. } => collect_identifiers(value, out),
                }
            }
        }
        ExprKind::Number(_) | ExprKind::QuantityLiteral { .. } | ExprKind::StringLiteral(_) => {}
    }
}

fn evaluate_residuals(
    equations: &[Equation],
    env: &HashMap<String, f64>,
    strings: &HashMap<String, String>,
    unknowns: &[String],
    provider: &dyn PropsProvider,
) -> Result<Vec<f64>, String> {
    equations
        .iter()
        .map(|eq| {
            let lhs = eval_expr(&eq.lhs, env, strings, unknowns, provider)?;
            let rhs = eval_expr(&eq.rhs, env, strings, unknowns, provider)?;
            Ok(lhs - rhs)
        })
        .collect()
}

fn eval_expr(
    expr: &Expr,
    env: &HashMap<String, f64>,
    strings: &HashMap<String, String>,
    unknowns: &[String],
    provider: &dyn PropsProvider,
) -> Result<f64, String> {
    match &expr.kind {
        ExprKind::Number(raw) => raw
            .parse::<f64>()
            .map_err(|_| format!("Invalid numeric literal '{raw}'")),
        ExprKind::QuantityLiteral { value, .. } => Ok(*value),
        ExprKind::Identifier(name) => {
            if let Some(value) = env.get(name) {
                Ok(*value)
            } else if unknowns.contains(name) {
                Err(format!(
                    "Unknown variable '{name}' has no value in environment"
                ))
            } else {
                Err(format!("Identifier '{name}' is not defined"))
            }
        }
        ExprKind::StringLiteral(value) => Err(format!(
            "String literal '{value}' cannot be used as numeric expression"
        )),
        ExprKind::Unary { op, expr } => {
            let inner = eval_expr(expr, env, strings, unknowns, provider)?;
            match op {
                UnaryOp::Plus => Ok(inner),
                UnaryOp::Minus => Ok(-inner),
            }
        }
        ExprKind::Binary { op, left, right } => {
            let l = eval_expr(left, env, strings, unknowns, provider)?;
            let r = eval_expr(right, env, strings, unknowns, provider)?;
            match op {
                BinaryOp::Add => Ok(l + r),
                BinaryOp::Subtract => Ok(l - r),
                BinaryOp::Multiply => Ok(l * r),
                BinaryOp::Divide => Ok(l / r),
                BinaryOp::Power => Ok(l.powf(r)),
            }
        }
        ExprKind::Group(inner) => eval_expr(inner, env, strings, unknowns, provider),
        ExprKind::Call { callee, args } => {
            eval_call(callee, args, env, strings, unknowns, provider)
        }
    }
}

fn eval_call(
    callee: &str,
    args: &[CallArg],
    env: &HashMap<String, f64>,
    strings: &HashMap<String, String>,
    unknowns: &[String],
    provider: &dyn PropsProvider,
) -> Result<f64, String> {
    let lower = callee.to_ascii_lowercase();
    match lower.as_str() {
        "sin" => eval_unary_math("sin", args, env, strings, unknowns, provider, f64::sin),
        "cos" => eval_unary_math("cos", args, env, strings, unknowns, provider, f64::cos),
        "tan" => eval_unary_math("tan", args, env, strings, unknowns, provider, f64::tan),
        "exp" => eval_unary_math("exp", args, env, strings, unknowns, provider, f64::exp),
        "ln" | "log" => eval_unary_math("ln/log", args, env, strings, unknowns, provider, f64::ln),
        "sqrt" => eval_unary_math("sqrt", args, env, strings, unknowns, provider, f64::sqrt),
        "abs" => eval_unary_math("abs", args, env, strings, unknowns, provider, f64::abs),
        "h" => eval_legacy_property_call("h", args, env, strings, unknowns, provider, Prop::H),
        "s" => eval_legacy_property_call("s", args, env, strings, unknowns, provider, Prop::S),
        "rho" => eval_legacy_property_call("rho", args, env, strings, unknowns, provider, Prop::D),
        "t_from_ph" => {
            eval_legacy_property_call("t_from_ph", args, env, strings, unknowns, provider, Prop::T)
        }
        "p_from_th" => {
            eval_legacy_property_call("p_from_th", args, env, strings, unknowns, provider, Prop::P)
        }
        "enthalpy" => {
            eval_ees_property_call(callee, args, env, strings, unknowns, provider, Prop::H)
        }
        "entropy" => {
            eval_ees_property_call(callee, args, env, strings, unknowns, provider, Prop::S)
        }
        "density" => {
            eval_ees_property_call(callee, args, env, strings, unknowns, provider, Prop::D)
        }
        _ => Err(format!("Unsupported function '{callee}'")),
    }
}

fn eval_unary_math(
    name: &str,
    args: &[CallArg],
    env: &HashMap<String, f64>,
    strings: &HashMap<String, String>,
    unknowns: &[String],
    provider: &dyn PropsProvider,
    op: fn(f64) -> f64,
) -> Result<f64, String> {
    if args.len() != 1 {
        return Err(format!("Function '{name}' expects 1 argument"));
    }
    let CallArg::Positional(value_expr) = &args[0] else {
        return Err(format!("Function '{name}' expects positional arguments"));
    };
    let value = eval_expr(value_expr, env, strings, unknowns, provider)?;
    Ok(op(value))
}

fn eval_legacy_property_call(
    name: &str,
    args: &[CallArg],
    env: &HashMap<String, f64>,
    strings: &HashMap<String, String>,
    unknowns: &[String],
    provider: &dyn PropsProvider,
    out: Prop,
) -> Result<f64, String> {
    if args.len() != 3 {
        return Err(format!("Function '{name}' expects 3 arguments"));
    }

    let fluid = eval_fluid_arg(name, &args[0], strings)?;
    let a1 = eval_state_value(&args[1], env, strings, unknowns, provider)?;
    let a2 = eval_state_value(&args[2], env, strings, unknowns, provider)?;

    let query = match out {
        Prop::H | Prop::S | Prop::D => {
            PropsQuery::new(fluid, out, (StateVar::T, a1), (StateVar::P, a2))
        }
        Prop::T => PropsQuery::new(fluid, out, (StateVar::P, a1), (StateVar::H, a2)),
        Prop::P => PropsQuery::new(fluid, out, (StateVar::T, a1), (StateVar::H, a2)),
    };

    provider
        .query(&query)
        .map_err(|err| format!("Property call '{name}' failed: {err}"))
}

fn eval_ees_property_call(
    name: &str,
    args: &[CallArg],
    env: &HashMap<String, f64>,
    strings: &HashMap<String, String>,
    unknowns: &[String],
    provider: &dyn PropsProvider,
    out: Prop,
) -> Result<f64, String> {
    if args.len() < 3 {
        return Err(format!(
            "Function '{name}' expects fluid plus two state keyword arguments"
        ));
    }

    let fluid = eval_fluid_arg(name, &args[0], strings)?;
    let mut states: Vec<(StateVar, f64)> = Vec::new();
    for arg in &args[1..] {
        let CallArg::Keyword {
            name: key, value, ..
        } = arg
        else {
            return Err(format!(
                "Function '{name}' expects keyword state arguments after fluid"
            ));
        };
        let var = parse_state_var(key)
            .ok_or_else(|| format!("Function '{name}' received unknown state key '{key}'"))?;
        if states.iter().any(|(existing, _)| *existing == var) {
            return Err(format!(
                "Function '{name}' received duplicate state key '{key}'"
            ));
        }
        let value = eval_state_value_expr(value, env, strings, unknowns, provider)?;
        states.push((var, value));
    }

    if states.len() != 2 {
        return Err(format!(
            "Function '{name}' requires exactly two state keywords"
        ));
    }

    let query = PropsQuery::new(fluid, out, states[0], states[1]);
    provider
        .query(&query)
        .map_err(|err| format!("Property call '{name}' failed: {err}"))
}

fn eval_fluid_arg<'a>(
    name: &str,
    arg: &'a CallArg,
    strings: &'a HashMap<String, String>,
) -> Result<&'a str, String> {
    match arg {
        CallArg::Positional(expr) => match &expr.kind {
            ExprKind::StringLiteral(value) => Ok(value.as_str()),
            ExprKind::Identifier(id) => strings
                .get(id)
                .map(String::as_str)
                .ok_or_else(|| format!("Function '{name}' requires first argument fluid as string literal or string variable")),
            _ => Err(format!("Function '{name}' requires first argument fluid as string literal or string variable")),
        },
        CallArg::Keyword { .. } => Err(format!("Function '{name}' requires first argument fluid as positional string")),
    }
}

fn eval_state_value(
    arg: &CallArg,
    env: &HashMap<String, f64>,
    strings: &HashMap<String, String>,
    unknowns: &[String],
    provider: &dyn PropsProvider,
) -> Result<f64, String> {
    let CallArg::Positional(expr) = arg else {
        return Err("Expected positional numeric argument".to_string());
    };
    eval_state_value_expr(expr, env, strings, unknowns, provider)
}

fn eval_state_value_expr(
    expr: &Expr,
    env: &HashMap<String, f64>,
    strings: &HashMap<String, String>,
    unknowns: &[String],
    provider: &dyn PropsProvider,
) -> Result<f64, String> {
    if let ExprKind::QuantityLiteral { value, unit, .. } = &expr.kind {
        let registry = UnitRegistry::default();
        let parsed = registry
            .parse_unit_string(unit)
            .map_err(|err| format!("Invalid unit '{unit}' in property argument: {err}"))?;
        return Ok(*value * parsed.scale);
    }

    // Plain numeric expressions are treated as already in base SI units.
    eval_expr(expr, env, strings, unknowns, provider)
}

fn parse_state_var(key: &str) -> Option<StateVar> {
    match key.to_ascii_lowercase().as_str() {
        "t" => Some(StateVar::T),
        "p" => Some(StateVar::P),
        "h" => Some(StateVar::H),
        "s" => Some(StateVar::S),
        "d" | "rho" => Some(StateVar::D),
        _ => None,
    }
}

fn build_jacobian(
    equations: &[Equation],
    env: &HashMap<String, f64>,
    strings: &HashMap<String, String>,
    unknowns: &[String],
    baseline_residuals: &[f64],
    provider: &dyn PropsProvider,
    options: &SolveOptions,
) -> Result<Vec<Vec<f64>>, String> {
    let mut jac = vec![vec![0.0; unknowns.len()]; equations.len()];

    for (j, name) in unknowns.iter().enumerate() {
        let Some(base_value) = env.get(name).copied() else {
            return Err(format!("Missing value for unknown '{name}'"));
        };

        let h = options.fd_epsilon * base_value.abs().max(1.0);
        let mut perturbed = env.clone();
        perturbed.insert(name.clone(), base_value + h);
        let perturbed_residuals =
            evaluate_residuals(equations, &perturbed, strings, unknowns, provider)?;

        for (i, row) in jac.iter_mut().enumerate() {
            row[j] = (perturbed_residuals[i] - baseline_residuals[i]) / h;
        }
    }

    Ok(jac)
}

fn solve_linear_system(mut a: Vec<Vec<f64>>, mut b: Vec<f64>) -> Option<Vec<f64>> {
    let n = b.len();

    for col in 0..n {
        let mut pivot_row = col;
        let mut pivot_value = a[col][col].abs();
        for (row, row_values) in a.iter().enumerate().skip(col + 1).take(n - col - 1) {
            let candidate = row_values[col].abs();
            if candidate > pivot_value {
                pivot_value = candidate;
                pivot_row = row;
            }
        }

        if pivot_value <= 1e-14 {
            return None;
        }

        if pivot_row != col {
            a.swap(col, pivot_row);
            b.swap(col, pivot_row);
        }

        let pivot = a[col][col];
        for value in &mut a[col][col..n] {
            *value /= pivot;
        }
        b[col] /= pivot;

        let pivot_segment = a[col][col..n].to_vec();
        for row in (col + 1)..n {
            let factor = a[row][col];
            if factor.abs() <= f64::EPSILON {
                continue;
            }
            for (offset, pivot_value) in pivot_segment.iter().enumerate() {
                a[row][col + offset] -= factor * pivot_value;
            }
            b[row] -= factor * b[col];
        }
    }

    let mut x = vec![0.0; n];
    for row in (0..n).rev() {
        let mut sum = b[row];
        for (col, value) in a[row].iter().enumerate().skip(row + 1).take(n - row - 1) {
            sum -= value * x[col];
        }
        x[row] = sum;
    }

    Some(x)
}

fn apply_step(
    env: &HashMap<String, f64>,
    unknowns: &[String],
    dx: &[f64],
    alpha: f64,
) -> HashMap<String, f64> {
    let mut next = env.clone();
    for (name, delta) in unknowns.iter().zip(dx) {
        if let Some(value) = next.get_mut(name) {
            *value += alpha * delta;
        }
    }
    next
}

fn scaled_step_norm(dx: &[f64], env: &HashMap<String, f64>, unknowns: &[String]) -> f64 {
    let sum_sq = unknowns
        .iter()
        .zip(dx)
        .map(|(name, delta)| {
            let x = env.get(name).copied().unwrap_or(1.0);
            let scale = x.abs().max(1.0);
            let scaled = delta / scale;
            scaled * scaled
        })
        .sum::<f64>();

    sum_sq.sqrt()
}

fn l2_norm(values: &[f64]) -> f64 {
    values.iter().map(|value| value * value).sum::<f64>().sqrt()
}

fn build_report(
    iterations: usize,
    final_norm: f64,
    equations: &[Equation],
    residuals: &[f64],
    status: ConvergenceStatus,
) -> ConvergenceReport {
    let mut worst: Vec<_> = equations
        .iter()
        .enumerate()
        .map(|(idx, eq)| EquationResidual {
            equation_index: idx,
            residual: residuals.get(idx).copied().unwrap_or(f64::NAN),
            span: eq.span,
        })
        .collect();

    worst.sort_by(|a, b| b.residual.abs().total_cmp(&a.residual.abs()));

    ConvergenceReport {
        iterations,
        final_norm,
        status,
        worst_residuals: worst,
    }
}
