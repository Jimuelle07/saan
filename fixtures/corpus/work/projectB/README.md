# ClinicBook mobile app (Project B)

A patient-facing mobile app for booking appointments at our partner clinics.

## Features
- Find a clinic by location or specialty (dermatology, pediatrics, physiotherapy)
- See open time slots and book in two taps
- Reminders by push notification 24 hours and 2 hours before the visit
- Cancel or reschedule up to 12 hours in advance
- Upload insurance card photos

## Tech stack
- React Native client (iOS and Android)
- GraphQL gateway in front of the clinic scheduling systems
- Push notifications via Firebase Cloud Messaging

## Getting started
```
yarn install
yarn ios     # or: yarn android
```
Use the staging environment by setting `API_ENV=staging` in `.env`.

## Privacy
Patient data is health information. Never log names, dates of birth or insurance numbers.
All screenshots in bug reports must use the demo patient account.

## Release cadence
Every two weeks, after QA sign-off on the release candidate build.
