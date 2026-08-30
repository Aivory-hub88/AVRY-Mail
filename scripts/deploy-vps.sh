#!/bin/bash
set -e
echo "Deploying Aivory Mail to VPS..."
docker network create aivory-network 2>/dev/null || true
docker compose up -d --build
echo "Waiting for health..."
sleep 5
curl -sf http://localhost:8095/health && echo " ✓ Aivory Mail healthy" || echo " ! health check failed"
