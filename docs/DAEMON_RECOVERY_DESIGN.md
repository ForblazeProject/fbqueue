# Linux daemon recovery — stage 1 and staged design

Status: stage 1 is implemented in Linux v0.9.6. Stage 2 remains a proposal.
The older v0.9.5 download
does not include this work. Windows recovery is outside this implementation's
scope. The guarantees below are fail-closed no-replay behavior, not exactly-once
execution or transparent recovery of running workers.

## Goal

Avoid starting an expensive job twice after daemon failure. Prefer an explicit
unknown/held outcome over guessing that a job is safe to replay. Do not promise
exactly-once execution or atomicity of a user's external side effects.

## Previous behavior and remaining limits

The older v0.9.5 daemon uses a PID-file check, not an exclusive lifetime lock. Startup moves
all `queue/running` records back to `queue/new`; worker identity and completion
results exist only in the old daemon's memory. An old worker can therefore survive
while a replacement launches the same job again. A PID alone cannot distinguish
process reuse. The earlier replay characterization has been replaced: the current
test leaves the original worker alive, verifies a replacement daemon can acquire
the lock, and checks that execution is neither replayed nor guessed successful.

## Implemented stage 1: fail closed (Linux v0.9.6)

- Hold an exclusive nonblocking `flock` on a permanent `run/daemon.lock` inode for
  the daemon's lifetime. Acquire before reading recovery state or scheduling.
  Never unlink/replace the lock inode. Ensure child processes do not retain the
  lock descriptor, including the fork-to-exec interval. PID metadata is not the lock.
- Publish daemon identity atomically. Use Linux boot ID, PID and process starttime;
  verify identity through a pidfd before signaling. Refuse stale/ambiguous targets.
- Never automatically move unresolved running records back to pending. Preserve
  them in `running` with an explicit `UNKNOWN`/recovery-held status and reason.
- Pause new dispatch globally while execution
  may still be alive but cannot be safely identified. Merely reserving recorded
  cost may underestimate surviving descendants.
- After a verified reboot, absent a valid completion record, preserve the job as
  `INTERRUPTED` (outcome unknown). Do not invent an exit code or automatically retry.
- Unknown/held work is not successful history and must not satisfy dependencies
  or be pruned. Cancellation cannot claim success without safely identified work.
- Require explicit reconciliation or a warned manual retry. A retry must not
  destroy the old evidence. The operator interface needs agreement before coding.

This stage prevents blind replay but does not recover exit codes from orphaned
workers. Legacy running records have no trustworthy identity and must be held.

Terminal metadata is written and synced before moving records. Completed/cancelled
history is not silently overwritten; failed metadata writes retain the source and
hold dispatch. Post-rename directory-sync errors latch `run/recovery.error`; an
empty running queue is not proof of repaired durability and cannot clear that
error. Storage that cannot persist even this warning cannot provide a durable
failure record; the daemon still fails closed in memory, with no exactly-once or
hardware-failure guarantee. Operator reconciliation/retry tooling is still deferred.

## Proposed stage 2: per-attempt supervisor and durable receipts

Use an internal Rust supervisor mode in the same binary, without a new service or
external database. Each job attempt has an immutable attempt ID and a versioned
sidecar directory. The supervisor remains the payload's parent even if the daemon
dies, waits for completion, enforces walltime, handles cancellation and records an
atomic completion receipt. A replacement daemon adopts monitoring/accounting,
not Unix parenthood; it cannot `waitpid` a process that is not its child.

### Launch/crash protocol

1. Persist launch intent and claim the pending job before spawning a supervisor.
2. The supervisor durably publishes its own boot ID/PID/starttime and attempt
   identity before launching the payload. It must not inherit the daemon lock.
3. Keep the payload in a process group separate from the supervisor so group
   cancellation does not kill the receipt writer. Persist the payload identity.
4. The supervisor writes a receipt containing the actual exit code or signal,
   completion time and policy reason. Use an attempt-specific cancellation request.
5. The daemon idempotently finalizes `.job` state from a matching receipt.

A crash after claim but without sufficient launch/receipt evidence means held
unknown work, not permission to retry. If the payload finishes before its receipt
commits and no supervisor can prove the outcome, that outcome remains unknown.
Authoritative records need temporary-file write, file sync, atomic rename and
directory sync; rename alone does not guarantee power-loss durability. I/O errors
must block dispatch rather than silently discarding evidence.

### Resource and status rules

- Verified live supervisors retain the original queue/cost reservation.
- Walltime uses the original boot-relative deadline; restart must not reset it.
- `DONE` requires a verified successful receipt; `FAILED` requires a verified
  failure. Keep terminating signals separate from process exit codes.
- `CANCELLED`/`TIMEOUT` require confirmed termination under the supported process
  containment model. A written request alone is not confirmation.
- `UNKNOWN` means execution may exist. `INTERRUPTED` means observation/execution
  was lost, not proof the user's computation had no effects.
- Preserve the stage-1 native/PBS status mapping: interrupted or unknown outcomes
  must not look successfully finished when stage-2 receipt handling is added.

Pidfds identify individual processes, not a whole process group. Do not signal a
stale PGID after ownership is lost. Process groups do not contain arbitrary
`setsid()` descendants; stronger containment would require a separate cgroup
scope/design decision.

## Compatibility and rollout

Preserve job IDs, existing `.job` fields, PBS environment/output conventions and
terminal history. Legacy pending work remains eligible under current resource
validation; legacy running work is held. Keep sidecar evidence until safe
finalization/archiving. Require a controlled upgrade with old daemons stopped;
old binaries do not honor the new lock/protocol. Limit guarantees to one Linux
host/PID namespace on persistent local storage. Do not claim equivalent Windows
behavior before its separate implementation and tests.

## Approved stage-1 decisions and deferred interfaces

1. Prefer a global dispatch pause for unresolved live work, rather than guessing
   available resources. Only valid different-boot evidence permits `INTERRUPTED`
   history in stage 1. Same-boot work remains UNKNOWN even after its worker exits;
   disappearance alone cannot recover the lost outcome.
2. Keep workers running during explicit `daemon stop`; use job cancellation to
   stop payloads. Only the proposed stage-2 supervisor would continue writing
   receipts without the daemon; no such supervisor exists in v0.9.6.
3. Native/PBS held/unknown displays are implemented. An explicit reconciliation/
   retry CLI remains deferred; stage 1 has no automatic way to clear UNKNOWN just
   because a worker disappeared. Preserve evidence and verify processes/external
   effects before any controlled administrative repair or manual resubmission.
4. Confirm process-group containment is sufficient initially; arbitrary escaping
   descendants/cgroups are not silently guaranteed.

## Tests and remaining acceptance work

Stage-1 tests cover external lifetime locks, concurrent cold starts, stable lock
inodes, a worker surviving daemon death without replay or inherited-lock blockage,
stale/corrupt daemon identity, old-boot fixtures, legacy/corrupt job boot evidence,
recovery I/O failure, cancellation record/history conflicts, request metadata
failure, and cancellation work preventing idle retirement. Boot mismatch is
simulated using isolated records; the test machine is not rebooted.

The following acceptance work belongs to the still-unimplemented stage 2:

Competing daemon starts; lock descriptor inheritance/release; crashes at each
launch and receipt boundary; a worker surviving daemon death without replay;
stale/reused PID and boot mismatch; corrupt/missing records; recovered walltime
and cancellation; unresolved resource holds; explicit retry behavior; and migration
of legacy pending/running/completed records. Tests must own/isolate every process
and never infer safety merely from a PID file or a successful ordinary restart.
