use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use modern_ees_core::props::{Prop, PropsError, PropsProvider, PropsQuery, StateVar};
use serde::{Deserialize, Serialize};

/// Default cache size for memoized property requests.
///
/// The cache stores the most recent 256 canonical queries and evicts least-recently-used entries.
const DEFAULT_CACHE_SIZE: usize = 256;

#[derive(Debug)]
pub struct CoolPropProvider {
    transport: RefCell<Box<dyn Transport>>,
    cache: RefCell<QueryCache>,
}

impl CoolPropProvider {
    pub fn new() -> Result<Self, PropsError> {
        Self::with_cache_capacity(DEFAULT_CACHE_SIZE)
    }

    pub fn with_cache_capacity(cache_capacity: usize) -> Result<Self, PropsError> {
        let transport = PythonTransport::spawn_default()?;
        Ok(Self::from_transport(transport, cache_capacity))
    }

    fn from_transport<T: Transport + 'static>(transport: T, cache_capacity: usize) -> Self {
        Self {
            transport: RefCell::new(Box::new(transport)),
            cache: RefCell::new(QueryCache::new(cache_capacity)),
        }
    }
}

impl PropsProvider for CoolPropProvider {
    fn query(&self, q: &PropsQuery) -> Result<f64, PropsError> {
        let key = CacheKey::from_query(q);
        if let Some(cached) = self.cache.borrow_mut().get(&key) {
            return Ok(cached);
        }

        let req = HelperRequest::from_query(q);
        let response = self.transport.borrow_mut().send_request(&req)?;
        let value = response.into_result()?;
        self.cache.borrow_mut().insert(key, value);
        Ok(value)
    }
}

trait Transport: std::fmt::Debug {
    fn send_request(&mut self, req: &HelperRequest) -> Result<HelperResponse, PropsError>;
}

#[derive(Debug)]
struct PythonTransport {
    child: Child,
    stdin: BufWriter<ChildStdin>,
    stdout: BufReader<ChildStdout>,
}

impl PythonTransport {
    fn spawn_default() -> Result<Self, PropsError> {
        let script_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("python")
            .join("coolprop_helper.py");

        let mut child = Command::new("python3")
            .arg("-u")
            .arg(script_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|err| PropsError::Provider(format!("failed to start python helper: {err}")))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| PropsError::Provider("python helper stdin unavailable".to_string()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| PropsError::Provider("python helper stdout unavailable".to_string()))?;

        Ok(Self {
            child,
            stdin: BufWriter::new(stdin),
            stdout: BufReader::new(stdout),
        })
    }
}

impl Transport for PythonTransport {
    fn send_request(&mut self, req: &HelperRequest) -> Result<HelperResponse, PropsError> {
        let mut serialized = serde_json::to_string(req)
            .map_err(|err| PropsError::Provider(format!("failed to serialize request: {err}")))?;
        serialized.push('\n');

        self.stdin
            .write_all(serialized.as_bytes())
            .and_then(|_| self.stdin.flush())
            .map_err(|err| {
                PropsError::Provider(format!("failed to write helper request: {err}"))
            })?;

        let mut line = String::new();
        let read = self.stdout.read_line(&mut line).map_err(|err| {
            PropsError::Provider(format!("failed to read helper response: {err}"))
        })?;
        if read == 0 {
            return Err(PropsError::Provider(
                "python helper closed stdout unexpectedly".to_string(),
            ));
        }

        serde_json::from_str(line.trim_end()).map_err(|err| {
            PropsError::Provider(format!("failed to decode helper response JSON: {err}"))
        })
    }
}

impl Drop for PythonTransport {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HelperRequest {
    fluid: String,
    out: String,
    in1: HelperInput,
    in2: HelperInput,
}

impl HelperRequest {
    fn from_query(q: &PropsQuery) -> Self {
        Self {
            fluid: q.fluid.clone(),
            out: prop_to_symbol(q.out).to_string(),
            in1: HelperInput::from_tuple(q.in1),
            in2: HelperInput::from_tuple(q.in2),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HelperInput {
    var: String,
    value: f64,
}

impl HelperInput {
    fn from_tuple(input: (StateVar, f64)) -> Self {
        Self {
            var: state_to_symbol(input.0).to_string(),
            value: input.1,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "ok")]
enum HelperResponse {
    #[serde(rename = "true")]
    Ok { value: f64 },
    #[serde(rename = "false")]
    Err { kind: String, message: String },
}

impl HelperResponse {
    fn into_result(self) -> Result<f64, PropsError> {
        match self {
            Self::Ok { value } => Ok(value),
            Self::Err { kind, message } => match kind.as_str() {
                "unknown_fluid" | "invalid_pair" => Err(PropsError::InvalidInput(message)),
                "out_of_range" => Err(PropsError::NotAvailable(message)),
                _ => Err(PropsError::Provider(message)),
            },
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
struct CacheKey {
    fluid: String,
    out: Prop,
    in_low: KeyInput,
    in_high: KeyInput,
}

impl CacheKey {
    fn from_query(q: &PropsQuery) -> Self {
        let left = KeyInput::from(q.in1);
        let right = KeyInput::from(q.in2);
        let (in_low, in_high) = if left <= right {
            (left, right)
        } else {
            (right, left)
        };

        Self {
            fluid: q.fluid.clone(),
            out: q.out,
            in_low,
            in_high,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct KeyInput {
    var: StateVar,
    value_bits: u64,
}

impl From<(StateVar, f64)> for KeyInput {
    fn from(value: (StateVar, f64)) -> Self {
        Self {
            var: value.0,
            value_bits: value.1.to_bits(),
        }
    }
}

#[derive(Debug)]
struct QueryCache {
    values: HashMap<CacheKey, f64>,
    order: VecDeque<CacheKey>,
    capacity: usize,
}

impl QueryCache {
    fn new(capacity: usize) -> Self {
        Self {
            values: HashMap::new(),
            order: VecDeque::new(),
            capacity,
        }
    }

    fn get(&mut self, key: &CacheKey) -> Option<f64> {
        let value = *self.values.get(key)?;
        if let Some(pos) = self.order.iter().position(|existing| existing == key) {
            if let Some(moved) = self.order.remove(pos) {
                self.order.push_back(moved);
            }
        }
        Some(value)
    }

    fn insert(&mut self, key: CacheKey, value: f64) {
        if self.capacity == 0 {
            return;
        }

        if self.values.insert(key.clone(), value).is_some() {
            if let Some(pos) = self.order.iter().position(|existing| existing == &key) {
                self.order.remove(pos);
            }
            self.order.push_back(key);
            return;
        }

        self.order.push_back(key);
        if self.order.len() > self.capacity {
            if let Some(evicted) = self.order.pop_front() {
                self.values.remove(&evicted);
            }
        }
    }
}

fn prop_to_symbol(p: Prop) -> &'static str {
    match p {
        Prop::T => "T",
        Prop::P => "P",
        Prop::H => "H",
        Prop::S => "S",
        Prop::D => "D",
    }
}

fn state_to_symbol(v: StateVar) -> &'static str {
    match v {
        StateVar::T => "T",
        StateVar::P => "P",
        StateVar::H => "H",
        StateVar::S => "S",
        StateVar::D => "D",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::rc::Rc;

    #[derive(Debug)]
    struct FakeTransport {
        calls: Rc<RefCell<usize>>,
        responses: VecDeque<Result<HelperResponse, PropsError>>,
    }

    impl FakeTransport {
        fn with_responses(
            calls: Rc<RefCell<usize>>,
            responses: Vec<Result<HelperResponse, PropsError>>,
        ) -> Self {
            Self {
                calls,
                responses: responses.into(),
            }
        }
    }

    impl Transport for FakeTransport {
        fn send_request(&mut self, _req: &HelperRequest) -> Result<HelperResponse, PropsError> {
            *self.calls.borrow_mut() += 1;
            self.responses.pop_front().unwrap_or_else(|| {
                Err(PropsError::Provider(
                    "fake transport missing response".to_string(),
                ))
            })
        }
    }

    #[test]
    fn cache_reuses_value_for_swapped_inputs() {
        let calls = Rc::new(RefCell::new(0));
        let transport = FakeTransport::with_responses(
            calls.clone(),
            vec![Ok(HelperResponse::Ok { value: 42.0 })],
        );
        let provider = CoolPropProvider::from_transport(transport, 4);

        let q1 = PropsQuery::new(
            "Water",
            Prop::H,
            (StateVar::T, 300.0),
            (StateVar::P, 101_325.0),
        );
        let q2 = PropsQuery::new(
            "Water",
            Prop::H,
            (StateVar::P, 101_325.0),
            (StateVar::T, 300.0),
        );

        assert_eq!(provider.query(&q1).expect("first call succeeds"), 42.0);
        assert_eq!(provider.query(&q2).expect("second call hits cache"), 42.0);
        assert_eq!(*calls.borrow(), 1);
    }

    #[test]
    fn cache_evicts_lru_entry_when_capacity_reached() {
        let calls = Rc::new(RefCell::new(0));
        let transport = FakeTransport::with_responses(
            calls.clone(),
            vec![
                Ok(HelperResponse::Ok { value: 1.0 }),
                Ok(HelperResponse::Ok { value: 2.0 }),
                Ok(HelperResponse::Ok { value: 1.5 }),
            ],
        );
        let provider = CoolPropProvider::from_transport(transport, 1);

        let q1 = PropsQuery::new(
            "Water",
            Prop::H,
            (StateVar::T, 300.0),
            (StateVar::P, 100_000.0),
        );
        let q2 = PropsQuery::new(
            "Water",
            Prop::S,
            (StateVar::T, 301.0),
            (StateVar::P, 100_000.0),
        );

        assert_eq!(provider.query(&q1).expect("first query"), 1.0);
        assert_eq!(provider.query(&q2).expect("second query"), 2.0);
        assert_eq!(provider.query(&q1).expect("evicted and recomputed"), 1.5);
        assert_eq!(*calls.borrow(), 3);
    }

    #[test]
    fn backend_error_mapping_is_readable() {
        let calls = Rc::new(RefCell::new(0));
        let transport = FakeTransport::with_responses(
            calls,
            vec![Ok(HelperResponse::Err {
                kind: "unknown_fluid".to_string(),
                message: "No fluid named Nope".to_string(),
            })],
        );
        let provider = CoolPropProvider::from_transport(transport, 2);
        let q = PropsQuery::new(
            "Nope",
            Prop::H,
            (StateVar::T, 300.0),
            (StateVar::P, 101_325.0),
        );

        let err = provider.query(&q).expect_err("must map to PropsError");
        assert!(matches!(err, PropsError::InvalidInput(_)));
        assert!(err.to_string().contains("No fluid named Nope"));
    }
}
