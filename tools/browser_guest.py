"""Guest-only browser service configuration used by VM preparation."""
import json


def browser_files():
    unit = '''[Unit]
Description=Ayati private browser broker
After=network.target
[Service]
Type=simple
User=ayati-browser
Group=ayati
ExecStartPre=/usr/bin/mkdir -p /run/ayati-browser/private
ExecStart=/usr/local/bin/ayati-guest browser-service
RuntimeDirectory=ayati-browser
RuntimeDirectoryMode=0750
StateDirectory=ayati-browser
StateDirectoryMode=0700
UMask=0077
NoNewPrivileges=yes
ProtectHome=yes
ProtectSystem=strict
PrivateTmp=yes
ReadWritePaths=/var/lib/ayati-browser /run/ayati-browser
Restart=on-failure
RestartSec=2
TimeoutStopSec=10
KillMode=mixed
[Install]
WantedBy=multi-user.target
'''
    return [{'path': '/etc/systemd/system/ayati-browser.service', 'content': unit, 'permissions': '0644'},
            {'path':'/etc/apparmor.d/ayati-chrome','content':'abi <abi/4.0>,\ninclude <tunables/global>\n/opt/ayati/browser/chrome-linux64/chrome flags=(unconfined) {\n  userns,\n}\n','permissions':'0644'}]


def browser_setup():
    return '''
import tarfile
with tarfile.open('/opt/ayati/browser.tar.gz') as archive:
    archive.extractall('/opt/ayati/browser',filter='data')
pathlib.Path('/opt/ayati/browser.tar.gz').unlink()
u=pwd.getpwnam('ayati-browser')
for name in ['','profile','transfers']:
    path=pathlib.Path('/var/lib/ayati-browser')/name
    path.mkdir(parents=True,exist_ok=True);path.chmod(0o700);os.chown(path,u.pw_uid,u.pw_gid)
# Downloads cannot fill the durable guest disk. Transfers are intentionally ephemeral.
line='tmpfs /var/lib/ayati-browser/transfers tmpfs size=8M,mode=0700,uid='+str(u.pw_uid)+',gid='+str(u.pw_gid)+',nosuid,nodev,noexec 0 0\\n'
with open('/etc/fstab','a') as f:f.write(line)
subprocess.run(['mount','/var/lib/ayati-browser/transfers'],check=True)
if pathlib.Path('/usr/sbin/apparmor_parser').exists():
    subprocess.run(['/usr/sbin/apparmor_parser','-r','/etc/apparmor.d/ayati-chrome'],check=True)
subprocess.run(['systemctl','daemon-reload'],check=True)
subprocess.run(['systemctl','enable','--now','ayati-browser.service'],check=True)
'''
