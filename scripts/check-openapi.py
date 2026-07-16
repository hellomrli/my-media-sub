#!/usr/bin/env python3
"""Keep the Axum literal route surface and OpenAPI operations synchronized.

The scanner intentionally accepts only literal `.route("/path", method(...))` calls. A
non-literal management route must be added to this parser or represented by a literal
contract entry; silently skipping it is not allowed by the resulting source/spec diff.
"""
from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
API_ROOT = ROOT / "src" / "api"
SPEC_PATH = ROOT / "static" / "openapi.json"
BASELINE_PATH = ROOT / "docs" / "openapi-baseline-v1.12.0.json"
HTTP_METHODS = ("get", "post", "put", "patch", "delete", "head", "options")
EXCLUDED_ROUTES = {("/api/{*path}", "any")}
# STRM is intentionally retired in v2.2.0 and will return as an independent
# module. Keep the historical baseline for all other operations.
INTENTIONALLY_REMOVED_ROUTES = {
    ("/api/subscriptions/{id}/strm", "post"),
    ("/api/subscriptions/{id}/strm/audit", "get"),
    ("/strm/quark/{fid}/{file_name}", "get"),
}


def route_calls(text: str):
    position = 0
    marker = ".route("
    while True:
        start = text.find(marker, position)
        if start < 0:
            return
        index = start + len(marker)
        depth = 1
        quote = None
        escaped = False
        while index < len(text) and depth:
            char = text[index]
            if quote:
                if escaped:
                    escaped = False
                elif char == "\\":
                    escaped = True
                elif char == quote:
                    quote = None
            elif char in "'\"":
                quote = char
            elif char == "(":
                depth += 1
            elif char == ")":
                depth -= 1
            index += 1
        if depth:
            raise ValueError("unclosed .route( call")
        yield text[start + len(marker) : index - 1]
        position = index


def split_first_argument(body: str):
    depth = 0
    quote = None
    escaped = False
    for index, char in enumerate(body):
        if quote:
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == quote:
                quote = None
        elif char in "'\"":
            quote = char
        elif char in "([{":
            depth += 1
        elif char in ")]}":
            depth -= 1
        elif char == "," and depth == 0:
            return body[:index], body[index + 1 :]
    return body, ""


def scan_routes() -> dict[str, set[str]]:
    routes: dict[str, set[str]] = {}
    for source in sorted(API_ROOT.rglob("*.rs")):
        for body in route_calls(source.read_text()):
            path_arg, handler = split_first_argument(body)
            literal = re.fullmatch(r'\s*"([^"]+)"\s*', path_arg)
            if not literal:
                raise ValueError(f"non-literal Axum route in {source}: {path_arg.strip()}")
            path = literal.group(1)
            methods = {
                method
                for method in HTTP_METHODS
                if re.search(rf"(?<![A-Za-z_]){method}\s*\(", handler)
                or re.search(rf"\.{method}\s*\(", handler)
            }
            if re.search(r"(?<![A-Za-z_])any\s*\(", handler):
                methods.add("any")
            if not methods:
                raise ValueError(f"route has no recognized HTTP method in {source}: {path}")
            if "any" in methods and (path, "any") not in EXCLUDED_ROUTES:
                raise ValueError(f"document explicit methods instead of any() in {source}: {path}")
            for method in methods:
                if (path, method) not in EXCLUDED_ROUTES:
                    routes.setdefault(path, set()).add(method)
    return routes


def spec_operations(spec: dict) -> dict[str, set[str]]:
    return {
        path: {key for key in item if key in HTTP_METHODS}
        for path, item in spec.get("paths", {}).items()
    }


def generic_operation(path: str, method: str) -> dict:
    words = path.strip("/").replace("api/", "").replace("-", " ") or "root"
    operation = {
        "summary": f"{method.upper()} {words}",
        "responses": {"200": {"description": "Successful response"}},
    }
    params = re.findall(r"\{([^}]+)\}", path)
    if params:
        operation["parameters"] = [
            {
                "name": name,
                "in": "path",
                "required": True,
                "schema": {"type": "string"},
            }
            for name in params
            if not name.startswith("*")
        ]
    if path == "/health":
        operation["security"] = []
    elif path.startswith("/strm/"):
        operation["security"] = [{"strmToken": []}]
    return operation


def ensure_components(spec: dict) -> None:
    components = spec.setdefault("components", {})
    schemes = components.setdefault("securitySchemes", {})
    schemes.setdefault("basicAuth", {"type": "http", "scheme": "basic"})
    schemes.setdefault(
        "strmToken",
        {"type": "apiKey", "in": "query", "name": "token", "description": "STRM access token"},
    )
    schemas = components.setdefault("schemas", {})
    schemas["Error"] = {
        "type": "object",
        "required": ["ok", "error", "message"],
        "properties": {
            "ok": {"const": False},
            "error": {"type": "string"},
            "message": {"type": "string"},
        },
    }
    schemas.setdefault(
        "Success",
        {
            "type": "object",
            "required": ["ok", "data"],
            "properties": {
                "ok": {"const": True},
                "data": {},
                "message": {"type": "string"},
            },
        },
    )
    responses = components.setdefault("responses", {})
    error_content = {
        "application/json": {"schema": {"$ref": "#/components/schemas/Error"}}
    }
    responses["BadRequest"] = {
        "description": "Invalid request",
        "content": error_content,
    }
    responses["Unauthorized"] = {
        "description": "Authentication required",
        "content": error_content,
    }
    responses["InternalError"] = {
        "description": "Internal server error",
        "content": error_content,
    }


def update_spec(spec: dict, routes: dict[str, set[str]]) -> None:
    ensure_components(spec)
    paths = spec.setdefault("paths", {})
    for path in list(paths):
        for method in HTTP_METHODS:
            if method in paths[path] and method not in routes.get(path, set()):
                del paths[path][method]
        if not any(method in paths[path] for method in HTTP_METHODS):
            del paths[path]
    for path, methods in sorted(routes.items()):
        item = paths.setdefault(path, {})
        for method in sorted(methods):
            operation = item.setdefault(method, generic_operation(path, method))
            expected_params = re.findall(r"\{([^}]+)\}", path)
            declared = {
                parameter.get("name")
                for parameter in operation.get("parameters", [])
                if parameter.get("in") == "path"
            }
            for name in expected_params:
                if not name.startswith("*") and name not in declared:
                    operation.setdefault("parameters", []).append(
                        {"name": name, "in": "path", "required": True, "schema": {"type": "string"}}
                    )
            responses = operation.setdefault("responses", {})
            responses.setdefault("400", {"$ref": "#/components/responses/BadRequest"})
            if path != "/health":
                responses.setdefault("401", {"$ref": "#/components/responses/Unauthorized"})
            responses.setdefault("500", {"$ref": "#/components/responses/InternalError"})
            if path == "/health":
                operation["security"] = []
            elif path.startswith("/strm/"):
                operation["security"] = [{"strmToken": []}]
    spec["paths"] = {path: paths[path] for path in sorted(paths)}


def surface(spec: dict) -> dict:
    return {
        "openapi": spec.get("openapi"),
        "version": spec.get("info", {}).get("version"),
        "operations": {
            path: sorted(methods)
            for path, methods in sorted(spec_operations(spec).items())
        },
        "error_schema": spec.get("components", {}).get("schemas", {}).get("Error"),
        "success_schema": spec.get("components", {}).get("schemas", {}).get("Success"),
    }


def check(spec: dict, routes: dict[str, set[str]], baseline: dict | None) -> list[str]:
    errors = []
    operations = spec_operations(spec)
    for path in sorted(set(routes) | set(operations)):
        missing = routes.get(path, set()) - operations.get(path, set())
        extra = operations.get(path, set()) - routes.get(path, set())
        if missing:
            errors.append(f"OpenAPI missing {path}: {', '.join(sorted(missing))}")
        if extra:
            errors.append(f"OpenAPI has unregistered operation {path}: {', '.join(sorted(extra))}")
    for path, methods in operations.items():
        for method in methods:
            operation = spec["paths"][path][method]
            if not operation.get("responses"):
                errors.append(f"{method.upper()} {path} has no responses")
            expected = {name for name in re.findall(r"\{([^}]+)\}", path) if not name.startswith("*")}
            declared = {
                parameter.get("name")
                for parameter in operation.get("parameters", [])
                if parameter.get("in") == "path" and parameter.get("required") is True
            }
            if expected != declared:
                errors.append(f"{method.upper()} {path} path parameters differ: expected {expected}, got {declared}")
    error_schema = spec.get("components", {}).get("schemas", {}).get("Error", {})
    if set(error_schema.get("required", [])) != {"ok", "error", "message"}:
        errors.append("Error schema must require ok, error and message")
    if baseline:
        current = surface(spec)
        for path, methods in baseline.get("operations", {}).items():
            removed = set(methods) - set(current["operations"].get(path, []))
            removed -= {
                method
                for method in removed
                if (path, method) in INTENTIONALLY_REMOVED_ROUTES
            }
            if removed:
                errors.append(f"breaking change removed {path}: {', '.join(sorted(removed))}")
        if baseline.get("error_schema") != current.get("error_schema"):
            errors.append("breaking change modified the stable Error schema")
        if baseline.get("success_schema") != current.get("success_schema"):
            errors.append("breaking change modified the stable Success schema")
    return errors


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--update", action="store_true")
    parser.add_argument("--write-baseline", action="store_true")
    args = parser.parse_args()
    spec = json.loads(SPEC_PATH.read_text())
    routes = scan_routes()
    if args.update:
        update_spec(spec, routes)
        SPEC_PATH.write_text(json.dumps(spec, ensure_ascii=False, separators=(",", ":")) + "\n")
    if args.write_baseline:
        BASELINE_PATH.write_text(json.dumps(surface(spec), ensure_ascii=False, indent=2) + "\n")
    baseline = json.loads(BASELINE_PATH.read_text()) if BASELINE_PATH.exists() and not args.write_baseline else None
    errors = check(spec, routes, baseline)
    if errors:
        print("OpenAPI contract check failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1
    operation_count = sum(len(methods) for methods in routes.values())
    print(f"OpenAPI contract check passed: {len(routes)} paths, {operation_count} operations")
    return 0

if __name__ == "__main__":
    raise SystemExit(main())
