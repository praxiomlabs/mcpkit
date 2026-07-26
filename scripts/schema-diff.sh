#!/usr/bin/env bash
#
# schema-diff.sh — diff mcpkit's wire vocabulary against the vendored official
# MCP schema (spec/2025-11-25/schema.json, pinned; see spec/PROVENANCE.md).
#
# This is Tier 1 of the spec-conformance audit: flat string-set comparisons.
# It is deliberately mechanical and read-only. It does not resolve $ref, does
# not diff object structure, and does not know anything about mcpkit's types
# beyond what serde attributes say on the wire.
#
# Rows:
#   1. Method names        — 31 `const`s on *.properties.method in $defs
#   2. Notification methods— the notifications/* subset of row 1
#   3. Closed enum variants— the 3 $defs carrying an `enum` array
#   4. Content types       — the 16 $defs carrying a `const` on properties.type
#
# Direction rules (see the audit brief):
#   Rows 1 and 2 are reported IN-SCHEMA-NOT-IN-MCPKIT only. The mcpkit side is a
#   deliberately wide literal extraction; its complement is ~562 arbitrary Rust
#   string literals and carries no conformance signal.
#   Rows 3 and 4 are reported in both directions.
#
# Usage:
#   scripts/schema-diff.sh              human-readable report
#   scripts/schema-diff.sh --baseline   machine-readable difference set, one per
#                                       line, sorted — diffed against
#                                       spec/schema-diff-baseline.txt by CI.
#
# The baseline is a set of *accepted* differences, not an expected-failure count.
# It is empty today: mcpkit implements 31/31 spec methods. Any new line is a
# regression; a removed line means a difference was closed and the baseline
# should be updated in the same commit.
#
# Requires: jq, grep, awk. Run from the repo root.

set -euo pipefail

MODE="${1:-report}"

SCHEMA="spec/2025-11-25/schema.json"
SRC_GLOB="crates"

if [ ! -f "$SCHEMA" ]; then
	echo "error: $SCHEMA not found — run from the repo root" >&2
	exit 1
fi
command -v jq >/dev/null || { echo "error: jq not found" >&2; exit 1; }

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
BASELINE="$WORK/baseline.txt"
: > "$BASELINE"

# In --baseline mode the human-readable report is suppressed; only the
# difference set reaches stdout.
if [ "$MODE" = "--baseline" ]; then
	exec 3>&1 1>/dev/null
fi

# emit <category> < lines-on-stdin — record each difference for the baseline.
emit() { sed "s|^|$1: |" >> "$BASELINE"; }

hr() { printf '\n== %s ==\n' "$1"; }

# ---------------------------------------------------------------------------
# Rust enum wire-name extractor.
#
# extract_enum <file> <EnumName>
#
# Emits one serde-visible wire string per line for that enum's variants:
# an explicit #[serde(rename = "x")] wins, otherwise the container's
# #[serde(rename_all = "...")] convention is applied to the variant identifier.
# ---------------------------------------------------------------------------
extract_enum() {
	# Join multi-line #[serde(...)] blocks first — a rename sitting on a
	# continuation line would otherwise be silently dropped.
	awk '
	{
		if (buf != "") { buf = buf " " $0 }
		else if ($0 ~ /^[ \t]*#\[/) { buf = $0 }
		else { print; next }
		o = gsub(/[([]/, "&"); c = gsub(/[)\]]/, "&")
		bal += o - c
		if (bal <= 0) { print buf; buf = ""; bal = 0 }
	}
	END { if (buf != "") print buf }
	' "$1" |
	awk -v want="$2" '
	function to_snake(s,   i, c, out) {
		out = ""
		for (i = 1; i <= length(s); i++) {
			c = substr(s, i, 1)
			if (c ~ /[A-Z]/) { if (i > 1) out = out "_"; c = tolower(c) }
			out = out c
		}
		return out
	}
	function convert(v, conv) {
		if (conv == "lowercase")            return tolower(v)
		if (conv == "UPPERCASE")            return toupper(v)
		if (conv == "snake_case")           return to_snake(v)
		if (conv == "SCREAMING_SNAKE_CASE") return toupper(to_snake(v))
		if (conv == "kebab-case")           { gsub(/_/, "-", v); return v }
		if (conv == "camelCase")            return tolower(substr(v,1,1)) substr(v,2)
		return v   # PascalCase or no rename_all: identity
	}
	# Collect attributes seen since the last item boundary.
	/#\[serde\(/ {
		if (match($0, /rename_all[ ]*=[ ]*"[^"]+"/)) {
			pend_all = substr($0, RSTART, RLENGTH)
			sub(/^rename_all[ ]*=[ ]*"/, "", pend_all); sub(/"$/, "", pend_all)
		}
		if (match($0, /(^|[(,][ ]*)rename[ ]*=[ ]*"[^"]+"/)) {
			pend_rename = substr($0, RSTART, RLENGTH)
			sub(/^.*rename[ ]*=[ ]*"/, "", pend_rename); sub(/"$/, "", pend_rename)
		}
		next
	}
	# Enter the wanted enum. The rename_all pending at this point is the
	# container convention.
	!done && !inside && $0 ~ ("enum[ ]+" want "[ ]*(<[^>]*>)?[ ]*\\{") {
		o = gsub(/\{/, "{"); c = gsub(/\}/, "}")
		depth = o - c
		if (depth <= 0) { done = 1; next }
		inside = 1; conv = pend_all; pend_rename = ""; next
	}
	inside {
		n = gsub(/\{/, "{"); depth += n
		n = gsub(/\}/, "}"); depth -= n
		if (depth <= 0) { inside = 0; done = 1; next }
		if ($0 ~ /^[ \t]*\/\//) next                 # doc / line comment
		if ($0 ~ /^[ \t]*#\[/)  next                 # non-serde attribute
		# A variant: an identifier at the head of the line.
		if (match($0, /^[ \t]*[A-Z][A-Za-z0-9_]*/)) {
			v = substr($0, RSTART, RLENGTH); gsub(/[ \t]/, "", v)
			print (pend_rename != "" ? pend_rename : convert(v, conv))
			pend_rename = ""
		}
	}
	'
}

# ---------------------------------------------------------------------------
# Set difference helper: diff_sets <label> <a-file> <b-file> <a-name> <b-name>
# ---------------------------------------------------------------------------
only_in() { comm -23 "$1" "$2"; }
both_in() { comm -12 "$1" "$2"; }

printf 'mcpkit spec-conformance diff — Tier 1\n'
printf 'schema: %s\n' "$SCHEMA"
printf 'schema sha256: %s\n' "$(sha256sum "$SCHEMA" | cut -d' ' -f1)"
printf 'defs: %s\n' "$(jq '.["$defs"] | length' "$SCHEMA")"

# ===========================================================================
# Wide mcpkit literal extraction (shared by rows 1 and 2).
#
# Deliberately wide: `initialize` and `ping` contain no slash, so a
# "<ns>/<method>" filter would silently drop them — including the one the
# whole negotiation claim rests on. Extract broadly, intersect one way.
# ===========================================================================
grep -rhoE '"[a-z][a-zA-Z_/]*"' "$SRC_GLOB"/*/src --include='*.rs' 2>/dev/null |
	tr -d '"' | sort -u > "$WORK/mcpkit_literals.txt"

# ===========================================================================
# Row 1 — method names
# ===========================================================================
jq -r '.["$defs"] | to_entries[] | select(.value.properties.method.const)
       | .value.properties.method.const' "$SCHEMA" | sort -u > "$WORK/schema_methods.txt"

hr "Row 1: method names (schema-side difference only)"
printf 'wide literals from %s/*/src : %s\n' "$SRC_GLOB" "$(wc -l < "$WORK/mcpkit_literals.txt")"
printf 'schema methods                : %s\n' "$(wc -l < "$WORK/schema_methods.txt")"
printf 'intersect with schema methods : %s / %s\n' \
	"$(both_in "$WORK/schema_methods.txt" "$WORK/mcpkit_literals.txt" | wc -l)" \
	"$(wc -l < "$WORK/schema_methods.txt")"
printf 'in-schema-not-in-mcpkit       :\n'
only_in "$WORK/schema_methods.txt" "$WORK/mcpkit_literals.txt" | sed 's/^/  /' || true
only_in "$WORK/schema_methods.txt" "$WORK/mcpkit_literals.txt" | grep -q . || printf '  (none)\n'
only_in "$WORK/schema_methods.txt" "$WORK/mcpkit_literals.txt" | emit 'method in-schema-not-in-mcpkit'

# ===========================================================================
# Row 2 — notification methods
# ===========================================================================
grep '^notifications/' "$WORK/schema_methods.txt" | sort -u > "$WORK/schema_notifs.txt"
grep '^notifications/' "$WORK/mcpkit_literals.txt" | sort -u > "$WORK/mcpkit_notifs.txt" || true

hr "Row 2: notification methods (schema-side difference only)"
printf 'mcpkit notifications/* literals: %s\n' "$(wc -l < "$WORK/mcpkit_notifs.txt")"
printf 'schema notification methods    : %s\n' "$(wc -l < "$WORK/schema_notifs.txt")"
printf 'intersect                      : %s / %s\n' \
	"$(both_in "$WORK/schema_notifs.txt" "$WORK/mcpkit_notifs.txt" | wc -l)" \
	"$(wc -l < "$WORK/schema_notifs.txt")"
printf 'in-schema-not-in-mcpkit        :\n'
only_in "$WORK/schema_notifs.txt" "$WORK/mcpkit_notifs.txt" | sed 's/^/  /' || true
only_in "$WORK/schema_notifs.txt" "$WORK/mcpkit_notifs.txt" | grep -q . || printf '  (none)\n'
only_in "$WORK/schema_notifs.txt" "$WORK/mcpkit_notifs.txt" | emit 'notification in-schema-not-in-mcpkit'

# ===========================================================================
# Row 3 — closed enum variants (both directions, per enum)
#
# Exactly three $defs carry an `enum` array. stopReason is an open string in
# 2025-11-25 and is deliberately not treated as a closed set here.
# ===========================================================================
hr "Row 3: closed enum variants (both directions)"
for name in $(jq -r '.["$defs"] | to_entries[] | select(.value.enum) | .key' "$SCHEMA" | sort); do
	jq -r --arg n "$name" '.["$defs"][$n].enum[]' "$SCHEMA" | sort -u > "$WORK/se.txt"
	src="$(grep -rl "pub enum $name\b" "$SRC_GLOB"/*/src --include='*.rs' 2>/dev/null | awk 'NR==1')"
	if [ -z "$src" ]; then
		printf '%-14s NOT FOUND in mcpkit sources\n' "$name"
		continue
	fi
	extract_enum "$src" "$name" | sort -u > "$WORK/me.txt"
	printf '%s  (%s)\n' "$name" "$src"
	printf '  in-mcpkit-not-in-schema: %s%s\n' \
		"$(only_in "$WORK/me.txt" "$WORK/se.txt" | wc -l)" \
		"$(only_in "$WORK/me.txt" "$WORK/se.txt" | paste -sd, - | sed 's/^/  -> /;s/^  -> $//')"
	printf '  in-schema-not-in-mcpkit: %s%s\n' \
		"$(only_in "$WORK/se.txt" "$WORK/me.txt" | wc -l)" \
		"$(only_in "$WORK/se.txt" "$WORK/me.txt" | paste -sd, - | sed 's/^/  -> /;s/^  -> $//')"
	printf '  both                   : %s\n' "$(both_in "$WORK/se.txt" "$WORK/me.txt" | wc -l)"
	only_in "$WORK/me.txt" "$WORK/se.txt" | emit "enum $name in-mcpkit-not-in-schema"
	only_in "$WORK/se.txt" "$WORK/me.txt" | emit "enum $name in-schema-not-in-mcpkit"
done

# ===========================================================================
# Row 4 — content types (both directions)
#
# Schema side: unique `const` values on properties.type across 16 $defs.
# mcpkit side: wire-visible variant names of every #[serde(tag = "type")]
# enum in crates/*/src, plus explicit renames. This over-collects on purpose
# (internal debug/discovery enums also tag on "type"); the over-collection
# lands in in-mcpkit-not-in-schema and is classified in the report, not here.
# ===========================================================================
jq -r '.["$defs"] | to_entries[] | select(.value.properties.type.const)
       | .value.properties.type.const' "$SCHEMA" | sort -u > "$WORK/schema_types.txt"

: > "$WORK/mcpkit_types.txt"
: > "$WORK/mcpkit_type_sources.txt"
while IFS=: read -r file line _; do
	# The enum declaration follows the #[serde(tag = "type")] attribute block.
	ename="$(awk -v start="$line" 'NR >= start && /enum[ ]+[A-Za-z0-9_]+/ {
		match($0, /enum[ ]+[A-Za-z0-9_]+/); s = substr($0, RSTART, RLENGTH)
		sub(/^enum[ ]+/, "", s); print s; exit }' "$file")"
	[ -n "$ename" ] || continue
	extract_enum "$file" "$ename" >> "$WORK/mcpkit_types.txt"
	printf '%s::%s (%s:%s)\n' "$(basename "$file" .rs)" "$ename" "$file" "$line" \
		>> "$WORK/mcpkit_type_sources.txt"
done < <(grep -rn 'tag = "type"' "$SRC_GLOB"/*/src --include='*.rs' 2>/dev/null)
sort -u -o "$WORK/mcpkit_types.txt" "$WORK/mcpkit_types.txt"

hr "Row 4: content types (both directions)"
printf 'tag=\"type\" enums scanned in mcpkit:\n'
sed 's/^/  /' "$WORK/mcpkit_type_sources.txt"
printf 'schema type consts (defs=%s, unique=%s)\n' \
	"$(jq '[.["$defs"][] | select(.properties.type.const)] | length' "$SCHEMA")" \
	"$(wc -l < "$WORK/schema_types.txt")"
printf 'mcpkit type discriminators: %s\n' "$(wc -l < "$WORK/mcpkit_types.txt")"
printf '  in-mcpkit-not-in-schema: %s\n' "$(only_in "$WORK/mcpkit_types.txt" "$WORK/schema_types.txt" | wc -l)"
only_in "$WORK/mcpkit_types.txt" "$WORK/schema_types.txt" | sed 's/^/    /'
printf '  in-schema-not-in-mcpkit: %s\n' "$(only_in "$WORK/schema_types.txt" "$WORK/mcpkit_types.txt" | wc -l)"
only_in "$WORK/schema_types.txt" "$WORK/mcpkit_types.txt" | sed 's/^/    /'
printf '  both                   : %s\n' "$(both_in "$WORK/schema_types.txt" "$WORK/mcpkit_types.txt" | wc -l)"
only_in "$WORK/mcpkit_types.txt" "$WORK/schema_types.txt" | emit 'content-type in-mcpkit-not-in-schema'
only_in "$WORK/schema_types.txt" "$WORK/mcpkit_types.txt" | emit 'content-type in-schema-not-in-mcpkit'

# ===========================================================================
# Error codes — manual-check aid only, NOT a set difference.
#
# schema.json contains exactly one error code; the JSON-RPC codes live in
# prose. A set difference here would be vacuous and alarming, so this section
# only prints both sides for a human to compare.
# ===========================================================================
hr "Error codes (informational — verify against prose spec by hand)"
printf 'in schema.json:\n'
grep -oE '\-3[0-9]{4}' "$SCHEMA" | sort -u | sed 's/^/  /'
printf 'in crates/mcpkit-core/src/error/codes.rs:\n'
grep -oE '=[ ]*-3[0-9]{4}' crates/mcpkit-core/src/error/codes.rs 2>/dev/null |
	grep -oE '\-3[0-9]{4}' | sort -u | sed 's/^/  /'

# ===========================================================================
# Tier 2 — structural field diff, SAMPLE ONLY.
#
# Seven types, chosen because the conformance claim leans on them. The
# remaining ~138 $defs are NOT checked. Do not read a clean Tier 2 here as a
# clean structural surface.
#
# Schema side: properties + required, with #/$defs/ $ref and allOf resolved
# (see resolve() below). Two levels of nesting is enough for these seven.
# Rust side: serde-visible field names and wire-optionality. A field is
# OPTIONAL if its type is Option<...>, or it carries #[serde(default)] or
# #[serde(skip_serializing_if = ...)]; otherwise REQUIRED.
# ===========================================================================

read -r -d '' RESOLVE_JQ <<'JQ' || true
def resolve($root):
  if type != "object" then {properties:{}, required:[]}
  elif has("$ref") then ($root["$defs"][(.["$ref"] | sub("^#/\\$defs/";""))] // {} | resolve($root))
  elif has("allOf") then
    ( reduce (.allOf[] | resolve($root)) as $m ({properties:{}, required:[]};
        {properties: (.properties + $m.properties), required: (.required + $m.required)}) ) as $merged
    | {properties: ($merged.properties + (.properties // {})),
       required:   (($merged.required + (.required // [])) | unique)}
  else {properties: (.properties // {}), required: (.required // [])}
  end;
JQ

# extract_struct <file> <StructName> -> "<wire-name>\t<req|opt>" per field
#
# Wire-optionality is NOT the same as Rust Option<T>:
#   skip_serializing_if  -> the field may be ABSENT           => opt
#   Option<T> alone      -> always emitted, possibly as null  => req (nullable)
#   default              -> absence TOLERATED on receive; does not change what
#                           mcpkit emits, so it is reported on a separate axis
#                           ("lenient") rather than as opt.
# Conflating Option<T> with wire-optional produces false positives — e.g.
# Task.ttl is `Option<u64>` with no skip_serializing_if, and the schema declares
# ttl required with type ["integer","null"]. mcpkit is correct there.
#
# Multi-line #[serde(...)] blocks are joined before parsing; not doing so drops
# renames that sit on a continuation line (e.g. CallToolResult.structuredContent).
extract_struct() {
	# Join multi-line attribute blocks into one line.
	awk '
	{
		if (buf != "") { buf = buf " " $0 }
		else if ($0 ~ /^[ \t]*#\[/) { buf = $0 }
		else { print; next }
		o = gsub(/[([]/, "&"); c = gsub(/[)\]]/, "&")
		bal += o - c
		if (bal <= 0) { print buf; buf = ""; bal = 0 }
	}
	END { if (buf != "") print buf }
	' "$1" |
	awk -v want="$2" '
	function to_snake(s,   i, c, out) {
		out = ""
		for (i = 1; i <= length(s); i++) {
			c = substr(s, i, 1)
			if (c ~ /[A-Z]/) { if (i > 1) out = out "_"; c = tolower(c) }
			out = out c
		}
		return out
	}
	function convert(v, conv) {
		if (conv == "camelCase") { n = split(v, p, "_"); r = p[1]
			for (i = 2; i <= n; i++) r = r toupper(substr(p[i],1,1)) substr(p[i],2)
			return r }
		if (conv == "snake_case")  return to_snake(v)
		if (conv == "lowercase")   return tolower(v)
		if (conv == "PascalCase")  { n = split(v, p, "_"); r = ""
			for (i = 1; i <= n; i++) r = r toupper(substr(p[i],1,1)) substr(p[i],2)
			return r }
		if (conv == "kebab-case")  { gsub(/_/, "-", v); return v }
		return v
	}
	/#\[serde\(/ {
		if (match($0, /rename_all[ ]*=[ ]*"[^"]+"/)) {
			a = substr($0, RSTART, RLENGTH)
			sub(/^rename_all[ ]*=[ ]*"/, "", a); sub(/"$/, "", a)
			if (!inside) pend_all = a
		}
		if (match($0, /(^|[(,][ ]*)rename[ ]*=[ ]*"[^"]+"/)) {
			r = substr($0, RSTART, RLENGTH)
			sub(/^.*rename[ ]*=[ ]*"/, "", r); sub(/"$/, "", r)
			pend_rename = r
		}
		if ($0 ~ /skip_serializing_if/) pend_opt = 1
		if ($0 ~ /[(,][ ]*default([ ]*=|[ ]*[,)])/) pend_def = 1
		if ($0 ~ /[(,][ ]*flatten[ ]*[,)]/) pend_flat = 1
		if ($0 ~ /[(,][ ]*skip[ ]*[,)]/) pend_skip = 1
		next
	}
	!done && !inside && $0 ~ ("struct[ ]+" want "[ ]*(<[^>]*>)?[ ]*\\{") {
		# Count braces on the declaration line: `pub struct X {}` opens and
		# closes here, and treating it as open bleeds into whatever follows
		# (test modules were being read as fields).
		o = gsub(/\{/, "{"); c = gsub(/\}/, "}")
		depth = o - c
		if (depth <= 0) { done = 1; next }
		inside = 1; conv = pend_all
		pend_rename = ""; pend_opt = 0; pend_def = 0; pend_flat = 0; pend_skip = 0; next
	}
	inside {
		n = gsub(/\{/, "{"); depth += n
		n = gsub(/\}/, "}"); depth -= n
		if (depth <= 0) { inside = 0; done = 1; next }
		if ($0 ~ /^[ \t]*\/\//) next
		if ($0 ~ /^[ \t]*#\[/)  next
		if (match($0, /^[ \t]*(pub[ ]+)?[a-z_][A-Za-z0-9_]*[ ]*:/)) {
			f = substr($0, RSTART, RLENGTH)
			sub(/^[ \t]*/, "", f); sub(/^pub[ ]+/, "", f); sub(/[ ]*:$/, "", f)
			# Only skip_serializing_if makes a field absent from the wire.
			opt = pend_opt ? "opt" : "req"
			if (pend_def && !pend_opt) opt = opt ",lenient"
			if (pend_skip) { }                                  # not on the wire at all
			else if (pend_flat) {
				# Splice marker: the driver resolves the flattened type.
				ty = $0; sub(/^[^:]*:[ ]*/, "", ty)
				sub(/[ ]*,?[ ]*$/, "", ty); sub(/<.*$/, "", ty)
				print "*flatten:" ty "\t" opt
			}
			else print (pend_rename != "" ? pend_rename : convert(f, conv)) "\t" opt
			pend_rename = ""; pend_opt = 0; pend_def = 0; pend_flat = 0; pend_skip = 0
		}
	}
	'
}

# extract_struct_flat <file> <StructName> — extract_struct with #[serde(flatten)]
# fields spliced in (one level; enough for the sampled types).
extract_struct_flat() {
	extract_struct "$1" "$2" | while IFS=$'\t' read -r name opt; do
		case "$name" in
		'*flatten:'*)
			ft="${name#*flatten:}"
			fsrc="$(grep -rl "pub struct $ft\b" "$SRC_GLOB"/*/src --include='*.rs' 2>/dev/null | awk 'NR==1')"
			if [ -n "$fsrc" ]; then extract_struct "$fsrc" "$ft"; else printf '%s\t%s\n' "$name" "$opt"; fi
			;;
		*) printf '%s\t%s\n' "$name" "$opt" ;;
		esac
	done
}

hr "Tier 2: structural field diff (every \$def with a same-named mcpkit struct)"
unresolved=0
diffcount=0

# Every $def with a same-named `pub struct` in the workspace is diffed. Auto-
# discovered rather than listed, so a type added to either side is picked up
# instead of silently staying unchecked — the hardcoded 7-type list this
# replaced left 138 defs unexamined and made the sample look like coverage.
#
# For a `*Request` def the schema node is the JSON-RPC envelope (id/jsonrpc/
# method/params) while the same-named Rust struct is the params, so when a def
# carries `properties.params` that is what gets diffed.
TIER2="$(
	jq -r '.["$defs"] | to_entries[]
	       | .key + "|" + (if (.value.properties.params) then "params" else "self" end)' "$SCHEMA" |
	while IFS='|' read -r ty kind; do
		file="$(grep -rl "^pub struct $ty\b" "$SRC_GLOB"/*/src --include='*.rs' 2>/dev/null | awk 'NR==1')"
		[ -n "$file" ] || continue
		if [ "$kind" = params ]; then
			printf '%s|%s|.["$defs"].%s.properties.params\n' "$ty" "$file" "$ty"
		else
			printf '%s|%s|.["$defs"].%s\n' "$ty" "$file" "$ty"
		fi
	done
)"

while IFS='|' read -r ty file path; do
	[ -n "$ty" ] || continue
	jq -r "$RESOLVE_JQ . as \$root | $path | resolve(\$root)
	       | (.required) as \$r | .properties | keys[] as \$k
	       | \$k + \"\t\" + (if (\$r | index(\$k)) then \"req\" else \"opt\" end)" \
		"$SCHEMA" | sort > "$WORK/s2.txt"
	extract_struct_flat "$file" "$ty" | sort > "$WORK/m2.txt"

	# A schema node that resolves to zero properties is one this resolver cannot
	# expand — an `anyOf` union (ElicitRequestParams) or an abstract JSON-RPC base
	# (Request, Notification) that concrete types extend. Reporting mcpkit's
	# fields as "extra" against an empty set is noise, and noise trains readers to
	# ignore the check. Count them instead.
	if [ "$(wc -l < "$WORK/s2.txt")" -eq 0 ]; then
		printf '\n%s  (%s)  UNRESOLVED — schema node has no expandable properties\n' "$ty" "$file"
		unresolved=$((unresolved + 1))
		continue
	fi

	printf '\n%s  (%s)\n' "$ty" "$file"
	printf '  schema path: %s\n' "$path"
	# `type` is the tagged-union discriminator. mcpkit supplies it from the
	# enum wrapper (#[serde(tag = "type")]), never as a struct field, so its
	# absence from a variant struct is the design, not a gap.
	cut -f1 "$WORK/s2.txt" | grep -vx 'type' | sort -u > "$WORK/s2n.txt"
	cut -f1 "$WORK/m2.txt" | sort -u > "$WORK/m2n.txt"
	printf '  fields  schema=%s  mcpkit=%s\n' \
		"$(wc -l < "$WORK/s2n.txt")" "$(wc -l < "$WORK/m2n.txt")"
	miss="$(only_in "$WORK/s2n.txt" "$WORK/m2n.txt" | paste -sd, -)"
	extra="$(only_in "$WORK/m2n.txt" "$WORK/s2n.txt" | paste -sd, -)"
	printf '  in-schema-not-in-mcpkit: %s\n' "${miss:-(none)}"
	printf '  in-mcpkit-not-in-schema: %s\n' "${extra:-(none)}"
	# Optionality mismatches on the shared fields only. The "lenient" marker
	# (serde default: absence tolerated on receive) is reported separately —
	# it does not change what mcpkit emits, so it is not a mismatch.
	printf '  optionality mismatches :'
	mm=""; len=""
	while IFS=$'\t' read -r k v; do
		case "$v" in *,lenient) len="$len $k"; v="${v%,lenient}";; esac
		sv="$(awk -F'\t' -v k="$k" '$1==k{print $2}' "$WORK/s2.txt")"
		[ -n "$sv" ] || continue
		[ "$sv" = "$v" ] || mm="$mm $k(schema=$sv,mcpkit=$v)"
	done < "$WORK/m2.txt"
	printf '%s\n' "${mm:- (none)}"
	printf '  receive-lenient fields : %s\n' "${len:-(none)}"

	only_in "$WORK/s2n.txt" "$WORK/m2n.txt" | sed "s|^|$ty |" | emit 'field in-schema-not-in-mcpkit'
	only_in "$WORK/m2n.txt" "$WORK/s2n.txt" | sed "s|^|$ty |" | emit 'field in-mcpkit-not-in-schema'
	if [ -n "$miss$extra$mm" ]; then diffcount=$((diffcount + 1)); fi
done <<< "$TIER2"

printf '\nTier 2 summary: %s types diffed, %s with differences, %s unresolved\n' \
	"$(printf '%s\n' "$TIER2" | grep -c .)" "$diffcount" "$unresolved"

printf '\ndone.\n'

if [ "$MODE" = "--baseline" ]; then
	exec 1>&3
	sort "$BASELINE"
fi
