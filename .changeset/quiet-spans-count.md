---
"@rustrak/server": "patch"
---

Storage stats and the cleanup preview estimate span counts from a random sample of 1,000 transactions per project instead of counting the spans table, which took over a minute on large instances. Estimated counts are marked with ≈ in the dashboard.
