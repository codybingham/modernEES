use std::collections::{BTreeSet, HashMap};
use std::fs;

use modern_ees_core::param_table::{
    run_param_table as core_run_param_table, ColumnSpec, ParamTableSpec, Sweep,
};
use modern_ees_core::parser::ast::{Expr, ExprKind, Program, StatementKind};
use modern_ees_core::props::MockPropsProvider;
use modern_ees_core::{
    analyze_units, parse_program, solve_program_with_options as core_solve_program_with_options,
    SolveOptions,
};
use rfd::{FileDialog, MessageButtons, MessageDialog, MessageLevel};
use serde::{Deserialize, Serialize};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Emitter};

#[derive(Debug, Deserialize)]
struct SolveOptionsInput {
    max_iters: Option<usize>,
    residual_tol: Option<f64>,
    step_tol: Option<f64>,
    fd_epsilon: Option<f64>,
    min_step_factor: Option<f64>,
    initial_guess: Option<HashMap<String, f64>>,
}

impl From<SolveOptionsInput> for SolveOptions {
    fn from(input: SolveOptionsInput) -> Self {
        let defaults = SolveOptions::default();
        Self {
            max_iters: input.max_iters.unwrap_or(defaults.max_iters),
            residual_tol: input.residual_tol.unwrap_or(defaults.residual_tol),
            step_tol: input.step_tol.unwrap_or(defaults.step_tol),
            fd_epsilon: input.fd_epsilon.unwrap_or(defaults.fd_epsilon),
            min_step_factor: input.min_step_factor.unwrap_or(defaults.min_step_factor),
            initial_guess: input.initial_guess.unwrap_or_default(),
        }
    }
}

#[derive(Debug, Serialize)]
struct UiSpan {
    line: usize,
    column: usize,
    end_line: usize,
    end_column: usize,
}

#[derive(Debug, Serialize)]
struct UiDiagnostic {
    source: String,
    message: String,
    span: UiSpan,
}

#[derive(Debug, Serialize)]
struct AnalyzeResponse {
    diagnostics: Vec<UiDiagnostic>,
    identifiers: Vec<String>,
}

#[derive(Debug, Serialize)]
struct SolveResponse {
    status: String,
    diagnostics: Vec<UiDiagnostic>,
    variables: HashMap<String, f64>,
    iterations: Option<usize>,
    final_norm: Option<f64>,
    message: Option<String>,
    worst_residual: Option<UiResidual>,
}

#[derive(Debug, Serialize)]
struct UiResidual {
    residual: f64,
    span: UiSpan,
}

#[derive(Debug, Deserialize)]
struct TableSpecInput {
    sweeps: Vec<SweepInput>,
    columns: Vec<ColumnInput>,
}

#[derive(Debug, Deserialize)]
struct SweepInput {
    var: String,
    values: Vec<f64>,
}

#[derive(Debug, Deserialize)]
struct ColumnInput {
    name: String,
    expression: String,
}

#[derive(Debug, Serialize)]
struct TableResponse {
    status: String,
    diagnostics: Vec<UiDiagnostic>,
    rows: Vec<HashMap<String, f64>>,
    message: Option<String>,
}

#[derive(Debug, Serialize)]
struct FileLoadResponse {
    path: String,
    contents: String,
}

#[derive(Debug, Serialize)]
struct FileSaveResponse {
    path: String,
}

#[tauri::command]
fn parse_and_analyze(equations_text: String) -> AnalyzeResponse {
    match parse_program(&equations_text) {
        Ok(program) => {
            let unit_diags = analyze_units(&program)
                .into_iter()
                .map(|diag| convert_diag("units", diag.message, diag.span))
                .collect();

            AnalyzeResponse {
                diagnostics: unit_diags,
                identifiers: discover_identifiers(&program),
            }
        }
        Err(parse_diags) => AnalyzeResponse {
            diagnostics: parse_diags
                .into_iter()
                .map(|diag| convert_diag("parse", diag.message, diag.span))
                .collect(),
            identifiers: Vec::new(),
        },
    }
}

#[tauri::command]
fn solve_program(equations_text: String, options: Option<SolveOptionsInput>) -> SolveResponse {
    let provider = MockPropsProvider::new().with_fallback_formula(true);

    let program = match parse_program(&equations_text) {
        Ok(program) => program,
        Err(parse_diags) => {
            return SolveResponse {
                status: "parse_error".to_string(),
                diagnostics: parse_diags
                    .into_iter()
                    .map(|diag| convert_diag("parse", diag.message, diag.span))
                    .collect(),
                variables: HashMap::new(),
                iterations: None,
                final_norm: None,
                message: Some("Cannot solve due to parse errors".to_string()),
                worst_residual: None,
            }
        }
    };

    let unit_diagnostics: Vec<UiDiagnostic> = analyze_units(&program)
        .into_iter()
        .map(|diag| convert_diag("units", diag.message, diag.span))
        .collect();

    if !unit_diagnostics.is_empty() {
        return SolveResponse {
            status: "unit_error".to_string(),
            diagnostics: unit_diagnostics,
            variables: HashMap::new(),
            iterations: None,
            final_norm: None,
            message: Some("Cannot solve due to unit diagnostics".to_string()),
            worst_residual: None,
        };
    }

    let solve_options = options.map_or_else(SolveOptions::default, Into::into);
    match core_solve_program_with_options(&program, &provider, &solve_options) {
        Ok(result) => SolveResponse {
            status: "ok".to_string(),
            diagnostics: Vec::new(),
            variables: result.solution,
            iterations: Some(result.report.iterations),
            final_norm: Some(result.report.final_norm),
            message: None,
            worst_residual: None,
        },
        Err(err) => SolveResponse {
            status: format!("solver_{:?}", err.report.status).to_lowercase(),
            diagnostics: Vec::new(),
            variables: HashMap::new(),
            iterations: Some(err.report.iterations),
            final_norm: Some(err.report.final_norm),
            message: Some(err.message),
            worst_residual: err
                .report
                .worst_residuals
                .first()
                .map(|residual| UiResidual {
                    residual: residual.residual,
                    span: UiSpan {
                        line: residual.span.start.line,
                        column: residual.span.start.column,
                        end_line: residual.span.end.line,
                        end_column: residual.span.end.column,
                    },
                }),
        },
    }
}

#[tauri::command]
fn run_param_table(
    equations_text: String,
    table_spec: TableSpecInput,
    options: Option<SolveOptionsInput>,
) -> TableResponse {
    let provider = MockPropsProvider::new().with_fallback_formula(true);

    let program = match parse_program(&equations_text) {
        Ok(program) => program,
        Err(parse_diags) => {
            return TableResponse {
                status: "parse_error".to_string(),
                diagnostics: parse_diags
                    .into_iter()
                    .map(|diag| convert_diag("parse", diag.message, diag.span))
                    .collect(),
                rows: Vec::new(),
                message: Some("Cannot run table due to parse errors".to_string()),
            }
        }
    };

    let unit_diagnostics: Vec<UiDiagnostic> = analyze_units(&program)
        .into_iter()
        .map(|diag| convert_diag("units", diag.message, diag.span))
        .collect();

    if !unit_diagnostics.is_empty() {
        return TableResponse {
            status: "unit_error".to_string(),
            diagnostics: unit_diagnostics,
            rows: Vec::new(),
            message: Some("Cannot run table due to unit diagnostics".to_string()),
        };
    }

    let spec = ParamTableSpec {
        sweeps: table_spec
            .sweeps
            .into_iter()
            .map(|sweep| Sweep {
                var: sweep.var,
                values: sweep.values,
            })
            .collect(),
        columns: table_spec
            .columns
            .into_iter()
            .map(|column| ColumnSpec {
                name: column.name,
                expression: column.expression,
            })
            .collect(),
    };

    let result = core_run_param_table(
        &program,
        &spec,
        &provider,
        options.map_or_else(SolveOptions::default, Into::into),
    );

    let rows = result
        .rows
        .into_iter()
        .map(|row| {
            let mut merged = row.inputs;
            for (key, value) in row.outputs {
                merged.insert(key, value);
            }
            if let Some(iterations) = row.iterations {
                merged.insert("_iterations".to_string(), iterations as f64);
            }
            if let Some(final_norm) = row.final_norm {
                merged.insert("_final_norm".to_string(), final_norm);
            }
            merged
        })
        .collect();

    TableResponse {
        status: "ok".to_string(),
        diagnostics: Vec::new(),
        rows,
        message: None,
    }
}

#[tauri::command]
fn open_equations_file() -> Result<FileLoadResponse, String> {
    let Some(path) = FileDialog::new()
        .add_filter("Text", &["txt"])
        .set_title("Open Equations File")
        .pick_file()
    else {
        return Err("Open canceled".to_string());
    };

    let contents = fs::read_to_string(&path).map_err(|err| err.to_string())?;
    Ok(FileLoadResponse {
        path: path.display().to_string(),
        contents,
    })
}

#[tauri::command]
fn save_equations_file(
    equations_text: String,
    current_path: Option<String>,
    save_as: bool,
) -> Result<FileSaveResponse, String> {
    let target_path = if !save_as {
        current_path
            .as_deref()
            .and_then(|path| (!path.trim().is_empty()).then_some(path.to_string()))
    } else {
        None
    }
    .map(std::path::PathBuf::from)
    .or_else(|| {
        FileDialog::new()
            .add_filter("Text", &["txt"])
            .set_title("Save Equations File")
            .set_file_name("equations.txt")
            .save_file()
    });

    let Some(path) = target_path else {
        return Err("Save canceled".to_string());
    };

    fs::write(&path, equations_text).map_err(|err| err.to_string())?;
    Ok(FileSaveResponse {
        path: path.display().to_string(),
    })
}

fn emit_menu_event(app: &AppHandle, event: &str) {
    let _ = app.emit(event, ());
}

fn handle_menu_event(app: &AppHandle, id: &str) {
    match id {
        "file_new" => emit_menu_event(app, "menu://file-new"),
        "file_open" => emit_menu_event(app, "menu://file-open"),
        "file_save" => emit_menu_event(app, "menu://file-save"),
        "file_save_as" => emit_menu_event(app, "menu://file-save-as"),
        "file_quit" => app.exit(0),
        "calc_solve" => emit_menu_event(app, "menu://calculate-solve"),
        "calc_analyze" => emit_menu_event(app, "menu://calculate-analyze"),
        "tables_run" => emit_menu_event(app, "menu://tables-run"),
        "help_about" => {
            MessageDialog::new()
                .set_level(MessageLevel::Info)
                .set_title("About modernEES")
                .set_description("modernEES\nA modern desktop shell for equation solving.")
                .set_buttons(MessageButtons::Ok)
                .show();
        }
        _ => {}
    }
}

fn build_menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let file_menu = Submenu::with_items(
        app,
        "File",
        true,
        &[
            &MenuItem::with_id(app, "file_new", "New", true, Some("Ctrl+N"))?,
            &MenuItem::with_id(app, "file_open", "Open…", true, Some("Ctrl+O"))?,
            &MenuItem::with_id(app, "file_save", "Save", true, Some("Ctrl+S"))?,
            &MenuItem::with_id(app, "file_save_as", "Save As…", true, Some("Ctrl+Shift+S"))?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "file_quit", "Quit", true, Some("Ctrl+Q"))?,
        ],
    )?;

    let edit_menu = Submenu::with_items(
        app,
        "Edit",
        true,
        &[
            &PredefinedMenuItem::undo(app, None)?,
            &PredefinedMenuItem::redo(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::cut(app, None)?,
            &PredefinedMenuItem::copy(app, None)?,
            &PredefinedMenuItem::paste(app, None)?,
            &PredefinedMenuItem::select_all(app, None)?,
        ],
    )?;

    let calculate_menu = Submenu::with_items(
        app,
        "Calculate",
        true,
        &[
            &MenuItem::with_id(app, "calc_solve", "Solve", true, Some("Ctrl+Enter"))?,
            &MenuItem::with_id(app, "calc_analyze", "Analyze", true, Some("F5"))?,
        ],
    )?;

    let tables_menu = Submenu::with_items(
        app,
        "Tables",
        true,
        &[&MenuItem::with_id(
            app,
            "tables_run",
            "Run Param Table",
            true,
            Some("Ctrl+R"),
        )?],
    )?;

    let plots_menu = Submenu::with_items(
        app,
        "Plots",
        true,
        &[&MenuItem::with_id(
            app,
            "plots_placeholder",
            "Plot Window",
            false,
            None::<&str>,
        )?],
    )?;
    let options_menu = Submenu::with_items(
        app,
        "Options",
        true,
        &[&MenuItem::with_id(
            app,
            "options_placeholder",
            "Preferences",
            false,
            None::<&str>,
        )?],
    )?;
    let search_menu = Submenu::with_items(
        app,
        "Search",
        true,
        &[&MenuItem::with_id(
            app,
            "search_placeholder",
            "Find",
            false,
            Some("Ctrl+F"),
        )?],
    )?;
    let window_menu = Submenu::with_items(
        app,
        "Window",
        true,
        &[&MenuItem::with_id(
            app,
            "window_placeholder",
            "Cascade",
            false,
            None::<&str>,
        )?],
    )?;
    let examples_menu = Submenu::with_items(
        app,
        "Examples",
        true,
        &[&MenuItem::with_id(
            app,
            "examples_placeholder",
            "Open Example",
            false,
            None::<&str>,
        )?],
    )?;
    let help_menu = Submenu::with_items(
        app,
        "Help",
        true,
        &[&MenuItem::with_id(
            app,
            "help_about",
            "About",
            true,
            None::<&str>,
        )?],
    )?;

    Menu::with_items(
        app,
        &[
            &file_menu,
            &edit_menu,
            &search_menu,
            &options_menu,
            &calculate_menu,
            &tables_menu,
            &plots_menu,
            &window_menu,
            &help_menu,
            &examples_menu,
        ],
    )
}

fn discover_identifiers(program: &Program) -> Vec<String> {
    let mut names = BTreeSet::new();
    for statement in &program.statements {
        let StatementKind::Assignment { lhs, rhs } = &statement.kind;
        collect_identifiers(lhs, &mut names);
        collect_identifiers(rhs, &mut names);
    }
    names.into_iter().collect()
}

fn collect_identifiers(expr: &Expr, names: &mut BTreeSet<String>) {
    match &expr.kind {
        ExprKind::Identifier(name) => {
            names.insert(name.clone());
        }
        ExprKind::Unary { expr, .. } | ExprKind::Group(expr) => {
            collect_identifiers(expr, names);
        }
        ExprKind::Binary { left, right, .. } => {
            collect_identifiers(left, names);
            collect_identifiers(right, names);
        }
        ExprKind::Call { args, .. } => {
            for arg in args {
                collect_identifiers(arg, names);
            }
        }
        ExprKind::Number(_) | ExprKind::QuantityLiteral { .. } | ExprKind::StringLiteral(_) => {}
    }
}

fn convert_diag(
    source: &str,
    message: String,
    span: modern_ees_core::diagnostic::Span,
) -> UiDiagnostic {
    UiDiagnostic {
        source: source.to_string(),
        message,
        span: UiSpan {
            line: span.start.line,
            column: span.start.column,
            end_line: span.end.line,
            end_column: span.end.column,
        },
    }
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let menu = build_menu(&app.handle())?;
            app.set_menu(menu)?;
            Ok(())
        })
        .on_menu_event(|app, event| {
            handle_menu_event(app, event.id().as_ref());
        })
        .invoke_handler(tauri::generate_handler![
            parse_and_analyze,
            solve_program,
            run_param_table,
            open_equations_file,
            save_equations_file
        ])
        .run(tauri::generate_context!())
        .expect("tauri app should run");
}
