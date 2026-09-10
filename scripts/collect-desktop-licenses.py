#!/usr/bin/env python3
"""Include available dependency license texts and a version/license inventory."""
import json, pathlib, subprocess, sys
root = pathlib.Path(sys.argv[1]).resolve()
out = root / 'desktop/src-tauri/resources/licenses/dependencies'
out.mkdir(parents=True, exist_ok=True)
target = subprocess.check_output(['rustc','-vV'], text=True).split('host: ')[1].splitlines()[0]
packages = {}
for manifest in [root / 'Cargo.toml', root / 'desktop/src-tauri/Cargo.toml']:
    meta = json.loads(subprocess.check_output(['cargo','metadata','--locked','--format-version','1','--filter-platform',target,'--manifest-path',str(manifest)]))
    for p in meta['packages']:
        if p.get('source'): packages[('rust',p['name'],p['version'])] = (pathlib.Path(p['manifest_path']).parent,p.get('license'),p.get('license_file'))
for package in (root / 'web/node_modules').rglob('package.json'):
    # Include shipped frontend dependencies and their available notices. Extra build
    # dependency notices are harmless and avoid guessing the bundler's tree shaking.
    if any(part.startswith('.') for part in package.relative_to(root / 'web/node_modules').parts): continue
    try: p=json.loads(package.read_text())
    except (ValueError,UnicodeDecodeError): continue
    if p.get('name') and p.get('version'): packages[('npm',p['name'],p['version'])] = (package.parent,p.get('license'),None)
index=[]
for (ecosystem,name,version),(directory,license,license_file) in sorted(packages.items()):
    dest=out / f'{ecosystem}-{name.replace("/","_")}-{version}'
    texts=[]
    candidates=list(directory.iterdir())
    if license_file: candidates.append(directory / license_file)
    for p in candidates:
        if p.is_file() and (p.name.lower().startswith(('license','licence','copying','notice','copyright')) or license_file and p == directory / license_file):
            dest.mkdir(exist_ok=True)
            (dest / p.name).write_bytes(p.read_bytes()); texts.append(p.name)
    index.append(dict(ecosystem=ecosystem,name=name,version=version,license=license,texts=sorted(set(texts))))
(out / 'inventory.json').write_text(json.dumps(index,indent=2)+'\n')
