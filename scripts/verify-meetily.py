#!/usr/bin/env python3
"""Compare the original snapshot without opening Meetily databases or sessions."""
import json,sys
from pathlib import Path
before=Path(sys.argv[1] if len(sys.argv)>1 else '../evidence/meetily-before.json')
rows=json.loads(before.read_text()); changes=[]
for row in rows:
 p=Path(row['path'])
 if not p.is_file():changes.append({'path':str(p),'change':'missing'});continue
 s=p.stat()
 if (s.st_size,s.st_mtime_ns)!=(row['size'],row['mtime_ns']):changes.append({'path':str(p),'change':'metadata changed'})
roots=[Path('/Applications/meetily.app'),Path.home()/'Library/Application Support/Meetily',Path.home()/'Library/Application Support/com.meetily.ai']
known={row['path'] for row in rows}
for root in roots:
 for p in root.rglob('*'):
  if p.is_file() and str(p) not in known:changes.append({'path':str(p),'change':'new file'})
print(json.dumps({'checked_files':len(rows),'changes':changes},indent=2))
raise SystemExit(bool(changes))
