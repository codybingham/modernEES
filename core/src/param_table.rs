use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::parser::ast::Program;
use crate::props::PropsProvider;
use crate::solver::{
    evaluate_expression_string, solve_program_with_options_and_fixed, ConvergenceStatus,
    SolveOptions,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParamTableSpec {
    pub sweeps: Vec<Sweep>,
    pub columns: Vec<ColumnSpec>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sweep {
    pub var: String,
    pub values: Vec<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ColumnSpec {
    pub name: String,
    pub expression: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParamTableResult {
    pub rows: Vec<ParamTableRowResult>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParamTableRowResult {
    pub inputs: HashMap<String, f64>,
    pub outputs: HashMap<String, f64>,
    pub convergence_status: ConvergenceStatus,
    pub iterations: Option<usize>,
    pub final_norm: Option<f64>,
    pub error: Option<String>,
}

pub fn run_param_table(
    program: &Program,
    table: &ParamTableSpec,
    provider: &dyn PropsProvider,
    options: SolveOptions,
) -> ParamTableResult {
    let rows = sweep_rows(&table.sweeps);
    let mut result_rows = Vec::with_capacity(rows.len());
    let mut previous_solution: Option<HashMap<String, f64>> = None;

    for row_inputs in rows {
        let mut row_options = options.clone();
        if let Some(previous) = &previous_solution {
            row_options.initial_guess = previous.clone();
        }

        match solve_program_with_options_and_fixed(program, provider, &row_options, &row_inputs) {
            Ok(solved) => {
                let mut outputs = HashMap::new();
                let mut eval_error = None;

                for column in &table.columns {
                    match evaluate_expression_string(&column.expression, &solved.solution, provider)
                    {
                        Ok(value) => {
                            outputs.insert(column.name.clone(), value);
                        }
                        Err(err) => {
                            eval_error = Some(format!(
                                "Failed to evaluate column '{}' expression '{}': {}",
                                column.name, column.expression, err
                            ));
                            break;
                        }
                    }
                }

                if let Some(error) = eval_error {
                    result_rows.push(ParamTableRowResult {
                        inputs: row_inputs,
                        outputs,
                        convergence_status: ConvergenceStatus::EvaluationError,
                        iterations: Some(solved.report.iterations),
                        final_norm: Some(solved.report.final_norm),
                        error: Some(error),
                    });
                } else {
                    previous_solution = Some(solved.solution.clone());
                    result_rows.push(ParamTableRowResult {
                        inputs: row_inputs,
                        outputs,
                        convergence_status: solved.report.status,
                        iterations: Some(solved.report.iterations),
                        final_norm: Some(solved.report.final_norm),
                        error: None,
                    });
                }
            }
            Err(err) => {
                result_rows.push(ParamTableRowResult {
                    inputs: row_inputs,
                    outputs: HashMap::new(),
                    convergence_status: err.report.status,
                    iterations: Some(err.report.iterations),
                    final_norm: Some(err.report.final_norm),
                    error: Some(err.message),
                });
            }
        }
    }

    ParamTableResult { rows: result_rows }
}

fn sweep_rows(sweeps: &[Sweep]) -> Vec<HashMap<String, f64>> {
    if sweeps.is_empty() {
        return vec![HashMap::new()];
    }

    let mut rows = vec![HashMap::new()];
    for sweep in sweeps {
        let mut next_rows = Vec::with_capacity(rows.len().saturating_mul(sweep.values.len()));
        for row in &rows {
            for value in &sweep.values {
                let mut next = row.clone();
                next.insert(sweep.var.clone(), *value);
                next_rows.push(next);
            }
        }
        rows = next_rows;
    }

    rows
}

pub fn save_param_table_spec(path: impl AsRef<Path>, spec: &ParamTableSpec) -> Result<(), String> {
    let json = serde_json::to_string_pretty(spec).map_err(|err| err.to_string())?;
    fs::write(path, json).map_err(|err| err.to_string())
}

pub fn load_param_table_spec(path: impl AsRef<Path>) -> Result<ParamTableSpec, String> {
    let json = fs::read_to_string(path).map_err(|err| err.to_string())?;
    serde_json::from_str(&json).map_err(|err| err.to_string())
}

pub fn save_param_table_result(
    path: impl AsRef<Path>,
    result: &ParamTableResult,
) -> Result<(), String> {
    let json = serde_json::to_string_pretty(result).map_err(|err| err.to_string())?;
    fs::write(path, json).map_err(|err| err.to_string())
}

pub fn load_param_table_result(path: impl AsRef<Path>) -> Result<ParamTableResult, String> {
    let json = fs::read_to_string(path).map_err(|err| err.to_string())?;
    serde_json::from_str(&json).map_err(|err| err.to_string())
}
