# Q3 Launch Sync - Inventory Sync rollout

Attendees: Priya (eng lead), Tom (PM), Grace (ops), Luis (storefront), me

## Status
- Worker is processing events in production shadow mode; no writes yet.
- Shadow comparison shows 99.6% of quantities match the nightly job.
- The 0.4% mismatches are mostly bundles (kits made of multiple SKUs).

## Decisions
- Launch date for the real-time sync: **August 12**, behind a feature flag.
- Roll out to 10% of storefront traffic first, then 50%, then 100% over one week.
- Bundles are excluded from the first launch; they keep using the nightly job.

## Risks
- Storefront API rate limit (600 requests per minute) could be hit during big receipts.
  Mitigation: batch updates and add a token bucket limiter.
- Ops is worried about alert fatigue; agree on thresholds before launch.

## Action items
- [ ] Priya: implement bundle exclusion list
- [ ] Luis: confirm the rate limit increase with the storefront vendor
- [ ] Grace: write the runbook for disabling the flag
- [ ] Tom: draft the launch announcement for the customer support team
- [ ] Me: dashboards for event lag and drift

Next sync: Thursday.
