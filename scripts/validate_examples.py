# /// script
# requires-python = ">=3.11"
# dependencies = ["jsonschema==4.23.0"]
# ///
"""Validate checked-in JSON schemas and examples without publishing anything."""

import argparse
import json
from pathlib import Path
import tomllib

from jsonschema.validators import validator_for


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--config-only", action="store_true")
    args = parser.parse_args()
    examples = Path(__file__).resolve().parent.parent / "examples"
    schema_paths = (
        [examples / "config.schema.json"]
        if args.config_only
        else sorted(examples.glob("*.schema.json"))
    )
    validators = {}
    for path in schema_paths:
        schema = json.loads(path.read_text())
        validator_type = validator_for(schema)
        validator_type.check_schema(schema)
        validators[path.name] = validator_type(schema)
        print(f"Valid schema: {path.name}")

    config = tomllib.loads((examples / "config.toml").read_text())
    validators["config.schema.json"].validate(config)
    print("Valid example: config.toml")
    if not args.config_only:
        for path in sorted((examples / "bundles").glob("*.json")):
            validators["bundle.schema.json"].validate(json.loads(path.read_text()))
            print(f"Valid example: bundles/{path.name}")


if __name__ == "__main__":
    main()
