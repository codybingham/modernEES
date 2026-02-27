use modern_ees_core::props::{
    h, rho, s, t_from_ph, MockPropsProvider, Prop, PropsError, PropsProvider, PropsQuery, StateVar,
};

#[test]
fn helper_h_builds_expected_query_and_calls_provider() {
    let provider = MockPropsProvider::new();
    let expected = PropsQuery::new(
        "Water",
        Prop::H,
        (StateVar::T, 300.0),
        (StateVar::P, 101_325.0),
    );
    provider.insert(expected.clone(), 1234.5);

    let result = h(&provider, "Water", 300.0, 101_325.0).expect("h should succeed");

    assert_eq!(result, 1234.5);
    assert_eq!(provider.calls().as_slice(), &[expected]);
}

#[test]
fn helpers_for_s_rho_and_t_from_ph_build_queries() {
    let provider = MockPropsProvider::new();
    provider.insert(
        PropsQuery::new(
            "R134a",
            Prop::S,
            (StateVar::T, 280.0),
            (StateVar::P, 900_000.0),
        ),
        5.5,
    );
    provider.insert(
        PropsQuery::new(
            "R134a",
            Prop::D,
            (StateVar::T, 280.0),
            (StateVar::P, 900_000.0),
        ),
        12.25,
    );
    provider.insert(
        PropsQuery::new(
            "R134a",
            Prop::T,
            (StateVar::P, 900_000.0),
            (StateVar::H, 410_000.0),
        ),
        285.0,
    );

    assert_eq!(
        s(&provider, "R134a", 280.0, 900_000.0).expect("s should succeed"),
        5.5
    );
    assert_eq!(
        rho(&provider, "R134a", 280.0, 900_000.0).expect("rho should succeed"),
        12.25
    );
    assert_eq!(
        t_from_ph(&provider, "R134a", 900_000.0, 410_000.0).expect("T from p,h should succeed"),
        285.0
    );
}

#[test]
fn unordered_pair_matching_accepts_swapped_inputs() {
    let provider = MockPropsProvider::new();
    provider.insert(
        PropsQuery::new(
            "Water",
            Prop::H,
            (StateVar::P, 101_325.0),
            (StateVar::T, 300.0),
        ),
        2222.0,
    );

    let result = h(&provider, "Water", 300.0, 101_325.0).expect("unordered inputs should match");

    assert_eq!(result, 2222.0);
}

#[test]
fn helper_propagates_provider_error() {
    struct AlwaysError;

    impl PropsProvider for AlwaysError {
        fn query(&self, _q: &PropsQuery) -> Result<f64, PropsError> {
            Err(PropsError::Provider("backend offline".to_string()))
        }
    }

    let err = h(&AlwaysError, "Water", 300.0, 101_325.0).expect_err("error should propagate");

    assert_eq!(err, PropsError::Provider("backend offline".to_string()));
}
