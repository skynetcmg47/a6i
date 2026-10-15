use aws_config::BehaviorVersion;
use aws_credential_types::provider::ProvideCredentials;
use aws_runtime::env_config::file::EnvConfigFiles;
use aws_types::{
    SdkConfig,
    os_shim_internal::{Env, Fs},
    region::Region,
};

use crate::app::Context;

pub struct VerifiedContext {
    pub context: Context,
    pub config: SdkConfig,
}

// Application reads are restricted to the audited operations in README.md.
pub async fn load_context(
    profile: Option<String>,
    region: Option<String>,
) -> Result<VerifiedContext, String> {
    let mut loader = aws_config::defaults(BehaviorVersion::latest());
    if let Some(name) = &profile {
        let profiles = aws_config::profile::load(
            &Fs::real(),
            &Env::real(),
            &EnvConfigFiles::default(),
            Some(name.clone().into()),
        )
        .await
        .map_err(|_| "AWS profile config unavailable; check AWS config files".to_string())?;
        if profiles.get_profile(name).is_none() {
            return Err(format!(
                "AWS profile '{name}' not found; check AWS config files"
            ));
        }
        loader = loader.profile_name(name).credentials_provider(
            aws_config::profile::ProfileFileCredentialsProvider::builder()
                .profile_name(name)
                .build(),
        );
    }
    if let Some(name) = &region {
        loader = loader.region(Region::new(name.clone()));
    }
    let config = loader.load().await;
    let region = config
        .region()
        .map(|value| value.as_ref().to_owned())
        .ok_or_else(|| {
            "AWS region unavailable; set --region REGION, AWS_REGION, or a profile region"
                .to_string()
        })?;
    let credentials = config.credentials_provider().ok_or_else(|| {
        "AWS credentials unavailable; configure a profile or AWS credential chain".to_string()
    })?;
    credentials.provide_credentials().await.map_err(|_| {
        "AWS credentials unavailable; check selected profile or AWS credential chain".to_string()
    })?;

    let account = aws_sdk_sts::Client::new(&config)
        .get_caller_identity()
        .send()
        .await
        .ok()
        .and_then(|identity| identity.account().map(str::to_owned));
    Ok(VerifiedContext {
        context: Context {
            account,
            profile,
            region,
        },
        config,
    })
}

#[cfg(test)]
mod safety_tests {
    #[test]
    fn sdk_calls_match_reviewed_inventory() {
        let inventory = [
            ("get_caller_identity", "GetCallerIdentity"),
            ("list_state_machines", "ListStateMachines"),
            ("describe_state_machine", "DescribeStateMachine"),
            ("list_executions", "ListExecutions"),
            ("describe_execution", "DescribeExecution"),
            ("get_execution_history", "GetExecutionHistory"),
            ("list_functions", "ListFunctions"),
            ("get_function_configuration", "GetFunctionConfiguration"),
            ("list_event_source_mappings", "ListEventSourceMappings"),
            ("list_hosted_zones", "ListHostedZones"),
            ("get_hosted_zone", "GetHostedZone"),
            ("list_resource_record_sets", "ListResourceRecordSets"),
            ("get_rest_apis", "GetRestApis"),
            ("get_rest_api", "GetRestApi"),
            ("get_resources", "GetResources"),
            ("get_method", "GetMethod"),
            ("get_integration", "GetIntegration"),
            ("get_stages", "GetStages"),
            ("get_apis", "GetApis"),
            ("get_api", "GetApi"),
            ("get_routes", "GetRoutes"),
            ("get_integrations", "GetIntegrations"),
            ("describe_alarms", "DescribeAlarms"),
            ("list_metrics", "ListMetrics"),
            ("describe_log_groups", "DescribeLogGroups"),
            ("describe_log_streams", "DescribeLogStreams"),
            ("get_log_events", "GetLogEvents"),
            ("list_topics", "ListTopics"),
            ("get_topic_attributes", "GetTopicAttributes"),
            ("list_subscriptions_by_topic", "ListSubscriptionsByTopic"),
            ("list_queues", "ListQueues"),
            ("get_queue_attributes", "GetQueueAttributes"),
            ("list_event_buses", "ListEventBuses"),
            ("describe_event_bus", "DescribeEventBus"),
            ("list_rules", "ListRules"),
            ("describe_rule", "DescribeRule"),
            ("list_targets_by_rule", "ListTargetsByRule"),
        ];
        let readme = include_str!("../README.md");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut calls = 0;
        let mut approved = 0;
        for entry in std::fs::read_dir(root).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_none_or(|extension| extension != "rs") {
                continue;
            }
            let name = path.file_name().unwrap().to_str().unwrap();
            let text = std::fs::read_to_string(&path).unwrap();
            let count = text.matches(&format!(".{}()", "send")).count();
            assert!(
                count == 0 || matches!(name, "aws.rs" | "reads.rs" | "coverage.rs"),
                "unreviewed SDK call in {name}"
            );
            calls += count;
            for (method, operation) in inventory {
                let found = text.matches(&format!(".{method}()")).count();
                if found > 0 {
                    assert!(
                        readme.contains(&format!("`{operation}`")),
                        "missing inventory entry: {operation}"
                    );
                }
                approved += found;
            }
        }
        assert_eq!(
            calls, approved,
            "new SDK call needs reviewed inventory entry"
        );
    }
}
