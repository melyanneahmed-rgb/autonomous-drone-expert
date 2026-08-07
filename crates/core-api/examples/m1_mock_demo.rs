#![forbid(unsafe_code)]

//! Executable M1 demonstration over project-owned simulation components only.

use ade_core_api::{
    CaseMetadata, REPORT_MARKERS, SimulationApprovals, TerminalClassification, run_beeper_lifecycle,
};
use ade_mock_fc::MockFc;
use ade_planning::SystemInitBeeperGoal;
use ade_protocol_msp::BeeperConfigSnapshot;
use ade_safety::ExecutionTarget;
use ade_transport::{InMemoryAudit, MockTransport};
use std::process::ExitCode;

fn main() -> ExitCode {
    let target = ExecutionTarget::Mock;
    let initial = BeeperConfigSnapshot {
        beeper_off_flags: 0,
        dshot_beacon_tone: 3,
        dshot_beacon_off_flags: 0x0000_0011,
    };
    let approvals = SimulationApprovals::obtain(target)
        .expect("the typed safety gate must allow the Mock target");
    let report = run_beeper_lifecycle(
        target,
        SystemInitBeeperGoal::Disable,
        MockTransport::new(MockFc::new(initial), InMemoryAudit::new()),
        CaseMetadata {
            case_id: "m1-mock-demo".to_string(),
            started_at_label: "deterministic-demo".to_string(),
        },
        approvals,
    );

    println!("{report:#?}");
    if report.terminal == TerminalClassification::CompletedVerified
        && report.verification_state == "MOCK_EXERCISED"
        && report.markers == REPORT_MARKERS
    {
        ExitCode::SUCCESS
    } else {
        eprintln!("M1 mock demonstration did not reach its verified simulation terminal");
        ExitCode::FAILURE
    }
}
