#!/usr/bin/env python3
# GlideDesk — Copyright (C) 2026 GlideDesk Contributors. GPL-3.0, see LICENSE.
# Master icon generator runner.
import subprocess
import sys
import os

here = os.path.dirname(os.path.abspath(__file__))
script = os.path.join(here, 'generate_assets.py')
sys.exit(subprocess.call([sys.executable, script]))
