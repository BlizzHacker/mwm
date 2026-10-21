#!/usr/bin/env python3
"""Application-only maintenance. Never runs apt upgrade or upgrades an OS."""
import concurrent.futures, datetime as dt, hashlib, io, json, os, pathlib, shlex, socket, subprocess, sys, time, zipfile
REPORT=pathlib.Path('/var/lib/mwm/media-apps.json')
ARCHIVE=pathlib.Path('/opt/repos/ai-state/wadei/mwm-application-backups')
ARR={'sonarr':(8989,'v3'),'radarr':(7878,'v3'),'lidarr':(8686,'v1'),'prowlarr':(9696,'v1')}
NAMES=set(ARR)|{'plex1','plex2','jellyfin','romm','seerr','qbittorrent','rommarr','maintainerr','cleanuparr','tautulli','nzbget','komga','flaresolverr'}
def run(args,timeout=60):
 p=subprocess.run(args,capture_output=True,timeout=timeout)
 if p.returncode:raise RuntimeError(p.stderr.decode(errors='replace')[-350:] or 'Remote command failed')
 return p.stdout

def guests():
 return [x for x in json.loads(run(['pvesh','get','/cluster/resources','--type','vm','--output-format','json'])) if x.get('type')=='lxc' and x.get('name','').lower() in NAMES]
def ips():return {k:v['ip'] for k,v in json.loads(pathlib.Path('/etc/pve/.members').read_text())['nodelist'].items()}
def nodecmd(node,args):
 if node.lower()==socket.gethostname().lower():return args
 return ['ssh','-o','BatchMode=yes','-o','ConnectTimeout=6','root@'+ips()[node],shlex.join(args)]
def guest(g,args,timeout=90):return run(nodecmd(g['node'],['pct','exec',str(g['vmid']),'--']+args),timeout)
def arr(g,op):
 name=g['name'].lower();port,version=ARR[name]
 code=r"""
import datetime,hashlib,json,os,pathlib,time,urllib.request,xml.etree.ElementTree as ET
app=APP;operation=OP;root='http://127.0.0.1:PORT/api/VER';key=ET.parse('/var/lib/'+app+'/config.xml').getroot().findtext('ApiKey')
def req(path,data=None):
 r=urllib.request.Request(root+'/'+path,data=None if data is None else json.dumps(data).encode(),headers={'X-Api-Key':key,'Content-Type':'application/json'})
 with urllib.request.urlopen(r,timeout=20) as f:return json.load(f)
if operation=='scan':
 s=req('system/status');u=req('update');new=next((x for x in u if x.get('installable') and not x.get('installed')),None);print(json.dumps({'version':s.get('version'),'update_version':new.get('version') if new else None,'health':req('health'),'update_supported':True}))
elif operation=='backup':
 c=req('command',{'name':'Backup'});end=time.time()+180
 while time.time()<end:
  status=req('command/'+str(c['id']))
  if status.get('status')=='completed':break
  if status.get('status') in ('failed','aborted'):raise RuntimeError('Native application backup failed')
  time.sleep(2)
 else:raise RuntimeError('Application backup timed out')
 backups=sorted([b for b in req('system/backup') if b.get('type')=='manual'],key=lambda b:b['time'],reverse=True);assert backups,'No manual backup was created';b=backups[0];assert b['path'].startswith('/backup/')
 r=urllib.request.Request('http://127.0.0.1:PORT'+b['path'],headers={'X-Api-Key':key});data=urllib.request.urlopen(r,timeout=90).read();assert data[:2]==b'PK','Backup is not a zip archive'
 folder=pathlib.Path('/var/tmp/mwm-app-backups');folder.mkdir(mode=0o700,exist_ok=True);p=folder/pathlib.Path(b['name']).name;p.write_bytes(data);p.chmod(0o600);print(json.dumps({'path':str(p),'sha256':hashlib.sha256(data).hexdigest(),'bytes':len(data)}))
elif operation=='update':
 before=req('system/status')['version'];new=next((x for x in req('update') if x.get('installable') and not x.get('installed')),None)
 if not new:print(json.dumps({'status':'current','version':before}))
 else:
  req('command',{'name':'ApplicationUpdate'});end=time.time()+300
  while time.time()<end:
   time.sleep(4)
   try:
    after=req('system/status')['version']
    if after==new['version']:print(json.dumps({'status':'updated','before':before,'version':after}));break
   except Exception:pass
  else:raise RuntimeError('Update did not reach expected version; retained backup and snapshot require review')
"""
 code=code.replace('APP',repr(name)).replace('OP',repr(op)).replace('PORT',str(port)).replace('VER',version)
 return json.loads(guest(g,['python3','-c',code],420 if op=='update' else 240))
def inspect(g):
 name=g['name'].lower();r={'name':name,'node':g['node'],'vmid':str(g['vmid']),'guest_status':g['status'],'update_supported':False}
 if g['status']!='running':r['state']='stopped';return r
 try:
  r['ip']=guest(g,['hostname','-I']).decode().split()[0]
  if name in ARR:r.update(arr(g,'scan'));r['state']='update_available' if r.get('update_version') else 'current';r['online']=True
  elif name.startswith('plex') or name=='jellyfin':
   code=r"""
import json,pathlib,subprocess,urllib.request,xml.etree.ElementTree as E
name=NAME;plex=name.startswith('plex');package='plexmediaserver' if plex else 'jellyfin-server';r={'update_supported':True}
p=subprocess.run(['dpkg-query','-W','-f=${Version}',package],text=True,capture_output=True);r['installed_package']=p.stdout.strip()
text=subprocess.run(['apt-cache','policy',package],text=True,capture_output=True).stdout;r['candidate']=next((x.split(':',1)[1].strip() for x in text.splitlines() if 'Candidate:' in x),'')
if plex:
 d=E.fromstring(urllib.request.urlopen('http://127.0.0.1:32400/identity',timeout=8).read());r['version']=d.get('version');r['server_id']=d.get('machineIdentifier');prefs=E.parse('/var/lib/plexmediaserver/Library/Application Support/Plex Media Server/Preferences.xml').getroot();r['server_name']=prefs.get('FriendlyName');token=prefs.get('PlexOnlineToken','')
 try:r['active_sessions']=int(E.fromstring(urllib.request.urlopen(urllib.request.Request('http://127.0.0.1:32400/status/sessions',headers={'X-Plex-Token':token}),timeout=8).read()).get('size','0'))
 except Exception:r['active_sessions']=None
else:
 d=json.load(urllib.request.urlopen('http://127.0.0.1:8096/System/Info/Public',timeout=8));r['version']=d.get('Version');r['server_name']=d.get('ServerName')
def normalized(v):
 parts=v.split('+')[0].split('-')[0].split('.');return tuple(parts+['0']*max(0,3-len(parts)))
r['online']=True;r['restart_needed']=bool(r.get('version') and normalized(r['version'])!=normalized(r['installed_package']));r['update_version']=r['candidate'] if r['candidate'] and r['candidate']!=r['installed_package'] else None;r['state']='restart_required' if r['restart_needed'] else ('update_available' if r['update_version'] else 'current');print(json.dumps(r))
""".replace('NAME',repr(name))
   r.update(json.loads(guest(g,['python3','-c',code])))
  else:
   r['state']='manual_review';r['note']='Custom installation or Docker project; see container update inventory.'
 except Exception as e:r.update(state='error',error=str(e),online=False)
 return r
def save(report):
 REPORT.parent.mkdir(parents=True,exist_ok=True);p=REPORT.with_suffix('.tmp');p.write_text(json.dumps(report,indent=2));p.chmod(0o640);p.replace(REPORT)
def scan():
 with concurrent.futures.ThreadPoolExecutor(max_workers=3) as pool:rows=list(pool.map(inspect,guests()))
 docker=pathlib.Path('/var/lib/mwm/docker-updates.json');report={'generated_at':dt.datetime.now(dt.timezone.utc).isoformat(),'applications':rows,'docker':json.loads(docker.read_text()) if docker.exists() else {},'os_upgrades_held':True};save(report);return report
def update(vmid):
 g=next((g for g in guests() if str(g['vmid'])==vmid),None)
 if not g or g['status']!='running':raise RuntimeError('Application target is not running')
 name=g['name'].lower();before=inspect(g)
 if name in ARR:
  if not before.get('update_version'):return {'status':'current','application':before}
  ARCHIVE.mkdir(mode=0o700,parents=True,exist_ok=True);backup=arr(g,'backup');data=guest(g,['cat',backup['path']],180);assert hashlib.sha256(data).hexdigest()==backup['sha256'],'Backup checksum mismatch';assert zipfile.ZipFile(io.BytesIO(data)).testzip() is None,'Backup archive CRC failed'
  p=ARCHIVE/(str(g['vmid'])+'-'+pathlib.Path(backup['path']).name);p.write_bytes(data);p.chmod(0o600)
  snap='mwmapp'+dt.datetime.now(dt.timezone.utc).strftime('%Y%m%d%H%M%S');binary_archive=None
  try:run(nodecmd(g['node'],['pct','snapshot',vmid,snap,'--description','MWM verified application backup before application-only update']),180)
  except RuntimeError as e:
   if 'snapshot feature is not available' not in str(e):raise
   snap=None
   info=guest(g,['python3','-c',"import pathlib,subprocess;pid=subprocess.check_output(['systemctl','show','-p','MainPID','--value','"+name+"']).decode().strip();print(pathlib.Path('/proc/'+pid+'/exe').resolve().parent)"]).decode().strip()
   if not info.startswith('/opt/') or name not in pathlib.Path(info).name.lower():raise RuntimeError('Cannot identify an application-only rollback directory')
   archive=guest(g,['tar','-C','/','-czf','-',info.lstrip('/')],240)
   import tarfile
   with tarfile.open(fileobj=io.BytesIO(archive),mode='r:gz') as check:
    if not check.getmembers():raise RuntimeError('Application rollback archive is empty')
   binary_archive=ARCHIVE/(vmid+'-'+name+'-binaries-'+dt.datetime.now(dt.timezone.utc).strftime('%Y%m%d%H%M%S')+'.tgz');binary_archive.write_bytes(archive);binary_archive.chmod(0o600)
   digest=hashlib.sha256(archive).hexdigest()
   if hashlib.sha256(binary_archive.read_bytes()).hexdigest()!=digest:raise RuntimeError('Binary archive verification failed')
  result=arr(g,'update');result.update(backup=str(p),sha256=backup['sha256'],snapshot=snap,binary_archive=str(binary_archive) if binary_archive else None);scan();return result
 if name.startswith('plex') or name=='jellyfin':
  if before.get('server_name','').upper()=='TRADEWINDSOUND' or before.get('server_id')=='23da31b6df9b5980ffcfc9a4989077ba89c885de':raise RuntimeError('TRADEWINDSOUND is deferred by the owner')
  if before.get('active_sessions'):raise RuntimeError('Active Plex streams; wait for an idle window')
  if before.get('restart_needed') and not before.get('update_version'):
   guest(g,['systemctl','restart','plexmediaserver' if name.startswith('plex') else 'jellyfin']);time.sleep(5);after=inspect(g)
   if not after.get('online') or after.get('restart_needed'):raise RuntimeError('Service did not activate the installed version')
   scan();return {'status':'restarted','before':before.get('version'),'version':after.get('version')}
  if not before.get('update_version'):return {'status':'current','application':before}
  raise RuntimeError('Native package upgrade needs an application backup and dependency review; OS upgrades are held')
 raise RuntimeError('This custom application requires its supported updater; full container OS upgrades are held')
if __name__=='__main__':
 try:
  import fcntl
  with open('/run/lock/mwm-media-apps.lock','w') as lock:
   fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB);print(json.dumps(scan() if len(sys.argv)<2 or sys.argv[1]=='scan' else update(sys.argv[2])))
 except Exception as e:print(json.dumps({'error':str(e)}));sys.exit(1)
