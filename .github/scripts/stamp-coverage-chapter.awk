# Replace the block between COVERAGE_STATUS_BEGIN/END markers in the book's
# code-coverage chapter with a fresh status line. Driven by `make coverage`.
# Required vars: commit, ts, covered, link.

/<!-- COVERAGE_STATUS_BEGIN -->/ {
    print "<!-- COVERAGE_STATUS_BEGIN -->"
    print "**Last collected:** " ts " for commit `" commit "` — **" covered " line coverage**."
    print ""
    print "[Open the report](" link ")"
    in_block = 1
    next
}

/<!-- COVERAGE_STATUS_END -->/ {
    print "<!-- COVERAGE_STATUS_END -->"
    in_block = 0
    next
}

!in_block { print }
