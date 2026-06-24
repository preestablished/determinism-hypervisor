# Privacy And Closeout

## Private Handoff Contract

The private env file must contain the bridge fields from the request:

```dotenv
BRIDGE_HYPERVISOR_ENDPOINT=unix:///run/dh/grpc.sock
BRIDGE_PRIVATE_ROOT=<private bridge root>
BRIDGE_WORKLOAD_IMAGE_REF=<operator-approved workload image ref>
BRIDGE_CAPTURE_SPEC_REF=<operator-approved capture spec ref>
BRIDGE_REFERENCE_WORKLOAD_CHECKOUT=/home/infra-admin/git/preestablished/reference-workload
BRIDGE_REAL_SNAPSHOT_REF=<64 hex snapshot ref>
SNAPSTORE_DATA_ROOT=<private snapstore data root>
SNAPSTORE_GRPC_UDS_PATH=<private snapstore uds path>
DH_M9_IMAGE_CACHE=<image cache path>
```

The file path should be operator-owned, outside any git checkout, and mode
`0600` or stricter. The parent directories should be mode `0700`.

## Redaction Rules

Never write these values to public docs, bead notes, commits, PR descriptions,
or shared terminal output:

- `BRIDGE_REAL_SNAPSHOT_REF`;
- lease token bytes;
- private worker or snapstore socket paths;
- private snapstore data root;
- private bridge root;
- credentials, cookies, session secrets, or operator tokens;
- raw worker errors;
- raw snapstore errors;
- raw manifest dumps.

The generator should keep a list of private literals and scan every public
summary before writing it. Treat a match as a hard failure.

## Sanitized Success Note

Use a note shape like this for the determinism-hypervisor bead:

```text
Durable M9 READY snapshot handoff generator implemented.

Public results:
- M9 artifact validation: pass
- image cache registration: pass
- durable snapstore root populated: pass
- READY TakeSnapshot: pass
- RestoreSnapshot verification: pass
- source/restored lease cleanup: pass
- private handoff file mode verified: pass
- public summary redaction sweep: pass

Private handoff path and snapshot ref were provided only through the operator
private channel.
```

Use a similar sanitized note in the original request tracker. Do not include
the private handoff path if that path is considered sensitive.

## Sanitized Blocker Note

If acceptance cannot complete, leave the bead open and use a note like:

```text
Durable M9 READY snapshot handoff remains blocked on <component>.

Sanitized status:
- preflight: <pass/fail/not reached>
- M9 artifact validation: <pass/fail/not reached>
- snapstore startup/connect: <pass/fail/not reached>
- READY snapshot generation: <pass/fail/not reached>
- RestoreSnapshot verification: <pass/fail/not reached>
- handoff file write: <pass/fail/not reached>
- public redaction sweep: <pass/fail/not reached>

Raw logs and private values remain only in the operator-private evidence root.
Next unblock step: <single concrete action>.
```

## Bridge Follow-Through

After the generator succeeds, the bridge operator can use the existing bridge
plan at:

```text
/home/infra-admin/git/preestablished/rom-operator-bridge/.agents/plans/live-restore-snapshot-acceptance-o73/
```

The produced snapstore data root should be served by `snapstore-server`, and
`dh-workerd` should be started with snapstore enabled:

```bash
cargo run -p dh-worker --bin dh-workerd -- serve \
  --uds /run/dh/grpc.sock \
  --snapstore-uds "<private snapstore uds>"
```

Do not use `--no-snapstore` for the bridge acceptance run.

## Repository Closeout

Before ending the implementation session:

```bash
git status --short
cargo test -p dh-worker --test arch_dependency_rule
cargo test -p dh-worker --bin dh-m9-ready-handoff
git add <changed files>
git commit -m "Add M9 READY snapshot handoff generator"
git pull --rebase
bd dolt push
git push
git status --short --branch
```

If only docs or the plan changed, replace the cargo commands with an explicit
statement that no code changed and no compile/test gate was required. The final
status must show the branch up to date with origin.
