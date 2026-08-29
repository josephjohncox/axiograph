from __future__ import annotations

import json
import os
import subprocess
import tempfile
import textwrap
import unittest
from pathlib import Path

from scripts.release_version import workspace_version

REPO_ROOT = Path(__file__).resolve().parents[2]


class GithubReleasePublicationTests(unittest.TestCase):
    def test_release_is_drafted_verified_and_only_then_published(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            first = root / "axiograph-linux.tar.gz"
            second = root / "axiograph-linux.tar.gz.sha256"
            first.write_bytes(b"archive")
            second.write_text("checksum\n", encoding="utf-8")
            log = root / "gh.log"
            fake_gh = root / "gh"
            fake_gh.write_text(
                textwrap.dedent(
                    """\
                    #!/usr/bin/env python3
                    import json
                    import os
                    import sys
                    from pathlib import Path

                    args = sys.argv[1:]
                    with Path(os.environ["FAKE_GH_LOG"]).open("a") as log:
                        log.write(json.dumps(args) + "\\n")
                    if args[:1] == ["api"]:
                        query = args[1]
                        selector = args[-1] if "--jq" in args else ""
                        if "/releases/tags/" in query:
                            print("gh: Not Found (HTTP 404)", file=sys.stderr)
                            raise SystemExit(1)
                        if selector == ".object.type":
                            print("commit" if "/git/tags/" in query else "tag")
                        elif selector == ".object.sha":
                            print(
                                os.environ["GITHUB_SHA"]
                                if "/git/tags/" in query
                                else "b" * 40
                            )
                    elif args[:2] == ["release", "view"]:
                        assets = [
                            {"name": Path(item).name}
                            for item in os.environ["RELEASE_FILES"].splitlines()
                            if item
                        ]
                        print(json.dumps({
                            "assets": assets,
                            "isDraft": True,
                            "tagName": os.environ["GITHUB_REF_NAME"],
                            "targetCommitish": os.environ["GITHUB_SHA"],
                        }))
                    """
                ),
                encoding="utf-8",
            )
            fake_gh.chmod(0o755)
            sha = "a" * 40
            version = workspace_version(REPO_ROOT)
            env = {
                **os.environ,
                "PATH": f"{root}:{os.environ['PATH']}",
                "FAKE_GH_LOG": str(log),
                "GH_TOKEN": "test-token",
                "GITHUB_REF": f"refs/tags/v{version}",
                "GITHUB_REF_NAME": f"v{version}",
                "GITHUB_REPOSITORY": "josephjohncox/axiograph",
                "GITHUB_SHA": sha,
                "RELEASE_FILES": f"{first}\n{second}",
            }

            subprocess.run(
                ["./scripts/publish_github_release.sh"],
                cwd=REPO_ROOT,
                env=env,
                check=True,
                capture_output=True,
                text=True,
            )

            calls = [json.loads(line) for line in log.read_text().splitlines()]
            create = next(call for call in calls if call[:2] == ["release", "create"])
            edit = next(call for call in calls if call[:2] == ["release", "edit"])
            self.assertIn("--draft", create)
            self.assertIn("--generate-notes", create)
            self.assertIn("--draft=false", edit)
            self.assertLess(calls.index(create), calls.index(edit))


if __name__ == "__main__":
    unittest.main()
