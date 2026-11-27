use super::*;

#[test]
fn durations_read_like_the_web_client() {
    let cases = [
        (0.0, "0 ms"),
        (999.0, "999 ms"),
        (1_000.0, "1 s"),
        (59_400.0, "59 s"),
        (60_000.0, "1 min"),
        (125_000.0, "2 min 5 s"),
        (3_600_000.0, "1 h"),
        (5_400_000.0, "1 h 30 min"),
        (86_400_000.0, "1 day"),
        (172_800_000.0, "2 days"),
        (90_000_000.0, "1 d 1 h"),
    ];
    for (ms, want) in cases {
        assert_eq!(format_duration(ms), want, "{ms}");
    }
}

#[test]
fn wall_clock_times_use_the_short_en_us_form() {
    // 2026-10-30T09:00:00Z is a Friday.
    let at = 1_793_350_800_000.0;
    assert_eq!(format_date_time(at, at, 0), "Fri, Oct 30, 9:00 AM");
    assert_eq!(format_date_time(at, at, 7 * 60), "Fri, Oct 30, 4:00 PM");
    assert_eq!(
        format_date_time(at, at + 400.0 * 86_400_000.0, 0),
        "Fri, Oct 30, 2026, 9:00 AM"
    );
    assert_eq!(format_date_time(0.0, 0.0, 0), "Thu, Jan 1, 12:00 AM");
}

#[test]
fn a_card_names_its_http_host_or_keeps_a_templated_url() {
    let http = |url: &str| NodeSpec::Http {
        method: super::super::types::HttpMethod::Post,
        url: url.to_owned(),
        headers: vec![],
        body: None,
        allowed_hosts: vec![],
    };
    assert_eq!(
        node_key_detail(&http("https://api.example.com/items?x=1")),
        "POST api.example.com"
    );
    assert_eq!(
        node_key_detail(&http("{{nodes.a.output}}")),
        "POST {{nodes.a.output}}"
    );
}
