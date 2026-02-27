use modern_ees_core::props::{Prop, PropsProvider, PropsQuery, StateVar};
use modern_ees_props_coolprop::CoolPropProvider;

#[test]
#[ignore = "requires python3 + CoolProp runtime"]
fn water_enthalpy_from_tp() {
    let provider = CoolPropProvider::new().expect("provider should initialize");
    let q = PropsQuery::new(
        "Water",
        Prop::H,
        (StateVar::T, 300.0),
        (StateVar::P, 101_325.0),
    );

    let value = provider.query(&q).expect("coolprop query should succeed");

    assert!(value.is_finite());
    assert!(value > 0.0);
}
