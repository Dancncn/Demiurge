#!/usr/bin/env python3
"""Runnable, dependency-free GOAI AgentTeams demonstration.

The demo intentionally uses synthetic A2L/CAN/test-document data. It models
the three business agents and produces traceable artifacts without claiming
that a real ECU, MDA session, or enterprise procedure was executed.
"""

from __future__ import annotations

import argparse
import csv
import hashlib
import json
import os
import re
import sys
import urllib.error
import urllib.request
import zipfile
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Iterable
from xml.sax.saxutils import escape


PACKAGE_ROOT = Path(__file__).resolve().parent
MEASUREMENT_RE = re.compile(
    r"/begin\s+MEASUREMENT\s+(?P<name>[^\s]+)(?P<body>.*?)/end\s+MEASUREMENT",
    re.IGNORECASE | re.DOTALL,
)
UNIT_RE = re.compile(r"\bUNIT\s+\"([^\"]+)\"", re.IGNORECASE)


def canonical_json(value: Any) -> bytes:
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode("utf-8")


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def sha256_file(path: Path) -> str:
    return sha256_bytes(path.read_bytes())


def write_json(path: Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def resolve_package_path(value: str | Path) -> Path:
    path = Path(value)
    return path if path.is_absolute() else PACKAGE_ROOT / path


def read_json(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text(encoding="utf-8"))


def decode_a2l(path: Path) -> tuple[str | None, str | None]:
    raw = path.read_bytes()
    for encoding in ("utf-8-sig", "gb18030", "latin-1"):
        try:
            return raw.decode(encoding), encoding
        except UnicodeDecodeError:
            continue
    return None, None


def parse_a2l(path: Path) -> dict[str, Any]:
    text, encoding = decode_a2l(path)
    if text is None:
        return {
            "file": path.name,
            "status": "rejected",
            "reason": "unsupported_encoding",
            "encoding": None,
            "measurement_count": 0,
            "signals": [],
        }

    errors: list[str] = []
    for marker in ("/begin PROJECT", "/end PROJECT", "/begin MODULE", "/end MODULE"):
        if marker.lower() not in text.lower():
            errors.append(f"missing:{marker}")

    if text.lower().count("/begin measurement") != text.lower().count("/end measurement"):
        errors.append("unbalanced_measurement_blocks")

    signals: list[dict[str, Any]] = []
    names: set[str] = set()
    for match in MEASUREMENT_RE.finditer(text):
        name = match.group("name")
        body = match.group("body")
        unit_match = UNIT_RE.search(body)
        if name in names:
            errors.append(f"duplicate_measurement:{name}")
        names.add(name)
        signals.append(
            {
                "signal_id": name,
                "unit": unit_match.group(1) if unit_match else "unknown",
                "source_file": path.name,
            }
        )

    if not signals:
        errors.append("no_measurement_blocks")

    return {
        "file": path.name,
        "status": "accepted" if not errors else "rejected",
        "reason": "ok" if not errors else ";".join(errors),
        "encoding": encoding,
        "measurement_count": len(signals),
        "signals": signals if not errors else [],
    }


def preflight_a2l(directory: Path) -> tuple[list[dict[str, Any]], dict[str, Any]]:
    results = [parse_a2l(path) for path in sorted(directory.glob("*.a2l"))]
    accepted = [item for item in results if item["status"] == "accepted"]
    if not accepted:
        raise RuntimeError("A2L 预筛选没有找到可用文件")
    selected = max(accepted, key=lambda item: item["measurement_count"])
    return results, selected


def read_seed_trace(path: Path) -> dict[tuple[str, str], list[float]]:
    seeds: dict[tuple[str, str], list[float]] = {}
    with path.open(newline="", encoding="utf-8") as handle:
        for row in csv.DictReader(handle):
            key = (row["ecu_id"], row["signal_id"])
            seeds.setdefault(key, []).append(float(row["value"]))
    return seeds


def capture_measurements(
    signals: list[dict[str, Any]],
    ecu_ids: list[str],
    seeds: dict[tuple[str, str], list[float]],
    sample_count: int,
) -> tuple[str, list[dict[str, Any]]]:
    rows: list[dict[str, Any]] = []
    for signal_index, signal in enumerate(signals):
        signal_id = signal["signal_id"]
        for ecu_index, ecu_id in enumerate(ecu_ids):
            seed_values = seeds.get((ecu_id, signal_id), [])
            if seed_values:
                base = sum(seed_values) / len(seed_values)
            elif signal_id == "CAN_ERROR_COUNT":
                base = 0.0
            elif signal_id == "CONDENSATION_FLAG":
                base = 0.0
            elif "TEMP" in signal_id:
                base = -18.0 + signal_index * 0.25 + ecu_index * 0.4
            elif "VOLT" in signal_id:
                base = 12.0 + ecu_index * 0.05
            elif "HUMID" in signal_id:
                base = 72.0 + signal_index * 0.3
            else:
                base = float((signal_index % 5) + ecu_index * 0.1)

            if signal_id in {"CAN_ERROR_COUNT", "CONDENSATION_FLAG"}:
                values = [0.0 for _ in range(sample_count)]
            else:
                values = [round(base + ((sample % 5) - 2) * 0.35, 3) for sample in range(sample_count)]
            rows.append(
                {
                    "ecu_id": ecu_id,
                    "signal_id": signal_id,
                    "unit": signal["unit"],
                    "sample_count": len(values),
                    "min": min(values),
                    "max": max(values),
                    "average": round(sum(values) / len(values), 3),
                    "source": "synthetic_mda_replay",
                }
            )
    capture_id = "sim-mda-" + sha256_bytes(canonical_json(rows))[:12]
    return capture_id, rows


def excel_column(index: int) -> str:
    result = ""
    number = index
    while number:
        number, remainder = divmod(number - 1, 26)
        result = chr(65 + remainder) + result
    return result


def xlsx_cell(reference: str, value: Any) -> str:
    if value is None:
        return f'<c r="{reference}"/>'
    if isinstance(value, bool):
        return f'<c r="{reference}" t="b"><v>{1 if value else 0}</v></c>'
    if isinstance(value, (int, float)) and not isinstance(value, bool):
        return f'<c r="{reference}"><v>{value}</v></c>'
    text = escape(str(value))
    return f'<c r="{reference}" t="inlineStr"><is><t xml:space="preserve">{text}</t></is></c>'


def worksheet_xml(rows: list[list[Any]]) -> str:
    row_xml: list[str] = []
    for row_number, row in enumerate(rows, start=1):
        cells = "".join(xlsx_cell(f"{excel_column(column_number)}{row_number}", value) for column_number, value in enumerate(row, start=1))
        row_xml.append(f'<row r="{row_number}">{cells}</row>')
    last_column = excel_column(max((len(row) for row in rows), default=1))
    return (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">'
        f'<dimension ref="A1:{last_column}{max(len(rows), 1)}"/>'
        f"<sheetData>{''.join(row_xml)}</sheetData>"
        "</worksheet>"
    )


def write_xlsx(path: Path, sheets: dict[str, list[list[Any]]]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    names = list(sheets)
    workbook_sheets = "".join(
        f'<sheet name="{escape(name)}" sheetId="{index}" r:id="rId{index}"/>'
        for index, name in enumerate(names, start=1)
    )
    workbook = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" '
        'xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">'
        f"<sheets>{workbook_sheets}</sheets></workbook>"
    )
    workbook_rels = "".join(
        f'<Relationship Id="rId{index}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet{index}.xml"/>'
        for index in range(1, len(names) + 1)
    )
    content_overrides = "".join(
        f'<Override PartName="/xl/worksheets/sheet{index}.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>'
        for index in range(1, len(names) + 1)
    )
    content_types = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
        '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
        '<Default Extension="xml" ContentType="application/xml"/>'
        '<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>'
        f"{content_overrides}</Types>"
    )
    root_rels = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
        '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/>'
        "</Relationships>"
    )
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as archive:
        archive.writestr("[Content_Types].xml", content_types)
        archive.writestr("_rels/.rels", root_rels)
        archive.writestr("xl/workbook.xml", workbook)
        archive.writestr("xl/_rels/workbook.xml.rels", f'<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">{workbook_rels}</Relationships>')
        for index, name in enumerate(names, start=1):
            archive.writestr(f"xl/worksheets/sheet{index}.xml", worksheet_xml(sheets[name]))


def docx_paragraph(text: str, style: str | None = None) -> str:
    style_xml = f'<w:pPr><w:pStyle w:val="{style}"/></w:pPr>' if style else ""
    return f"<w:p>{style_xml}<w:r><w:t xml:space=\"preserve\">{escape(text)}</w:t></w:r></w:p>"


def write_docx(path: Path, lines: Iterable[tuple[str, str | None]]) -> None:
    document_body = "".join(docx_paragraph(text, style) for text, style in lines)
    document = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">'
        f"<w:body>{document_body}<w:sectPr/></w:body></w:document>"
    )
    content_types = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
        '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
        '<Default Extension="xml" ContentType="application/xml"/>'
        '<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>'
        "</Types>"
    )
    root_rels = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
        '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>'
        "</Relationships>"
    )
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as archive:
        archive.writestr("[Content_Types].xml", content_types)
        archive.writestr("_rels/.rels", root_rels)
        archive.writestr("word/document.xml", document)
        archive.writestr("word/_rels/document.xml.rels", '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"/>')


def build_compliance_model(outline: dict[str, Any], manual: dict[str, Any], report_spec: dict[str, Any]) -> dict[str, Any]:
    coverage = []
    acceptance_criteria: list[dict[str, Any]] = []
    for requirement in outline.get("requirements", []):
        criteria = requirement.get("acceptance_criteria", [])
        coverage.append(
            {
                "requirement_id": requirement["id"],
                "source": "trial_outline",
                "status": "covered",
                "evidence_refs": ["raw-data.xlsx", "a2l-preflight.json"],
                "acceptance_criteria_ids": [criterion["id"] for criterion in criteria],
            }
        )
        acceptance_criteria.extend(
            {
                **criterion,
                "requirement_id": requirement["id"],
                "source": "trial_outline",
            }
            for criterion in criteria
        )
    model = {
        "title": outline.get("title", "Synthetic ECU environmental test"),
        "revision": outline.get("revision", "SIM-R1"),
        "environment": outline.get("environment", {}),
        "ordered_steps": manual.get("ordered_steps", []),
        "report_sections": report_spec.get("required_sections", []),
        "coverage": coverage,
        "acceptance_criteria": acceptance_criteria,
        "source_documents": {
            "trial_outline": outline.get("source_uri", "samples/docs/trial_outline.json"),
            "process_manual": manual.get("source_uri", "samples/docs/process_manual.json"),
            "report_specification": report_spec.get("source_uri", "samples/docs/report_spec.json"),
        },
    }
    model["model_checksum"] = sha256_bytes(canonical_json(model))
    return model


def compare_against_criterion(value: float, criterion: dict[str, Any]) -> bool:
    operator = criterion.get("operator")
    if operator == "gte":
        return value >= float(criterion["value"])
    if operator == "lte":
        return value <= float(criterion["value"])
    if operator == "between":
        return float(criterion["lower"]) <= value <= float(criterion["upper"])
    raise ValueError(f"unsupported acceptance criterion operator: {operator}")


def format_criterion_expectation(criterion: dict[str, Any]) -> str:
    operator = criterion.get("operator")
    expected = criterion.get("expected", criterion)
    if operator == "gte":
        return f">= {expected['value']}"
    if operator == "lte":
        return f"<= {expected['value']}"
    if operator == "between":
        return f"{expected['lower']} ~ {expected['upper']}"
    return str(criterion)


def evaluate_acceptance_criteria(criteria: list[dict[str, Any]], rows: list[dict[str, Any]]) -> list[dict[str, Any]]:
    rows_by_signal: dict[str, list[dict[str, Any]]] = {}
    for row in rows:
        rows_by_signal.setdefault(row["signal_id"], []).append(row)

    results: list[dict[str, Any]] = []
    for criterion in criteria:
        criterion_id = criterion["id"]
        signal_id = criterion["signal_id"]
        metric = criterion["metric"]
        signal_rows = rows_by_signal.get(signal_id, [])
        base_result = {
            "criterion_id": criterion_id,
            "requirement_id": criterion["requirement_id"],
            "description": criterion.get("description", criterion_id),
            "signal_id": signal_id,
            "metric": metric,
            "unit": criterion.get("unit"),
            "operator": criterion.get("operator"),
            "expected": {
                key: criterion[key]
                for key in ("value", "lower", "upper")
                if key in criterion
            },
            "evidence_refs": ["raw-data.xlsx", "compliance-model.json"],
        }
        if not signal_rows:
            results.append(
                {
                    **base_result,
                    "status": "needs_human_review",
                    "observed": None,
                    "evaluated_rows": 0,
                    "findings": [f"原始数据中缺少信号 {signal_id}"],
                }
            )
            continue

        evaluated_rows = []
        values: list[float] = []
        for row in signal_rows:
            value = float(row[metric])
            passed = compare_against_criterion(value, criterion)
            values.append(value)
            evaluated_rows.append(
                {
                    "ecu_id": row["ecu_id"],
                    "value": value,
                    "pass": passed,
                }
            )

        failures = [item for item in evaluated_rows if not item["pass"]]
        results.append(
            {
                **base_result,
                "status": "pass" if not failures else "fail",
                "observed": {
                    "min": min(values),
                    "max": max(values),
                    "average": round(sum(values) / len(values), 3),
                },
                "evaluated_rows": len(evaluated_rows),
                "row_results": evaluated_rows,
                "findings": [
                    f"{item['ecu_id']} 的 {metric}={item['value']} 不满足 {format_criterion_expectation(criterion)}"
                    for item in failures
                ],
            }
        )
    return results


def summarize_data_verdict(checks: list[dict[str, Any]]) -> str:
    if not checks or any(check["status"] == "needs_human_review" for check in checks):
        return "needs_human_review"
    if any(check["status"] == "fail" for check in checks):
        return "fail"
    return "pass"


def build_report_lines(
    config: dict[str, Any],
    preflight: list[dict[str, Any]],
    model: dict[str, Any],
    rows: list[dict[str, Any]],
    verdict: str,
    data_verdict: str,
    data_checks: list[dict[str, Any]],
) -> list[tuple[str, str | None]]:
    environment = model.get("environment", {})
    lines: list[tuple[str, str | None]] = [
        ("车载冷藏设备 ECU 环境测试报告（模拟演示）", "Title"),
        ("本报告使用虚构 A2L、模拟 CAN/MDA 数据和脱敏试验文档生成，不代表真实企业测试结果。", None),
        ("1. 测试目的", "Heading1"),
        ("验证温度冲击、高温高湿/结露等环境条件下的 ECU 信号采集、统计和报告追溯流程。", None),
        ("2. 测试环境", "Heading1"),
        (f"低温 {environment.get('low_temperature_c', 'N/A')} °C；高温 {environment.get('high_temperature_c', 'N/A')} °C；湿度 {environment.get('humidity_percent', 'N/A')} %RH。", None),
        ("数据链路：模拟 A2L → MDA 回放适配器 → 原始 Excel → 合规模型 → 正式报告。", None),
        ("3. A2L 文件预筛选", "Heading1"),
        (f"扫描 {len(preflight)} 个文件，接受 {sum(item['status'] == 'accepted' for item in preflight)} 个，拒绝 {sum(item['status'] == 'rejected' for item in preflight)} 个。", None),
        ("拒绝文件不进入测量链路，并保留原因供人工复核。", None),
        ("4. 原始数据摘要", "Heading1"),
        (f"覆盖 {len({row['signal_id'] for row in rows})} 个信号、{len({row['ecu_id'] for row in rows})} 个模拟 ECU，原始数据见 raw-data.xlsx。", None),
        ("统计字段：最小值、最大值、平均值、样本数。", None),
        ("5. 原始数据合规判定", "Heading1"),
        (f"按试验大纲的 {len(data_checks)} 条判定标准逐项核验：{data_verdict}。", None),
    ]
    for check in data_checks:
        observed = check.get("observed") or {}
        observed_range = "N/A" if not observed else f"{observed['min']} ~ {observed['max']}"
        lines.append(
            (
                f"{check['criterion_id']}：{check['status']}；{check['signal_id']}.{check['metric']} 观测范围 {observed_range}；标准 {format_criterion_expectation(check)}。",
                None,
            )
        )
    lines.extend(
        [
        ("6. 合规覆盖", "Heading1"),
    ]
    )
    for item in model["coverage"]:
        lines.append((f"{item['requirement_id']}：{item['status']}；证据：{', '.join(item['evidence_refs'])}", None))
    lines.extend(
        [
            ("7. 验证结论", "Heading1"),
            (f"自动校验结果：{verdict}。", None),
            (f"原始数据按试验大纲判定：{data_verdict}；该结果仅针对合成输入。", None),
            ("若存在无法打开的 A2L 文件，最终结论必须保留人工复核状态。", None),
            ("8. 证据索引", "Heading1"),
            ("a2l-preflight.json；raw-data.xlsx；compliance-model.json；validation-result.json。", None),
        ]
    )
    return lines


def call_deepseek_review(
    config: dict[str, Any],
    preflight: dict[str, Any],
    compliance_model: dict[str, Any],
    validation: dict[str, Any],
) -> dict[str, Any]:
    llm_config = config.get("llm", {})
    api_key_env = llm_config.get("api_key_env", "DEEPSEEK_API_KEY")
    api_key = os.environ.get(api_key_env, "").strip()
    if not api_key:
        raise RuntimeError(f"未找到 {api_key_env}；请在当前终端临时配置 API Key，不要写入仓库文件")

    base_url = str(llm_config.get("base_url", "https://api.deepseek.com/v1")).rstrip("/")
    model_name = str(llm_config.get("model", "deepseek-v4-flash"))
    endpoint = f"{base_url}/chat/completions"
    prompt = {
        "task": "审查一条车载冷藏设备 ECU 环境测试模拟链路",
        "constraints": [
            "输入全部为合成数据，不得声称是真实车辆或企业测试",
            "不可解析的 A2L 必须保留人工复核意见",
            "不要修改统计值，不要补造缺失证据",
            "raw-data.xlsx 已由本地流程生成；不得声称该文件缺失",
        ],
        "preflight": preflight,
        "compliance_model": compliance_model,
        "validation": validation,
        "required_output": [
            "用简短中文总结可用输入和异常输入",
            "指出报告中必须人工确认的事项",
            "指出下一步接入真实 MDA/Excel 环境时的一个风险",
        ],
    }
    request_body = {
        "model": model_name,
        "messages": [
            {
                "role": "system",
                "content": "你是测试报告复核助手，只能基于输入事实进行审查，输出中文，不得生成企业机密或虚构实验结论。",
            },
            {"role": "user", "content": json.dumps(prompt, ensure_ascii=False)},
        ],
        "temperature": 0.1,
        "max_tokens": 1200,
        "stream": False,
        "thinking": {"type": "disabled"},
    }
    request = urllib.request.Request(
        endpoint,
        data=json.dumps(request_body, ensure_ascii=False).encode("utf-8"),
        headers={"Authorization": f"Bearer {api_key}", "Content-Type": "application/json"},
        method="POST",
    )
    try:
        with urllib.request.urlopen(request, timeout=60) as response:
            payload = json.loads(response.read().decode("utf-8"))
    except urllib.error.HTTPError as error:
        detail = error.read().decode("utf-8", errors="replace")[:1000]
        raise RuntimeError(f"DeepSeek 返回 HTTP {error.code}: {detail}") from error
    except urllib.error.URLError as error:
        raise RuntimeError(f"DeepSeek 网络请求失败: {error.reason}") from error

    choices = payload.get("choices", [])
    content = choices[0].get("message", {}).get("content", "") if choices else ""
    if not content:
        raise RuntimeError("DeepSeek 返回中没有可用的复核文本")
    return {
        "provider": "deepseek",
        "model": model_name,
        "endpoint": endpoint,
        "synthetic_input": True,
        "status": "success",
        "review": content,
        "usage": payload.get("usage"),
    }


def run_demo(config_path: Path, output_override: Path | None = None, llm_mode: bool = False) -> dict[str, Any]:
    config = read_json(config_path)
    output_dir = output_override or resolve_package_path(config.get("output_dir", "outputs"))
    output_dir.mkdir(parents=True, exist_ok=True)

    a2l_results, selected = preflight_a2l(resolve_package_path(config["a2l_dir"]))
    preflight_payload = {
        "agent": "data-processing-expert",
        "purpose": "筛选可解析的 A2L 文件并保留失败原因",
        "selected_file": selected["file"],
        "results": a2l_results,
        "human_review_required": any(item["status"] == "rejected" for item in a2l_results),
    }
    write_json(output_dir / "a2l-preflight.json", preflight_payload)

    seeds = read_seed_trace(resolve_package_path(config["can_trace"]))
    selected_signals = selected["signals"]
    capture_id, rows = capture_measurements(
        selected_signals,
        config["ecu_ids"],
        seeds,
        int(config.get("sample_count", 5)),
    )
    raw_rows = [
        ["ECU", "Signal", "Unit", "Samples", "Min", "Max", "Average", "Source"]
    ]
    raw_rows.extend(
        [
            [
                row["ecu_id"],
                row["signal_id"],
                row["unit"],
                row["sample_count"],
                row["min"],
                row["max"],
                row["average"],
                row["source"],
            ]
            for row in rows
        ]
    )
    write_xlsx(
        output_dir / "raw-data.xlsx",
        {
            "Summary": [
                ["Artifact", "Value"],
                ["Capture ID", capture_id],
                ["Data mode", "synthetic_mda_replay"],
                ["A2L source", selected["file"]],
                ["ECU count", len(config["ecu_ids"])],
                ["Signal count", len(selected_signals)],
            ],
            "RawData": raw_rows,
        },
    )

    outline = read_json(resolve_package_path(config["trial_outline"]))
    manual = read_json(resolve_package_path(config["process_manual"]))
    report_spec = read_json(resolve_package_path(config["report_specification"]))
    model = build_compliance_model(outline, manual, report_spec)
    write_json(output_dir / "compliance-model.json", model)

    data_checks = evaluate_acceptance_criteria(model.get("acceptance_criteria", []), rows)
    data_verdict = summarize_data_verdict(data_checks)
    if data_verdict == "fail":
        verdict = "needs_human_review" if preflight_payload["human_review_required"] else "fail"
    elif data_verdict == "needs_human_review" or preflight_payload["human_review_required"]:
        verdict = "needs_human_review"
    else:
        verdict = "pass"
    section_checks = [
        {
            "section_id": section,
            "status": (
                data_verdict
                if section in {"原始数据摘要", "原始数据合规判定", "合规覆盖"}
                else "needs_human_review"
                if section == "A2L 文件预筛选" and preflight_payload["human_review_required"]
                else verdict
                if section == "验证结论"
                else "pass"
            ),
            "evidence_refs": ["raw-data.xlsx", "compliance-model.json"],
            "findings": (
                [finding for check in data_checks for finding in check["findings"]]
                if section in {"原始数据摘要", "原始数据合规判定", "合规覆盖"} and data_verdict != "pass"
                else ["存在被 A2L 预筛选拒绝的文件"]
                if section == "A2L 文件预筛选" and preflight_payload["human_review_required"]
                else []
            ),
        }
        for section in report_spec.get("required_sections", [])
    ]
    validation = {
        "verdict": verdict,
        "data_verdict": data_verdict,
        "acceptance_criteria_checks": data_checks,
        "section_checks": section_checks,
        "traceability": {
            "a2l_preflight_checksum": sha256_file(output_dir / "a2l-preflight.json"),
            "raw_data_checksum": sha256_file(output_dir / "raw-data.xlsx"),
            "compliance_model_checksum": model["model_checksum"],
            "evidence_complete": True,
        },
        "human_review_reason": "A2L 文件可用性存在拒绝项" if verdict == "needs_human_review" else None,
    }
    write_json(output_dir / "validation-result.json", validation)

    report_lines = build_report_lines(config, a2l_results, model, rows, verdict, data_verdict, data_checks)
    markdown_report = "\n".join(
        [f"# {text}" if style == "Title" else f"## {text}" if style else text for text, style in report_lines]
    ) + "\n"
    (output_dir / "trial-report.md").write_text(markdown_report, encoding="utf-8")
    write_docx(output_dir / "trial-report.docx", report_lines)

    llm_review = None
    if llm_mode:
        llm_review = call_deepseek_review(config, preflight_payload, model, validation)
        write_json(output_dir / "llm-review.json", llm_review)

    evidence = {
        "demo": "goai-ecu-environmental-test",
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "synthetic_data": True,
        "llm_mode": llm_mode,
        "agent_trace": [
            {"agent": "goai-manager", "action": "decompose_task", "status": "complete"},
            {"agent": "data-processing-expert", "action": "a2l_preflight_and_mda_replay", "status": "complete"},
            {"agent": "compliance-process-agent", "action": "parse_outline_and_manual", "status": "complete"},
            {"agent": "report-agent", "action": "build_and_validate_report", "status": "complete"},
            {"agent": "human-operator", "action": "review_required_for_rejected_a2l", "status": "pending" if verdict == "needs_human_review" else "not_required"},
        ],
        "artifacts": {
            name: {"path": name, "sha256": sha256_file(output_dir / name)}
            for name in (
                "a2l-preflight.json",
                "raw-data.xlsx",
                "compliance-model.json",
                "trial-report.docx",
                "trial-report.md",
                "validation-result.json",
            )
        },
        "validation_verdict": verdict,
        "data_verdict": data_verdict,
        "acceptance_criteria": {
            "total": len(data_checks),
            "passed": sum(check["status"] == "pass" for check in data_checks),
            "failed": sum(check["status"] == "fail" for check in data_checks),
            "needs_human_review": sum(check["status"] == "needs_human_review" for check in data_checks),
        },
    }
    if llm_review is not None:
        evidence["artifacts"]["llm-review.json"] = {
            "path": "llm-review.json",
            "sha256": sha256_file(output_dir / "llm-review.json"),
        }
        evidence["llm_review"] = {"status": "success", "artifact": "llm-review.json"}
    write_json(output_dir / "run-evidence.json", evidence)
    return {
        "output_dir": str(output_dir),
        "selected_a2l": selected["file"],
        "accepted_a2l": sum(item["status"] == "accepted" for item in a2l_results),
        "rejected_a2l": sum(item["status"] == "rejected" for item in a2l_results),
        "ecu_count": len(config["ecu_ids"]),
        "signal_count": len(selected_signals),
        "capture_id": capture_id,
        "verdict": verdict,
        "data_verdict": data_verdict,
        "acceptance_criteria_passed": sum(check["status"] == "pass" for check in data_checks),
        "acceptance_criteria_total": len(data_checks),
        "llm_mode": llm_mode,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description="Run the synthetic GOAI AgentTeams ECU test demo")
    parser.add_argument("--config", default=str(PACKAGE_ROOT / "config" / "demo-config.json"))
    parser.add_argument("--output", default=None, help="Optional output directory")
    parser.add_argument("--llm", action="store_true", help="Use DeepSeek to review synthetic intermediate results")
    args = parser.parse_args()
    try:
        result = run_demo(
            Path(args.config).resolve(),
            Path(args.output).resolve() if args.output else None,
            llm_mode=args.llm,
        )
    except Exception as error:
        print(f"demo failed: {error}", file=sys.stderr)
        return 1
    print(json.dumps(result, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
