"""Small dependency-free regression checks for the synthetic AgentTeams demo."""

from __future__ import annotations

import unittest
from pathlib import Path

import run


class AcceptanceCriteriaTests(unittest.TestCase):
    def test_outline_criteria_are_evaluated_against_each_ecu_summary(self) -> None:
        config = run.read_json(run.PACKAGE_ROOT / "config" / "demo-config.json")
        _, selected = run.preflight_a2l(run.resolve_package_path(config["a2l_dir"]))
        seeds = run.read_seed_trace(run.resolve_package_path(config["can_trace"]))
        _, rows = run.capture_measurements(
            selected["signals"],
            config["ecu_ids"],
            seeds,
            int(config["sample_count"]),
        )
        outline = run.read_json(run.resolve_package_path(config["trial_outline"]))
        manual = run.read_json(run.resolve_package_path(config["process_manual"]))
        report_spec = run.read_json(run.resolve_package_path(config["report_specification"]))
        model = run.build_compliance_model(outline, manual, report_spec)

        checks = run.evaluate_acceptance_criteria(model["acceptance_criteria"], rows)

        self.assertEqual(len(checks), 6)
        self.assertTrue(all(check["status"] == "pass" for check in checks))
        self.assertTrue(all(check["evaluated_rows"] == len(config["ecu_ids"]) for check in checks))
        self.assertEqual(run.summarize_data_verdict(checks), "pass")

    def test_failed_value_is_not_silently_reported_as_pass(self) -> None:
        criterion = {
            "id": "TEMP-BOX-MIN",
            "requirement_id": "ENV-TEMP-SHOCK",
            "signal_id": "BOX_TEMP_C",
            "metric": "min",
            "operator": "gte",
            "value": -20,
        }
        checks = run.evaluate_acceptance_criteria(
            [criterion],
            [
                {
                    "ecu_id": "SIM-ECU-01",
                    "signal_id": "BOX_TEMP_C",
                    "min": -25.0,
                    "max": 10.0,
                    "average": -5.0,
                    "sample_count": 5,
                }
            ],
        )

        self.assertEqual(checks[0]["status"], "fail")
        self.assertEqual(run.summarize_data_verdict(checks), "fail")
        self.assertTrue(checks[0]["findings"])


if __name__ == "__main__":
    unittest.main()
