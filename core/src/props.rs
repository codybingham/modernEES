//! Property facade and helper functions.
//!
//! All state values are currently assumed to be base SI units.

use std::cell::RefCell;
use std::error::Error;
use std::fmt::{Display, Formatter};

/// Output property requested from a provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Prop {
    T,
    P,
    H,
    S,
    D,
}

/// Input state variable identifier for property queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum StateVar {
    T,
    P,
    H,
    S,
    D,
}

/// A query for a thermophysical property.
///
/// By convention, `(in1, in2)` represents an unordered pair for equivalence;
/// [`PropsQuery::equivalent_inputs`] treats `(T, P)` and `(P, T)` as the same state.
#[derive(Debug, Clone, PartialEq)]
pub struct PropsQuery {
    pub fluid: String,
    pub out: Prop,
    pub in1: (StateVar, f64),
    pub in2: (StateVar, f64),
}

impl PropsQuery {
    #[must_use]
    pub fn new(
        fluid: impl Into<String>,
        out: Prop,
        in1: (StateVar, f64),
        in2: (StateVar, f64),
    ) -> Self {
        Self {
            fluid: fluid.into(),
            out,
            in1,
            in2,
        }
    }

    #[must_use]
    pub fn equivalent_inputs(&self, other: &Self) -> bool {
        self.fluid == other.fluid
            && self.out == other.out
            && ((self.in1 == other.in1 && self.in2 == other.in2)
                || (self.in1 == other.in2 && self.in2 == other.in1))
    }
}

/// User-facing error type for property requests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PropsError {
    InvalidInput(String),
    NotAvailable(String),
    Provider(String),
}

impl Display for PropsError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidInput(message) => write!(f, "Invalid property input: {message}"),
            Self::NotAvailable(message) => {
                write!(f, "Requested property is not available: {message}")
            }
            Self::Provider(message) => write!(f, "Property provider error: {message}"),
        }
    }
}

impl Error for PropsError {}

/// Trait for a property backend.
pub trait PropsProvider {
    fn query(&self, q: &PropsQuery) -> Result<f64, PropsError>;
}

pub fn h(provider: &dyn PropsProvider, fluid: &str, t: f64, p: f64) -> Result<f64, PropsError> {
    let q = PropsQuery::new(fluid, Prop::H, (StateVar::T, t), (StateVar::P, p));
    provider.query(&q)
}

pub fn s(provider: &dyn PropsProvider, fluid: &str, t: f64, p: f64) -> Result<f64, PropsError> {
    let q = PropsQuery::new(fluid, Prop::S, (StateVar::T, t), (StateVar::P, p));
    provider.query(&q)
}

pub fn rho(provider: &dyn PropsProvider, fluid: &str, t: f64, p: f64) -> Result<f64, PropsError> {
    let q = PropsQuery::new(fluid, Prop::D, (StateVar::T, t), (StateVar::P, p));
    provider.query(&q)
}

pub fn t_from_ph(
    provider: &dyn PropsProvider,
    fluid: &str,
    p: f64,
    h_value: f64,
) -> Result<f64, PropsError> {
    let q = PropsQuery::new(fluid, Prop::T, (StateVar::P, p), (StateVar::H, h_value));
    provider.query(&q)
}

pub fn p_from_th(
    provider: &dyn PropsProvider,
    fluid: &str,
    t: f64,
    h_value: f64,
) -> Result<f64, PropsError> {
    let q = PropsQuery::new(fluid, Prop::P, (StateVar::T, t), (StateVar::H, h_value));
    provider.query(&q)
}

/// In-memory provider useful for unit and integration tests.
#[derive(Debug, Default)]
pub struct MockPropsProvider {
    expectations: RefCell<Vec<(PropsQuery, f64)>>,
    calls: RefCell<Vec<PropsQuery>>,
    unordered_inputs: bool,
    use_fallback_formula: bool,
}

impl MockPropsProvider {
    #[must_use]
    pub fn new() -> Self {
        Self {
            expectations: RefCell::new(Vec::new()),
            calls: RefCell::new(Vec::new()),
            unordered_inputs: true,
            use_fallback_formula: false,
        }
    }

    #[must_use]
    pub fn with_fallback_formula(mut self, enabled: bool) -> Self {
        self.use_fallback_formula = enabled;
        self
    }

    #[must_use]
    pub fn with_unordered_inputs(mut self, unordered_inputs: bool) -> Self {
        self.unordered_inputs = unordered_inputs;
        self
    }

    pub fn insert(&self, q: PropsQuery, value: f64) {
        self.expectations.borrow_mut().push((q, value));
    }

    #[must_use]
    pub fn calls(&self) -> Vec<PropsQuery> {
        self.calls.borrow().clone()
    }

    fn matches_query(&self, left: &PropsQuery, right: &PropsQuery) -> bool {
        if self.unordered_inputs {
            left.equivalent_inputs(right)
        } else {
            left == right
        }
    }

    fn fallback_formula(q: &PropsQuery) -> f64 {
        let out_weight = match q.out {
            Prop::T => 10.0,
            Prop::P => 20.0,
            Prop::H => 30.0,
            Prop::S => 40.0,
            Prop::D => 50.0,
        };
        out_weight + q.in1.1 * 0.1 + q.in2.1 * 0.01 + q.fluid.len() as f64
    }
}

impl PropsProvider for MockPropsProvider {
    fn query(&self, q: &PropsQuery) -> Result<f64, PropsError> {
        self.calls.borrow_mut().push(q.clone());

        if let Some((_, value)) = self
            .expectations
            .borrow()
            .iter()
            .find(|(expected, _)| self.matches_query(expected, q))
        {
            return Ok(*value);
        }

        if self.use_fallback_formula {
            return Ok(Self::fallback_formula(q));
        }

        Err(PropsError::NotAvailable(format!(
            "No mock response registered for query: {q:?}"
        )))
    }
}
