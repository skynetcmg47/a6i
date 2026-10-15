use crate::{
    app::{Service, valid_region},
    model::ResourceLink,
};

#[derive(Debug, PartialEq, Eq)]
pub struct Arn<'a> {
    pub partition: &'a str,
    pub service: &'a str,
    pub region: &'a str,
    pub account: &'a str,
    pub resource: &'a str,
}

impl<'a> Arn<'a> {
    pub fn parse(value: &'a str) -> Option<Self> {
        let mut parts = value.splitn(6, ':');
        if parts.next()? != "arn" {
            return None;
        }
        let partition = parts.next()?;
        let service = parts.next()?;
        let region = parts.next()?;
        let account = parts.next()?;
        let resource = parts.next()?;
        let expected_partition = if region.starts_with("cn-") {
            "aws-cn"
        } else if region.starts_with("us-gov-") {
            "aws-us-gov"
        } else {
            "aws"
        };
        if partition != expected_partition
            || !valid_region(region)
            || account.is_empty()
            || !account.bytes().all(|byte| byte.is_ascii_digit())
            || resource.is_empty()
            || resource
                .chars()
                .any(|character| character.is_control() || matches!(character, '$' | '{' | '}'))
        {
            return None;
        }
        Some(Self {
            partition,
            service,
            region,
            account,
            resource,
        })
    }

    pub fn target(&self, original: &str) -> Option<ResourceLink> {
        let (service, kind, name) = match self.service {
            "lambda" => (
                Service::Lambda,
                "function",
                self.resource.strip_prefix("function:")?,
            ),
            "states" => (
                Service::StepFunctions,
                "machine",
                self.resource.strip_prefix("stateMachine:")?,
            ),
            "sns" => (Service::Sns, "topic", self.resource),
            "sqs" => (Service::Sqs, "queue", self.resource),
            _ => return None,
        };
        if name.is_empty()
            || (name.contains(':') && service != Service::Lambda)
            || !name.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':')
            })
        {
            return None;
        }
        if service == Service::Lambda
            && (name.split(':').count() > 2 || name.split(':').any(str::is_empty))
        {
            return None;
        }
        Some(ResourceLink {
            service,
            kind: kind.into(),
            id: original.into(),
        })
    }
}

pub fn static_link(value: &str) -> Option<ResourceLink> {
    Arn::parse(value)?.target(value)
}

pub fn add_link(links: &mut Vec<ResourceLink>, value: &str) {
    if let Some(target) = static_link(value)
        && !links.contains(&target)
    {
        links.push(target);
    }
}

pub fn redrive_link(policy: &str) -> Option<ResourceLink> {
    let value: serde_json::Value = serde_json::from_str(policy).ok()?;
    static_link(value.get("deadLetterTargetArn")?.as_str()?)
        .filter(|target| target.service == Service::Sqs)
}

pub fn integration_link(uri: &str) -> Option<ResourceLink> {
    if let Some(target) = static_link(uri) {
        return (target.service == Service::Lambda).then_some(target);
    }
    let (prefix, suffix) = uri.split_once(":lambda:path/2015-03-31/functions/")?;
    let mut wrapper = prefix.split(':');
    if wrapper.next()? != "arn"
        || !matches!(wrapper.next()?, "aws" | "aws-cn" | "aws-us-gov")
        || wrapper.next()? != "apigateway"
    {
        return None;
    }
    let region = wrapper.next()?;
    if wrapper.next().is_some() || suffix.matches("/invocations").count() != 1 {
        return None;
    }
    let function = suffix.strip_suffix("/invocations")?;
    let target = static_link(function)?;
    let function_arn = Arn::parse(function)?;
    (target.service == Service::Lambda
        && prefix.split(':').nth(1)? == function_arn.partition
        && region == function_arn.region)
        .then_some(target)
}

pub fn queue_url(arn: &str) -> Option<String> {
    let parsed = Arn::parse(arn)?;
    if parsed.service != "sqs" || parsed.target(arn).is_none() {
        return None;
    }
    let domain = if parsed.partition == "aws-cn" {
        "amazonaws.com.cn"
    } else {
        "amazonaws.com"
    };
    Some(format!(
        "https://sqs.{}.{}/{}/{}",
        parsed.region, domain, parsed.account, parsed.resource
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_references_only() {
        for (partition, region) in [
            ("aws", "us-east-1"),
            ("aws-cn", "cn-north-1"),
            ("aws-us-gov", "us-gov-west-1"),
        ] {
            let lambda = format!("arn:{partition}:lambda:{region}:123:function:worker");
            assert_eq!(static_link(&lambda).unwrap().service, Service::Lambda);
        }
        for invalid in [
            "arn:aws:lambda::123:function:a",
            "arn:aws:lambda:us-east-1:123:function:${name}",
            "arn:aws:sqs:us-east-1:123:",
            "arn:aws:events:us-east-1:123:rule/x",
            "arn:aws:sqs:us-east-1:123:evil/../queue",
            "arn:aws-cn:lambda:us-east-1:123:function:wrong-partition",
        ] {
            assert!(static_link(invalid).is_none());
        }
        assert!(redrive_link("not json").is_none());
        assert!(
            redrive_link(r#"{"deadLetterTargetArn":"arn:aws:sns:us-east-1:123:topic"}"#).is_none()
        );
        assert!(integration_link("https://example.com").is_none());
        assert!(integration_link("arn:aws:apigateway:us-east-1:lambda:path/2015-03-31/functions/arn:aws:lambda:eu-west-1:123:function:worker/invocations").is_none());
        let mut links = vec![];
        add_link(&mut links, "arn:aws:sqs:us-east-1:123:dead");
        add_link(&mut links, "arn:aws:sqs:us-east-1:123:dead");
        assert_eq!(links.len(), 1);
        assert!(integration_link("arn:aws:apigateway:us-east-1:lambda:path/2015-03-31/functions/arn:aws:lambda:us-east-1:123:function:worker/invocations").is_some());
        assert!(integration_link("arn:aws:apigateway:eu-west-1:lambda:path/2015-03-31/functions/arn:aws:lambda:us-east-1:123:function:worker/invocations").is_none());
        assert_eq!(
            queue_url("arn:aws-cn:sqs:cn-north-1:123:dead"),
            Some("https://sqs.cn-north-1.amazonaws.com.cn/123/dead".into())
        );
    }
}
