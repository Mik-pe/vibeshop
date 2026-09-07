#!/usr/bin/env python3
"""Measure a prebuilt compositor benchmark on Linux, excluding compiler memory."""
import resource
import subprocess
import sys

if sys.platform != "linux" or len(sys.argv) != 2:
    raise SystemExit("Usage (Linux): scripts/measure-compositor.py PATH_TO_BUILT_BENCHMARK")
result = subprocess.run([sys.argv[1]], check=False)
print(f"process_peak_rss_kib={resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss}", flush=True)
raise SystemExit(result.returncode)
