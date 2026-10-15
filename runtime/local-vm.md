# Local agent VM integration

Milestone L2, tested 2026-10-15 on an x86_64 Omarchy/Arch host with KVM API 12.
Use [README setup](../README.md#run-the-agent-in-its-own-vm). The default host
adapter remains available; VM mode is explicit and has no host fallback.

## Dependency choice

QEMU/KVM supplies a maintained Linux VM backend without building a hypervisor,
root networking service or cloud orchestrator. Its restricted user-mode network,
SSH forwarding, qcow2 overlays and QMP shutdown cover this local proof. This is
not a claim that QEMU is the smallest possible cloud backend. Evaluate any later
Firecracker/cloud backend using the same lifecycle and runtime contracts.

[The artifact manifest](local-vm-release.json) pins QEMU 11.1.2 plus runtime
libraries/firmware, Ubuntu Minimal 24.04 build 20261008, and pycdlib 1.14.0.
Downloads are SHA-256 checked. The Arch package extraction is project-local,
requires compatible host libraries, and was tested only on this host family.
Pinned mirror artifacts may eventually need an immutable archive location;
helpers fail if a pinned download is unavailable or differs, rather than selecting
a new version. Goose stays at [1.54.0](goose-release.json).

The guest bridge is Ayati-owned Rust, built as a static Linux executable to avoid
depending on the development host's newer glibc. pycdlib only creates the private
NoCloud seed ISO at development time; it is not a shipped service or guest runtime.
QEMU uses GPL licensing with component-specific terms; Ubuntu contains separately
licensed packages, and the seed helper dependency reports LGPLv2. Preserve package
licenses/notices and review distribution obligations when packaging artifacts.

Primary references: [QEMU invocation and restricted networking](https://www.qemu.org/docs/master/system/invocation.html),
[cloud-init NoCloud](https://docs.cloud-init.io/en/latest/reference/datasources/nocloud.html),
and [the pinned Ubuntu image](https://cloud-images.ubuntu.com/minimal/releases/noble/release-20261008/).

## Implemented boundary

- Ayati keeps SQLite records, approvals, chat streaming and model credentials on
  the host. The VM owns its private Goose sessions and workspace on an 8 GiB
  sparse overlay, with separate UEFI variables and a pinned immutable base image.
- A per-VM SSH client key authenticates a forced Rust bridge command. The host
  pins the guest public key, ignores user SSH configuration, and verifies bridge
  version/agent identity. The bridge accepts no caller-selected cwd, file path,
  executable or shell command. The current controller supports only local-agent.
- Goose ACP crosses that private duplex transport unchanged. The harmless MCP
  proof server runs inside the VM. Only explicitly selected extensions are enabled.
- QEMU runs without host folder/device passthrough. KVM, one vCPU and 768 MiB RAM
  are explicit. User-mode network restriction blocks general Internet, host LAN
  and cloud metadata access; only the SSH and narrow model forwards are selected.
- The host gateway accepts an ephemeral guest bearer token, a fixed chat endpoint
  and the selected model. It substitutes the real provider key outside the guest,
  refuses redirects, and redacts provider error bodies. This is model access,
  not the future business connector/grant broker.
- A private, read-only seed ISO provisions keys. A temporary loopback server
  supplies only pinned binaries, never keys; the seed is removed after setup.
- A VM file lock prevents concurrent disk ownership. The host observes health,
  boot time, memory, CPU and process identity. QMP requests graceful shutdown;
  forced termination is reported. Owned process groups and Linux parent-death
  signalling prevent orphan execution after the controlling host exits.

## Evidence and remaining work

The real KVM guest passed nine integration scenarios with pinned Goose and a
local scripted model: chat streaming/session loading, MCP allow/deny, Stop,
approval invalidation, web interaction, settings mismatch, disk/transport/network
checks, provider-error recovery and host-crash recovery. Another freshly provisioned guest had its own
key/disk, an empty workspace, and rejected another identity. The core rejects
claiming another agent's manifest. These checks do not establish production
customer isolation, protection from hypervisor vulnerabilities or live AI quality.

One development sample: prepared guest readiness about 2.8 seconds, roughly
380 MiB host QEMU RSS, and 2-5% of one CPU in brief post-boot idle samples.
During a pending model request, host QEMU RSS was about 506 MiB.
First provisioning took about 42 seconds. These observations vary with caching,
boot activity and hardware; they are not capacity, uptime or pricing guarantees.
The test suite prints current measurements. No live provider/account was tested.

The first durable queue/ownership/action-observation increment is now described
in [the recovery contract](durable-work.md). Safe automatic sleep/wake, active-work
leases, schedules, uncertain external
action recovery, general tools, browser networking/takeover, multi-agent routing,
central connector credentials, live model acceptance, backup/restore and tested
next-release upgrades remain later work. The guest OS and Goose are pinned;
updates require a rebuilt/revalidated guest and a separate state migration or
reconfiguration process. Existing host sessions are not automatically migrated.
Goose may retry provider requests within a turn; Ayati does not automatically
replay interrupted jobs or tool actions. Error-recovery checks explicitly restore
the fixture provider before the owner sends a new message.
