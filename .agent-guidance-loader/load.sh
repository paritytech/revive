#!/usr/bin/env bash
#
# Prints company agent guidance as SessionStart hook output.
#
# Usage: load.sh [claude|codex|gemini|cursor|copilot]
#
# Clones the guidance repository into <repository root>/.agent-guidance/ and
# prints its GUIDANCE.md as one JSON object in the shape the named harness
# reads. Without an argument it prints the Claude Code shape. For the harnesses
# that show a hook's systemMessage to the user (Claude Code, Codex, Gemini CLI),
# the object also carries a one-line status that says whether the guidance was
# loaded. stdout carries only that object, diagnostics go to stderr, and the
# exit status is always 0, so a network or authentication failure never blocks
# a session. When the cache exists, the loader serves it and fetches updates in
# the background for the next session; see the cache branch below.
#
# The loader owns .agent-guidance/: an update discards local edits there. It
# reads text from the fetched copy and never executes it. GUIDANCE.md is still
# a set of instructions that agents act on, so whoever can push to the guidance
# repository can steer every agent that loads it. Changes to this script reach
# a repository only when the installer is run there again. When the fetched
# loader-version is greater than LOADER_VERSION, the status line tells the user
# how to upgrade. Cursor and Copilot have no status line, so for them the
# context asks the agent to tell the user instead.
#
# AGENT_GUIDANCE_URL overrides the repository URL (mirrors, tests).
# AGENT_GUIDANCE_BACKGROUND_BUDGET overrides the time limit of the background
# fetch in seconds (tests).
#
# Compatible with bash 3.2 (macOS /bin/bash). Needs git and standard utilities
# (awk, tr, find, touch); uses python3 for JSON escaping when it is available.

# Must equal the loader-version file at the root of the guidance repository.
LOADER_VERSION=4

# fd 3 keeps the real stdout for the JSON object. fd 1 becomes the real stderr,
# so echo and git output are diagnostics. fd 2 goes to /dev/null: on macOS the
# setpgid call bash makes in each new background job sometimes fails with EPERM
# after the parent has already moved the job into its group, and bash reports
# that, and job status changes, on fd 2. Commands whose errors matter send
# their stderr to fd 1.
exec 3>&1 1>&2 2>/dev/null

harness=${1:-claude}
url=${AGENT_GUIDANCE_URL:-https://github.com/paritytech/agent-guidance.git}
# The URL as the status line shows it, without user name and password, which
# would otherwise reach the user's screen and transcripts. In a URL with a
# scheme the userinfo ends at the last @ before the first / of the authority.
# In the scp form (user@host:path) everything up to the last @ before the last
# : is dropped. git reads user:password@host:path as host "user", so dropping
# the text before the @ there only keeps a mistyped token off the screen.
# Local paths are shown as they are.
shown_url=$url
case $url in
*://*)
	url_rest=${url#*://}
	url_authority=${url_rest%%/*}
	case $url_authority in
	*@*) shown_url=${url%%://*}://${url_authority##*@}${url_rest#"$url_authority"} ;;
	esac
	;;
/* | .*) ;;
*@*:*)
	url_head=${url%:*}
	case $url_head in
	*@*) shown_url=${url_head##*@}:${url##*:} ;;
	esac
	;;
esac
# Seconds a blocking git command may run before the watchdog stops it.
git_budget=4
# Seconds the background fetch may run. Anything but a positive integer means 30.
background_budget=${AGENT_GUIDANCE_BACKGROUND_BUDGET:-30}
case $background_budget in
'' | *[!0123456789]*) background_budget=30 ;;
*) [ "$background_budget" -gt 0 ] || background_budget=30 ;;
esac

# With CDPATH set, cd prints the directory it changed to.
unset CDPATH
root=$(cd "$(dirname "$0")/.." && pwd -P)
cache=$root/.agent-guidance
# Touched after every successful fetch or clone.
marker=$cache/.git/agent-guidance-last-fetch
background_fetch=
tmp=
git_pid=
watchdog_pid=

# Never prompt for credentials or host keys, and give up on stalled transfers.
# GIT_SSH_COMMAND also covers HTTPS URLs that an insteadOf rule rewrites to SSH.
export GIT_TERMINAL_PROMPT=0
export GCM_INTERACTIVE=never
export GIT_SSH_COMMAND='ssh -o BatchMode=yes -o ConnectTimeout=3'
git_opts=(-c http.lowSpeedLimit=1000 -c http.lowSpeedTime=3)

# git must only ever operate on the cache. Without these, a cache directory
# that lacks .git would make git search upwards and act on the downstream
# repository, and variables inherited from a git hook would redirect it. The
# unset list follows `git rev-parse --local-env-vars`, except GIT_CONFIG_COUNT
# and GIT_CONFIG_PARAMETERS, which carry configuration given on purpose.
export GIT_CEILING_DIRECTORIES=$root
unset GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE GIT_COMMON_DIR GIT_OBJECT_DIRECTORY
unset GIT_ALTERNATE_OBJECT_DIRECTORIES GIT_NAMESPACE GIT_SHALLOW_FILE GIT_GRAFT_FILE
unset GIT_REPLACE_REF_BASE GIT_NO_REPLACE_OBJECTS GIT_IMPLICIT_WORK_TREE GIT_PREFIX
unset GIT_CONFIG

# Succeeds when the cache is a git repository of its own: git, confined by
# GIT_CEILING_DIRECTORIES, resolves the cache's git directory to $cache/.git.
# An empty or damaged .git fails the check, so the cache is cloned again.
cache_is_repository() {
	[ -d "$cache/.git" ] || return 1
	local git_dir
	git_dir=$(git -C "$cache" rev-parse --absolute-git-dir </dev/null) || return 1
	[ "$git_dir" = "$cache/.git" ]
}

# Job control puts every background job in its own process group. The watchdog
# can then stop git together with the helpers it starts (git-remote-https, ssh),
# which would otherwise keep running and hold stderr open.
set -m

# Runs git in the background and stops it with SIGTERM after git_budget seconds.
# SIGTERM lets git remove its lock files and partial clones before it exits.
#
# Neither git nor the watchdog inherits fd 3: a process holding the harness's
# stdout pipe would make the harness wait for it after the loader has exited.
run_git() {
	git "${git_opts[@]}" "$@" </dev/null 2>&1 3>&- &
	git_pid=$!
	(
		sleep "$git_budget"
		kill -TERM -- "-$git_pid"
	) >/dev/null 2>&1 3>&- &
	watchdog_pid=$!
	disown "$watchdog_pid"
	wait "$git_pid"
	local status=$?
	if [ "$status" -gt 128 ]; then
		echo "agent-guidance: git did not finish within $git_budget seconds and was stopped"
	fi
	kill -TERM -- "-$watchdog_pid"
	git_pid=
	watchdog_pid=
	return "$status"
}

# When the harness stops the loader, stop git and the watchdog as well and
# remove a partial clone.
trap '
	[ -z "$git_pid" ] || kill -TERM -- "-$git_pid"
	[ -z "$watchdog_pid" ] || kill -TERM -- "-$watchdog_pid"
	rm -rf "$tmp"
	exit 0
' HUP INT TERM

# How this run obtained the cache, for the status line: cached, fetched,
# cloned, fetch_failed or apply_failed. reason says why nothing was loaded.
state=
reason=

if [ -z "$root" ]; then
	reason="cannot locate the repository root from $0"
	echo "agent-guidance: $reason"
elif ! command -v git >/dev/null; then
	reason="git not found"
	echo "agent-guidance: $reason"
elif cache_is_repository; then
	# With a fetch from the last 7 days, the session starts from the cached copy
	# and a background fetch picks up changes for the next session. Otherwise,
	# for example when the harness kills background jobs, fetch before serving.
	if [ -f "$marker" ] && [ -z "$(find "$marker" -mmin +10080)" ]; then
		background_fetch=1
		state=cached
	elif run_git -C "$cache" fetch --depth 1 --quiet "$url" HEAD; then
		touch "$marker" 2>&1
		state=fetched
	else
		echo "agent-guidance: fetch failed, using the cached copy"
		state=fetch_failed
	fi
	# Check out the commit the last fetch stored in FETCH_HEAD, when it differs
	# from the current one. A depth 1 fetch cuts the new commit off from the
	# cached one, so merge --ff-only would refuse it. reset --hard moves to it
	# and discards local edits; the loader owns the cache. reset rewrites the
	# files in place, so a session that reads the cache at the same moment can
	# see a mix of old and new files. If the reset fails, for example because
	# another session holds index.lock, the current files are served. FETCH_HEAD
	# is empty while a fetch runs and is removed once applied.
	if [ -s "$cache/.git/FETCH_HEAD" ]; then
		head=$(git -C "$cache" rev-parse -q --verify HEAD)
		fetched=$(git -C "$cache" rev-parse -q --verify FETCH_HEAD)
		if [ -n "$fetched" ] && [ "$head" != "$fetched" ]; then
			if run_git -C "$cache" reset --hard --quiet "$fetched"; then
				rm -f "$cache/.git/FETCH_HEAD"
			else
				echo "agent-guidance: could not apply the last fetch, using the cached copy"
				# A failed fetch is the more useful thing to report.
				[ "$state" = fetch_failed ] || state=apply_failed
			fi
		fi
	fi
else
	# Clone next to the cache and move it into place only when complete, so an
	# interrupted clone never leaves a partial cache behind.
	tmp=$root/.agent-guidance.tmp.$$
	rm -rf "$tmp"
	if run_git clone --depth 1 --quiet "$url" "$tmp"; then
		# A cache directory that is not a repository, for example files copied
		# by hand or an empty .git, is not usable. Replace it now that a complete
		# clone exists.
		#
		# Two first runs can race here without a lock: one may remove the cache
		# the other has just moved into place, or the cache may vanish while a
		# third session reads it. This is acceptable: each run removes only
		# $cache, the race ends with one valid clone in place, and the worst case
		# is one session that serves the fallback line.
		if [ -e "$cache" ] && ! cache_is_repository; then
			rm -rf "$cache" 2>&1
		fi
		if [ ! -e "$cache" ]; then
			mv "$tmp" "$cache" 2>&1
		fi
		# A concurrent loader can create the cache between the test and the mv,
		# in which case mv moved this clone inside it.
		rm -rf "${cache:?}/${tmp##*/}"
		touch "$marker" 2>&1
		state=cloned
	else
		echo "agent-guidance: could not clone $url"
		reason="could not clone $shown_url"
	fi
	rm -rf "$tmp"
	tmp=
fi

# Only a cache that is a repository of its own is served.
serve=
if [ -n "$root" ] && cache_is_repository && [ -f "$cache/GUIDANCE.md" ]; then
	serve=1
fi

# The version in the fetched loader-version file when it is greater than
# LOADER_VERSION. A missing file or a value that is not a short integer is
# ignored.
outdated=
if [ -n "$serve" ]; then
	latest=
	read -r latest <"$cache/loader-version"
	case $latest in
	'' | *[!0123456789]* | ??????????*) ;;
	*) [ "$latest" -le "$LOADER_VERSION" ] || outdated=$latest ;;
	esac
fi
# The installer embeds the loader it writes, so it has to be updated first.
upgrade="run cargo install --git https://github.com/paritytech/agent-guidance"
upgrade="$upgrade, then agent-guidance-install, and commit"

# The status line for the user. The commit comes from git's own records in the
# cache, a local operation like the rev-parse calls above; run_git is not used
# because inside a command substitution job control is off, so its watchdog
# could not stop git. Anything but a hex string is dropped.
status_line() {
	local message sha=
	if [ -n "$serve" ]; then
		sha=$(git -C "$cache" rev-parse --short HEAD </dev/null)
		case $sha in
		'' | *[!0123456789abcdef]*) sha= ;;
		*) sha=" $sha" ;;
		esac
		case $state in
		fetch_failed) message="fetch failed, loaded cached$sha" ;;
		apply_failed) message="could not apply fetch, loaded cached$sha" ;;
		*) message="loaded$sha (${state:-cached})" ;;
		esac
	elif [ -n "$reason" ]; then
		message="NOT loaded, $reason"
	elif [ "$state" = fetch_failed ]; then
		message="NOT loaded, fetch failed, no usable copy in .agent-guidance/"
	else
		message="NOT loaded, no usable copy in .agent-guidance/"
	fi
	if [ -n "$outdated" ]; then
		message="$message; loader v$LOADER_VERSION out of date (v$outdated available): $upgrade"
	fi
	printf 'parity-agent-guidance: %s\n' "$message"
}

context() {
	if [ -n "$serve" ]; then
		printf "Company agent guidance follows. Files it links to live under \`%s/\`.\n\n" \
			"$cache"
		cat "$cache/GUIDANCE.md"
		if [ -n "$outdated" ] && [ -n "$notice_in_context" ]; then
			printf '\n%s%s%s\n' \
				"The agent-guidance loader committed in this repository is version" \
				" $LOADER_VERSION; the current version is $outdated. Tell the user to" \
				" update it: $upgrade."
		fi
	else
		printf '%s%s\n' \
			"Company agent guidance could not be fetched for this session. Follow the" \
			" agent-guidance instruction in AGENTS.md or CLAUDE.md to read it."
	fi
}

# Both escapers produce the same bytes: control characters other than tab, LF
# and CR are removed first, one trailing newline is dropped, and non-ASCII
# bytes pass through unchanged.
#
# /usr/bin/python3 on macOS is a stub that opens an installation dialog when the
# command line tools are missing, so it is used only when they are installed.
# -I keeps python from importing modules from the current directory or from
# PYTHON* variables.
use_python=
if command -v python3 >/dev/null; then
	if [ "$(command -v python3)" != /usr/bin/python3 ] || [ "$(uname)" != Darwin ]; then
		use_python=1
	elif xcode-select -p >/dev/null; then
		use_python=1
	fi
fi

# Prints standard input as a JSON string. The input is read once, so the awk
# escaper can run on it when python fails. The x appended to it keeps the
# command substitution from removing trailing newlines; tr has removed every
# NUL byte, which a shell variable cannot hold.
json_string() {
	local text out=
	# In some multibyte locales (GBK, Big5, Shift-JIS) an x can be the second
	# byte of a character, and ${text%x} would then keep it.
	local LC_ALL=C
	text=$(
		LC_ALL=C tr -d '\000-\010\013\014\016-\037\177'
		printf x
	)
	text=${text%x}
	if [ -n "$use_python" ]; then
		out=$(printf '%s' "$text" | python3 -I -c '
import json, sys
s = sys.stdin.buffer.read().decode("utf-8", "surrogateescape")
if s.endswith("\n"):
    s = s[:-1]
out = json.dumps(s, ensure_ascii=False)
sys.stdout.buffer.write(out.encode("utf-8", "surrogateescape"))
') || out=
	fi
	if [ -z "$out" ]; then
		out=$(printf '%s' "$text" | LC_ALL=C awk '
BEGIN { printf "\"" }
{
	if (NR > 1) printf "\\n"
	line = ""
	n = length($0)
	for (i = 1; i <= n; i++) {
		c = substr($0, i, 1)
		if (c == "\\") c = "\\\\"
		else if (c == "\"") c = "\\\""
		else if (c == "\t") c = "\\t"
		else if (c == "\r") c = "\\r"
		line = line c
	}
	printf "%s", line
}
END { printf "\"" }
')
	fi
	if [ -z "$out" ]; then
		out='""'
	fi
	printf '%s' "$out"
}

# Where a harness shows the status line, the out-of-date notice goes only there.
notice_in_context=
case $harness in
cursor | copilot) notice_in_context=1 ;;
esac
escaped=$(context | json_string)

# Claude Code, Codex and Gemini CLI show a top-level systemMessage to the user.
# Cursor and Copilot document no such field, so their output omits it.
case $harness in
cursor)
	printf '{"additional_context":%s}\n' "$escaped" >&3
	;;
copilot)
	printf '{"additionalContext":%s}\n' "$escaped" >&3
	;;
*)
	case $harness in
	claude | codex | gemini) ;;
	*) echo "agent-guidance: unknown harness '$harness', printing the Claude Code shape" ;;
	esac
	printf '{"hookSpecificOutput":{"hookEventName":"SessionStart","additionalContext":%s},"systemMessage":%s}\n' \
		"$escaped" "$(status_line | json_string)" >&3
	;;
esac

# The background fetch only updates .git; the next session checks out the
# result. It runs in its own process group (set -m), so it survives a harness
# that kills the loader's group after it exits, and with every descriptor
# redirected, so the harness does not wait for it. Its timer stops the whole
# group, git helpers included, after background_budget seconds. When the fetch
# ends first, the subshell stops its own group, which ends the timer and its
# sleep, so no process keeps the group alive after its leader has exited.
if [ -n "$background_fetch" ]; then
	(
		git "${git_opts[@]}" -C "$cache" fetch --depth 1 --quiet "$url" HEAD &
		fetch_pid=$!
		(
			sleep "$background_budget"
			kill -TERM 0
		) &
		if wait "$fetch_pid"; then
			touch "$marker"
		fi
		kill -TERM 0
	) </dev/null >/dev/null 2>&1 3>&- &
	disown "$!"
fi
exit 0
