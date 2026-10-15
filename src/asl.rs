use std::collections::HashSet;

use serde_json::Value;

use crate::{app::Service, links::static_link, model::ResourceLink};

pub fn lambda_links(definition: &str) -> Vec<ResourceLink> {
    let Ok(document) = serde_json::from_str::<Value>(definition) else {
        return vec![];
    };
    let mut seen = HashSet::new();
    let mut links = Vec::new();
    visit(&document, &mut seen, &mut links);
    links
}

fn visit(scope: &Value, seen: &mut HashSet<String>, links: &mut Vec<ResourceLink>) {
    let Some(states) = scope.get("States").and_then(Value::as_object) else {
        return;
    };
    for state in states.values() {
        if state.get("Type").and_then(Value::as_str) == Some("Task") {
            let target = match state.get("Resource").and_then(Value::as_str) {
                Some("arn:aws:states:::lambda:invoke") => state
                    .get("Parameters")
                    .and_then(|params| params.get("FunctionName"))
                    .and_then(Value::as_str)
                    .or_else(|| {
                        state
                            .get("Arguments")
                            .and_then(|args| args.get("FunctionName"))
                            .and_then(Value::as_str)
                    }),
                Some(value) if value.starts_with("arn:") && value.contains(":lambda:") => {
                    Some(value)
                }
                _ => None,
            };
            if let Some(arn) = target
                .filter(|arn| static_link(arn).is_some_and(|link| link.service == Service::Lambda))
                && seen.insert(arn.to_owned())
            {
                links.push(ResourceLink {
                    service: Service::Lambda,
                    kind: "function".into(),
                    id: arn.into(),
                });
            }
        }
        if let Some(branches) = state.get("Branches").and_then(Value::as_array) {
            for branch in branches {
                visit(branch, seen, links);
            }
        }
        for field in ["ItemProcessor", "Iterator"] {
            if let Some(nested) = state.get(field) {
                visit(nested, seen, links);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_literal_links_only() {
        let definition = r#"{"States":{"a":{"Type":"Task","Resource":"arn:aws:lambda:us-east-1:123:function:first"},"b":{"Type":"Parallel","Branches":[{"States":{"c":{"Type":"Task","Resource":"arn:aws:states:::lambda:invoke","Parameters":{"FunctionName":"arn:aws:lambda:us-east-1:123:function:second"}}}}]},"d":{"Type":"Map","ItemProcessor":{"States":{"e":{"Type":"Task","Resource":"arn:aws:lambda:us-east-1:123:function:first"},"f":{"Type":"Task","Resource":"arn:aws:states:::lambda:invoke","Arguments":{"FunctionName.$":"$.lambda"}}}}}}}"#;
        let links = lambda_links(definition);
        assert_eq!(links.len(), 2);
        assert!(links.iter().any(|link| link.id.ends_with(":first")));
        assert!(links.iter().any(|link| link.id.ends_with(":second")));
        for invalid in [
            "invalid",
            r#"{"States":{"x":{"Type":"Task","Resource":"arn:aws:lambda:us-east-1:123:function:${name}"}}}"#,
            r#"{"States":{"x":{"Type":"Task","Resource":"arn:aws:events:us-east-1:123:rule/x"}}}"#,
        ] {
            assert!(lambda_links(invalid).is_empty());
        }
        assert!(lambda_links(r#"{"States":{"x":{"Type":"Task","Resource":"arn:aws:lambda:eu-west-1:123:function:elsewhere"}}}"#).len() == 1);
    }

    #[test]
    fn iterator_arguments_and_dynamic_names() {
        let definition = r#"{"States":{"map":{"Type":"Map","Iterator":{"States":{"literal":{"Type":"Task","Resource":"arn:aws:states:::lambda:invoke","Arguments":{"FunctionName":"arn:aws:lambda:us-east-1:123:function:literal"}},"dynamic":{"Type":"Task","Resource":"arn:aws:states:::lambda:invoke","Parameters":{"FunctionName.$":"$.name"}},"missing":{"Type":"Task","Resource":"arn:aws:states:::lambda:invoke","Parameters":{}}}}}}}"#;
        let links = lambda_links(definition);
        assert_eq!(links.len(), 1);
        assert!(links[0].id.ends_with(":literal"));
    }
}
