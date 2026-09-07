#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = [
#
# ]
# ///

CODE_TODO_GLOBS = {
  '**/*.rs': ['// TODO'],
  '**/*.md': ['// TODO', '# TODO'],
}

import os
import sys
import pathlib

REPO_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
for src_glob, todo_fragments in CODE_TODO_GLOBS.items():
  for todo_fragment in todo_fragments:
    todo_fragment = todo_fragment.casefold()
    for path in pathlib.Path(REPO_DIR).rglob(src_glob):
      we_printed_anything = False
      file_fraction = path.relative_to(REPO_DIR)
      with open(path.resolve(), 'r') as fd:
        src_text = fd.read()
        for i, line in enumerate(src_text.splitlines(), 1):
          if todo_fragment in line.casefold():
            print(f'{file_fraction}:{i}')
            print(f'{line}')
            we_printed_anything = True

      if we_printed_anything:
        print() # per-file separation

