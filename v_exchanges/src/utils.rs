use serde_json::Value;

/// # Panics
/// Fine, because given prospected usages, theoretically only developer will see it.
pub fn join_params(a: Value, b: Value) -> Value {
	if let (Value::Object(mut a_map), Value::Object(b_map)) = (a, b) {
		a_map.extend(b_map);
		Value::Object(a_map)
	} else {
		panic!("Both inputs must be JSON objects");
	}
}
