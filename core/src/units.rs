use std::collections::HashMap;

use crate::parser::ast::{BinaryOp, Expr, ExprKind, Program, StatementKind, UnaryOp};
use crate::parser::diagnostic::{Diagnostic, Span};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Dimension {
    pub mass: i32,
    pub length: i32,
    pub time: i32,
    pub temperature: i32,
}

impl Dimension {
    #[must_use]
    pub fn dimensionless() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn add(self, rhs: Self) -> Self {
        Self {
            mass: self.mass + rhs.mass,
            length: self.length + rhs.length,
            time: self.time + rhs.time,
            temperature: self.temperature + rhs.temperature,
        }
    }

    #[must_use]
    pub fn sub(self, rhs: Self) -> Self {
        Self {
            mass: self.mass - rhs.mass,
            length: self.length - rhs.length,
            time: self.time - rhs.time,
            temperature: self.temperature - rhs.temperature,
        }
    }

    #[must_use]
    pub fn scale(self, factor: i32) -> Self {
        Self {
            mass: self.mass * factor,
            length: self.length * factor,
            time: self.time * factor,
            temperature: self.temperature * factor,
        }
    }

    #[must_use]
    pub fn is_dimensionless(self) -> bool {
        self == Self::dimensionless()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Unit {
    pub dimension: Dimension,
    pub scale: f64,
}

impl Unit {
    #[must_use]
    pub fn dimensionless() -> Self {
        Self {
            dimension: Dimension::dimensionless(),
            scale: 1.0,
        }
    }

    #[must_use]
    pub fn compatible_with(self, rhs: Self) -> bool {
        self.dimension == rhs.dimension
    }

    #[must_use]
    pub fn multiply(self, rhs: Self) -> Self {
        Self {
            dimension: self.dimension.add(rhs.dimension),
            scale: self.scale * rhs.scale,
        }
    }

    #[must_use]
    pub fn divide(self, rhs: Self) -> Self {
        Self {
            dimension: self.dimension.sub(rhs.dimension),
            scale: self.scale / rhs.scale,
        }
    }

    #[must_use]
    pub fn power(self, exponent: i32) -> Self {
        Self {
            dimension: self.dimension.scale(exponent),
            scale: self.scale.powi(exponent),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum UnitKnowledge {
    Known(Unit),
    Unknown,
}

impl UnitKnowledge {
    fn known(unit: Unit) -> Self {
        Self::Known(unit)
    }

    fn as_known(self) -> Option<Unit> {
        match self {
            Self::Known(unit) => Some(unit),
            Self::Unknown => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct UnitRegistry {
    units: HashMap<String, Unit>,
}

impl Default for UnitRegistry {
    fn default() -> Self {
        let mut units = HashMap::new();

        let length = |scale| Unit {
            dimension: Dimension {
                length: 1,
                ..Dimension::default()
            },
            scale,
        };
        let time = |scale| Unit {
            dimension: Dimension {
                time: 1,
                ..Dimension::default()
            },
            scale,
        };
        let mass = |scale| Unit {
            dimension: Dimension {
                mass: 1,
                ..Dimension::default()
            },
            scale,
        };
        let temp = |scale| Unit {
            dimension: Dimension {
                temperature: 1,
                ..Dimension::default()
            },
            scale,
        };

        units.insert("m".to_string(), length(1.0));
        units.insert("cm".to_string(), length(0.01));
        units.insert("mm".to_string(), length(0.001));
        units.insert("km".to_string(), length(1000.0));
        units.insert("in".to_string(), length(0.0254));
        units.insert("ft".to_string(), length(0.3048));

        units.insert("s".to_string(), time(1.0));
        units.insert("min".to_string(), time(60.0));
        units.insert("hr".to_string(), time(3600.0));

        units.insert("kg".to_string(), mass(1.0));
        units.insert("g".to_string(), mass(0.001));
        units.insert("lbm".to_string(), mass(0.453_592_37));

        units.insert("K".to_string(), temp(1.0));

        Self { units }
    }
}

impl UnitRegistry {
    pub fn parse_unit_string(&self, input: &str) -> Result<Unit, String> {
        let text: String = input.chars().filter(|c| !c.is_whitespace()).collect();
        if text.is_empty() {
            return Err("Unit string is empty".to_string());
        }

        let chars: Vec<char> = text.chars().collect();
        let mut idx = 0usize;
        let mut unit = Unit::dimensionless();
        let mut divide = false;

        while idx < chars.len() {
            let op = chars[idx];
            if op == '*' {
                divide = false;
                idx += 1;
                continue;
            }
            if op == '/' {
                divide = true;
                idx += 1;
                continue;
            }

            if !chars[idx].is_ascii_alphabetic() {
                return Err(format!("Invalid unit token starting at '{}'", chars[idx]));
            }

            let start = idx;
            while idx < chars.len() && (chars[idx].is_ascii_alphabetic() || chars[idx] == '_') {
                idx += 1;
            }
            let symbol: String = chars[start..idx].iter().collect();
            let mut base = *self
                .units
                .get(&symbol)
                .ok_or_else(|| format!("Unknown unit '{}'", symbol))?;

            let mut exponent = 1;
            if idx < chars.len() && chars[idx] == '^' {
                idx += 1;
                if idx >= chars.len() {
                    return Err("Missing exponent after '^'".to_string());
                }

                let exp_start = idx;
                if chars[idx] == '-' {
                    idx += 1;
                }
                while idx < chars.len() && chars[idx].is_ascii_digit() {
                    idx += 1;
                }
                if exp_start == idx || (exp_start + 1 == idx && chars[exp_start] == '-') {
                    return Err("Exponent must be an integer".to_string());
                }

                let exp_text: String = chars[exp_start..idx].iter().collect();
                exponent = exp_text
                    .parse::<i32>()
                    .map_err(|_| "Exponent out of range".to_string())?;
            }

            base = base.power(exponent);
            unit = if divide {
                unit.divide(base)
            } else {
                unit.multiply(base)
            };
            divide = false;
        }

        Ok(unit)
    }
}

pub fn analyze_units(program: &Program) -> Vec<Diagnostic> {
    let registry = UnitRegistry::default();
    let mut diagnostics = Vec::new();
    let mut env: HashMap<String, UnitKnowledge> = HashMap::new();

    for statement in &program.statements {
        let StatementKind::Assignment { lhs, rhs } = &statement.kind;

        let lhs_unit = infer_expr_unit(lhs, &env, &registry, &mut diagnostics);
        let rhs_unit = infer_expr_unit(rhs, &env, &registry, &mut diagnostics);

        if let (Some(left), Some(right)) = (lhs_unit.as_known(), rhs_unit.as_known()) {
            if !left.compatible_with(right) {
                diagnostics.push(Diagnostic::new(
                    "Assignment has incompatible units",
                    statement.span,
                ));
            }
        }

        if let ExprKind::Identifier(name) = &lhs.kind {
            env.insert(name.clone(), rhs_unit);
        }
    }

    diagnostics
}

#[cfg(test)]
fn infer_unit_for_program_rhs(source: &str) -> Result<Option<Unit>, Vec<Diagnostic>> {
    let program = crate::parse_program(source).expect("program should parse");
    let StatementKind::Assignment { rhs, .. } = &program.statements[0].kind;

    let mut diagnostics = Vec::new();
    let unit = infer_expr_unit(
        rhs,
        &HashMap::new(),
        &UnitRegistry::default(),
        &mut diagnostics,
    );
    if diagnostics.is_empty() {
        Ok(unit.as_known())
    } else {
        Err(diagnostics)
    }
}

fn infer_expr_unit(
    expr: &Expr,
    env: &HashMap<String, UnitKnowledge>,
    registry: &UnitRegistry,
    diagnostics: &mut Vec<Diagnostic>,
) -> UnitKnowledge {
    match &expr.kind {
        ExprKind::Number(_) => UnitKnowledge::known(Unit::dimensionless()),
        ExprKind::QuantityLiteral { unit, span, .. } => match registry.parse_unit_string(unit) {
            Ok(parsed) => UnitKnowledge::known(parsed),
            Err(message) => {
                diagnostics.push(Diagnostic::new(
                    format!("Invalid unit string '{}': {message}", unit),
                    *span,
                ));
                UnitKnowledge::Unknown
            }
        },
        ExprKind::Identifier(name) => env.get(name).copied().unwrap_or(UnitKnowledge::Unknown),
        ExprKind::StringLiteral(_) => UnitKnowledge::Unknown,
        ExprKind::Unary { op, expr } => {
            let inner = infer_expr_unit(expr, env, registry, diagnostics);
            match op {
                UnaryOp::Plus | UnaryOp::Minus => inner,
            }
        }
        ExprKind::Binary { op, left, right } => {
            let left_unit = infer_expr_unit(left, env, registry, diagnostics);
            let right_unit = infer_expr_unit(right, env, registry, diagnostics);
            match op {
                BinaryOp::Add | BinaryOp::Subtract => {
                    match (left_unit.as_known(), right_unit.as_known()) {
                        (Some(lhs), Some(rhs)) => {
                            if !lhs.compatible_with(rhs) {
                                diagnostics.push(Diagnostic::new(
                                    "Addition/subtraction requires compatible units",
                                    expr.span,
                                ));
                            }
                            UnitKnowledge::known(lhs)
                        }
                        _ => UnitKnowledge::Unknown,
                    }
                }
                BinaryOp::Multiply => match (left_unit.as_known(), right_unit.as_known()) {
                    (Some(lhs), Some(rhs)) => UnitKnowledge::known(lhs.multiply(rhs)),
                    _ => UnitKnowledge::Unknown,
                },
                BinaryOp::Divide => match (left_unit.as_known(), right_unit.as_known()) {
                    (Some(lhs), Some(rhs)) => UnitKnowledge::known(lhs.divide(rhs)),
                    _ => UnitKnowledge::Unknown,
                },
                BinaryOp::Power => {
                    let exponent = extract_integer_exponent(right);
                    let right_known = right_unit.as_known();
                    let right_dimensionless = right_known
                        .map(|unit| unit.dimension.is_dimensionless())
                        .unwrap_or(false);

                    if !right_dimensionless {
                        diagnostics.push(Diagnostic::new(
                            "Exponent must be dimensionless",
                            right.span,
                        ));
                        return UnitKnowledge::Unknown;
                    }

                    let Some(exponent) = exponent else {
                        diagnostics.push(Diagnostic::new(
                            "Exponent must be an integer literal",
                            right.span,
                        ));
                        return UnitKnowledge::Unknown;
                    };

                    match left_unit.as_known() {
                        Some(base) => UnitKnowledge::known(base.power(exponent)),
                        None => UnitKnowledge::Unknown,
                    }
                }
            }
        }
        ExprKind::Call { callee, args } if callee == "to" => {
            infer_conversion_call(args, registry, env, diagnostics, expr.span)
        }
        ExprKind::Call { .. } => UnitKnowledge::Unknown,
        ExprKind::Group(inner) => infer_expr_unit(inner, env, registry, diagnostics),
    }
}

fn infer_conversion_call(
    args: &[Expr],
    registry: &UnitRegistry,
    env: &HashMap<String, UnitKnowledge>,
    diagnostics: &mut Vec<Diagnostic>,
    span: Span,
) -> UnitKnowledge {
    if args.len() != 2 {
        return UnitKnowledge::Unknown;
    }

    let source = infer_expr_unit(&args[0], env, registry, diagnostics);
    let ExprKind::StringLiteral(unit_text) = &args[1].kind else {
        diagnostics.push(Diagnostic::new(
            "to(x, unit) requires a string literal unit",
            args[1].span,
        ));
        return UnitKnowledge::Unknown;
    };

    let target = match registry.parse_unit_string(unit_text) {
        Ok(unit) => unit,
        Err(message) => {
            diagnostics.push(Diagnostic::new(
                format!("Invalid conversion target unit '{}': {message}", unit_text),
                args[1].span,
            ));
            return UnitKnowledge::Unknown;
        }
    };

    if let Some(source_unit) = source.as_known() {
        if !source_unit.compatible_with(target) {
            diagnostics.push(Diagnostic::new(
                "to() conversion requires compatible units",
                span,
            ));
        }
    }

    UnitKnowledge::known(target)
}

fn extract_integer_exponent(expr: &Expr) -> Option<i32> {
    match &expr.kind {
        ExprKind::Number(n) => {
            if n.contains('.') || n.contains('e') || n.contains('E') {
                return None;
            }
            n.parse().ok()
        }
        ExprKind::Unary {
            op: UnaryOp::Minus,
            expr,
        } => extract_integer_exponent(expr).map(|v| -v),
        ExprKind::Group(inner) => extract_integer_exponent(inner),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{analyze_units, infer_unit_for_program_rhs, Dimension};

    #[test]
    fn matching_units_no_diagnostic() {
        let program = crate::parse_program("a = 1 [m] + 2 [m]\n").expect("parse should succeed");
        let diagnostics = analyze_units(&program);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn mismatch_units_has_diagnostic() {
        let program = crate::parse_program("a = 1 [m] + 2 [s]\n").expect("parse should succeed");
        let diagnostics = analyze_units(&program);
        assert!(!diagnostics.is_empty());
    }

    #[test]
    fn multiply_dims_infers_area() {
        let unit = infer_unit_for_program_rhs("A = 2 [m] * 3 [m]\n")
            .expect("unit inference should succeed")
            .expect("unit should be known");
        assert_eq!(
            unit.dimension,
            Dimension {
                length: 2,
                ..Dimension::default()
            }
        );
    }

    #[test]
    fn division_dims_infers_density() {
        let unit = infer_unit_for_program_rhs("rho = 1 [kg] / 1 [m^3]\n")
            .expect("unit inference should succeed")
            .expect("unit should be known");
        assert_eq!(
            unit.dimension,
            Dimension {
                mass: 1,
                length: -3,
                ..Dimension::default()
            }
        );
    }

    #[test]
    fn conversion_ok_no_diagnostic() {
        let program =
            crate::parse_program("x = to(12 [in], \"ft\")\n").expect("parse should succeed");
        let diagnostics = analyze_units(&program);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn conversion_mismatch_has_diagnostic() {
        let program = crate::parse_program("x = to(1 [m], \"s\")\n").expect("parse should succeed");
        let diagnostics = analyze_units(&program);
        assert!(!diagnostics.is_empty());
    }

    #[test]
    fn unknown_var_tolerated() {
        let program = crate::parse_program("a = b + 1 [m]\n").expect("parse should succeed");
        let diagnostics = analyze_units(&program);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn invalid_unit_string_has_diagnostic() {
        let program = crate::parse_program("a = 1 [not_a_unit]\n").expect("parse should succeed");
        let diagnostics = analyze_units(&program);
        assert!(!diagnostics.is_empty());
    }
}
