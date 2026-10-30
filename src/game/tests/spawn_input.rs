//! Regression for the four-number NaN tuple being reinterpreted as three numbers.
//! Pure parsing of six short strings; no environment mutation or asset cost.
#[test]
fn invalid_spawn_component_never_changes_tuple_arity() {
    for value in ["1,NaN,2,90", "1,2,inf,90", "NaN,2,90", "1,2,bad,90"] {
        assert!(
            super::parse_spawn_override(value).is_none(),
            "reject the whole override: {value}"
        );
    }
    assert!(
        super::parse_spawn_override("1,2,90").is_some(),
        "existing floor-relative form stays available"
    );
    assert!(
        super::parse_spawn_override("1,1.6,2,90").is_some(),
        "existing explicit-eye form stays available"
    );
}
