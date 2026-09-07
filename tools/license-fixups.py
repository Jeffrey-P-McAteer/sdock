#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = [
#
# ]
# ///

CODE_LICENSE_TEXTS = {
  '**/*.rs': '''
/*
 *  sdock - a cross-platform dock reminiscient of Apple's Skeuomorphic MacOS Dock from the 2009-2014 era
 *  Copyright (C) 2026  Jeffrey McAteer <jeffrey@jmcateer.com>
 *
 *  This program is free software; you can redistribute it and/or modify
 *  it under the terms of the GNU General Public License as published by
 *  the Free Software Foundation; version 2 of the License ONLY.
 *
 *  This program is distributed in the hope that it will be useful,
 *  but WITHOUT ANY WARRANTY; without even the implied warranty of
 *  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 *  GNU General Public License for more details.
 *
 *  You should have received a copy of the GNU General Public License along
 *  with this program; if not, write to the Free Software Foundation, Inc.,
 *  51 Franklin Street, Fifth Floor, Boston, MA 02110-1301 USA.
 */
'''.strip()

}

DELETABLE_OLD_LICENSE_TEXTS = [
'''
/*
 *  sdock - an experimental environment for traveling salesman solution analysis
 *  Copyright (C) 2026  Jeffrey McAteer <jeffrey@jmcateer.com>
 *
 *  This program is free software; you can redistribute it and/or modify
 *  it under the terms of the GNU General Public License as published by
 *  the Free Software Foundation; version 2 of the License ONLY.
 *
 *  This program is distributed in the hope that it will be useful,
 *  but WITHOUT ANY WARRANTY; without even the implied warranty of
 *  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 *  GNU General Public License for more details.
 *
 *  You should have received a copy of the GNU General Public License along
 *  with this program; if not, write to the Free Software Foundation, Inc.,
 *  51 Franklin Street, Fifth Floor, Boston, MA 02110-1301 USA.
 */
'''.strip(),
]


import os
import sys
import pathlib

REPO_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
for src_glob, license_string in CODE_LICENSE_TEXTS.items():
  for path in pathlib.Path(REPO_DIR).rglob(src_glob):
    file_fraction = path.relative_to(REPO_DIR)
    license_is_missing = False
    with open(path.resolve(), 'r') as fd:
      src_text = fd.read()
      if not license_string in src_text:
        print(f'{file_fraction} is missing the following license header:')
        print()
        print(license_string)
        print()
        license_is_missing = True

    if license_is_missing and not ( (src_text is None) or (src_text == '') ):
      # Step 1: IF the file contains any of DELETABLE_OLD_LICENSE_TEXTS, delete those from the original source code.
      for deletable_license_text in DELETABLE_OLD_LICENSE_TEXTS:
        if deletable_license_text in src_text:
          src_text = src_text.replace(deletable_license_text, '')
      # Step 2: Prepend license text
      with open(path.resolve(), 'w') as fd:
        fd.write(license_string+os.linesep+os.linesep+src_text)
      print(f'{file_fraction} fixed')

