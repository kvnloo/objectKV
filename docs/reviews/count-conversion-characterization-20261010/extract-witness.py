#!/usr/bin/env python3
"""Extract the pinned, unchanged numeric helper into a standalone Rust test."""

import argparse
import hashlib
import json
from pathlib import Path
import re


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    arguments = parser.parse_args()
    here = Path(__file__).resolve().parent
    root = here.parents[2]
    source = (root / "crates/okv-eval/src/main.rs").read_bytes()
    expected_source = "cd23e358bdd27fdf0c9b6299faec004d9665b490cd0cc9268ac8d8f37f58e7c2"
    if sha256(source) != expected_source:
        raise SystemExit("Source changed; this pinned witness refuses a different main.rs")
    pattern = rb"^fn bounded_count\(value: u64\) -> f64 \{\n    f64::from\(u32::try_from\(value\).unwrap_or\(u32::MAX\)\)\n\}"
    matches = list(re.finditer(pattern, source, re.M))
    if len(matches) != 1:
        raise SystemExit("Expected exactly one byte-identical bounded_count body")
    helper = matches[0].group(0) + b"\n"
    oracle_bytes = (here / "oracle.json").read_bytes()
    if sha256(oracle_bytes) != "6803ea237b39c318390d75bf1bc6ace5e63dabad15bd07548b4fa627a49bdfe0":
        raise SystemExit("Frozen portable oracle changed")
    oracle = json.loads(oracle_bytes)
    tests = "\n#[cfg(test)]\nmod tests {\n    use super::bounded_count;\n\n"
    for fixture in oracle["fixtures"]:
        name = fixture["name"]
        value = fixture["input_u64"]
        expected = fixture["expected_f64_decimal"]
        if not re.fullmatch(r"[a-z][a-z0-9_]*", name):
            raise SystemExit("Invalid fixture name")
        if not isinstance(value, int) or not 0 <= value <= 9007199254740992:
            raise SystemExit("Fixture is outside the exact binary64 integer range")
        if expected != f"{value}.0":
            raise SystemExit("Fixture expectation must equal its exact integer")
        tests += (
            f"    #[test]\n    fn {name}() {{\n"
            f"        let expected: f64 = {expected};\n"
            f"        assert_eq!(bounded_count({value}_u64).to_bits(), expected.to_bits(), "
            f'"input={value}; reporting should preserve this exactly representable integer");\n'
            "    }\n\n"
        )
    tests += "}\n"
    harness = helper + tests.encode()
    if sha256(helper) != "5bedb4287f0acfe8850a8f5e1cd6e56735f501ebeea4726443d1d2621043d278":
        raise SystemExit("Extracted helper changed")
    if sha256(harness) != "5c803b9e505e70ce9dd96dc178041d75f33880867290502dc7f605bc0e8c21aa":
        raise SystemExit("Frozen harness changed")
    arguments.output.mkdir(parents=True, exist_ok=False)
    for name, data in [("bounded_count.extracted.rs", helper), ("witness.rs", harness)]:
        with (arguments.output / name).open("xb") as output:
            output.write(data)
    identity = {
        "witness_kind": "Native exact-source-extracted helper numeric characterization",
        "pinned_source_commit": "4d4fc69ca7dea24acdbc9da8997b07498e1d02f5",
        "source_git_blob": "38e9c9bc380454a6007613c64ee1be71ff6e1c6a",
        "source_sha256": sha256(source),
        "extracted_helper_sha256": sha256(helper),
        "harness_sha256": sha256(harness),
        "oracle_sha256": sha256(oracle_bytes),
        "production_crate_built": False,
        "workload_executed": False,
        "production_source_modified": False,
        "expected_native_results": {"passed": 3, "failed": 5},
    }
    with (arguments.output / "identity.json").open("x") as output:
        json.dump(identity, output, indent=2)
        output.write("\n")
    print(json.dumps(identity, indent=2))


if __name__ == "__main__":
    main()
