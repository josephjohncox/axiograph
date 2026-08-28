from __future__ import annotations

import unittest
from pathlib import Path

from scripts.publish_release_local import INJECTION_POINTS
from scripts.rehearse_release_publication import rehearse

REPO_ROOT = Path(__file__).resolve().parents[2]


class ReleasePublicationRehearsalTests(unittest.TestCase):
    def test_all_failure_points_and_corruption_fail_before_atomic_publication(
        self,
    ) -> None:
        report = rehearse(REPO_ROOT)
        self.assertEqual(report["failure_injections"], sorted(INJECTION_POINTS))
        self.assertTrue(report["corruption_rejected"])
        self.assertTrue(report["success_committed_once"])
        self.assertEqual(report["bundle_count"], 3)
        self.assertEqual(report["image_digest_count"], 2)
        self.assertIn("GitHub release transactionality", report["scope"]["non_claims"])


if __name__ == "__main__":
    unittest.main()
