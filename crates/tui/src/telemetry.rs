use std::time::Duration;

const REPORT_URL: &str = "https://stasshstats.bylazar.com/report";

fn telemetry_secret() -> Option<&'static str> {
    option_env!("STASSH_TELEMETRY_SECRET").filter(|s| !s.trim().is_empty())
}

pub(crate) fn report_host_count_async(uuid: String, host_count: usize) {
    let Some(secret) = telemetry_secret() else {
        return;
    };

    std::thread::spawn(move || {
        let payload = serde_json::json!({
            "uuid": uuid,
            "hostCount": host_count,
            "secret": secret,
        });

        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_connect(Some(Duration::from_secs(2)))
            .timeout_global(Some(Duration::from_secs(4)))
            .build()
            .into();

        let _ = agent
            .post(REPORT_URL)
            .header("content-type", "application/json")
            .send_json(&payload);
    });
}
