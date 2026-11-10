#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only OR MIT
"""Compile the production M4 external-profile selector against a branch matrix."""
import argparse
from pathlib import Path
import subprocess
import tempfile


def definition(source, name):
    start = source.index(name)
    start = source.rfind("\n", 0, start) + 1
    if source[start - 2:start] != ";\n":
        start = source.rfind("\n", 0, start - 1) + 1
    brace = source.index("{", start)
    depth = 1
    end = brace + 1
    while depth:
        depth += (source[end] == "{") - (source[end] == "}")
        end += 1
    return source[start:end]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path,
                        default=Path(__file__).resolve().parents[6])
    parser.add_argument("--cc", default="cc")
    args = parser.parse_args()
    iomfb = (args.source / "drivers/gpu/drm/apple/iomfb_m4.c").read_text()
    selector = definition(iomfb, "dcp_m4_external_profile(")
    unit = r'''
#include <assert.h>
#include <stdbool.h>
#include <string.h>
struct dcp_m4_profile { unsigned int tag; };
static const struct dcp_m4_profile dcp_m4_profile_j713 = { 713 };
static const struct dcp_m4_profile existing = { 715 };
static bool j773g;
static bool of_machine_is_compatible(const char *s)
{ return j773g && !strcmp(s, "apple,j773g"); }
''' + selector + r'''
int main(void)
{
 j773g = true;
 assert(dcp_m4_external_profile(0, true) == &dcp_m4_profile_j713);
 assert(dcp_m4_external_profile(&existing, true) == &existing);
 assert(dcp_m4_external_profile(0, false) == 0);
 j773g = false;
 assert(dcp_m4_external_profile(0, true) == 0);
 return 0;
}
'''
    with tempfile.TemporaryDirectory(prefix="m4-profile-select-") as tmp:
        src = Path(tmp) / "test.c"
        exe = Path(tmp) / "test"
        src.write_text(unit)
        subprocess.run([args.cc, "-std=c11", "-Wall", "-Wextra", "-Werror",
                        str(src), "-o", str(exe)], check=True)
        subprocess.run([str(exe)], check=True)
    print("PASS: J773g main-external fallback selected; all other missing profiles fail closed")


if __name__ == "__main__":
    main()
