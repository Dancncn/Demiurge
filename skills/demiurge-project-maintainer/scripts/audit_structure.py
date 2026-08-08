#!/usr/bin/env python3
"""检查 Demiurge 模块结构中稳定且可确定验证的部分。"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path


EXPECTED_DIRS = (
    "frontend/src/app",
    "frontend/src/features",
    "frontend/src/shared/components",
    "frontend/src/lib",
    "backend/Demiurge-common/src",
    "backend/Demiurge-core/src",
    "backend/Demiurge-framework/src",
    "backend/Demiurge-desktop/src/controller",
    "backend/Demiurge-desktop/src/biz",
    "backend/Demiurge-desktop/src/starter",
)

CONTROLLER_FORBIDDEN = (
    ".sessions.lock(",
    ".settings.lock(",
    ".http.",
    "persist_sessions(",
    "persist_settings(",
    "reqwest::",
    "std::fs::",
    "tokio::process",
)


def read(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def check(root: Path) -> list[str]:
    failures: list[str] = []

    for relative in EXPECTED_DIRS:
        path = root / relative
        if not path.is_dir():
            failures.append(f"缺少必需目录：{relative}")

    legacy_components = root / "frontend/src/components"
    if legacy_components.exists():
        failures.append("不应继续存在旧的 frontend/src/components 混合目录")

    lib_rs = root / "backend/Demiurge-desktop/src/lib.rs"
    if lib_rs.is_file():
        source = read(lib_rs)
        if len(source.splitlines()) > 40:
            failures.append("backend/Demiurge-desktop/src/lib.rs 超过 40 行")
        if re.search(r"(?m)^\s*#\[tauri::command\]\s*$", source):
            failures.append("backend/Demiurge-desktop/src/lib.rs 中不应定义 Tauri 命令")
    else:
        failures.append("缺少 backend/Demiurge-desktop/src/lib.rs")

    controller_dir = root / "backend/Demiurge-desktop/src/controller"
    command_count = 0
    if controller_dir.is_dir():
        for path in sorted(controller_dir.glob("*.rs")):
            source = read(path)
            command_count += len(
                re.findall(r"(?m)^\s*#\[tauri::command\]\s*$", source)
            )
            for token in CONTROLLER_FORBIDDEN:
                if token in source:
                    failures.append(
                        f"{path.relative_to(root)} 使用 {token!r} 绕过了 Biz 层"
                    )
        if command_count == 0:
            failures.append("Controller 模块中没有找到 Tauri 命令")

    for path in (root / "backend/Demiurge-desktop/src").glob("*.rs"):
        source = read(path)
        if re.search(r"(?m)^\s*#\[tauri::command\]\s*$", source):
            failures.append(
                f"Controller 之外发现 Tauri 命令：{path.relative_to(root)}"
            )

    return failures


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "root",
        nargs="?",
        default=".",
        help="Demiurge 仓库根目录（默认：当前目录）",
    )
    args = parser.parse_args()
    root = Path(args.root).resolve()

    failures = check(root)
    if failures:
        print("Demiurge 结构检查失败：")
        for failure in failures:
            print(f"  - {failure}")
        return 1

    print("Demiurge 结构检查通过。")
    return 0


if __name__ == "__main__":
    sys.exit(main())
