use serde_json as json;

use std::{borrow::Cow, path::PathBuf};

pub fn str_to_json(s: &str) -> json::Value {
    json::from_str(s).unwrap_or_else(|_| json::Value::String(s.into()))
}

pub fn json_value_to_string(v: &json::Value) -> Cow<'_, String> {
    match v {
        json::Value::String(s) => Cow::Borrowed(s),
        _ => Cow::Owned(v.to_string()),
    }
}

pub fn json_value_into_string(v: json::Value) -> String {
    match v {
        json::Value::String(s) => s,
        _ => v.to_string(),
    }
}

pub fn tweak_path(rest: &mut String, base: &PathBuf) {
    *rest = base.with_file_name(&rest).to_string_lossy().into();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_value_to_string_works() {
        let expect = r#"{"foo":123}"#;
        let json = json::json!({"foo": 123});
        assert_eq!(json_value_to_string(&json).as_str(), expect);

        let expect = r#"asdf " foo"#;
        let json = expect.to_string().into();
        assert_eq!(json_value_to_string(&json).as_str(), expect);

        let expect = r#"["foo",1,2,3,null]"#;
        let json = json::json!(["foo", 1, 2, 3, null]);
        assert_eq!(json_value_to_string(&json).as_str(), expect);
    }
}
