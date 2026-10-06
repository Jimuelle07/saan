# Architecture notes - inventory sync

## Components
- **ERP connector**: polls the legacy ERP every 5 seconds and publishes events. (The ERP can't push.)
- **Queue**: partitioned by warehouse id so that events for a warehouse stay ordered.
- **Sync worker**: stateless; scales horizontally by partition count.
- **State store**: Postgres table `sku_levels(sku, warehouse_id, on_hand, reserved, updated_at)`.
- **Publisher**: pushes `available = on_hand - reserved` to the storefront.

## Ordering and idempotency
Every event carries a monotonically increasing sequence per warehouse. The worker stores the
last applied sequence and ignores anything older. This makes replays safe.

## Failure handling
- Storefront errors: retry with exponential backoff and jitter, then dead-letter queue.
- Poison messages: moved to the DLQ after 5 attempts; alert if the DLQ grows above 50.
- Worker crash: offsets are committed only after the database transaction commits.

## Why not change data capture?
We considered CDC on the ERP database, but the vendor contract forbids direct DB access.

## Open questions
- Should reservations expire automatically after 30 minutes of cart inactivity?
- Do we need multi-region? Probably not until the EU warehouse opens.
