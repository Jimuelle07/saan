# Inventory Sync Service (Project A)

Keeps warehouse stock levels in sync between the ERP system and the online storefront.

## Why
Customers were ordering items that were out of stock because the storefront only refreshed
inventory once a night. This service streams stock changes within seconds.

## How it works
1. The ERP publishes stock movement events (receipts, picks, adjustments) to a message queue.
2. The sync worker consumes events, de-duplicates by event id, and computes available quantity.
3. Updated quantities are pushed to the storefront API in batches of up to 100 SKUs.
4. A nightly reconciliation job compares full snapshots and reports drift.

## Running locally
```
docker compose up -d queue postgres
make migrate
make run-worker
```

## Configuration
| Variable            | Default | Description                           |
|---------------------|---------|---------------------------------------|
| `BATCH_SIZE`        | 100     | SKUs per storefront API call          |
| `RECONCILE_CRON`    | 0 2 * * * | Nightly snapshot comparison         |
| `MAX_RETRY`         | 5       | Retries with exponential backoff      |

## Owners
Warehouse platform team. Pager rotation in the on-call doc.
