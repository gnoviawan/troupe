use super::notice::{RATE_LIMIT_MESSAGE, STREAM_INTERRUPT_MESSAGE, format_agent_error};

#[test]
fn rate_limits_and_dropped_streams_read_as_advice() {
    for raw in ["Stream closed: NGHTTP2_ENHANCE_YOUR_CALM", "HTTP 429 Too Many Requests", "rate_limit_error"] {
        assert_eq!(format_agent_error(raw), RATE_LIMIT_MESSAGE, "{raw}");
    }
    for raw in ["write EPIPE", "Connection reset by peer", "stream ended unexpectedly", "RetriableError: x", "connection to host timed out", "agent thread dropped"] {
        assert_eq!(format_agent_error(raw), STREAM_INTERRUPT_MESSAGE, "{raw}");
    }
    for raw in ["Invalid params", "pipeline failed", "build 4290 failed"] {
        assert_eq!(format_agent_error(raw), raw, "{raw}");
    }
}
