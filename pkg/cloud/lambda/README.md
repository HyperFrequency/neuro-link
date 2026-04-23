# Lambda Labs deploy

Lambda Labs has no first-party Python SDK — deploy is via REST API + SSH. This directory ships:

- `client.py` — thin Python wrapper around `POST https://cloud.lambdalabs.com/api/v1/instances`, polling for `status=active`, returning `(instance_id, ssh_host)`.
- `bootstrap.sh` — runs on the instance after SSH; installs `uv`, clones the monorepo, pulls the model triple, starts `neuro-link` via systemd.
- `deploy.sh` — end-to-end: provision → wait → scp bootstrap → run → smoke.
- `terraform/` — optional Terraform module (requires the community `carlpett/lambdalabs` provider).

## Env vars

```
LAMBDA_API_KEY   # https://cloud.lambdalabs.com/api-keys
LAMBDA_SSH_KEY   # registered key name (must match public key on Lambda console)
LAMBDA_REGION    # us-west-1 | us-east-1 | ...
LAMBDA_TYPE      # gpu_1x_h100 | gpu_1x_a100_pcie_40gb | ...
```

## Deploy flow (`deploy.sh`)

```bash
INSTANCE=$(python3 pkg/cloud/lambda/client.py provision)
python3 pkg/cloud/lambda/client.py wait "$INSTANCE"
HOST=$(python3 pkg/cloud/lambda/client.py host "$INSTANCE")
scp -o StrictHostKeyChecking=no pkg/cloud/lambda/bootstrap.sh ubuntu@$HOST:/tmp/
ssh ubuntu@$HOST 'bash /tmp/bootstrap.sh'
# Smoke test
curl -sf "http://$HOST:8080/" >/dev/null  # optuna-dashboard reachable
```

## Smoke test

Same contract as other targets — write `pkg/.proof/lambda.ready.json` with instance id, SSH host, optuna-dashboard URL, and SHA of running `neuro-link` binary.

## Pricing gotcha

Hourly, not per-second. Keep `deploy.sh --teardown-after $MINUTES` for cost control in CI runs. Instance provisioning 1–3 min typical; budget accordingly.
