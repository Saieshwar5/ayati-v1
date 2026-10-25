// Container transport for the existing same-host worker journeys. No project mount.
import assert from 'node:assert/strict';
import { spawn, execFileSync } from 'node:child_process';
import { once } from 'node:events';
import { appendFileSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { randomBytes, randomUUID } from 'node:crypto';
import { dirname, join, resolve } from 'node:path';
import { assertResourceLimits } from './load_limits.mjs';

const processes = new Map();
function docker(args, options = {}) {
  try { return execFileSync('docker', args, { encoding:'utf8',stdio:['pipe','pipe','pipe'],...options }); }
  catch (error) {
    if (process.env.AYATI_TEST_IMAGE_CONFIG) appendFileSync(join(dirname(process.env.AYATI_TEST_IMAGE_CONFIG),'container-errors.log'),
      `${args[0]} failed\n${String(error.stdout??'').slice(-65536)}${String(error.stderr??'').slice(-65536)}\n`,{mode:0o600});
    throw new Error(`Owned image operation failed: docker ${args[0]}; inspect container-errors.log`);
  }
}
function inspect(name) { return JSON.parse(docker(['container','inspect',name]))[0]; }

export async function containerTarget(pid) {
  const record = processes.get(pid);
  if (!record) return undefined;
  const info = inspect(record.container);
  assert.equal(info.Config.Labels['ayati.test.session'],record.session);
  assert.ok(info.State.Running && info.State.Pid>0,'Expected a running owned container');
  return { pid:info.State.Pid,id:info.Id };
}
export function assertContainerLimits(snapshot, profile, role, id) {
  assert.match(id,/^[a-f0-9]{64}$/);
  assert.ok(snapshot.path.includes(id),'Process escaped its owned container cgroup');
  assertResourceLimits(snapshot,profile,role);
}

export function imageRuntime(data, profile, adminSql) {
  const config=JSON.parse(readFileSync(process.env.AYATI_TEST_IMAGE_CONFIG,'utf8'));
  assert.equal(config.version,1);assert.match(config.image,/^sha256:[a-f0-9]{64}$/);
  assert.match(config.session,/^[a-f0-9]{32}$/);
  assert.ok(profile,'Image rehearsals require verified resource limits');
  const state=join(data,'image-state');mkdirSync(state,{mode:0o700});
  const records=[];
  function name(role) {
    const value=`ayati-image-${config.session}-${role}-${randomUUID().replaceAll('-','')}`;
    appendFileSync(config.containers_file,value+'\n',{mode:0o600});
    return value;
  }
  function audit(info,role) {
    assert.equal(info.Image,config.image);
    assert.equal(info.Config.Labels['ayati.test.session'],config.session);
    if (['controller','executor','migration','bridge'].includes(role)) {
      const limit=role==='bridge'?{cpu_percent:50,memory_bytes:268435456,tasks:128}:profile.roles[role==='migration'?'controller':role];
      assert.equal(info.Config.User,'10001:10001');assert.equal(info.HostConfig.Privileged,false);
      assert.equal(info.HostConfig.ReadonlyRootfs,true);assert.equal(info.HostConfig.PidMode,'');
      assert.equal(info.HostConfig.NanoCpus,limit.cpu_percent*10000000);
      assert.equal(info.HostConfig.Memory,limit.memory_bytes);assert.equal(info.HostConfig.MemorySwap,limit.memory_bytes);
      assert.equal(info.HostConfig.PidsLimit,limit.tasks);
      assert.ok(info.HostConfig.CapDrop.includes('ALL'));
      assert.ok(info.HostConfig.SecurityOpt.includes('no-new-privileges'));
    }
    appendFileSync(config.audit_file,JSON.stringify({name:info.Name,id:info.Id,image:info.Image,role,
      user:info.Config.User,state:info.State,host_config:{read_only:info.HostConfig.ReadonlyRootfs,
        network:info.HostConfig.NetworkMode,pid_mode:info.HostConfig.PidMode,nano_cpus:info.HostConfig.NanoCpus,
        memory:info.HostConfig.Memory,swap:info.HostConfig.MemorySwap,pids:info.HostConfig.PidsLimit,
        cap_drop:info.HostConfig.CapDrop,security_opt:info.HostConfig.SecurityOpt},
      environment_names:info.Config.Env.map(value=>value.split('=',1)[0])})+'\n',{mode:0o600});
    assert.equal(info.State.OOMKilled,false,'Image exceeded its memory budget');
  }
  function url(value) {
    const result=new URL(value);
    assert.match(result.pathname,/^\/relay_simulation_/);
    result.hostname='127.0.0.1';result.port=String(config.port);
    result.searchParams.delete('host');result.searchParams.delete('port');
    const runtime=result.username==='relay_app';
    result.username=runtime?'relay_app':config.owner_role;
    result.password=runtime?config.app_password:config.owner_password;
    return result.href;
  }
  function prepareSecrets(environment,role) {
    const folder=join(data,'image-secrets-'+randomUUID());mkdirSync(folder,{mode:0o700});
    const result={};let count=0;
    for (const [key,original] of Object.entries(environment)) {
      let value=original;
      if (['PATH','DBUS_SESSION_BUS_ADDRESS','XDG_RUNTIME_DIR'].includes(key)) continue;
      if (key==='AYATI_DATA_DIR') value='/var/lib/ayati';
      if (key==='AYATI_GOOSE_BIN' && value.endsWith('/.cache/goose/v1.54.0/goose')) value='/opt/ayati/goose';
      if (key==='AYATI_PREPARED_GOOSE_BIN' && value.endsWith('/.cache/goose/v1.54.0-ayati.1/goose')) value='/opt/ayati/prepared-goose/goose';
      if (key==='AYATI_WORKER_CONTROL_KEY_FILE') {
        writeFileSync(join(folder,'bootstrap'),readFileSync(value),{mode:0o600});count++;
        result[key]='/run/secrets/ayati/bootstrap';continue;
      }
      if (['RELAY_DATABASE_URL','RELAY_SIMULATION_DATABASE_URL','OPENAI_API_KEY','FIREWORKS_API_KEY','WORKOS_API_KEY'].includes(key)) {
        if (key.startsWith('RELAY_')) value=url(value);
        writeFileSync(join(folder,key),value+'\n',{mode:0o600});count++;
        result[key+'_FILE']='/run/secrets/ayati/'+key;continue;
      }
      result[key]=value;
    }
    if (role==='executor') for (const key of Object.keys(result)) {
      assert.ok(!/^(RELAY_.*DATABASE_URL|WORK_?OS_API_KEY)/.test(key),'Core secret configured on executor');
    }
    if (count) {
      const helper=name('prepare');
      const args=['run','--name',helper,'--label','ayati.test.session='+config.session,'--network','none',
        '--user','0:0','--read-only','--cap-drop','ALL','--cap-add','CHOWN','--cap-add','FOWNER',
        '--security-opt','no-new-privileges','--memory','64m','--memory-swap','64m','--cpus','0.25','--pids-limit','32',
        '--mount',`type=bind,src=${folder},dst=/secrets`, '--mount',`type=bind,src=${state},dst=/state`,
        '--entrypoint','/bin/sh',config.image,'-c','chown 0:0 /secrets; for file in /secrets/*; do chown 10001:10001 "$file"; chmod 600 "$file"; done; chown 10001:10001 /secrets /state'];
      docker(args,{timeout:30000});audit(inspect(helper),'prepare');docker(['rm',helper]);
    }
    return {folder,environment:result};
  }
  function command(args,environment,input) {
    const role=args[0]==='simulate'||args[0]==='identity'?'controller':args[0]==='workspace-db'?'migration':'executor';
    const limits=profile.roles[role==='migration'?'controller':role];
    const prepared=prepareSecrets(environment,role);
    const container=name(role);
    const command=['run','--name',container,'--label','ayati.test.session='+config.session,
      '--read-only','--cap-drop','ALL','--security-opt','no-new-privileges','--network','host',
      '--cpus',String(limits.cpu_percent/100),'--cpuset-cpus',profile.cpu_list,
      '--memory',String(limits.memory_bytes),'--memory-swap',String(limits.memory_bytes),
      '--pids-limit',String(limits.tasks),'--stop-timeout','15',
      '--tmpfs','/tmp:rw,nosuid,nodev,size=64m,mode=1777',
      '--mount',`type=bind,src=${prepared.folder},dst=/run/secrets/ayati,readonly`];
    if (role==='controller') command.push('--mount',`type=bind,src=${state},dst=/var/lib/ayati`);
    else command.push('--tmpfs','/var/lib/ayati:rw,nosuid,nodev,size=256m,mode=0700,uid=10001,gid=10001');
    for (const [key,value] of Object.entries(prepared.environment)) command.push('--env',key+'='+value);
    if (input) command.push('--interactive');
    command.push(config.image,...(args[0]==='simulate'?['simulate','/var/lib/ayati/simulation']:args));
    return {command,container,role};
  }
  return {
    config,
    bridge(directory, control, provider) {
      const folder=join(directory,'image-state');mkdirSync(folder,{mode:0o700});
      const destination='/var/lib/ayati/bridge';
      writeFileSync(join(folder,'operator.key'),readFileSync(control.keyFile),{mode:0o600});
      writeFileSync(join(folder,'model-info.json'),JSON.stringify({url:provider.url,key:provider.key??randomBytes(32).toString('base64url')}),{mode:0o600});
      writeFileSync(join(folder,'config.json'),JSON.stringify({directory:destination,control_url:control.url,
        bootstrap_file:destination+'/operator.key',model_info:destination+'/model-info.json'}),{mode:0o600});
      const helper=name('prepare');
      docker(['run','--name',helper,'--label','ayati.test.session='+config.session,'--network','none',
        '--user','0:0','--read-only','--cap-drop','ALL','--cap-add','CHOWN',
        '--security-opt','no-new-privileges','--memory','64m','--memory-swap','64m','--cpus','0.25','--pids-limit','32',
        '--mount',`type=bind,src=${folder},dst=/state`, '--entrypoint','/bin/sh',config.image,
        '-c','chown 0:0 /state; for file in /state/*; do chown 10001:10001 "$file"; done; chown 10001:10001 /state'],{timeout:30000});
      audit(inspect(helper),'prepare');docker(['rm',helper]);
      const container=name('bridge');
      const child=spawn('docker',['run','--name',container,'--label','ayati.test.session='+config.session,
        '--read-only','--cap-drop','ALL','--security-opt','no-new-privileges','--network','host',
        '--cpus','0.5','--cpuset-cpus',profile.cpu_list,
        '--memory','268435456','--memory-swap','268435456','--pids-limit','128',
        '--tmpfs','/tmp:rw,nosuid,nodev,size=16m,mode=1777',
        '--mount',`type=bind,src=${folder},dst=${destination}`,
        '--mount',`type=bind,src=${resolve('tests/fixtures/aws_gateway.py')},dst=/opt/ayati/test/aws_gateway.py,readonly`,
        '--mount',`type=bind,src=${resolve('tests/aws_bridge_startup_test.py')},dst=/opt/ayati/test/aws_bridge_startup_test.py,readonly`,
        '--env','AWS_EC2_METADATA_DISABLED=true','--entrypoint','/opt/ayati/bridge/bin/python',
        config.image,'/opt/ayati/test/aws_gateway.py',destination+'/config.json'],{stdio:['ignore','pipe','pipe']});
      const record={child,logs:'',container,session:config.session,role:'bridge'};
      child.stdout.on('data',chunk=>{record.logs+=chunk;});child.stderr.on('data',chunk=>{record.logs+=chunk;});
      records.push(record);processes.set(child.pid,record);
      return record;
    },
    migrate(scoped,environment,schema) {
      assert.match(schema,/^relay_worker_[a-f0-9]{32}$/);
      adminSql(`GRANT USAGE,CREATE ON SCHEMA ${schema} TO relay_image_owner`);
      const invocation=command(['workspace-db','migrate'],{...environment,RELAY_DATABASE_URL:scoped},false);
      docker(invocation.command,{timeout:60000});audit(inspect(invocation.container),'migration');
      docker(['rm',invocation.container]);
    },
    launch(args,environment,input) {
      const invocation=command(args,environment,input);
      const child=spawn('docker',invocation.command,{stdio:[input?'pipe':'ignore','pipe','pipe']});
      if (input) child.stdin.end(JSON.stringify(input));
      const record={child,logs:'',container:invocation.container,session:config.session,role:invocation.role};
      child.stdout.on('data',chunk=>{record.logs+=chunk;});child.stderr.on('data',chunk=>{record.logs+=chunk;});
      records.push(record);processes.set(child.pid,record);
      return record;
    },
    async stop(record,signal='SIGTERM') {
      if (record.child.exitCode!==null||record.child.signalCode!==null) return;
      const info=inspect(record.container);assert.equal(info.Config.Labels['ayati.test.session'],config.session);
      const exited=once(record.child,'exit');
      docker(['kill','--signal',signal,record.container],{timeout:10000});
      let timer;
      try {await Promise.race([exited,new Promise((_,reject)=>{timer=setTimeout(()=>{
        docker(['kill','--signal','KILL',record.container]);reject(new Error('Owned image did not stop within its drain deadline'));
      },15000);})]);}finally{clearTimeout(timer);}
      audit(inspect(record.container),record.role);
      if (signal==='SIGTERM') assert.equal(record.child.exitCode,0,record.logs);
    },
    children(record) {
      const info=inspect(record.container);
      const rows=docker(['top',record.container,'-eo','pid,ppid']).trim().split('\n').slice(1).map(line=>line.trim().split(/\s+/).map(Number));
      return rows.filter(([,parent])=>parent===info.State.Pid).map(([pid])=>pid);
    },
    hostPids(slots) {
      return slots.map(slot=>{
        const record=records.find(record=>record.role==='executor'&&record.logs.includes(slot.executor));
        assert.ok(record,'Missing owning container for an assignment PID');
        const info=inspect(record.container);assert.equal(info.Config.Labels['ayati.test.session'],config.session);
        const pids=docker(['top',record.container,'-eo','pid']).trim().split('\n').slice(1).map(line=>Number(line.trim()));
        const host=pids.find(pid=>{
          try {
            const status=readFileSync(`/proc/${pid}/status`,'utf8');
            return Number(status.match(/^NSpid:\s+(.+)$/m)?.[1].trim().split(/\s+/).at(-1))===slot.pid;
          } catch(error) {if(error.code==='ENOENT')return false;throw error;}
        });
        assert.ok(host,'An assigned container PID had no matching host process');
        return host;
      });
    },
    inspect(record) { const info=inspect(record.container);audit(info,record.role);return info; },
    exec(record,args) {return docker(['exec',record.container,...args]);},
    refuseCoreSecret() {
      const container=name('negative');
      let code=0,logs='';
      try {logs=execFileSync('docker',['run','--name',container,'--label','ayati.test.session='+config.session,
        '--network','none','--read-only','--cap-drop','ALL','--security-opt','no-new-privileges',
        '--memory','128m','--memory-swap','128m','--pids-limit','32','--cpus','1',
        '--env','RELAY_DATABASE_URL=fixture-invalid',config.image,'executor'],{encoding:'utf8',stdio:['ignore','pipe','pipe']});}
      catch(error){code=error.status;logs=String(error.stdout??'')+String(error.stderr??'');}
      audit(inspect(container),'negative');
      return {code,logs};
    },
    finish() {for (const record of records) {audit(inspect(record.container),record.role);processes.delete(record.child.pid);}},
  };
}
