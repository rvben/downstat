//! `downstat schema` must validate against the published clispec v0.2 JSON
//! Schema (vendored at schemas/clispec-v0.2.json).

#[test]
fn schema_conforms_to_clispec_v0_2() {
    let schema: serde_json::Value =
        serde_json::from_str(include_str!("../schemas/clispec-v0.2.json"))
            .expect("vendored clispec schema is valid JSON");

    let instance = downstat::schema::contract();
    let validator = jsonschema::validator_for(&schema).expect("compile clispec schema");

    if !validator.is_valid(&instance) {
        let errors: Vec<String> = validator
            .iter_errors(&instance)
            .map(|e| format!("{} at {}", e, e.instance_path()))
            .collect();
        panic!(
            "downstat schema does not conform to clispec v0.2:\n{}",
            errors.join("\n")
        );
    }
}

#[test]
fn schema_declares_expected_shape() {
    let v = downstat::schema::contract();
    assert_eq!(v["clispec"], "0.2");
    assert_eq!(v["name"], "downstat");
    // downstat is entirely read-only.
    for c in v["commands"].as_array().unwrap() {
        assert_eq!(c["mutating"], false, "{} must be read-only", c["name"]);
    }
    assert!(v["global_args"].as_array().is_some_and(|g| !g.is_empty()));
    assert!(v["errors"].as_array().is_some_and(|e| !e.is_empty()));
}
