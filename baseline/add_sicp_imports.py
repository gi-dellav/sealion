#!/usr/bin/env python3
from __future__ import annotations

import re
import sys
from pathlib import Path

SKIP_FILES = {"sicp.js", "rune.js"}
IDENT = re.compile(r"[A-Za-z_$][A-Za-z0-9_$]*")


def exported_names(sicp_path: Path) -> list[str]:
    source = sicp_path.read_text(encoding="utf-8")
    names: list[str] = []
    for block in re.findall(r"export\s*\{([^}]*)\}", source, re.S):
        for specifier in block.split(","):
            specifier = specifier.strip()
            if not specifier:
                continue
            match = re.search(r"\bas\s+([A-Za-z_$][A-Za-z0-9_$]*)$", specifier)
            if match:
                names.append(match.group(1))
            else:
                names.extend(IDENT.findall(specifier))
    for match in re.finditer(
        r"export\s+(?:async\s+)?(?:function|const|let|var|class)\s+"
        r"([A-Za-z_$][A-Za-z0-9_$]*)",
        source,
    ):
        names.append(match.group(1))
    return sorted(set(names))


def skip_comment(source: str, i: int) -> int:
    if source.startswith("//", i):
        end = source.find("\n", i + 2)
        return len(source) if end < 0 else end
    end = source.find("*/", i + 2)
    return len(source) if end < 0 else end + 2


def skip_quoted(source: str, i: int, quote: str) -> int:
    i += 1
    while i < len(source):
        if source[i] == "\\":
            i += 2
        elif source[i] == quote:
            return i + 1
        else:
            i += 1
    return i


def top_level_declarations(source: str) -> set[str]:
    names: set[str] = set()
    depth = 0
    i = 0
    length = len(source)
    while i < length:
        char = source[i]
        if source.startswith("//", i) or source.startswith("/*", i):
            i = skip_comment(source, i)
        elif char in "\"'`":
            i = skip_quoted(source, i, char)
        elif char in "([{":
            depth += 1
            i += 1
        elif char in ")]}":
            depth -= 1
            i += 1
        elif depth == 0 and (char.isalpha() or char in "_$"):
            match = IDENT.match(source, i)
            if not match:
                i += 1
                continue
            word = match.group(0)
            j = match.end()
            if word in {"function", "const", "let", "var", "class"}:
                while j < length and source[j].isspace():
                    j += 1
                if word == "function" and j < length and source[j] == "*":
                    j += 1
                    while j < length and source[j].isspace():
                        j += 1
                declared = IDENT.match(source, j)
                if declared:
                    names.add(declared.group(0))
            elif word == "async":
                k = j
                while k < length and source[k].isspace():
                    k += 1
                if source.startswith("function", k):
                    k += len("function")
                    while k < length and source[k].isspace():
                        k += 1
                    declared = IDENT.match(source, k)
                    if declared:
                        names.add(declared.group(0))
            i = j
        else:
            i += 1
    return names


def imported_names(source: str) -> set[str]:
    names: set[str] = set()
    pattern = re.compile(
        r"import\s+(?:\{([^}]*)\}|\*\s+as\s+([A-Za-z_$][A-Za-z0-9_$]*)|"
        r"([A-Za-z_$][A-Za-z0-9_$]*))\s+from\s+['\"][^'\"]+['\"]",
        re.S,
    )
    for match in pattern.finditer(source):
        if match.group(1):
            for specifier in match.group(1).split(","):
                specifier = specifier.strip()
                if not specifier:
                    continue
                parts = specifier.split()
                names.add(parts[-1])
        elif match.group(2):
            names.add(match.group(2))
        elif match.group(3):
            names.add(match.group(3))
    return names


def remove_sicp_import(source: str) -> str:
    return re.sub(
        r"^[ \t]*import\s+\{[^}]*\}\s+from\s+['\"]\./sicp\.js['\"];?[ \t]*\r?\n?",
        "",
        source,
        flags=re.M | re.S,
    )


def add_import(path: Path, names: list[str]) -> bool:
    source = path.read_text(encoding="utf-8")
    stripped = remove_sicp_import(source)
    declared = top_level_declarations(stripped) | imported_names(stripped)
    referenced = [
        name for name in names
        if name not in declared and name in stripped
    ]
    line = (
        "import { " + ", ".join(referenced) + " } from './sicp.js';\n"
        if referenced
        else ""
    )
    result = line + stripped
    if result == source:
        return False
    path.write_text(result, encoding="utf-8")
    return True


def main() -> int:
    directory = Path(sys.argv[1]) if len(sys.argv) > 1 else Path(__file__).resolve().parent
    sicp_path = directory / "sicp.js"
    if not sicp_path.is_file():
        print(f"sicp.js not found in {directory}", file=sys.stderr)
        return 1
    names = exported_names(sicp_path)
    changed = 0
    for path in sorted(directory.glob("*.js")):
        if path.name in SKIP_FILES or path == sicp_path:
            continue
        if add_import(path, names):
            changed += 1
    print(f"Updated {changed} file(s) in {directory}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
