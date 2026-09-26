#!/usr/bin/env bash
# The Launcher: starts a Test-bed (a real Compliance built from mvdmio-suite, with its own throwaway Postgres and
# data folder, and a Personal token for seed user 1 on account 1), prints one JSON line {"url","token"} to stdout,
# and keeps it running until Ctrl-C or SIGTERM. Linux and macOS only.
#
#   scripts/test-bed.sh [--until-stdin-closes]
#
# --until-stdin-closes also stops the Test-bed when stdin reaches end of file, so a caller that holds the pipe
# open stops it by exiting, however it exits.
#
# Environment:
#   MVDMIO_SUITE_DIR          the mvdmio-suite checkout (default: ../mvdmio-suite next to this repo)
#   TEST_BED_BOOT_TIMEOUT     seconds allowed for the build lock, the build, and the boot together
#                             (default: 1800)
#
# Nothing but the JSON line goes to stdout. Progress goes to stderr; build and server logs go to files in the
# Test-bed's own folder, and their tail goes to stderr when something fails.

set -euo pipefail

POSTGRES_IMAGE="postgres:18.1"
POSTGRES_PASSWORD="test-bed"
DATABASE="compliance"
# Auth's own Development address: the Test-bed never names production Auth, and Auth does not need to run.
LOCAL_AUTH_URL="http://localhost:13001"

say() { printf 'test-bed: %s\n' "$*" >&2; }

until_stdin_closes=false
for argument in "$@"; do
   case "$argument" in
      --until-stdin-closes) until_stdin_closes=true ;;
      *) say "unknown argument: $argument"; exit 2 ;;
   esac
done

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
suite_dir="${MVDMIO_SUITE_DIR:-$repo_root/../mvdmio-suite}"
if [[ ! -d "$suite_dir/Compliance/src/mvdmio.Compliance.Web" ]]; then
   say "no mvdmio-suite checkout at $suite_dir; set MVDMIO_SUITE_DIR to one"
   exit 1
fi
suite_dir="$(cd "$suite_dir" && pwd)"
project_dir="$suite_dir/Compliance/src/mvdmio.Compliance.Web"

for tool in docker dotnet bun curl; do
   if ! command -v "$tool" >/dev/null 2>&1; then
      say "$tool is not on PATH"
      exit 1
   fi
done

boot_timeout="${TEST_BED_BOOT_TIMEOUT:-1800}"
deadline=$((SECONDS + boot_timeout))

bed_dir="$(mktemp -d "${TMPDIR:-/tmp}/compliance-test-bed.XXXXXXXX")"
bed_id="$(basename "$bed_dir" | sed 's/^compliance-test-bed\.//' | tr '[:upper:]' '[:lower:]')"
container="compliance-test-bed-$bed_id"
server_pid=""
build_pid=""
watcher_pid=""
lock_dir=""

# Stops one child process: SIGTERM, up to 10 seconds, then SIGKILL.
stop_process() {
   local pid="$1"
   kill -0 "$pid" 2>/dev/null || return 0
   kill -TERM "$pid" 2>/dev/null || true
   for _ in $(seq 1 50); do
      kill -0 "$pid" 2>/dev/null || break
      sleep 0.2
   done
   kill -KILL "$pid" 2>/dev/null || true
   wait "$pid" 2>/dev/null || true
}

# Stops Compliance, removes the container, and deletes the folder. Safe to run more than once.
cleanup() {
   trap - EXIT INT TERM
   if [[ -n "$watcher_pid" ]]; then
      kill "$watcher_pid" 2>/dev/null || true
   fi
   if [[ -n "$build_pid" ]]; then
      stop_process "$build_pid"
   fi
   if [[ -n "$lock_dir" ]]; then
      rm -rf "$lock_dir"
   fi
   if [[ -n "$server_pid" ]]; then
      stop_process "$server_pid"
   fi
   docker rm -f "$container" >/dev/null 2>&1 || true
   rm -rf "$bed_dir"
}

fail() {
   say "$1"
   if [[ -n "${2:-}" && -f "$2" ]]; then
      say "last lines of $(basename "$2"):"
      tail -n 40 "$2" >&2 || true
   fi
   cleanup
   exit 1
}

trap cleanup EXIT
trap 'cleanup; exit 130' INT
trap 'cleanup; exit 143' TERM

if $until_stdin_closes; then
   # A bash loop rather than `cat`, so stopping this one process stops the watch.
   exec 3<&0
   ( while IFS= read -r _ <&3; do :; done; kill -TERM $$ 2>/dev/null ) &
   watcher_pid=$!
   exec 3<&-
fi

# A free TCP port on the loopback: a random one nothing answers on.
free_port() {
   local port
   for _ in $(seq 1 100); do
      port=$((20000 + RANDOM % 40000))
      if ! (: <"/dev/tcp/127.0.0.1/$port") 2>/dev/null; then
         echo "$port"
         return 0
      fi
   done
   return 1
}

say "starting Postgres ($POSTGRES_IMAGE) as $container"
docker run --detach --name "$container" --label compliance-test-bed=1 \
   --env POSTGRES_PASSWORD="$POSTGRES_PASSWORD" --env POSTGRES_DB="$DATABASE" \
   --publish 127.0.0.1::5432 "$POSTGRES_IMAGE" -c fsync=off -c synchronous_commit=off \
   >/dev/null 2>"$bed_dir/docker.log" || fail "docker run failed" "$bed_dir/docker.log"
postgres_port="$(docker port "$container" 5432/tcp | head -n 1 | sed 's/.*://')"

# The image's init server listens only on its Unix socket, so a TCP answer means the real server is up.
for _ in $(seq 1 120); do
   docker exec "$container" pg_isready --host 127.0.0.1 --username postgres >/dev/null 2>&1 && break
   sleep 0.5
done
docker exec "$container" pg_isready --host 127.0.0.1 --username postgres >/dev/null 2>&1 \
   || fail "Postgres did not become ready"

# One build at a time per checkout: two Launchers building into the same bin and obj break each other. The
# build output is then copied into this Test-bed's own folder, so a later build cannot change a running server.
build_lock="$project_dir/obj/.test-bed-build.lock"
mkdir -p "$project_dir/obj"
while ! mkdir "$build_lock" 2>/dev/null; do
   holder="$(cat "$build_lock/pid" 2>/dev/null || true)"
   # Stale: its Launcher is gone, or it died between making the lock and writing its PID a minute ago.
   if { [[ -n "$holder" ]] && ! kill -0 "$holder" 2>/dev/null; } \
      || { [[ -z "$holder" ]] && [[ -n "$(find "$build_lock" -maxdepth 0 -mmin +1 2>/dev/null)" ]]; }; then
      rm -rf "$build_lock"
      continue
   fi
   ((SECONDS < deadline)) || fail "another Launcher held the build lock ($build_lock) for too long"
   sleep 1
done
lock_dir="$build_lock"
echo $$ >"$build_lock/pid"

say "building Compliance (the first build takes minutes)"
# In the background, so a SIGTERM during the build is handled at once rather than after it.
dotnet build "$project_dir" --configuration Debug </dev/null >"$bed_dir/build.log" 2>&1 &
build_pid=$!
while kill -0 "$build_pid" 2>/dev/null; do
   ((SECONDS < deadline)) || fail "the Compliance build did not finish within ${boot_timeout}s" "$bed_dir/build.log"
   sleep 1
done
wait "$build_pid" || fail "the Compliance build failed" "$bed_dir/build.log"
build_pid=""
built_dll="$(find "$project_dir/bin/Debug" -maxdepth 2 -name mvdmio.Compliance.Web.dll | head -n 1)"
[[ -n "$built_dll" ]] || fail "the build left no mvdmio.Compliance.Web.dll under $project_dir/bin/Debug"
output_dir="$(dirname "$built_dll")"
mkdir -p "$bed_dir/app"
cp -R "$output_dir/." "$bed_dir/app/"
rm -rf "$lock_dir"
lock_dir=""

port="$(free_port)" || fail "found no free port"
url="http://localhost:$port"
mkdir -p "$bed_dir/data"

say "starting Compliance at $url"
(
   cd "$project_dir"
   ASPNETCORE_ENVIRONMENT=Development \
   DOTNET_ENVIRONMENT=Development \
   urls="$url" \
   DbConnection="Server=127.0.0.1;Port=$postgres_port;Database=$DATABASE;User Id=postgres;Password=$POSTGRES_PASSWORD;" \
   Data__Path="$bed_dir/data" \
   Hosting__BaseUrl="$url" \
   Compliance__PublicBaseUrl="$url" \
   IdentityServer__BaseUrl="$LOCAL_AUTH_URL" \
   Email__Enabled=false \
   Assistant__UseScriptedModelClient=true \
   exec dotnet exec "$bed_dir/app/mvdmio.Compliance.Web.dll"
) </dev/null >"$bed_dir/server.log" 2>&1 &
server_pid=$!

until curl --silent --fail --output /dev/null "$url/openapi/v1.json"; do
   kill -0 "$server_pid" 2>/dev/null || fail "Compliance stopped while starting" "$bed_dir/server.log"
   ((SECONDS < deadline)) || fail "Compliance did not answer within ${boot_timeout}s" "$bed_dir/server.log"
   sleep 1
done

# Every Development boot truncates and reseeds, so the Legal Acceptance, the Personal token, and the Assistant
# allowance are written here. The seed leaves Account 1 no included Assistant funds, so the allowance gets 100 USD of
# extra, which scripted Turns spend.
# The token is PersonalTokenSecret's shape: the prefix, then 32 random bytes in base64url without padding; the row
# keeps only the SHA-256 of its UTF-8 bytes.
token="mvdm_pat_$(head -c 32 /dev/urandom | base64 | tr '+/' '-_' | tr -d '=\n')"
if ! docker exec --interactive "$container" psql --username postgres --dbname "$DATABASE" --quiet \
   --set ON_ERROR_STOP=1 --set token="$token" >/dev/null 2>"$bed_dir/seed.log" <<'SQL'
INSERT INTO compliance.legal_acceptances (account_id, accepted_by_user_id)
VALUES (1, 1)
ON CONFLICT (account_id) DO NOTHING;

INSERT INTO auth.personal_tokens (user_id, name, secret_hash, account_id, expires_at)
VALUES (1, 'Test-bed', sha256(convert_to(:'token', 'UTF8')), 1, NULL);

INSERT INTO compliance.assistant_allowances (account_id, included_period_key, extra_usd)
VALUES (1, 'comp:' || to_char(now() AT TIME ZONE 'UTC', 'YYYY-MM'), 100)
ON CONFLICT (account_id) DO UPDATE SET extra_usd = EXCLUDED.extra_usd;
SQL
then
   fail "writing the seed rows failed" "$bed_dir/seed.log"
fi

say "ready; Ctrl-C stops it"
printf '{"url":"%s","token":"%s"}\n' "$url" "$token"

# Waits for Compliance; a trapped signal interrupts the wait and runs the cleanup.
set +e
wait "$server_pid"
status=$?
set -e
server_pid=""
fail "Compliance stopped (exit $status)" "$bed_dir/server.log"
