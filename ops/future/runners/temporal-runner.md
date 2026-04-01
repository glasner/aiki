# Temporal Cloud Runner Plugin

> Back to [Runner Plugins Architecture](../runners.md)

An external executable that models agent task execution as a [Temporal](https://temporal.io/) workflow — gaining durable execution, automatic retries, observability, and multi-step orchestration.

## What is Temporal Cloud?

[Temporal Cloud](https://temporal.io/cloud) is a fully managed workflow orchestration service. It coordinates work by persisting workflow state and routing tasks to your workers, but **does not provide compute** — workers are your processes, running on your infrastructure.

- **Workflow durability** — Every state transition is persisted. If a worker dies mid-task, Temporal replays history on another worker.
- **Automatic retries** — Configurable retry policies with backoff
- **Multi-step orchestration** — Chain activities into complex workflows
- **gRPC API** — CLI (`temporal`), Go/Python/TypeScript/Java/.NET SDKs
- **Pricing** — Starting at $50/million actions, plus storage

## Different Execution Model: Orchestrated

Temporal introduces a **fourth execution model**:

```
Other runners:                          Temporal runner:
  aiki → Runner → Compute              aiki → Temporal Cloud → Worker (on compute)
  (direct)                              (orchestrated)
```

Workers are long-running processes that poll Temporal task queues and execute workflow/activity code. They must be pre-deployed with agent tools installed.

## Worker Deployment

```python
# worker.py — Temporal worker that runs agent tasks
import asyncio, os
from dataclasses import dataclass
from temporalio import activity, workflow
from temporalio.client import Client
from temporalio.worker import Worker

@dataclass
class AgentTaskInput:
    command: list[str]
    env: dict[str, str]
    cwd: str

@dataclass
class AgentTaskResult:
    stdout: str
    stderr: str
    exit_code: int

@activity.defn
async def run_agent_command(input: AgentTaskInput) -> AgentTaskResult:
    proc = await asyncio.create_subprocess_exec(
        *input.command,
        stdout=asyncio.subprocess.PIPE, stderr=asyncio.subprocess.PIPE,
        cwd=input.cwd, env={**dict(os.environ), **input.env},
    )
    while proc.returncode is None:
        activity.heartbeat("agent running")
        try:
            stdout, stderr = await asyncio.wait_for(proc.communicate(), timeout=30)
        except asyncio.TimeoutError:
            continue
    return AgentTaskResult(stdout=stdout.decode(), stderr=stderr.decode(), exit_code=proc.returncode)

@workflow.defn
class RunAgentWorkflow:
    @workflow.run
    async def run(self, input: AgentTaskInput) -> AgentTaskResult:
        return await workflow.execute_activity(
            run_agent_command, input,
            start_to_close_timeout=timedelta(hours=2),
            heartbeat_timeout=timedelta(minutes=5),
            retry_policy=RetryPolicy(maximum_attempts=3),
        )
```

## `provision`

```bash
TEMPORAL_ADDRESS="$NAMESPACE.$ACCOUNT.tmprl.cloud:7233"

temporal workflow list --address "$TEMPORAL_ADDRESS" --namespace "$NAMESPACE" \
    --api-key "$TEMPORAL_API_KEY" --limit 1 > /dev/null 2>&1 || {
    echo '{"status":"error","error":"Cannot connect to Temporal Cloud namespace '$NAMESPACE'"}'
    exit 1
}

echo '{"status":"ok","environment":{"type":"temporal","namespace":"'$NAMESPACE'","task_queue":"'$TASK_QUEUE'"}}'
```

## `exec`

```bash
WORKFLOW_ID="aiki-task-${TASK_ID}"

temporal workflow execute \
    --address "$TEMPORAL_ADDRESS" --namespace "$NAMESPACE" --api-key "$TEMPORAL_API_KEY" \
    --task-queue "$TASK_QUEUE" --type "RunAgentWorkflow" --workflow-id "$WORKFLOW_ID" \
    --input "$(jq -n --argjson command "$COMMAND_JSON" --argjson env "$ENV_JSON" --arg cwd "$CWD" \
        '{command: $command, env: $env, cwd: $cwd}')" \
    --execution-timeout "$TIMEOUT" --output json > /tmp/temporal-result

EXIT_CODE=$(jq -r '.exit_code // 1' /tmp/temporal-result)
STDOUT=$(jq -r '.stdout // ""' /tmp/temporal-result)
STDERR=$(jq -r '.stderr // ""' /tmp/temporal-result)

jq -n --arg stdout "$STDOUT" --arg stderr "$STDERR" --argjson exit_code "$EXIT_CODE" \
    '{"status":"ok","exit_code":$exit_code,"stdout":$stdout,"stderr":$stderr}'
```

## `cleanup`

```bash
STATUS=$(temporal workflow describe --address "$TEMPORAL_ADDRESS" --namespace "$NAMESPACE" \
    --api-key "$TEMPORAL_API_KEY" --workflow-id "$WORKFLOW_ID" --output json 2>/dev/null | jq -r '.status')

if [ "$STATUS" = "RUNNING" ] || [ "$STATUS" = "Running" ]; then
    temporal workflow cancel --address "$TEMPORAL_ADDRESS" --namespace "$NAMESPACE" \
        --api-key "$TEMPORAL_API_KEY" --workflow-id "$WORKFLOW_ID" 2>/dev/null || true
fi
echo '{"status":"ok"}'
```

## Configuration

```yaml
runner: temporal
runners:
  temporal:
    namespace: my-namespace
    account: my-account
    task_queue: aiki-agent-tasks
    workflow_type: RunAgentWorkflow
    execution_timeout: 7200
    task_timeout: 3600
    retry:
      max_attempts: 3
      initial_interval: 10
      backoff_coefficient: 2.0
    # Auth: reads TEMPORAL_API_KEY from env
```

## Composability with Other Runners

Workers can run on ANY compute platform:

```
Temporal + Kubernetes    → Workers on K8s pods (most common)
Temporal + exe.dev       → Workers on persistent exe.dev VMs
Temporal + Coder         → Workers in Coder workspaces
Temporal + Fargate       → Workers as serverless ECS tasks
Temporal + Fly.io        → Workers on Fly Machines
```

## Limitations

- **Not a compute platform** — Workers must be deployed and managed separately.
- **Highest complexity** — Requires Temporal Cloud account + worker fleet + task queue configuration.
- **Worker pre-deployment** — Workers must be running BEFORE tasks are submitted.
- **Payload size limits** — Activity inputs/outputs limited to 2MB.
- **Cost** — You pay for both Temporal Cloud AND the compute layer where workers run.
