"""Mechanical: add `project: &ProjectConfig` param to generate() methods
and pass to render_template() calls.
"""

import re
import os
import glob
import sys

BASE = os.path.dirname(os.path.abspath(__file__))

def fix_file(filepath):
    with open(filepath) as f:
        content = f.read()
    
    old = content
    
    # Fix 1: Add project param to generate() signatures.
    # Match pattern:
    #   "        tera: &tera::Tera," (8-space indent)
    #   "    ) -> Result<..."        (4-space indent, closing paren)
    # Insert: "        project: &ProjectConfig,"
    # between them.
    content = re.sub(
        r'(        tera: &tera::Tera,)\n(    )\) ->',
        r'\1\n        project: &ProjectConfig,\n\2) ->',
        content,
    )
    
    # Fix 2: Add project to render_template() calls.
    # Match: render_template(tera, "str", expr) where expr doesn't contain parens
    # Replace: render_template(tera, "str", expr, project)
    content = re.sub(
        r'render_template\(([^,]+),\s*("[^"]+")\s*,\s*([^,)]+)\)',
        r'render_template(\1, \2, \3, project)',
        content,
    )
    
    if content != old:
        with open(filepath, 'w') as f:
            f.write(content)
        return True
    return False


def main():
    patterns = ["api/*.rs", "cli/*.rs", "codelist/*.rs", "db/*.rs", "ddd/*.rs",
                "domain_types/*.rs", "hooks/*.rs", "integration/*.rs",
                "playwright/*.rs", "scaffold/*.rs", "test/*.rs",
                "ui/*.rs", "webhook/*.rs", "ifml/*.rs"]
    
    count = 0
    for pat in patterns:
        for fp in sorted(glob.glob(os.path.join(BASE, pat))):
            if fp.endswith("mod.rs") or "fix_" in fp:
                continue
            if fix_file(fp):
                print(f"  {os.path.relpath(fp, BASE)}")
                count += 1
    
    print(f"\nUpdated {count} files")


if __name__ == "__main__":
    main()
