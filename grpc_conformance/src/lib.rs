use std::fmt::Write;

pub const ALL_METHODS: [&str; 18] = [
    "AccountService.GetAccountState",
    "AccountService.ListAccountHistory",
    "BlockService.PublishStateBlock",
    "BlockService.GetBlock",
    "BlockService.GetBlockStatuses",
    "BlockService.RequestBlockConfirmation",
    "LedgerService.FrontierCount",
    "LedgerService.ListReceivables",
    "NetworkService.Peers",
    "NetworkService.Telemetry",
    "NodeService.Status",
    "NodeService.Version",
    "NodeService.Keepalive",
    "EventService.WatchConfirmations",
    "EventService.WatchBlockProcessing",
    "EventService.WatchElections",
    "EventService.WatchVotes",
    "EventService.WatchTelemetry",
];

#[derive(Debug, Clone)]
pub struct ScenarioResult {
    pub method: &'static str,
    pub scenario: &'static str,
    pub result: Result<(), String>,
}

impl ScenarioResult {
    pub fn new(method: &'static str, scenario: &'static str, result: Result<(), String>) -> Self {
        Self {
            method,
            scenario,
            result,
        }
    }

    pub fn passed(&self) -> bool {
        self.result.is_ok()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MethodStatus {
    Passing,
    Failing,
    NotTested,
}

impl MethodStatus {
    fn label(self) -> &'static str {
        match self {
            Self::Passing => "PASS",
            Self::Failing => "FAIL",
            Self::NotTested => "NOT TESTED",
        }
    }
}

pub struct Report {
    rpc_url: String,
    grpc_url: String,
    scenarios: Vec<ScenarioResult>,
    setup_error: Option<String>,
}

impl Report {
    pub fn new(rpc_url: impl Into<String>, grpc_url: impl Into<String>) -> Self {
        Self {
            rpc_url: rpc_url.into(),
            grpc_url: grpc_url.into(),
            scenarios: Vec::new(),
            setup_error: None,
        }
    }

    pub fn record(&mut self, result: ScenarioResult) {
        self.scenarios.push(result);
    }

    pub fn set_setup_error(&mut self, error: impl Into<String>) {
        self.setup_error = Some(error.into());
    }

    pub fn has_failures(&self) -> bool {
        self.setup_error.is_some() || self.scenarios.iter().any(|scenario| !scenario.passed())
    }

    pub fn method_status(&self, method: &str) -> MethodStatus {
        let mut scenarios = self
            .scenarios
            .iter()
            .filter(|scenario| scenario.method == method);

        match scenarios.next() {
            None => MethodStatus::NotTested,
            Some(first) if first.passed() && scenarios.all(ScenarioResult::passed) => {
                MethodStatus::Passing
            }
            Some(_) => MethodStatus::Failing,
        }
    }

    pub fn render_markdown(&self) -> String {
        let passed_scenarios = self
            .scenarios
            .iter()
            .filter(|scenario| scenario.passed())
            .count();
        let executed_scenarios = self.scenarios.len();
        let passing_methods = ALL_METHODS
            .iter()
            .filter(|method| self.method_status(method) == MethodStatus::Passing)
            .count();
        let failing_methods = ALL_METHODS
            .iter()
            .filter(|method| self.method_status(method) == MethodStatus::Failing)
            .count();
        let tested_methods = passing_methods + failing_methods;
        let scenario_percent = percentage(passed_scenarios, executed_scenarios);
        let completion_percent = percentage(passing_methods, ALL_METHODS.len());
        let coverage_percent = percentage(tested_methods, ALL_METHODS.len());

        let mut output = String::new();
        writeln!(output, "# RsNano gRPC conformance report\n").unwrap();
        writeln!(
            output,
            "This contract-level report compares the current gRPC implementation with RsNano JSON-RPC on the same deterministic development-network ledger. It is not a claim of full Node API coverage; complex wallet, peer-topology, election, and streaming state remains outside this first 80:20 suite.\n"
        )
        .unwrap();
        writeln!(output, "- JSON-RPC endpoint: `{}`", self.rpc_url).unwrap();
        writeln!(output, "- gRPC endpoint: `{}`", self.grpc_url).unwrap();
        writeln!(output, "- Total gRPC methods: `{}`", ALL_METHODS.len()).unwrap();
        writeln!(output, "- Methods exercised: `{tested_methods}`").unwrap();
        writeln!(output, "- Scenarios executed: `{executed_scenarios}`\n").unwrap();

        if let Some(error) = &self.setup_error {
            writeln!(output, "> Suite setup failed: {}\n", escape_markdown(error)).unwrap();
        }

        writeln!(output, "## Scoreboard\n").unwrap();
        writeln!(output, "| Measure | Result | Visualization |").unwrap();
        writeln!(output, "|---|---:|---|").unwrap();
        writeln!(
            output,
            "| Method coverage | {tested_methods}/{} ({coverage_percent}%) | `{}` |",
            ALL_METHODS.len(),
            progress_bar(coverage_percent)
        )
        .unwrap();
        writeln!(
            output,
            "| Passing methods | {passing_methods}/{} ({completion_percent}%) | `{}` |",
            ALL_METHODS.len(),
            progress_bar(completion_percent)
        )
        .unwrap();
        writeln!(
            output,
            "| Scenario correctness | {passed_scenarios}/{executed_scenarios} ({scenario_percent}%) | `{}` |\n",
            progress_bar(scenario_percent)
        )
        .unwrap();

        writeln!(output, "## Method completion\n").unwrap();
        writeln!(output, "| Method | Status | Scenarios |").unwrap();
        writeln!(output, "|---|---|---:|").unwrap();
        for method in ALL_METHODS {
            let status = self.method_status(method);
            let scenario_count = self
                .scenarios
                .iter()
                .filter(|scenario| scenario.method == method)
                .count();
            writeln!(
                output,
                "| `{method}` | {} | {scenario_count} |",
                status.label()
            )
            .unwrap();
        }

        writeln!(output, "\n## Scenario results\n").unwrap();
        writeln!(output, "| Method | Scenario | Result | Detail |").unwrap();
        writeln!(output, "|---|---|---|---|").unwrap();
        for scenario in &self.scenarios {
            let (status, detail) = match &scenario.result {
                Ok(()) => ("PASS", "—".to_string()),
                Err(error) => ("FAIL", escape_markdown(error)),
            };
            writeln!(
                output,
                "| `{}` | {} | {status} | {detail} |",
                scenario.method, scenario.scenario
            )
            .unwrap();
        }

        writeln!(output, "\n## Interpretation\n").unwrap();
        writeln!(
            output,
            "A method passes only when every executed scenario for that method passes. Untested methods remain in the denominator so the completion percentage cannot hide missing behavior. Scenario failures identify semantic differences or incorrect gRPC status mappings; they do not stop report generation."
        )
        .unwrap();

        output
    }
}

fn percentage(numerator: usize, denominator: usize) -> usize {
    (numerator * 100).checked_div(denominator).unwrap_or(0)
}

fn progress_bar(percent: usize) -> String {
    let completed = percent / 10;
    format!(
        "{}{} {percent}%",
        "#".repeat(completed),
        "-".repeat(10 - completed)
    )
}

fn escape_markdown(value: &str) -> String {
    value
        .replace('|', "\\|")
        .replace(['\r', '\n'], " ")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn method_without_scenarios_is_not_tested() {
        let report = Report::new("rpc", "grpc");

        assert_eq!(
            report.method_status("AccountService.GetAccountState"),
            MethodStatus::NotTested
        );
    }

    #[test]
    fn method_with_only_passing_scenarios_passes() {
        let mut report = Report::new("rpc", "grpc");
        report.record(ScenarioResult::new(
            "AccountService.GetAccountState",
            "genesis account",
            Ok(()),
        ));

        assert_eq!(
            report.method_status("AccountService.GetAccountState"),
            MethodStatus::Passing
        );
    }

    #[test]
    fn one_failing_scenario_fails_the_method() {
        let mut report = Report::new("rpc", "grpc");
        report.record(ScenarioResult::new(
            "AccountService.GetAccountState",
            "genesis account",
            Ok(()),
        ));
        report.record(ScenarioResult::new(
            "AccountService.GetAccountState",
            "invalid account",
            Err("wrong code".to_string()),
        ));

        assert_eq!(
            report.method_status("AccountService.GetAccountState"),
            MethodStatus::Failing
        );
    }

    #[test]
    fn markdown_keeps_untested_methods_in_completion_denominator() {
        let mut report = Report::new("rpc", "grpc");
        report.record(ScenarioResult::new(
            "AccountService.GetAccountState",
            "genesis account",
            Ok(()),
        ));

        let markdown = report.render_markdown();

        assert!(markdown.contains("Passing methods | 1/18 (5%)"));
        assert!(markdown.contains("Method coverage | 1/18 (5%)"));
    }
}
