use modern_ees_core::param_table::{
    run_param_table, ColumnSpec, ParamTableResult, ParamTableSpec, Sweep,
};
use modern_ees_core::props::MockPropsProvider;
use modern_ees_core::{parse_program, ConvergenceStatus, SolveOptions};

fn parse(source: &str) -> modern_ees_core::ast::Program {
    parse_program(source).expect("program should parse")
}

#[test]
fn simple_sweep_converges_and_evaluates_columns() {
    let provider = MockPropsProvider::new();
    let program = parse(
        "x + y = a
x - y = 1
",
    );
    let table = ParamTableSpec {
        sweeps: vec![Sweep {
            var: "a".to_string(),
            values: vec![5.0, 7.0],
        }],
        columns: vec![
            ColumnSpec {
                name: "x_col".to_string(),
                expression: "x".to_string(),
            },
            ColumnSpec {
                name: "sum".to_string(),
                expression: "x + y".to_string(),
            },
        ],
    };

    let result = run_param_table(&program, &table, &provider, SolveOptions::default());

    assert_eq!(result.rows.len(), 2);
    assert_eq!(
        result.rows[0].convergence_status,
        ConvergenceStatus::Converged
    );
    assert!((result.rows[0].outputs["x_col"] - 3.0).abs() < 1e-9);
    assert!((result.rows[0].outputs["sum"] - 5.0).abs() < 1e-9);
    assert_eq!(
        result.rows[1].convergence_status,
        ConvergenceStatus::Converged
    );
    assert!((result.rows[1].outputs["x_col"] - 4.0).abs() < 1e-9);
    assert!((result.rows[1].outputs["sum"] - 7.0).abs() < 1e-9);
}

#[test]
fn previous_row_solution_is_reused_as_initial_guess() {
    let provider = MockPropsProvider::new();
    let program = parse("x^2 = a\n");
    let table = ParamTableSpec {
        sweeps: vec![Sweep {
            var: "a".to_string(),
            values: vec![4.0, 4.0],
        }],
        columns: vec![ColumnSpec {
            name: "x_col".to_string(),
            expression: "x".to_string(),
        }],
    };

    let result = run_param_table(&program, &table, &provider, SolveOptions::default());

    let first_iters = result.rows[0]
        .iterations
        .expect("first row should have iteration count");
    let second_iters = result.rows[1]
        .iterations
        .expect("second row should have iteration count");

    assert!(first_iters > 0);
    assert_eq!(second_iters, 0);
}

#[test]
fn failed_rows_are_recorded_without_aborting() {
    let provider = MockPropsProvider::new();
    let program = parse("x * a = 1\n");
    let table = ParamTableSpec {
        sweeps: vec![Sweep {
            var: "a".to_string(),
            values: vec![2.0, 0.0, 4.0],
        }],
        columns: vec![ColumnSpec {
            name: "x_col".to_string(),
            expression: "x".to_string(),
        }],
    };

    let result = run_param_table(&program, &table, &provider, SolveOptions::default());

    assert_eq!(result.rows.len(), 3);
    assert_eq!(
        result.rows[0].convergence_status,
        ConvergenceStatus::Converged
    );
    assert_eq!(
        result.rows[1].convergence_status,
        ConvergenceStatus::SingularJacobian
    );
    assert!(result.rows[1].error.is_some());
    assert_eq!(
        result.rows[2].convergence_status,
        ConvergenceStatus::Converged
    );
    assert!((result.rows[2].outputs["x_col"] - 0.25).abs() < 1e-9);
}

#[test]
fn serialization_round_trip_for_spec_and_result() {
    let provider = MockPropsProvider::new();
    let program = parse("x = a\n");
    let spec = ParamTableSpec {
        sweeps: vec![Sweep {
            var: "a".to_string(),
            values: vec![1.0, 2.0],
        }],
        columns: vec![ColumnSpec {
            name: "x_col".to_string(),
            expression: "x".to_string(),
        }],
    };

    let json_spec = serde_json::to_string(&spec).expect("spec to json");
    let round_trip_spec: ParamTableSpec = serde_json::from_str(&json_spec).expect("spec from json");
    assert_eq!(spec, round_trip_spec);

    let result = run_param_table(&program, &spec, &provider, SolveOptions::default());
    let json_result = serde_json::to_string(&result).expect("result to json");
    let round_trip_result: ParamTableResult =
        serde_json::from_str(&json_result).expect("result from json");

    assert_eq!(result.rows.len(), round_trip_result.rows.len());
    for (left, right) in result.rows.iter().zip(&round_trip_result.rows) {
        assert_eq!(left.inputs, right.inputs);
        assert_eq!(left.convergence_status, right.convergence_status);
        assert_eq!(left.iterations, right.iterations);
        assert_eq!(left.error, right.error);
        for (name, left_value) in &left.outputs {
            let right_value = right.outputs.get(name).expect("column should exist");
            assert!((left_value - right_value).abs() < 1e-12);
        }
    }
}
