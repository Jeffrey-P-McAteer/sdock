#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = [
#
# ]
# ///

import os
import sys
import pathlib
import subprocess
import shlex
import functools

REPO_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

@functools.lru_cache()
def get_host_triple_raw():
  out = subprocess.check_output(['rustc', '-vV']).decode('utf-8')
  for line in out.splitlines():
    if line.startswith('host: '):
      return line.split('host: ')[-1].strip()
  return None

def get_host_triple_arg():
  triple = get_host_triple_raw()
  if triple is not None:
    return f'--target={triple}'
  return None

def map_host_triple_to_dsock_project_name():
  triple = get_host_triple_raw()
  if triple is None:
    triple = 'x86_64-unknown-linux-gnu'

  if 'linux' in triple:
    if 'x86' in triple:
      if '64' in triple:
        return 'sdock-linux-x64'
      else:
        raise Exception('32-bit binaries not supported! target='+triple)
    elif 'arm' in triple or 'aarch64' in triple:
      return 'sdock-linux-arm64'
    else:
        raise Exception('unsupported target='+triple)

  if 'windows' in triple:
    if 'x86' in triple:
      if '64' in triple:
        return 'sdock-windows-x64'
      else:
        raise Exception('32-bit binaries not supported! target='+triple)
    elif 'arm' in triple or 'aarch64' in triple:
      return 'sdock-windows-arm64'
    else:
        raise Exception('unsupported target='+triple)

  if 'apple' in triple:
    if 'x86' in triple:
      if '64' in triple:
        return 'sdock-macos-x64'
      else:
        raise Exception('32-bit binaries not supported! target='+triple)
    elif 'arm' in triple or 'aarch64' in triple:
      return 'sdock-macos-arm64'
    else:
        raise Exception('unsupported target='+triple)

def get_host_triple_built_file(build_type='release'):
  triple = get_host_triple_raw()
  if triple is not None:
    if 'windows' in triple:
      return os.path.join(REPO_DIR, 'target', f'{triple}', build_type, map_host_triple_to_dsock_project_name()+'.exe')
    else:
      return os.path.join(REPO_DIR, 'target', f'{triple}', build_type, map_host_triple_to_dsock_project_name())

  return os.path.join(REPO_DIR, 'target', build_type, map_host_triple_to_dsock_project_name())

def cmd(*args, **argv):
  c = [arg for arg in args if not arg is None]
  print(f'> {shlex.join(c)}')
  subprocess.run(
    c, cwd=REPO_DIR
  )


cmd('cargo', 'build', '--release', get_host_triple_arg())
print()
print(f'Built ', get_host_triple_built_file())
print()

cmd(get_host_triple_built_file(), *sys.argv[1:])



