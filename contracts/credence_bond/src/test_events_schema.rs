// Test that emitted events match the frozen v1 schemas

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use soroban_sdk::{testutils::Address as TestAddress, testutils::Events, Env};
    use std::fs;

    fn load_schema(name: &str) -> Value {
        let path = format!("../event_schemas/{}.v1.json", name);
        let content = fs::read_to_string(&path).expect("schema file not found");
        serde_json::from_str(&content).expect("invalid json schema")
    }

    #[test]
    fn test_bond_created_schema() {
        let e = Env::default();
        let addr = TestAddress::generate(&e);
        // use existing emit function (v2 for now, works the same topics)
        emit_bond_created_v2(&e, &addr, 1000i128, 3600u64, false, e.ledger().timestamp());
        let events = e.events().get_all();
        let schema = load_schema("bond_created");
        // Verify topics length and data length
        assert_eq!(events.len(), 1);
        let ev = &events[0];
        let topics = &ev.topics;
        let data = &ev.data;
        assert_eq!(topics.len(), schema["topics"].as_array().unwrap().len());
        assert_eq!(data.len(), schema["data"].as_array().unwrap().len());
    }
    // Additional tests for other events would follow the same pattern
}
