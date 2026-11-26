pub type ScenarioError = (&'static str, String);

pub fn report_step(name: &'static str, result: Result<(), String>) -> Result<usize, ScenarioError> {
    match result {
        Ok(()) => {
            println!("[PASS] {name}");
            Ok(1)
        }
        Err(error) => Err((name, error)),
    }
}

#[cfg(test)]
mod test_scenario_runner {
    use super::*;

    #[test]
    fn report_step_returns_one_on_ok_and_preserves_error_on_err() {
        assert_eq!(report_step("suite::pass", Ok(())), Ok(1));
        assert_eq!(
            report_step("suite::fail", Err("boom".to_string())),
            Err(("suite::fail", "boom".to_string()))
        );
    }
}
