#!/usr/bin/env python3
"""Relocate non-system Mach-O dependencies for Tauri's Frameworks directory."""
import json, pathlib, shutil, subprocess, sys
root = pathlib.Path(sys.argv[1]).resolve()
frameworks = root / 'frameworks'
frameworks.mkdir(exist_ok=True)
seen = {}
def dependencies(path):
    lines = subprocess.check_output(['otool', '-L', str(path)], text=True).splitlines()[1:]
    return [line.strip().split(' (compatibility')[0] for line in lines]
def relocate(path, library=False):
    for dep in dependencies(path):
        if dep.startswith(('/System/', '/usr/lib/')): continue
        if library and pathlib.Path(dep).name == path.name: continue
        if not dep.startswith('/'):
            raise SystemExit(f'Unresolved dependency {dep} in {path}; build with absolute library paths')
        original = pathlib.Path(dep).resolve()
        name = original.name
        if name in seen and seen[name] != original:
            raise SystemExit(f'Conflicting library names: {name}')
        if name not in seen:
            seen[name] = original
            destination = frameworks / name
            shutil.copy2(original, destination)
            destination.chmod(0o755)
            subprocess.run(['install_name_tool', '-id', '@rpath/' + name, str(destination)], check=True)
            relocate(destination, True)
        new = ('@loader_path/' if library else '@executable_path/../Frameworks/') + name
        subprocess.run(['install_name_tool', '-change', dep, new, str(path)], check=True)
    # Modified helpers must have valid ad-hoc signatures even for unsigned local builds.
    subprocess.run(['codesign', '--force', '--sign', '-', str(path)], check=True)
for binary in (root / 'binaries').iterdir():
    if binary.is_file(): relocate(binary)
(root / 'bundle.generated.json').write_text(json.dumps({'bundle': {'macOS': {'frameworks': [str(frameworks / n) for n in seen]}}}, indent=2))
