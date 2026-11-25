use super::super::AntigravityAgentService;
use crate::acp::antigravity_accounts::usage::AntigravityUsageStatus;

#[test]
fn every_account_reads_as_unavailable_with_no_quota() {
    let root = tempfile::tempdir().unwrap();
    let service = AntigravityAgentService::with_logins_for_test(root.path(), &[]);
    let work = service.insert_profile("Work").unwrap().id;

    let usage = service.accounts_usage().unwrap();

    let ids: Vec<&str> = usage.iter().map(|read| read.account_id.as_str()).collect();
    assert_eq!(ids, vec!["default", work.as_str()]);
    for read in &usage {
        assert_eq!(read.status, AntigravityUsageStatus::Unavailable);
        assert_eq!(read.plan_type, None);
        assert!(read.windows.is_empty());
        assert_eq!(
            read.message.as_deref(),
            Some("Troupe does not read Antigravity limits. Antigravity says when a limit is reached.")
        );
    }
}
