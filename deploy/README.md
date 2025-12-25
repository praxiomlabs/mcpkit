# Deployment Examples

This directory contains production-ready deployment configurations for mcpkit MCP servers.

## Docker

### Building the Image

```bash
# From the repository root
docker build -f deploy/docker/Dockerfile -t mcpkit/mcp-server:latest .
```

### Running with Docker

```bash
docker run -p 3000:3000 mcpkit/mcp-server:latest
```

### Docker Compose

**Development:**
```bash
docker-compose -f deploy/docker/docker-compose.yml up
```

**Production with replicas:**
```bash
docker-compose -f deploy/docker/docker-compose.yml -f deploy/docker/docker-compose.prod.yml up -d
```

**With observability (Jaeger + Prometheus):**
```bash
docker-compose -f deploy/docker/docker-compose.yml --profile observability up
```

## Kubernetes

### Prerequisites

- Kubernetes cluster (1.25+)
- kubectl configured
- (Optional) nginx-ingress controller for external access
- (Optional) metrics-server for HPA

### Quick Start

```bash
# Deploy to cluster
kubectl apply -k deploy/kubernetes/

# Check status
kubectl -n mcp-system get pods

# Port forward for local testing
kubectl -n mcp-system port-forward svc/mcp-server 3000:80
```

### Testing

```bash
curl -X POST http://localhost:3000/mcp \
  -H "Content-Type: application/json" \
  -H "MCP-Protocol-Version: 2025-11-25" \
  -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","clientInfo":{"name":"curl","version":"1.0"}}}'
```

### Production Deployment

1. Create a production overlay:

```yaml
# deploy/kubernetes/overlays/production/kustomization.yaml
apiVersion: kustomize.config.k8s.io/v1beta1
kind: Kustomization

resources:
  - ../../

namespace: mcp-production

images:
  - name: mcpkit/mcp-server
    newName: your-registry/mcpkit/mcp-server
    newTag: v0.4.0

patches:
  - patch: |-
      - op: replace
        path: /spec/replicas
        value: 3
    target:
      kind: Deployment
      name: mcp-server

configMapGenerator:
  - name: mcp-server-config
    behavior: merge
    literals:
      - RUST_LOG=warn,http_server_example=info
```

2. Deploy:
```bash
kubectl apply -k deploy/kubernetes/overlays/production/
```

### Components

| File | Description |
|------|-------------|
| `namespace.yaml` | Dedicated namespace for MCP resources |
| `configmap.yaml` | Environment configuration |
| `deployment.yaml` | Pod deployment with health checks |
| `service.yaml` | ClusterIP service + ServiceAccount |
| `ingress.yaml` | External access (nginx-ingress) |
| `hpa.yaml` | Horizontal Pod Autoscaler |
| `pdb.yaml` | Pod Disruption Budget |

### Security Features

- Non-root container execution
- Read-only root filesystem
- Dropped capabilities
- ServiceAccount with no default token mount
- Resource limits and requests
- Pod anti-affinity for distribution

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `MCP_BIND_ADDR` | `0.0.0.0:3000` | Server bind address |
| `RUST_LOG` | `info` | Log level filter |
| `OTEL_SERVICE_NAME` | `mcp-server` | OpenTelemetry service name |
| `OTEL_EXPORTER_OTLP_ENDPOINT` | - | OTLP collector endpoint |
