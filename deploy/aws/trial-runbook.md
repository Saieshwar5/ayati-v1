# Five employees / five agents: bounded cloud trial

This is the M2/M3 test brief. No cloud trial has run in M0. The current templates
keep compute at zero until authenticated remote execution is implemented.

## Scope and start gate

Use five authorized test users, five managed agents and synthetic business
content in one private workspace. Prepare identities explicitly; this brief does
not authorize email invitations or outside business actions. Proposed hostname:
`trial.cuckoo.onl`, under the owner's domain. Use only a separately reviewed model
route; the recorded Fireworks calibration used an OpenAI-compatible test proxy
and does not establish native Fireworks cloud acceptance.

Start the **15-minute active timer** only after infrastructure, identity, schema,
private storage and executor readiness pass. Record the earlier resource-creation
time and final cleanup time separately for cost. A failed start gate aborts the
trial; do not extend it or repeatedly call models to make a failed test pass.

Before starting, fill the total budget, expiry, operator, exact image/model,
provider rates and request/token limits. The proposed maximum is 20 inference
attempts, including retries/cancelled calls, concurrency at most 4 and 1,024 output
tokens per call. These are future runner requirements, not an implemented model
billing cap. Confirm input bounds and total model allowance separately.

## Candidate journey

| Active time | Journey | Evidence |
| --- | --- | --- |
| 0–3 min | Test users sign in and open the prepared workspace/channels | Correct Owner/User visibility; unauthorized resources denied; no raw secrets |
| 3–7 min | Five users each request their assigned agent, with a simultaneous batch | Four ready slots serve at most four jobs; fifth queues; all authorized replies publish once; real first-visible latency and usage |
| 7–10 min | Hold permitted background work while another user requests a short chat | Actual compatible chat reserve; bounded queue; one active run per agent; no background reclassification shortcut |
| 10–12 min | Stop one request; reconnect/reload another client | Durable cancellation, no late output; replay retains saved answer without a duplicate send/model retry |
| 12–15 min | Verify two-task identity denial and drain an executor | Wrong task/slot/generation and direct backend denied; draining receives no new offers; surviving work remains scoped |

The pretrial machine-identity proof must already pass before any real context;
the timed denial checks confirm that same verified boundary. If fault recovery
does not fit the remaining timer, record it as incomplete rather than extending
the paid test. Public launch, customer data, computer/browser providers, business
writes and general autoscaling acceptance are separate milestones.

## Stop and cleanup

At 15 minutes, stop new test submissions and model attempts, persist final status
and mark unfinished work. Operator cleanup follows even after a failed journey.
An external owner/operator must track the resource deadline; M0 has no deployed
watchdog and a lost browser tab does not stop AWS resources.

Disable scaling/replacement, drain/stop the fleet, remove trial edge/compute/
network dependencies and inspect the actual inventory. List retained RDS snapshots,
S3 versions, ECR data, secrets and logs with their follow-up cleanup responsibility.
Do not delete pre-existing account resources or evidence by name/age alone.

The report must distinguish 15 active minutes, total billable resource lifetime,
estimated charges, unknown/cancelled provider usage and later actual billing.
Success requires useful scoped replies and denial/recovery checks, not merely a
reachable web page or successful container launch.
