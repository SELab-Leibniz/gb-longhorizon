# 120 — `gb-trace --profile` lists equal counts in the wrong order

**Reporter:** developer · **Component:** `gb-trace`

When several addresses have the same count, `--profile` lists them in a
different order from the one GEP 1 Appendix A specifies, so our diffs
against reference profiles fail even though the counts are the same.
