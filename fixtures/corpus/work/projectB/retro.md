# Sprint 22 Retrospective - ClinicBook

## What went well
- Shipped the reschedule flow two days early.
- Crash-free sessions climbed to 99.7% after fixing the calendar permission bug on Android 14.
- Pairing between design and engineering on the booking screen saved a lot of back and forth.

## What didn't go well
- App Store review rejected the build because the privacy manifest was missing a reason code.
  Lost three days.
- Too many unplanned requests from the clinic partners in the middle of the sprint.
- Flaky end-to-end tests on the iOS simulator blocked merges twice.

## Ideas
- Add a release checklist item for privacy manifest and permission strings.
- Partner requests go through the PM and land in the next sprint unless urgent.
- Quarantine flaky tests automatically after two consecutive failures.

## Action items
| Owner  | Item                                           |
|--------|------------------------------------------------|
| Sam    | Create release checklist in the repo           |
| Aisha  | Set up flaky test quarantine job               |
| Jordan | Talk to partner managers about intake process  |

Team mood (1-5): average 3.6, up from 3.1 last sprint.
