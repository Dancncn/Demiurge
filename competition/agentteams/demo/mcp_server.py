#!/usr/bin/env python3
"""Minimal local stdio MCP server for the synthetic GOAI demo.

It intentionally exposes deterministic, sandboxed tools only:
  - a2l_preflight
  - mda_capture_measurements
  - excel_build_raw_data

This is a demo adapter, not a production ECU/MDA driver.
"""

from __future__ import annotations

import json
import re
import subprocess
import sys
from pathlib import Path
from typing import Any

from run import (
    PACKAGE_ROOT,
    capture_measurements,
    parse_a2l,
    write_xlsx,
)


TOOLS = [
    {
        "name": "run_goai_demo",
        "title": "Run repository GOAI demo",
        "description": "Run the repository entrypoint run.py against the bundled synthetic ECU fixtures and return the generated outputs directory and validation summary. This tool never accepts production paths or real vehicle data.",
        "annotations": {
            "readOnlyHint": False,
            "destructiveHint": False,
            "openWorldHint": False,
        },
        "inputSchema": {
            "type": "object",
            "properties": {
                "case_label": {
                    "type": "string",
                    "description": "Optional short label for the evidence run; letters, numbers, hyphen and underscore only.",
                }
            },
            "additionalProperties": False,
        },
    },
    {
        "name": "a2l_preflight",
        "description": "Scan synthetic A2L files and return accepted/rejected parse diagnostics.",
        "inputSchema": {
            "type": "object",
            "properties": {"directory": {"type": "string", "description": "Sample directory name under the demo package."}},
            "required": [],
        },
    },
    {
        "name": "mda_capture_measurements",
        "description": "Replay synthetic MDA measurements for a normalized signal catalog.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "catalog": {"type": "array", "items": {"type": "object"}},
                "ecu_ids": {"type": "array", "items": {"type": "string"}},
                "sample_count": {"type": "integer", "minimum": 1, "maximum": 100},
            },
            "required": ["catalog", "ecu_ids"],
        },
    },
    {
        "name": "excel_build_raw_data",
        "description": "Write synthetic measurement rows to a local XLSX artifact inside the demo outputs directory.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "rows": {"type": "array", "items": {"type": "object"}},
                "output_file": {"type": "string", "description": "Basename only, for example mcp-raw-data.xlsx."},
            },
            "required": ["rows", "output_file"],
        },
    },
]


def response(request_id: Any, result: Any = None, error: dict[str, Any] | None = None) -> None:
    payload: dict[str, Any] = {"jsonrpc": "2.0", "id": request_id}
    if error is not None:
        payload["error"] = error
    else:
        payload["result"] = result
    sys.stdout.write(json.dumps(payload, ensure_ascii=False) + "\n")
    sys.stdout.flush()


def text_result(value: Any) -> dict[str, Any]:
    return {"content": [{"type": "text", "text": json.dumps(value, ensure_ascii=False)}], "structuredContent": value}


def safe_sample_directory(value: str | None) -> Path:
    candidate = (PACKAGE_ROOT / (value or "samples/a2l")).resolve()
    if PACKAGE_ROOT not in candidate.parents and candidate != PACKAGE_ROOT:
        raise ValueError("directory must remain inside the demo package")
    if not candidate.exists():
        raise ValueError(f"directory does not exist: {candidate}")
    return candidate


def safe_output_path(value: str) -> Path:
    filename = Path(value).name
    if filename != value or not filename.endswith(".xlsx"):
        raise ValueError("output_file must be an XLSX basename")
    output_dir = (PACKAGE_ROOT / "outputs").resolve()
    output_dir.mkdir(parents=True, exist_ok=True)
    return output_dir / filename


def call_tool(name: str, arguments: dict[str, Any]) -> dict[str, Any]:
    if name == "run_goai_demo":
        raw_label = str(arguments.get("case_label", "ui-run")).strip()
        label = re.sub(r"[^A-Za-z0-9_-]+", "-", raw_label).strip("-_")[:32] or "ui-run"
        output_dir = PACKAGE_ROOT / "outputs" / f"ui-{label}"
        command = [
            sys.executable,
            str(PACKAGE_ROOT / "run.py"),
            "--config",
            str(PACKAGE_ROOT / "config" / "demo-config.json"),
            "--output",
            str(output_dir),
        ]
        completed = subprocess.run(
            command,
            cwd=PACKAGE_ROOT,
            capture_output=True,
            text=True,
            timeout=120,
            check=False,
        )
        if completed.returncode != 0:
            detail = (completed.stderr or completed.stdout).strip()[-2000:]
            raise RuntimeError(f"run.py returned exit code {completed.returncode}: {detail}")
        try:
            summary = json.loads(completed.stdout)
        except json.JSONDecodeError as error:
            raise RuntimeError(f"run.py returned non-JSON output: {completed.stdout[-1000:]}") from error
        invocation_record = {
            "caller": "Demiurge UI via MCP",
            "mcp_server": "goai-ecu-repository",
            "mcp_tool": "run_goai_demo",
            "entrypoint": "competition/agentteams/demo/run.py",
            "working_directory": str(PACKAGE_ROOT),
            "command": "python competition/agentteams/demo/run.py --config competition/agentteams/demo/config/demo-config.json --output <generated-ui-output>",
            "synthetic_data": True,
            "summary": summary,
        }
        (output_dir / "ui-invocation.json").write_text(
            json.dumps(invocation_record, ensure_ascii=False, indent=2) + "\n",
            encoding="utf-8",
        )
        artifacts = sorted(path.name for path in output_dir.iterdir() if path.is_file())
        return text_result(
            {
                "status": "completed",
                "synthetic_data": True,
                "entrypoint": "competition/agentteams/demo/run.py",
                "working_directory": str(PACKAGE_ROOT),
                "output_dir": str(output_dir),
                "artifacts": artifacts,
                "invocation_record": "ui-invocation.json",
                "summary": summary,
            }
        )

    if name == "a2l_preflight":
        directory = safe_sample_directory(arguments.get("directory"))
        results = [parse_a2l(path) for path in sorted(directory.glob("*.a2l"))]
        return text_result({"results": results, "accepted": sum(item["status"] == "accepted" for item in results)})

    if name == "mda_capture_measurements":
        catalog = arguments["catalog"]
        ecu_ids = arguments["ecu_ids"]
        sample_count = int(arguments.get("sample_count", 5))
        capture_id, rows = capture_measurements(catalog, ecu_ids, {}, sample_count)
        return text_result({"capture_id": capture_id, "rows": rows, "status": "complete", "synthetic": True})

    if name == "excel_build_raw_data":
        path = safe_output_path(arguments["output_file"])
        rows = arguments["rows"]
        headers = ["ECU", "Signal", "Unit", "Samples", "Min", "Max", "Average", "Source"]
        values = [
            [
                row.get("ecu_id"),
                row.get("signal_id"),
                row.get("unit"),
                row.get("sample_count"),
                row.get("min"),
                row.get("max"),
                row.get("average"),
                row.get("source", "mcp_synthetic"),
            ]
            for row in rows
        ]
        write_xlsx(path, {"RawData": [headers, *values]})
        return text_result({"raw_data_uri": str(path), "row_count": len(rows), "synthetic": True})

    raise ValueError(f"unknown tool: {name}")


def main() -> int:
    for line in sys.stdin:
        if not line.strip():
            continue
        try:
            request = json.loads(line)
            method = request.get("method")
            request_id = request.get("id")
            if method == "initialize":
                response(
                    request_id,
                    {
                        "protocolVersion": "2024-11-05",
                        "capabilities": {"tools": {}},
                        "serverInfo": {"name": "goai-ecu-simulator", "version": "0.1.0"},
                    },
                )
            elif method == "tools/list":
                response(request_id, {"tools": TOOLS})
            elif method == "tools/call":
                params = request.get("params", {})
                response(request_id, call_tool(params["name"], params.get("arguments", {})))
            elif request_id is not None:
                response(request_id, {})
        except Exception as error:
            if "request_id" in locals() and request_id is not None:
                response(request_id, error={"code": -32000, "message": str(error)})
            else:
                print(f"MCP server error: {error}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
