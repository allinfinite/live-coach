#!/usr/bin/env python3
"""Export a reviewed source allowlist; never copy app data or local evidence."""
import argparse, hashlib, json, re, shutil
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TOP = {'README.md','LICENSE','ATTRIBUTION.md','THIRD-PARTY-LICENSES.md','CONTRIBUTING.md','SECURITY.md','.gitignore','package.json','pnpm-lock.yaml','rust-toolchain.toml'}
DIRS = {'ui','docs','scripts','tests','voice-runtime','.github','src-tauri'}
SKIP = {'target','node_modules','dist','evidence','gen','__pycache__','.venv','.codebase-memory','LiveCoach.iconset'}
SUFFIX = {'.md','.rs','.toml','.lock','.json','.js','.css','.html','.py','.sh','.txt','.plist','.png','.icns','.yml','.yaml'}
SECRET = re.compile(r'(?:sk-[A-Za-z0-9_-]{20,}|gh[pousr]_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{20,}|-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----)')

def main():
 p=argparse.ArgumentParser();p.add_argument('destination',type=Path);p.add_argument('--audit-only',action='store_true');a=p.parse_args()
 selected=[];findings=[]
 # Path iteration prunes large/private directories before reading contents.
 import os
 for parent,dirs,names in os.walk(ROOT):
  rel=Path(parent).relative_to(ROOT)
  dirs[:]=[d for d in dirs if d not in SKIP and 'icloud-backup' not in d and (rel.parts or d in DIRS)]
  for name in sorted(names):
   src=Path(parent)/name; r=src.relative_to(ROOT)
   if src.is_symlink() or 'icloud-backup' in name or name.startswith('.env') or name=='tauri.local.json': continue
   if len(r.parts)==1 and name not in TOP: continue
   if len(r.parts)>1 and (r.parts[0] not in DIRS or src.suffix not in SUFFIX): continue
   data=src.read_bytes()
   if src.suffix not in {'.png','.icns'}:
    txt=data.decode('utf-8',errors='replace')
    if SECRET.search(txt): findings.append({'path':str(r),'issue':'credential pattern'})
    if re.search(r'/Users/' + r'[A-Za-z][^/\s]+/', txt): findings.append({'path':str(r),'issue':'personal absolute path'})
   selected.append((r,data))
 manifest=[{'path':str(r),'bytes':len(d),'sha256':hashlib.sha256(d).hexdigest()} for r,d in selected]
 print(json.dumps({'files':manifest,'findings':findings},indent=2))
 if findings: raise SystemExit('Resolve public-export findings before export.')
 if a.audit_only:return
 if a.destination.exists():raise SystemExit('Destination must not exist; refusing to overwrite.')
 for r,data in selected:
  dst=a.destination/r;dst.parent.mkdir(parents=True,exist_ok=True);dst.write_bytes(data)
  if r.suffix=='.sh' or r.name=='export-public.py':dst.chmod(0o755)

if __name__=='__main__':main()
