#!/usr/bin/env bash
# Drops all leftover UUID-named test databases created by the integration tests.
# Safe: the regex only matches names starting with 8 hex chars + a dash (UUIDs),
# so it never touches `newsletter` or `postgres`.
#
# Usage:  ./scripts/clean_test_dbs.sh
#
# `set -euo pipefail` = stop on the first error instead of plowing ahead:
#   -e  exit if any command fails
#   -u  error on use of an unset variable
#   -o pipefail  a pipeline fails if ANY part fails (not just the last)
set -euo pipefail

# Connection settings (override by exporting these before running, e.g. PGHOST=...).
PGHOST="${PGHOST:-localhost}"
PGUSER="${PGUSER:-jyothi}"

# 1) First psql PRINTS one `DROP DATABASE "<name>";` line per UUID-named DB.
# 2) The pipe `|` feeds those lines into a second psql, which RUNS them.
psql -h "$PGHOST" -U "$PGUSER" -d postgres -t -A \
  -c "SELECT 'DROP DATABASE \"' || datname || '\";'
      FROM pg_database
      WHERE datname ~ '^[0-9a-f]{8}-';" \
| psql -h "$PGHOST" -U "$PGUSER" -d postgres

echo "Done. Remaining test DBs:"
psql -h "$PGHOST" -U "$PGUSER" -d postgres -t -A \
  -c "SELECT count(*) FROM pg_database WHERE datname ~ '^[0-9a-f]{8}-';"
