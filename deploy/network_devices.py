#!/usr/bin/env python3
"""Pinned OPNsense API adapter; TP-Link native management and availability."""
import base64, concurrent.futures, datetime, json, pathlib, sys, time, urllib3
CONFIG=pathlib.Path('/etc/mwm/firewall.json')
def api(path,data=None):
 c=json.loads(CONFIG.read_text());pool=urllib3.PoolManager(cert_reqs='CERT_NONE',assert_fingerprint=c['sha256'],timeout=urllib3.Timeout(connect=3,read=8),retries=False)
 auth=base64.b64encode((c['key']+':'+c['secret']).encode()).decode()
 r=pool.request('GET' if data is None else 'POST',c['url']+path,body=None if data is None else json.dumps(data).encode(),headers=({'Authorization':'Basic '+auth,'Accept':'application/json'} if data is None else {'Authorization':'Basic '+auth,'Content-Type':'application/json'}),redirect=False)
 if r.status!=200:raise RuntimeError('OPNsense API returned HTTP '+str(r.status))
 return json.loads(r.data)
def attempt(path):
 try:return api(path)
 except Exception as e:return {'error':str(e)}
def status():
 fw={'id':'opnsense','name':'OPNsense physical firewall','url':'https://192.168.0.1/','address':'192.168.0.1','role':'Firewall / gateway','configured':CONFIG.exists(),'legacy_gateway':'192.168.0.201','wan_interface':'igb3','lan_interface':'igb0','lte':'No activated SIM; failover not configured'}
 if CONFIG.exists():
  endpoints={'interfaces':'/api/diagnostics/interface/get_interface_statistics','system':'/api/diagnostics/system/system_information','resources':'/api/diagnostics/system/system_resources','system_time':'/api/diagnostics/system/system_time','system_disk':'/api/diagnostics/system/system_disk','firmware':'/api/core/firmware/status','gateways':'/api/routes/gateway/status','services':'/api/core/service/search','ids_status':'/api/ids/service/status','ids_rulesets':'/api/ids/settings/list_rulesets','ids_alert_logs':'/api/ids/service/get_alert_logs','leases':'/api/dnsmasq/leases/search'}
  with concurrent.futures.ThreadPoolExecutor(max_workers=5) as pool:
   values=list(pool.map(attempt,endpoints.values()))
  fw.update(zip(endpoints,values))
  raw=fw.get('leases') or {}
  if isinstance(raw,dict) and 'error' in raw:
   fw['leases']={'rows':[],'error':raw['error']}
  else:
   records=raw.get('rows',[]) if isinstance(raw,dict) else []
   fw['leases']={'rows':[{'address':r.get('address',''),'hostname':r.get('hostname',''),'hwaddr':r.get('hwaddr',''),'lease_type':r.get('lease_type','dynamic'),'expire':r.get('expire'),'if_descr':r.get('if_descr','')} for r in records[:512] if isinstance(r,dict)],'error':''}
  fw['online']=any('error' not in v for v in values if isinstance(v,dict));fw['api_connected']='error' not in fw['interfaces']
  if isinstance(fw.get('ids_status'),dict) and 'error' not in fw['ids_status']:
   rules=(fw.get('ids_rulesets') or {}).get('rows',[])
   fw['ids']={'running':fw['ids_status'].get('status')=='running','status':fw['ids_status'].get('status'),'active_rulesets':[{'name':r.get('description'),'filename':r.get('filename'),'updated':r.get('modified_local')} for r in rules if r.get('enabled')=='1'],'alert_logs':fw.get('ids_alert_logs',[])}
 else:fw.update(online=False,api_connected=False,error='API credentials are not configured on this MWM host')
 ap={'id':'tplink','name':'TP-Link Archer BE3600','url':'http://192.168.0.2/','address':'192.168.0.2','role':'Wi-Fi access point','mode':'Access Point','management':'native_ui','firmware_update':'Use the native management page; match firmware to the installed hardware revision.'}
 start=time.monotonic()
 try:
  r=urllib3.PoolManager(timeout=3,retries=False).request('GET',ap['url'],redirect=False);ap['online']=r.status in (200,301,302,401,403);ap['http_status']=r.status;ap['latency_ms']=round((time.monotonic()-start)*1000)
 except Exception as e:ap.update(online=False,error='Management interface is not reachable')
 modem={'id':'arris_g36','name':'ARRIS SURFboard G36','model':'ARRIS SURFboard G36','standard':'DOCSIS 3.1','url':'https://192.168.100.1/','address':'192.168.100.1','role':'Cable modem','management':'web_ui'}
 start=time.monotonic()
 try:
  urllib3.disable_warnings(urllib3.exceptions.InsecureRequestWarning)
  r=urllib3.PoolManager(cert_reqs='CERT_NONE',timeout=urllib3.Timeout(connect=3,read=4),retries=False).request('GET',modem['url']+'custom/1/landing.php',redirect=False)
  location=r.headers.get('Location','')
  modem.update(online=r.status in (200,301,302,401,403),http_status=r.status,latency_ms=round((time.monotonic()-start)*1000),login_required=(r.status in (401,403) or 'login' in location.lower()))
  modem['access_state']='login_required' if modem['login_required'] else 'reachable'
 except Exception:
  modem.update(online=False,login_required=False,access_state='unreachable',error='Modem management page is not reachable')
 return {'available':True,'generated_at':datetime.datetime.now(datetime.timezone.utc).isoformat(),'devices':[fw,ap,modem]}
def action(kind,service=''):
 if kind=='check_updates':return api('/api/core/firmware/check',{})
 if kind=='restart_service':
  allowed={'unbound','dnsmasq','dhcpd','ntpd','chrony','kea-dhcp4','kea-dhcp6'}
  rows=api('/api/core/service/search').get('rows',[])
  row=next((r for r in rows if r.get('id')==service or r.get('name')==service),None)
  if service not in allowed or not row or row.get('locked'):raise RuntimeError('This service is not available for restart from MWM')
  return api('/api/core/service/restart/'+service,{})
 raise RuntimeError('Unsupported management action')
if __name__=='__main__':
 try:print(json.dumps(status() if len(sys.argv)<2 or sys.argv[1]=='status' else action(sys.argv[1],sys.argv[2] if len(sys.argv)>2 else '')))
 except Exception as e:print(json.dumps({'error':str(e)}));sys.exit(1)
